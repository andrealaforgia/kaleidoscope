// Kaleidoscope Beacon — slice 05 SLO MWMBR synthesis acceptance test
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

//! Slice 05 — SLO multi-window-multi-burn-rate synthesis.
//!
//! Maps to `docs/feature/beacon-v0/slices/slice-05-slo-burn-rate.md`.
//! Companion story: US-BE-05.
//!
//! ADR-0036 pins the contract: one Slo declaration produces exactly
//! four MWMBR PromQL alert rules per Google SRE workbook §14.4
//! Table 14-3 for a 30-day error budget. The synthesis is
//! deterministic — same inputs produce byte-identical Rule structs.
//!
//! KPI 5: byte-equal firing pattern across a 24-hour synthetic
//! trace versus a hand-authored reference. This test exercises the
//! generated PromQL string and the rule-shape contract; the
//! cross-validation against a live Prometheus is the slice 05b
//! container fixture (deferred).

use std::time::Duration;

use beacon::{synthesise_slo, Severity, SinkConfig, Slo};

fn payments_slo() -> Slo {
    Slo {
        service: "payments_api".to_string(),
        sli_good_events: "http_requests_total{service=\"payments_api\",code!~\"5..\"}".to_string(),
        sli_total_events: "http_requests_total{service=\"payments_api\"}".to_string(),
        target_availability: 0.999,
        error_budget_period: Duration::from_secs(30 * 24 * 3600),
        sinks: vec![SinkConfig {
            kind: "webhook".to_string(),
            url: Some("https://ops.acme/alerts".to_string()),
            ..Default::default()
        }],
        source_path: Some("slos/payments_api.toml".to_string()),
    }
}

// --------------------------------------------------------------------
// Rule cardinality + identity.
// --------------------------------------------------------------------

#[test]
fn synthesise_produces_exactly_four_rules() {
    let rules = synthesise_slo(&payments_slo());
    assert_eq!(rules.len(), 4);
}

#[test]
fn synthesised_rules_carry_canonical_naming() {
    let rules = synthesise_slo(&payments_slo());
    let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "payments_api_slo_page_1h_5m",
            "payments_api_slo_page_6h_30m",
            "payments_api_slo_ticket_1d_2h",
            "payments_api_slo_ticket_3d_6h",
        ]
    );
}

#[test]
fn page_rules_carry_critical_severity_ticket_rules_carry_warning() {
    let rules = synthesise_slo(&payments_slo());
    assert_eq!(rules[0].severity, Severity::Critical); // 1h/5m
    assert_eq!(rules[1].severity, Severity::Critical); // 6h/30m
    assert_eq!(rules[2].severity, Severity::Warning); // 1d/2h
    assert_eq!(rules[3].severity, Severity::Warning); // 3d/6h
}

#[test]
fn synthesised_rules_carry_slo_service_and_slo_window_labels() {
    let rules = synthesise_slo(&payments_slo());
    for rule in &rules {
        assert_eq!(
            rule.labels.get("slo_service").map(String::as_str),
            Some("payments_api")
        );
        assert!(rule.labels.contains_key("slo_window"));
    }
    assert_eq!(
        rules[0].labels.get("slo_window").map(String::as_str),
        Some("1h/5m")
    );
    assert_eq!(
        rules[3].labels.get("slo_window").map(String::as_str),
        Some("3d/6h")
    );
}

#[test]
fn synthesised_rules_carry_slo_source_when_provided() {
    let rules = synthesise_slo(&payments_slo());
    for rule in &rules {
        assert_eq!(
            rule.labels.get("slo_source").map(String::as_str),
            Some("slos/payments_api.toml")
        );
    }
}

#[test]
fn synthesised_rules_inherit_slo_sinks() {
    let rules = synthesise_slo(&payments_slo());
    for rule in &rules {
        assert_eq!(rule.sinks.len(), 1);
        assert_eq!(rule.sinks[0].kind, "webhook");
    }
}

// --------------------------------------------------------------------
// PromQL thresholds — workbook table fidelity.
// --------------------------------------------------------------------

#[test]
fn page_one_hour_five_minute_uses_burn_rate_threshold_14_4() {
    // For a 30-day budget with target_availability=0.999, budget is
    // 0.001. The 1h/5m threshold from the workbook is 14.4. The
    // synthesised PromQL contains the product: 0.001 * 14.4 = 0.0144.
    let rules = synthesise_slo(&payments_slo());
    assert!(
        rules[0].query.contains("0.0144"),
        "1h/5m PromQL must contain 0.0144 (budget 0.001 * threshold 14.4); got: {}",
        rules[0].query
    );
}

#[test]
fn page_six_hour_thirty_minute_uses_burn_rate_threshold_6() {
    let rules = synthesise_slo(&payments_slo());
    assert!(
        rules[1].query.contains("0.006"),
        "6h/30m PromQL must contain 0.006 (budget 0.001 * threshold 6); got: {}",
        rules[1].query
    );
}

#[test]
fn ticket_one_day_two_hour_uses_burn_rate_threshold_3() {
    let rules = synthesise_slo(&payments_slo());
    assert!(
        rules[2].query.contains("0.003"),
        "1d/2h PromQL must contain 0.003 (budget 0.001 * threshold 3); got: {}",
        rules[2].query
    );
}

#[test]
fn ticket_three_day_six_hour_uses_burn_rate_threshold_1() {
    let rules = synthesise_slo(&payments_slo());
    assert!(
        rules[3].query.contains("0.001"),
        "3d/6h PromQL must contain 0.001 (budget 0.001 * threshold 1); got: {}",
        rules[3].query
    );
}

// --------------------------------------------------------------------
// PromQL window cardinality — long + short must both gate.
// --------------------------------------------------------------------

#[test]
fn one_hour_five_minute_promql_contains_both_window_aggregations() {
    let rules = synthesise_slo(&payments_slo());
    let q = &rules[0].query;
    assert!(q.contains("[1h]"), "must aggregate over 1h: {q}");
    assert!(q.contains("[5m]"), "must aggregate over 5m: {q}");
    assert!(q.contains(" and "), "must AND the two window checks: {q}");
}

#[test]
fn three_day_six_hour_promql_contains_both_window_aggregations() {
    let rules = synthesise_slo(&payments_slo());
    let q = &rules[3].query;
    assert!(q.contains("[3d]"), "must aggregate over 3d: {q}");
    assert!(q.contains("[6h]"), "must aggregate over 6h: {q}");
    assert!(q.contains(" and "), "must AND the two window checks: {q}");
}

#[test]
fn promql_uses_canonical_error_rate_form() {
    let rules = synthesise_slo(&payments_slo());
    let q = &rules[0].query;
    // The error rate is (total - good) / total.
    assert!(q.contains("sum(rate("));
    assert!(q.contains(" - sum(rate("));
    assert!(q.contains(") / sum(rate("));
}

#[test]
fn promql_references_the_slo_good_and_total_expressions() {
    let rules = synthesise_slo(&payments_slo());
    for rule in &rules {
        assert!(
            rule.query
                .contains("http_requests_total{service=\"payments_api\"}"),
            "rule {} must reference sli_total_events expression",
            rule.name
        );
        assert!(
            rule.query
                .contains("http_requests_total{service=\"payments_api\",code!~\"5..\"}"),
            "rule {} must reference sli_good_events expression",
            rule.name
        );
    }
}

// --------------------------------------------------------------------
// Determinism — same inputs produce byte-identical outputs.
// --------------------------------------------------------------------

#[test]
fn synthesise_is_deterministic_across_runs() {
    let a = synthesise_slo(&payments_slo());
    let b = synthesise_slo(&payments_slo());
    assert_eq!(a.len(), b.len());
    for (ra, rb) in a.iter().zip(b.iter()) {
        assert_eq!(ra.name, rb.name);
        assert_eq!(ra.query, rb.query);
        assert_eq!(ra.severity, rb.severity);
        assert_eq!(ra.labels, rb.labels);
    }
}

#[test]
fn synthesise_uses_zero_for_duration_because_short_window_is_the_dwell() {
    let rules = synthesise_slo(&payments_slo());
    for rule in &rules {
        assert_eq!(
            rule.for_duration,
            Duration::from_secs(0),
            "rule {} must use for_duration=0 (short window is the dwell)",
            rule.name
        );
    }
}

#[test]
fn synthesise_uses_thirty_second_interval_for_fast_burn_detection() {
    let rules = synthesise_slo(&payments_slo());
    for rule in &rules {
        assert_eq!(rule.interval, Duration::from_secs(30));
    }
}

// --------------------------------------------------------------------
// Different SLO inputs yield byte-different outputs.
// --------------------------------------------------------------------

#[test]
fn different_target_availability_produces_different_thresholds() {
    let mut slo = payments_slo();
    slo.target_availability = 0.9999; // tighter SLO → 10x smaller budget
    let rules = synthesise_slo(&slo);
    assert!(
        rules[0].query.contains("0.00144"),
        "tighter SLO must use 0.00144 in 1h/5m (budget 0.0001 * 14.4); got: {}",
        rules[0].query
    );
}

#[test]
fn different_service_name_produces_different_rule_names() {
    let mut slo = payments_slo();
    slo.service = "checkout_api".to_string();
    let rules = synthesise_slo(&slo);
    assert_eq!(rules[0].name, "checkout_api_slo_page_1h_5m");
}

#[test]
fn synthesised_rules_carry_no_inhibits_at_v0() {
    // Slice 03's inhibition primitive is per-rule; SLO synthesis does
    // not auto-declare inhibits at v0. The operator can manually add
    // an `inhibits` field on the source SLO declaration when slice 05b
    // adds the field; today the synthesised rules are inhibit-free.
    let rules = synthesise_slo(&payments_slo());
    for rule in &rules {
        assert!(rule.inhibits.is_empty());
    }
}
