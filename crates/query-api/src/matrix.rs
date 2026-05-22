// Kaleidoscope query-api — Pulse rows -> Prometheus matrix translation
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! The translation logic (DD4).
//!
//! Groups a `Vec<(Metric, MetricPoint)>` into one `PromMatrixEntry` per
//! distinct merged label set. The label map for a point is the union
//! `metric.resource_attributes ∪ point.attributes ∪ {"__name__": name}`
//! with point attributes winning over resource attributes on a key
//! clash, and `__name__` always authoritative for the name key.
//!
//! Time: `time_unix_nano / 1_000_000_000` as an integer-seconds number.
//! Value: `f64` -> minimal-decimal string; `NaN` -> `"NaN"`;
//! `0.0` -> `"0"` (Prometheus' minimal-decimal rendering).

use std::collections::BTreeMap;

use pulse::{Metric, MetricPoint};
use regex::Regex;
use serde::Serialize;

use crate::selector::{LabelMatcher, MatchOp};

/// One Prometheus matrix series. Serialises to
/// `{ "metric": {labels}, "values": [[seconds_number, "value_string"]] }`.
#[derive(Debug, Serialize)]
pub struct PromMatrixEntry {
    pub metric: BTreeMap<String, String>,
    pub values: Vec<(u64, String)>,
}

/// Group Pulse rows into Prometheus matrix series. Rows sharing the
/// identical merged label set form one series; each series' `values`
/// array preserves the ascending-time order Pulse returns. Series are
/// emitted in label-set order (the grouping map is ordered) so the
/// output is deterministic.
pub fn to_matrix(rows: Vec<(Metric, MetricPoint)>) -> Vec<PromMatrixEntry> {
    let mut grouped: BTreeMap<BTreeMap<String, String>, Vec<(u64, String)>> = BTreeMap::new();
    for (metric, point) in rows {
        let labels = merge_labels(&metric, &point);
        let seconds = nanos_to_seconds(point.time_unix_nano);
        let value = format_value(point.value);
        grouped.entry(labels).or_default().push((seconds, value));
    }
    grouped
        .into_iter()
        .map(|(metric, values)| PromMatrixEntry { metric, values })
        .collect()
}

/// Merge the label set for one point (DD4a). Resource attributes first,
/// then point attributes (winning on key clash), then `__name__` always
/// last so it is authoritative for the name key.
fn merge_labels(metric: &Metric, point: &MetricPoint) -> BTreeMap<String, String> {
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in &metric.resource_attributes {
        labels.insert(key.clone(), value.clone());
    }
    for (key, value) in &point.attributes {
        labels.insert(key.clone(), value.clone());
    }
    labels.insert("__name__".to_string(), metric.name.as_str().to_string());
    labels
}

/// A row filter built ONCE per query at filter-build time (ADR-0046
/// Decision 3). Each regex matcher's raw pattern is compiled here once,
/// before the row scan, so the only super-linear-in-pattern step runs
/// once per query rather than per row. The compiled `regex::Regex` lives
/// ONLY here, never in `MatchOp`/`LabelMatcher` (a compiled `Regex` is
/// neither `Eq` nor `Hash`, which those parsed types derive).
#[derive(Debug)]
pub struct MatcherFilter {
    matchers: Vec<CompiledMatcher>,
}

/// One matcher with its compiled form. Exact matchers carry no regex;
/// regex matchers carry the anchored, compiled pattern.
#[derive(Debug)]
struct CompiledMatcher {
    name: String,
    op: MatchOp,
    value: String,
    regex: Option<Regex>,
}

/// Build the per-query filter from the parsed matchers, compiling each
/// regex matcher's raw pattern ONCE wrapped as `^(?:{pattern})$` (the
/// Prometheus full-anchoring rule; ADR-0046 Decision 2). A compile
/// failure is the SINGLE origin of the invalid-regex 400; the returned
/// reason names the matcher as invalid and NEVER echoes the offending
/// pattern, the raw query, or a forwarded header (DD6 redaction).
pub fn build_filter(matchers: &[LabelMatcher]) -> Result<MatcherFilter, String> {
    let mut compiled = Vec::with_capacity(matchers.len());
    for matcher in matchers {
        let regex = match matcher.op {
            MatchOp::Matches | MatchOp::NotMatches => {
                Some(compile_anchored(&matcher.value).ok_or("invalid regex matcher")?)
            }
            MatchOp::Equal | MatchOp::NotEqual => None,
        };
        compiled.push(CompiledMatcher {
            name: matcher.name.clone(),
            op: matcher.op,
            value: matcher.value.clone(),
            regex,
        });
    }
    Ok(MatcherFilter { matchers: compiled })
}

/// Compile the raw user pattern wrapped as `^(?:{pattern})$` (full-string
/// anchoring with the pattern's own alternation correctly bounded).
fn compile_anchored(pattern: &str) -> Option<Regex> {
    Regex::new(&format!("^(?:{pattern})$")).ok()
}

/// True iff the row's derived label set satisfies EVERY matcher (the
/// matchers are ANDed). The label set is derived with the SAME
/// `merge_labels` logic `to_matrix` groups on, so the predicate sees
/// exactly what grouping later folds on (DD2). An empty filter keeps
/// every row (the bare-name behaviour).
pub fn keep_row(metric: &Metric, point: &MetricPoint, filter: &MatcherFilter) -> bool {
    let labels = merge_labels(metric, point);
    filter
        .matchers
        .iter()
        .all(|matcher| matches(&labels, matcher))
}

/// True iff one already-derived label set satisfies one matcher
/// (DD2). Treating an absent label as the empty string yields exactly
/// the Prometheus semantics for every operator and both the empty and
/// non-empty value cases:
///
/// - `label="value"` (non-empty): keep iff present and equal.
/// - `label=""`: keep iff absent OR present-and-empty.
/// - `label!="value"` (non-empty): keep iff absent OR present-and-different.
/// - `label!=""`: keep iff present and non-empty.
/// - `label=~"re"`: keep iff the anchored regex matches the absent-as-empty value.
/// - `label!~"re"`: the exact negation of `=~"re"`.
fn matches(labels: &BTreeMap<String, String>, matcher: &CompiledMatcher) -> bool {
    let actual = labels.get(&matcher.name).map(String::as_str).unwrap_or("");
    match matcher.op {
        MatchOp::Equal => actual == matcher.value,
        MatchOp::NotEqual => actual != matcher.value,
        MatchOp::Matches => regex_matches(matcher, actual),
        MatchOp::NotMatches => !regex_matches(matcher, actual),
    }
}

/// True iff the matcher's compiled, anchored regex matches `actual`. The
/// regex was compiled at filter-build, so it is always present for a
/// regex matcher.
fn regex_matches(matcher: &CompiledMatcher, actual: &str) -> bool {
    matcher
        .regex
        .as_ref()
        .expect("a regex matcher carries its compiled pattern")
        .is_match(actual)
}

/// `time_unix_nano` -> integer seconds (DD4b).
fn nanos_to_seconds(time_unix_nano: u64) -> u64 {
    time_unix_nano / 1_000_000_000
}

/// `f64` -> Prometheus minimal-decimal string (DD4c). `NaN` is the
/// literal string `"NaN"`; finite values render without a trailing
/// `.0` (`0.0` -> `"0"`). Rust's `{}` for `f64` already renders
/// `0.0` as `"0"` and `0.4` as `"0.4"`, matching Prometheus.
fn format_value(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "+Inf".to_string()
        } else {
            "-Inf".to_string()
        };
    }
    format!("{value}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulse::{MetricKind, MetricName};

    fn metric(name: &str, service: &str, resource_extra: &[(&str, &str)]) -> Metric {
        let mut resource = BTreeMap::new();
        resource.insert("service.name".to_string(), service.to_string());
        for (k, v) in resource_extra {
            resource.insert((*k).to_string(), (*v).to_string());
        }
        Metric {
            name: MetricName::new(name),
            description: String::new(),
            unit: "1".to_string(),
            kind: MetricKind::Gauge,
            points: Vec::new(),
            resource_attributes: resource,
        }
    }

    fn point(time_unix_nano: u64, value: f64, attrs: &[(&str, &str)]) -> MetricPoint {
        let mut attributes = BTreeMap::new();
        for (k, v) in attrs {
            attributes.insert((*k).to_string(), (*v).to_string());
        }
        MetricPoint {
            time_unix_nano,
            start_time_unix_nano: 0,
            attributes,
            value,
        }
    }

    // The acceptance suite reaches whole-series shapes; these inline
    // tests pin the merge precedence and the formatting boundaries the
    // acceptance suite cannot isolate.

    #[test]
    fn name_label_always_wins_over_a_colliding_attribute() {
        // A point attribute literally named __name__ must NOT override
        // the metric's authoritative name (DD4a: __name__ is inserted
        // last and is authoritative for the name key).
        let m = metric("real_name", "checkout", &[]);
        let p = point(1_000_000_000, 1.0, &[("__name__", "spoofed")]);
        let series = to_matrix(vec![(m, p)]);
        assert_eq!(series[0].metric["__name__"], "real_name");
    }

    #[test]
    fn point_attribute_wins_over_a_colliding_resource_attribute() {
        // DD4a: point attributes win over resource attributes on a key
        // clash.
        let m = metric("m", "checkout", &[("region", "eu-resource")]);
        let p = point(1_000_000_000, 1.0, &[("region", "us-point")]);
        let series = to_matrix(vec![(m, p)]);
        assert_eq!(series[0].metric["region"], "us-point");
    }

    #[test]
    fn nanos_convert_to_integer_seconds_truncating() {
        // 1_716_200_000_500_000_000 ns is 1_716_200_000 s with a
        // sub-second remainder dropped by integer division.
        let m = metric("m", "checkout", &[]);
        let p = point(1_716_200_000_500_000_000, 1.0, &[]);
        let series = to_matrix(vec![(m, p)]);
        assert_eq!(series[0].values[0].0, 1_716_200_000);
    }

    #[test]
    fn value_formatting_covers_the_prometheus_boundaries() {
        let m = metric("m", "checkout", &[]);
        let series = to_matrix(vec![
            (m.clone(), point(1_000_000_000, 0.0, &[("b", "0")])),
            (m.clone(), point(2_000_000_000, f64::NAN, &[("b", "1")])),
            (m.clone(), point(3_000_000_000, 0.55, &[("b", "2")])),
        ]);
        // Distinct label sets keep them in three series, ordered by the
        // "b" label.
        assert_eq!(series[0].values[0].1, "0", "0.0 renders as \"0\"");
        assert_eq!(series[1].values[0].1, "NaN", "NaN renders as \"NaN\"");
        assert_eq!(series[2].values[0].1, "0.55");
    }

    fn equal(name: &str, value: &str) -> LabelMatcher {
        LabelMatcher {
            name: name.to_string(),
            op: MatchOp::Equal,
            value: value.to_string(),
        }
    }

    fn not_equal(name: &str, value: &str) -> LabelMatcher {
        LabelMatcher {
            name: name.to_string(),
            op: MatchOp::NotEqual,
            value: value.to_string(),
        }
    }

    fn matches_op(name: &str, value: &str) -> LabelMatcher {
        LabelMatcher {
            name: name.to_string(),
            op: MatchOp::Matches,
            value: value.to_string(),
        }
    }

    fn not_matches(name: &str, value: &str) -> LabelMatcher {
        LabelMatcher {
            name: name.to_string(),
            op: MatchOp::NotMatches,
            value: value.to_string(),
        }
    }

    /// Build a never-failing filter for the exact-matcher inline tests.
    fn filter(matchers: &[LabelMatcher]) -> MatcherFilter {
        build_filter(matchers).expect("exact matchers always compile")
    }

    /// True iff the row survives the filter built from `matchers`.
    fn kept(metric: &Metric, point: &MetricPoint, matchers: &[LabelMatcher]) -> bool {
        keep_row(metric, point, &filter(matchers))
    }

    // The filter's four-arm semantics are the correctness oracle
    // (ADR-0044 Decision 3). Each arm is pinned here against a single
    // derived label set so a flipped == / != or a dropped absent-as-empty
    // rule is caught, complementing the per-arm acceptance scenarios.

    #[test]
    fn equality_keeps_present_and_equal_excludes_present_and_different() {
        let m = metric("http_requests_total", "checkout", &[]);
        let p = point(1_000_000_000, 1.0, &[("code", "200")]);
        assert!(kept(&m, &p, &[equal("code", "200")]), "present and equal");
        assert!(
            !kept(&m, &p, &[equal("code", "500")]),
            "present and different"
        );
        assert!(
            kept(&m, &p, &[equal("service.name", "checkout")]),
            "a resource attribute is matchable like a point attribute"
        );
        assert!(
            kept(&m, &p, &[equal("__name__", "http_requests_total")]),
            "__name__ is matchable"
        );
    }

    #[test]
    fn equality_against_empty_string_treats_absent_as_empty() {
        let m = metric("m", "checkout", &[]);
        let absent = point(1_000_000_000, 1.0, &[]);
        let present = point(1_000_000_000, 1.0, &[("code", "200")]);
        assert!(
            kept(&m, &absent, &[equal("code", "")]),
            "an absent label satisfies =\"\""
        );
        assert!(
            !kept(&m, &present, &[equal("code", "")]),
            "a present non-empty label does not satisfy =\"\""
        );
    }

    #[test]
    fn inequality_keeps_absent_and_different_excludes_present_and_equal() {
        let m = metric("m", "checkout", &[]);
        let absent = point(1_000_000_000, 1.0, &[]);
        let present = point(1_000_000_000, 1.0, &[("code", "500")]);
        assert!(
            kept(&m, &absent, &[not_equal("code", "500")]),
            "an absent label satisfies != against a non-empty value"
        );
        assert!(
            !kept(&m, &present, &[not_equal("code", "500")]),
            "a present, equal label fails !="
        );
    }

    #[test]
    fn inequality_against_empty_string_keeps_only_present_non_empty() {
        let m = metric("m", "checkout", &[]);
        let absent = point(1_000_000_000, 1.0, &[]);
        let present = point(1_000_000_000, 1.0, &[("code", "200")]);
        assert!(
            kept(&m, &present, &[not_equal("code", "")]),
            "a present, non-empty label satisfies !=\"\""
        );
        assert!(
            !kept(&m, &absent, &[not_equal("code", "")]),
            "an absent label does not satisfy !=\"\""
        );
    }

    #[test]
    fn matchers_are_anded_and_an_empty_list_keeps_every_row() {
        let m = metric("m", "checkout", &[]);
        let p = point(1_000_000_000, 1.0, &[("code", "200")]);
        assert!(
            kept(
                &m,
                &p,
                &[equal("service.name", "checkout"), equal("code", "200")]
            ),
            "both matchers hold"
        );
        assert!(
            !kept(
                &m,
                &p,
                &[equal("service.name", "checkout"), equal("code", "500")]
            ),
            "one failing matcher excludes the row"
        );
        assert!(kept(&m, &p, &[]), "an empty matcher list keeps the row");
    }

    // The regex arms (ADR-0046). The acceptance suite reaches each arm
    // end to end; these inline tests pin the boundaries a mutant could
    // slip past against a single derived label set: full anchoring, every
    // absent-as-empty arm, the exact `=~`/`!~` negation, and the sharp
    // invalid-compile (filter-build error) versus valid-no-match (200
    // empty) distinction.

    #[test]
    fn a_regex_matcher_anchors_both_ends() {
        // `=~"check"` is compiled as `^(?:check)$`, so a substring-only
        // match must NOT keep the row. A mutant dropping the `^...$`
        // wrapping would wrongly keep "checkout" and is killed here.
        let m = metric("m", "checkout", &[]);
        let p = point(1_000_000_000, 1.0, &[("service.name", "checkout")]);
        assert!(
            !kept(&m, &p, &[matches_op("service.name", "check")]),
            "\"check\" does not fully match \"checkout\""
        );
        assert!(
            kept(&m, &p, &[matches_op("service.name", "check.*")]),
            "\"check.*\" fully matches \"checkout\""
        );
    }

    #[test]
    fn regex_arms_treat_an_absent_label_as_the_empty_string() {
        let m = metric("m", "checkout", &[]);
        let absent = point(1_000_000_000, 1.0, &[]);
        let present = point(1_000_000_000, 1.0, &[("env", "prod")]);
        // =~"" keeps absent (empty string fully matches the empty pattern).
        assert!(
            kept(&m, &absent, &[matches_op("env", "")]),
            "=~\"\" keeps absent"
        );
        assert!(
            !kept(&m, &present, &[matches_op("env", "")]),
            "=~\"\" excludes present non-empty"
        );
        // =~".+" keeps only present non-empty.
        assert!(
            kept(&m, &present, &[matches_op("env", ".+")]),
            "=~\".+\" keeps present non-empty"
        );
        assert!(
            !kept(&m, &absent, &[matches_op("env", ".+")]),
            "=~\".+\" excludes absent-as-empty"
        );
        // !~"" keeps only present non-empty (exact negation of =~"").
        assert!(
            kept(&m, &present, &[not_matches("env", "")]),
            "!~\"\" keeps present non-empty"
        );
        assert!(
            !kept(&m, &absent, &[not_matches("env", "")]),
            "!~\"\" excludes absent-as-empty"
        );
        // !~".+" keeps absent-as-empty (exact negation of =~".+").
        assert!(
            kept(&m, &absent, &[not_matches("env", ".+")]),
            "!~\".+\" keeps absent-as-empty"
        );
        assert!(
            !kept(&m, &present, &[not_matches("env", ".+")]),
            "!~\".+\" excludes present non-empty"
        );
        // !~"prod" keeps the absent-env row (absent-as-empty != "prod").
        assert!(
            kept(&m, &absent, &[not_matches("env", "prod")]),
            "!~\"prod\" keeps absent-as-empty"
        );
    }

    #[test]
    fn not_matches_is_the_exact_negation_of_matches() {
        let m = metric("m", "checkout", &[]);
        let p = point(1_000_000_000, 1.0, &[("route", "/api/orders")]);
        for pattern in ["/api/.*", "/admin/.*", "", ".+", "/api/orders"] {
            let positive = kept(&m, &p, &[matches_op("route", pattern)]);
            let negative = kept(&m, &p, &[not_matches("route", pattern)]);
            assert_ne!(
                positive, negative,
                "!~ is the exact negation of =~ for {pattern:?}"
            );
        }
    }

    #[test]
    fn an_invalid_pattern_fails_filter_build_while_a_never_matching_one_does_not() {
        // An invalid pattern (unclosed group) is a filter-build error
        // (the single origin of the invalid-regex 400) that NEVER echoes
        // the offending pattern. A valid-but-never-matching pattern
        // builds cleanly and is the calm 200 empty arm, not an error.
        let reason = build_filter(&[matches_op("route", "/api/(")]).expect_err("invalid compile");
        assert_eq!(reason, "invalid regex matcher");
        assert!(
            !reason.contains("/api/("),
            "the reason never echoes the pattern"
        );

        let m = metric("m", "checkout", &[]);
        let p = point(1_000_000_000, 1.0, &[("route", "/health")]);
        let filter = build_filter(&[matches_op("route", "/admin/.*")])
            .expect("a valid never-matching pattern compiles cleanly");
        assert!(
            !keep_row(&m, &p, &filter),
            "valid-but-never-matching excludes the row without erroring"
        );
    }

    #[test]
    fn rows_with_the_same_label_set_fold_into_one_series_preserving_order() {
        let m = metric("m", "checkout", &[]);
        let series = to_matrix(vec![
            (m.clone(), point(1_000_000_000, 1.0, &[])),
            (m.clone(), point(2_000_000_000, 2.0, &[])),
        ]);
        assert_eq!(series.len(), 1, "same label set folds to one series");
        assert_eq!(
            series[0].values,
            vec![(1u64, "1".to_string()), (2u64, "2".to_string())],
            "ascending time order preserved"
        );
    }
}
