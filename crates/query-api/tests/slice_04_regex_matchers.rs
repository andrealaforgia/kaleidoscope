// Kaleidoscope query-api — regex label-matcher acceptance suite
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

//! Regex label matchers — pattern filtering of query_range with `=~`/`!~`.
//!
//! Maps to
//! `docs/feature/query-api-regex-matchers-v0/slices/slice-01-regex-matchers.md`.
//! Stories: US-09 (filter by a fully-anchored pattern), US-10 (the
//! absent-label / empty-pattern regex matrix), US-11 (reject an invalid
//! regex honestly, with DD6 redaction). All in `discuss/user-stories.md`.
//! Semantics are authoritative in ADR-0046.
//!
//! The user-centric outcome: an on-call operator (Priya, tenant
//! "acme-prod") narrows a noisy metric to a FAMILY of series with one
//! pattern (`{route=~"/api/.*"}`) instead of naming each exact value, and
//! trusts the result because the absent-label arms behave exactly like
//! Prometheus and an invalid pattern is refused honestly rather than
//! silently mis-answered.
//!
//! Every scenario drives query-api through the single public driving port
//! `query_api::router(store, tenant, static_dir)` via `oneshot`, passing
//! `None` for `static_dir` (API-only), exactly as the slice-03 suite does.
//! The metric name still selects the metric via Pulse's `query`; the regex
//! matchers FILTER the translated result by each row's DERIVED label set
//! (`resource_attributes UNION point.attributes UNION {__name__: name}`),
//! point attributes winning over resource attributes, `__name__`
//! authoritative.
//!
//! RED state (BEHAVIOURAL, not compile-level): the crate compiles today;
//! these tests fail on ASSERTIONS. Today `selector::parse` rejects any
//! `=~`/`!~` operator with a 400 `regex_reason()` ("regex matchers (=~, !~)
//! are not supported at v0"). Every scenario here asserts the CORRECT
//! POST-DELIVER behaviour — a 200 filtered matrix, or the NEW
//! "invalid regex matcher" 400 (ADR-0046 Decision 3) — NOT today's
//! "not supported" 400. So each `=~`/`!~` scenario fails today because the
//! current operator arm returns the wrong status / wrong body, and goes
//! green once the DELIVER turns `=~`/`!~` into real anchored matchers.
//!
//! Full anchoring (ADR-0046 Decision 2): `label=~"re"` keeps a row iff the
//! label value FULLY matches `^(?:re)$`, so `service.name=~"check"` does
//! NOT match "checkout"; `check.*` does. `!~` is the exact negation.
//! Absent label is treated as the empty string (Decision 4), giving the
//! five-arm matrix in US-10. An invalid pattern is a 400; a
//! valid-but-never-matching pattern is the calm 200 empty arm.
//!
//! One-at-a-time outer loop: the walking skeleton is enabled; every
//! following scenario is `#[ignore]`d and gets enabled one at a time as the
//! crafter drives each inward.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, gauge, open_durable_store, point, prism_accepts_error, prism_accepts_success,
    query_range_request, query_range_request_with_auth, result_series, secs_to_nanos, tenant,
};
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricStore};

// ---------------------------------------------------------------------
// Local seed helpers. The shared `gauge` helper fixes only `service.name`
// as a resource attribute and lets points carry arbitrary point
// attributes; that is enough to give every series a distinguishable
// identity here:
//
//   * US-09 distinguishes series by the `route` / `code` POINT attribute
//     under one `service.name`, so the result Vec groups by `route`.
//   * US-10 needs series A carrying an `env` label and series B carrying
//     NONE. Modelling `env` as a RESOURCE attribute makes the two series
//     genuinely distinct identities and lets us group by `service.name`:
//     series A is "checkout" with `env="prod"`, series B is "cart" with no
//     `env`. `gauge_with_env` supplies the env-carrying series; the bare
//     `gauge` supplies the env-absent series. The `env` label is genuinely
//     absent from B's derived label set, so the absent-as-empty arms have
//     something to bite on.
// ---------------------------------------------------------------------

/// A gauge carrying BOTH `service.name` and an `env` resource attribute,
/// so the US-10 absent-label arms have a series where `env` is present and
/// non-empty. The bare `gauge` (env omitted) supplies the absent-`env`
/// series.
fn gauge_with_env(name: &str, service: &str, env: &str, points: Vec<pulse::MetricPoint>) -> Metric {
    let mut resource = std::collections::BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    resource.insert("env".to_string(), env.to_string());
    Metric {
        name: MetricName::new(name),
        description: "acceptance gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

/// Collect the `service.name` value of each returned series, for terse
/// assertions about which services survived a filter.
fn service_names(body: &serde_json::Value) -> Vec<String> {
    result_series(body)
        .iter()
        .filter_map(|s| s["metric"]["service.name"].as_str().map(str::to_string))
        .collect()
}

/// Collect the `route` POINT-attribute value of each returned series. US-09
/// distinguishes series by `route`, so grouping the flat result Vec by this
/// label set and asserting the SET of surviving routes is order-insensitive
/// across series.
fn routes(body: &serde_json::Value) -> Vec<String> {
    result_series(body)
        .iter()
        .filter_map(|s| s["metric"]["route"].as_str().map(str::to_string))
        .collect()
}

// =====================================================================
// US-09 — Walking skeleton: operator narrows a noisy metric by a pattern
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-09
///
/// Given tenant "acme-prod" holds "http_requests_total" with four route
/// series ("/api/orders", "/api/payments", "/health", "/metrics") seeded
/// in a REAL durable Pulse store,
/// When the operator queries that metric narrowed by
/// `{route=~"/api/.*"}` over a covering range,
/// Then she sees exactly the two "/api/*" series, and Prism's success
/// validator accepts the response.
///
/// This is the demo-able outcome of the slice: a noisy metric narrowed to a
/// FAMILY of series by one pattern, filtered server-side. It seeds REAL
/// durable storage (the same `FileBackedMetricStore` adapter the gateway
/// writes through) so the skeleton proves wiring, the Pulse name-select,
/// the new anchored-regex filter, and the matrix shape end to end.
///
/// RED today: `=~` is parsed as the unsupported-regex 400, so the status is
/// 400 not 200 and there is no matrix to inspect. Goes green when `=~`
/// becomes a real anchored matcher.
#[tokio::test]
async fn operator_narrows_a_noisy_metric_by_a_route_pattern() {
    let (store, _base) = open_durable_store("regex-walking");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(
                        secs_to_nanos(1_716_200_000),
                        12.0,
                        &[("route", "/api/orders")],
                    ),
                    point(
                        secs_to_nanos(1_716_200_000),
                        3.0,
                        &[("route", "/api/payments")],
                    ),
                    point(secs_to_nanos(1_716_200_000), 7.0, &[("route", "/health")]),
                    point(secs_to_nanos(1_716_200_000), 9.0, &[("route", "/metrics")]),
                ],
            )]),
        )
        .expect("seed durable store");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{route=~\"/api/.*\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        prism_accepts_success(&body),
        "Prism's isPromSuccess must accept the filtered response: {body}"
    );
    assert_eq!(body["data"]["resultType"], "matrix");
    let mut kept = routes(&body);
    kept.sort();
    assert_eq!(
        kept,
        vec!["/api/orders".to_string(), "/api/payments".to_string()],
        "only the /api/* route family survives the pattern"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("/health") && !rendered.contains("/metrics"),
        "non-/api routes are filtered out: {rendered}"
    );
}

// =====================================================================
// US-09 — Full anchoring: a substring does not match, `.*` does (edge)
// =====================================================================

/// @driving_port @US-09
///
/// Given tenant "acme-prod" has "http_requests_total" with service.name
/// "checkout" and "checkout-canary",
/// When the operator queries `{service.name=~"check"}`,
/// Then NO series is returned, because neither value FULLY matches the
/// anchored pattern "check" (Prometheus anchors both ends); and when she
/// instead queries `{service.name=~"check.*"}`, BOTH series return.
///
/// This pins the full-anchor rule: a mutant that drops the `^...$` wrapping
/// (matching "check" as a substring of "checkout") would wrongly keep both
/// series in the first arm and is killed here.
#[tokio::test]
async fn full_anchoring_excludes_a_substring_only_match() {
    let (store, _base) = open_durable_store("regex-full-anchor");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![
                gauge(
                    "http_requests_total",
                    "checkout",
                    vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "checkout-canary",
                    vec![point(secs_to_nanos(1_716_200_000), 4.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    // Substring pattern: anchored, so it fully matches NEITHER value.
    let router = query_api::router(
        store.clone() as Arc<dyn MetricStore + Send + Sync>,
        Some(t.clone()),
        None,
    );
    let request = query_range_request(
        "http_requests_total{service.name=~\"check\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert!(
        result_series(&body).is_empty(),
        "\"check\" fully matches neither \"checkout\" nor \"checkout-canary\": {body}"
    );

    // `check.*` fully matches both.
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name=~\"check.*\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let mut names = service_names(&body);
    names.sort();
    assert_eq!(
        names,
        vec!["checkout".to_string(), "checkout-canary".to_string()],
        "check.* fully matches both services"
    );
}

// =====================================================================
// US-09 — A regex matcher composes with an exact matcher under AND (edge)
// =====================================================================

/// @driving_port @US-09
///
/// Given the checkout service carries an "/api/orders" series with code
/// "200" and one with code "500", plus a non-/api "/health" series,
/// When the operator queries `{route=~"/api/.*", code="200"}`,
/// Then only the "/api/orders" series carrying code "200" survives (both
/// the regex and the exact matcher must hold).
#[tokio::test]
async fn a_regex_matcher_composes_with_an_equality_matcher_under_and() {
    let (store, _base) = open_durable_store("regex-and-exact");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(
                        secs_to_nanos(1_716_200_000),
                        1.0,
                        &[("route", "/api/orders"), ("code", "200")],
                    ),
                    point(
                        secs_to_nanos(1_716_200_000),
                        9.0,
                        &[("route", "/api/orders"), ("code", "500")],
                    ),
                    point(
                        secs_to_nanos(1_716_200_000),
                        5.0,
                        &[("route", "/health"), ("code", "200")],
                    ),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{route=~\"/api/.*\", code=\"200\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the /api/orders code 200 series");
    assert_eq!(series[0]["metric"]["route"], "/api/orders");
    assert_eq!(series[0]["metric"]["code"], "200");
    assert!(
        !body.to_string().contains("500") && !body.to_string().contains("/health"),
        "the code 500 series and the non-/api route are excluded"
    );
}

// =====================================================================
// US-09 — A valid pattern that matches nothing is the calm 200 empty arm
// =====================================================================

/// @driving_port @US-09
///
/// Given tenant "acme-prod" has "http_requests_total" with only "/api/*"
/// and "/health" routes, none under "/admin/",
/// When the operator queries `{route=~"/admin/.*"}` (a VALID pattern that
/// happens to match nothing),
/// Then she sees the calm empty arm (200, `result:[]`), NOT an error;
/// Prism's success validator accepts it. This is the
/// valid-but-never-matching boundary that scenario 10 contrasts against the
/// invalid-syntax 400.
#[tokio::test]
async fn a_valid_pattern_matching_nothing_is_the_calm_empty_arm() {
    let (store, _base) = open_durable_store("regex-never-matching");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(
                        secs_to_nanos(1_716_200_000),
                        12.0,
                        &[("route", "/api/orders")],
                    ),
                    point(secs_to_nanos(1_716_200_000), 7.0, &[("route", "/health")]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{route=~\"/admin/.*\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a valid pattern that matches nothing is a calm 200, not an error: {body}"
    );
    assert!(
        prism_accepts_success(&body),
        "an all-excluded result is a calm empty success, not an error: {body}"
    );
    assert_eq!(body["data"]["resultType"], "matrix");
    assert!(
        result_series(&body).is_empty(),
        "no route fully matches /admin/.*, so the result is empty"
    );
}

// =====================================================================
// US-10 — `env=~""` keeps the absent-env series (absent-as-empty)
// =====================================================================

/// @driving_port @US-10
///
/// Given series A carries `env="prod"` (service.name "checkout") and series
/// B carries NO `env` label (service.name "cart"),
/// When the operator queries `{env=~""}`,
/// Then only series B is kept: an absent label is treated as the empty
/// string and fully matches the empty pattern; the present-non-empty env
/// series is excluded.
#[tokio::test]
async fn regex_empty_pattern_keeps_the_absent_label_series() {
    let (store, _base) = open_durable_store("regex-eq-empty");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![
                gauge_with_env(
                    "http_requests_total",
                    "checkout",
                    "prod",
                    vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("http_requests_total{env=~\"\"}", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert_eq!(
        service_names(&body),
        vec!["cart".to_string()],
        "only the absent-env series is kept; the env=prod series is excluded"
    );
}

// =====================================================================
// US-10 — `env=~".+"` keeps only present non-empty env
// =====================================================================

/// @driving_port @US-10
///
/// Given series A carries `env="prod"` and series B carries NO `env` label,
/// When the operator queries `{env=~".+"}`,
/// Then only series A is kept: a present non-empty value fully matches
/// `.+`; the absent-env series (the empty string) does not.
#[tokio::test]
async fn regex_non_empty_required_pattern_keeps_only_present_non_empty() {
    let (store, _base) = open_durable_store("regex-eq-plus");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![
                gauge_with_env(
                    "http_requests_total",
                    "checkout",
                    "prod",
                    vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{env=~\".+\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert_eq!(
        service_names(&body),
        vec!["checkout".to_string()],
        "only the present-non-empty env series is kept; the absent-env series is excluded"
    );
}

// =====================================================================
// US-10 — `env!~""` keeps only present non-empty env (negation of =~"")
// =====================================================================

/// @driving_port @US-10
///
/// Given series A carries `env="prod"` and series B carries NO `env` label,
/// When the operator queries `{env!~""}`,
/// Then only series A is kept: `!~""` is the exact negation of `=~""`, so it
/// keeps present non-empty and excludes the absent-as-empty series.
#[tokio::test]
async fn negated_empty_pattern_keeps_only_present_non_empty() {
    let (store, _base) = open_durable_store("regex-neq-empty");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![
                gauge_with_env(
                    "http_requests_total",
                    "checkout",
                    "prod",
                    vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("http_requests_total{env!~\"\"}", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert_eq!(
        service_names(&body),
        vec!["checkout".to_string()],
        "!~\"\" keeps only the present-non-empty env series"
    );
}

// =====================================================================
// US-10 — `env!~".+"` keeps the absent-or-empty series (negation of =~".+")
// =====================================================================

/// @driving_port @US-10
///
/// Given series A carries `env="prod"` and series B carries NO `env` label,
/// When the operator queries `{env!~".+"}`,
/// Then only series B is kept: `!~".+"` is the exact negation of `=~".+"`,
/// so it keeps the absent-or-empty series and excludes present non-empty.
#[tokio::test]
async fn negated_non_empty_required_pattern_keeps_the_absent_series() {
    let (store, _base) = open_durable_store("regex-neq-plus");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![
                gauge_with_env(
                    "http_requests_total",
                    "checkout",
                    "prod",
                    vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{env!~\".+\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert_eq!(
        service_names(&body),
        vec!["cart".to_string()],
        "!~\".+\" keeps only the absent-or-empty env series"
    );
}

// =====================================================================
// US-10 — `env!~"prod"` keeps the absent-env series (absent satisfies !~)
// =====================================================================

/// @driving_port @US-10
///
/// Given series A carries `env="prod"` and series B carries NO `env` label,
/// When the operator queries `{env!~"prod"}`,
/// Then only series B is kept: the absent label, treated as the empty
/// string, does NOT fully match "prod", so `!~"prod"` keeps it; series A
/// (present and equal to "prod") is excluded.
#[tokio::test]
async fn negated_pattern_keeps_a_series_where_the_label_is_absent() {
    let (store, _base) = open_durable_store("regex-neq-prod");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![
                gauge_with_env(
                    "http_requests_total",
                    "checkout",
                    "prod",
                    vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{env!~\"prod\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert_eq!(
        service_names(&body),
        vec!["cart".to_string()],
        "the absent-env series does not match \"prod\", so !~ keeps it; env=prod is excluded"
    );
}

// =====================================================================
// US-11 — An invalid regex (unclosed group) is a 400 status:error (error)
// =====================================================================

/// @driving_port @US-11
///
/// Given the operator submits `{route=~"/api/("}` whose regex has an
/// unclosed group,
/// When the service compiles the regex matcher,
/// Then it returns HTTP 400 `{status:"error", error:<names the regex
/// matcher invalid>}`, which Prism's error validator accepts.
///
/// RED today the status is ALSO 400, but for the WRONG reason: the current
/// arm reports regex as unsupported. This asserts the post-DELIVER body
/// that names the regex as INVALID (ADR-0046 Decision 3), so it fails today
/// on the message assertion and goes green only once compilation is
/// attempted and fails.
#[tokio::test]
async fn an_invalid_regex_with_an_unclosed_group_is_rejected() {
    let (store, _base) = open_durable_store("regex-invalid-group");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{route=~\"/api/(\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        prism_accepts_error(&body),
        "Prism's isPromError must accept the rejection: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        message.contains("invalid") && message.contains("regex"),
        "the error names the regex matcher as invalid: {message}"
    );
}

// =====================================================================
// US-11 — An invalid negative regex (dangling quantifier) is a 400 (error)
// =====================================================================

/// @driving_port @US-11
///
/// Given the operator submits `{service.name!~"*abc"}` whose regex has a
/// dangling quantifier (a `*` with nothing to repeat),
/// When the service compiles the regex matcher,
/// Then it returns HTTP 400 status:error naming the regex as invalid. The
/// `!~` operator parses fine; the pattern is what fails.
#[tokio::test]
async fn an_invalid_negative_regex_with_a_dangling_quantifier_is_rejected() {
    let (store, _base) = open_durable_store("regex-invalid-quantifier");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name!~\"*abc\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        message.contains("invalid") && message.contains("regex"),
        "the error names the regex matcher as invalid: {message}"
    );
}

// =====================================================================
// US-11 — Invalid syntax (400) versus never-matching valid (200 empty)
// =====================================================================

/// @driving_port @US-11
///
/// Given tenant "acme-prod" has "http_requests_total" with no /admin route,
/// When the operator submits the VALID-but-never-matching
/// `{route=~"/admin/.*"}` and, separately, the INVALID `{route=~"/api/("}`,
/// Then the valid-but-never-matching query is a calm 200 empty (reaffirming
/// scenario 4) and the invalid-syntax query is a 400 status:error. The
/// distinction is sharp: a mutant that degrades the 400 to a 200 (or the
/// reverse) is killed by asserting BOTH outcomes side by side.
#[tokio::test]
async fn invalid_syntax_is_a_400_while_never_matching_is_a_200_empty() {
    let (store, _base) = open_durable_store("regex-invalid-vs-empty");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(
                    secs_to_nanos(1_716_200_000),
                    12.0,
                    &[("route", "/health")],
                )],
            )]),
        )
        .expect("seed");

    // Valid but never-matching: calm 200 empty.
    let router = query_api::router(
        store.clone() as Arc<dyn MetricStore + Send + Sync>,
        Some(t.clone()),
        None,
    );
    let request = query_range_request(
        "http_requests_total{route=~\"/admin/.*\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a valid never-matching pattern is a calm 200 empty, not a 400: {body}"
    );
    assert!(prism_accepts_success(&body));
    assert!(result_series(&body).is_empty());

    // Invalid syntax: 400 status:error.
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{route=~\"/api/(\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an invalid pattern is a 400, never degraded to a calm empty 200: {body}"
    );
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-11 — An invalid-regex rejection never leaks a forwarded header,
// the pattern, or the raw query (security / DD6 redaction)
// =====================================================================

/// @driving_port @US-11
///
/// Given the operator's request carries a forwarded Authorization header
/// "Bearer SECRET" and an INVALID regex `{route=~"/api/("}`,
/// When the service returns a status:error for the invalid regex matcher,
/// Then the error text contains neither the secret, nor the offending
/// pattern, nor the raw query (DD6 redaction symmetry; ADR-0046 Decision 3).
#[tokio::test]
async fn an_invalid_regex_rejection_never_leaks_a_header_pattern_or_query() {
    let (store, _base) = open_durable_store("regex-redaction");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request_with_auth(
        "http_requests_total{route=~\"/api/(\"}",
        "1716200000",
        "1716200060",
        "Bearer SECRET",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        !message.contains("SECRET"),
        "the error text must not echo the forwarded secret: {message}"
    );
    assert!(
        !message.contains("/api/("),
        "the error text must not echo the offending pattern: {message}"
    );
    assert!(
        !message.contains("http_requests_total"),
        "the error text must not echo the raw query: {message}"
    );
}
