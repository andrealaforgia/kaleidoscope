// Kaleidoscope query-api — label-matcher acceptance suite
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

//! Label matchers — equality and inequality filtering of query_range.
//!
//! Maps to
//! `docs/feature/query-api-label-matchers-v0/slices/slice-01-equality-and-inequality-matchers.md`.
//! Stories: US-06 (equality), US-07 (inequality), US-08 (reject
//! regex/malformed). All in `discuss/user-stories.md`.
//!
//! The user-centric outcome: an on-call operator (Sara, tenant
//! "acme-prod") querying a noisy metric mid-incident narrows it to the
//! one service she cares about with `{service.name="checkout"}`, or
//! excludes a noisy series with `{service.name!="batch"}`, and sees only
//! the matching series filtered server-side out of Pulse. A regex or
//! malformed matcher is rejected honestly with a 400, never silently
//! mis-answered.
//!
//! Every scenario drives query-api through the single public driving
//! port `query_api::router(store, tenant, static_dir)` via `oneshot`,
//! passing `None` for `static_dir` (API-only), exactly as the slice 01
//! suite does. The metric name still selects the metric via Pulse's
//! `query`; the OTHER matchers filter the translated result by each
//! row's DERIVED label set
//! (`resource_attributes UNION point.attributes UNION {__name__: name}`).
//!
//! RED state: today `selector::parse` rejects ANY `{` with a 400 (see the
//! slice 01 arm `a_label_matcher_is_rejected_at_slice_01`). Every
//! scenario here that expects a 200 filtered result therefore FAILS until
//! the DELIVER slice extends the parser and adds the filter. The behaviour
//! is driven entirely through the existing HTTP handler; no new public API
//! signature is required (the parser and filter are internal to the
//! `selector` and `matrix` modules).
//!
//! One-at-a-time outer loop: the walking skeleton is enabled; every
//! following scenario is `#[ignore]`d and gets enabled one at a time as
//! the crafter drives each inward.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, gauge, open_durable_store, point, prism_accepts_error, prism_accepts_success,
    query_range_request, result_series, secs_to_nanos, tenant,
};
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricStore};

// ---------------------------------------------------------------------
// Local seed helper. The shared `gauge` helper fixes only
// `service.name`; this feature also needs series that DIFFER by a point
// attribute (e.g. `route` or `code`) and series where a label is ABSENT,
// so the absent-label arms have something to bite on. Both resource
// attributes and point attributes surface in the derived label set, so
// either axis gives a distinguishable series. `bare_gauge` supplies a
// series with no resource attributes at all.
// ---------------------------------------------------------------------

/// A metric with NO resource attributes at all (not even `service.name`),
/// so the absent-label arms have a series where the matched label is
/// genuinely absent from the derived set.
fn bare_gauge(name: &str, points: Vec<pulse::MetricPoint>) -> Metric {
    Metric {
        name: MetricName::new(name),
        description: "acceptance gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: std::collections::BTreeMap::new(),
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

// =====================================================================
// US-06 — Walking skeleton: operator narrows a noisy metric to one service
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-06
///
/// Given tenant "acme-prod" holds "http_requests_total" with three
/// services (checkout, cart, search) seeded in a REAL durable Pulse
/// store,
/// When the operator queries that metric narrowed by
/// `{service.name="checkout"}` over a covering range,
/// Then she sees exactly one matrix series, the checkout one, and Prism's
/// success validator accepts the response.
///
/// This is the demo-able outcome of the slice: a noisy metric narrowed to
/// the one service that matters, filtered server-side. It seeds REAL
/// durable storage (the same `FileBackedMetricStore` adapter the gateway
/// writes through) so the skeleton proves wiring, the Pulse name-select,
/// the new label filter, and the matrix shape end to end.
#[tokio::test]
async fn operator_narrows_a_noisy_metric_to_one_service() {
    let (store, _base) = open_durable_store("matchers-walking");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![
                gauge(
                    "http_requests_total",
                    "checkout",
                    vec![
                        point(secs_to_nanos(1_716_200_000), 12.0, &[]),
                        point(secs_to_nanos(1_716_200_015), 18.0, &[]),
                    ],
                ),
                gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "search",
                    vec![point(secs_to_nanos(1_716_200_000), 7.0, &[])],
                ),
            ]),
        )
        .expect("seed durable store");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name=\"checkout\"}",
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
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the checkout series survives");
    assert_eq!(series[0]["metric"]["service.name"], "checkout");
    assert_eq!(series[0]["metric"]["__name__"], "http_requests_total");
    // Cart and search must be entirely absent.
    let rendered = body.to_string();
    assert!(
        !rendered.contains("cart") && !rendered.contains("search"),
        "non-matching services are filtered out: {rendered}"
    );
}

// =====================================================================
// US-06 — Two ANDed equality matchers narrow further (edge case)
// =====================================================================

/// @driving_port @US-06
///
/// Given the checkout service has two series under one metric, one
/// carrying point label code "200" and one carrying code "500",
/// When the operator queries
/// `{service.name="checkout", code="200"}`,
/// Then only the checkout series carrying code "200" is returned (both
/// matchers must hold).
#[tokio::test]
async fn two_anded_equality_matchers_narrow_further() {
    let (store, _base) = open_durable_store("matchers-anded");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 1.0, &[("code", "200")]),
                    point(secs_to_nanos(1_716_200_000), 9.0, &[("code", "500")]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name=\"checkout\", code=\"200\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the code 200 checkout series");
    assert_eq!(series[0]["metric"]["code"], "200");
    assert!(
        !body.to_string().contains("500"),
        "the code 500 series is excluded"
    );
}

// =====================================================================
// US-06 — Equality empty-string matches an absent label (boundary)
// =====================================================================

/// @driving_port @US-06
///
/// Given one series carries point label code "200" and another series has
/// no code label at all,
/// When the operator queries `{code=""}`,
/// Then only the series with no code label is kept; the Prometheus
/// empty-string rule treats an absent label as matching `=""`.
#[tokio::test]
async fn equality_empty_string_matches_an_absent_label() {
    let (store, _base) = open_durable_store("matchers-eq-empty");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 1.0, &[("code", "200")]),
                    point(secs_to_nanos(1_716_200_015), 2.0, &[]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("http_requests_total{code=\"\"}", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the series with no code label");
    assert!(
        series[0]["metric"].get("code").is_none(),
        "the kept series has no code label: {}",
        series[0]
    );
}

// =====================================================================
// US-07 — Inequality excludes the named series (happy path)
// =====================================================================

/// @driving_port @US-07
///
/// Given tenant "acme-prod" holds "http_requests_total" for checkout,
/// cart, and batch,
/// When the operator queries `{service.name!="batch"}`,
/// Then she sees the checkout and cart series and the batch series is
/// excluded.
#[tokio::test]
async fn inequality_excludes_the_named_series() {
    let (store, _base) = open_durable_store("matchers-neq");
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
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
                gauge(
                    "http_requests_total",
                    "batch",
                    vec![point(secs_to_nanos(1_716_200_000), 9999.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name!=\"batch\"}",
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
        vec!["cart".to_string(), "checkout".to_string()],
        "checkout and cart remain, batch is excluded"
    );
    assert!(
        !body.to_string().contains("batch"),
        "the batch series is excluded"
    );
}

// =====================================================================
// US-07 — Inequality keeps a series where the label is absent (boundary)
// =====================================================================

/// @driving_port @US-07
///
/// Given one series carries point label code "500" and another has no
/// code label at all,
/// When the operator queries `{code!="500"}`,
/// Then the series with no code label is KEPT (an absent label satisfies
/// `!=`) and the code "500" series is excluded.
#[tokio::test]
async fn inequality_keeps_a_series_where_the_label_is_absent() {
    let (store, _base) = open_durable_store("matchers-neq-absent");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 9.0, &[("code", "500")]),
                    point(secs_to_nanos(1_716_200_015), 2.0, &[]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{code!=\"500\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the absent-code series survives");
    assert!(
        series[0]["metric"].get("code").is_none(),
        "the kept series has no code label (absent satisfies !=): {}",
        series[0]
    );
}

// =====================================================================
// US-07 — `!=""` keeps only present, non-empty labels (boundary)
// =====================================================================

/// @driving_port @US-07
///
/// Given one series carries point label code "200" and another has no
/// code label at all,
/// When the operator queries `{code!=""}`,
/// Then only the series carrying a present, non-empty code is kept; the
/// absent-code series is excluded (the mirror of the `=""` rule).
#[tokio::test]
async fn inequality_against_empty_string_keeps_only_present_non_empty_labels() {
    let (store, _base) = open_durable_store("matchers-neq-empty");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 1.0, &[("code", "200")]),
                    point(secs_to_nanos(1_716_200_015), 2.0, &[]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{code!=\"\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the present-non-empty-code series");
    assert_eq!(
        series[0]["metric"]["code"], "200",
        "the kept series carries code 200"
    );
}

// =====================================================================
// US-07 — Equality and inequality compose under AND (edge case)
// =====================================================================

/// @driving_port @US-07
///
/// Given the checkout service has a code "200" and a code "500" series,
/// When the operator queries
/// `{service.name="checkout", code!="500"}`,
/// Then only the checkout series carrying code "200" survives (both
/// matchers AND together).
#[tokio::test]
async fn equality_and_inequality_compose_under_and() {
    let (store, _base) = open_durable_store("matchers-mixed-and");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 1.0, &[("code", "200")]),
                    point(secs_to_nanos(1_716_200_000), 9.0, &[("code", "500")]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name=\"checkout\", code!=\"500\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the checkout code 200 series");
    assert_eq!(series[0]["metric"]["code"], "200");
    assert!(
        !body.to_string().contains("500"),
        "the code 500 series is excluded"
    );
}

// =====================================================================
// US-06 — A matcher that matches a label ABSENT from every series:
// equality against a non-existent label name (boundary, calm empty)
// =====================================================================

/// @driving_port @US-06
///
/// Given no series carries the label "nonexistent",
/// When the operator queries `{nonexistent="x"}` (a non-empty value),
/// Then no series matches and she sees the calm empty arm (200,
/// `result:[]`), not an error.
#[tokio::test]
async fn equality_against_a_label_no_series_has_returns_calm_empty() {
    let (store, _base) = open_durable_store("matchers-absent-eq-empty");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{nonexistent=\"x\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        prism_accepts_success(&body),
        "an all-excluded result is a calm empty success, not an error: {body}"
    );
    assert_eq!(body["data"]["resultType"], "matrix");
    assert!(
        result_series(&body).is_empty(),
        "no series equals the absent label, so the result is empty"
    );
}

// =====================================================================
// US-06 — Equality empty-string against an absent label keeps every
// series (boundary)
// =====================================================================

/// @driving_port @US-06
///
/// Given several series, NONE of which carries the label "nonexistent",
/// When the operator queries `{nonexistent=""}`,
/// Then every series is kept, because an absent label is treated as the
/// empty string and so satisfies `=""`.
#[tokio::test]
async fn equality_empty_string_against_an_absent_label_keeps_all_series() {
    let (store, _base) = open_durable_store("matchers-absent-eq-keep-all");
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
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{nonexistent=\"\"}",
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
        vec!["cart".to_string(), "checkout".to_string()],
        "an absent label satisfies =\"\", so every series is kept"
    );
}

// =====================================================================
// US-07 — `service.name!=""` keeps only series that HAVE a non-empty
// service.name (boundary)
// =====================================================================

/// @driving_port @US-07
///
/// Given one series carries `service.name="checkout"` and another series
/// has no resource attributes at all (no service.name),
/// When the operator queries `{service.name!=""}`,
/// Then only the series with a present, non-empty service.name is kept.
#[tokio::test]
async fn inequality_empty_string_on_service_name_keeps_only_present() {
    let (store, _base) = open_durable_store("matchers-svc-neq-empty");
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
                bare_gauge(
                    "http_requests_total",
                    vec![point(
                        secs_to_nanos(1_716_200_000),
                        5.0,
                        &[("route", "/health")],
                    )],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name!=\"\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the series with a service.name");
    assert_eq!(series[0]["metric"]["service.name"], "checkout");
}

// =====================================================================
// US-06 — A point attribute is matchable just like a resource attribute
// (edge case: both surface in the derived label set)
// =====================================================================

/// @driving_port @US-06
///
/// Given two series under one metric distinguished only by the POINT
/// attribute `route` (/a and /b),
/// When the operator queries `{route="/a"}`,
/// Then only the /a series is returned, proving point attributes surface
/// in the derived label set and are matchable.
#[tokio::test]
async fn a_point_attribute_is_matchable_like_a_resource_attribute() {
    let (store, _base) = open_durable_store("matchers-point-attr");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 1.0, &[("route", "/a")]),
                    point(secs_to_nanos(1_716_200_000), 2.0, &[("route", "/b")]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{route=\"/a\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only the /a route series");
    assert_eq!(series[0]["metric"]["route"], "/a");
}

// =====================================================================
// US-06 — __name__ is matchable (edge case)
// =====================================================================

/// @driving_port @US-06
///
/// Given a metric whose name equals the queried name,
/// When the operator queries `{__name__="http_requests_total"}` (the
/// name is also the bare selector),
/// Then the result behaves like the bare-name query: every series under
/// the name is returned.
#[tokio::test]
async fn name_label_is_matchable() {
    let (store, _base) = open_durable_store("matchers-name-label");
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
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{__name__=\"http_requests_total\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert_eq!(
        result_series(&body).len(),
        2,
        "__name__ matches the metric name, so all series are kept"
    );
}

// =====================================================================
// US-06 — A bare metric name with no braces still works (regression)
// =====================================================================

/// @driving_port @US-06
///
/// Given tenant "acme-prod" has a metric with several series,
/// When the operator queries the bare name with NO brace section,
/// Then all series under the name are returned: the slice 01 behaviour is
/// unchanged by the matcher extension.
#[tokio::test]
async fn a_bare_metric_name_with_no_braces_still_works() {
    let (store, _base) = open_durable_store("matchers-bare-regression");
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
                    "cart",
                    vec![point(secs_to_nanos(1_716_200_000), 3.0, &[])],
                ),
            ]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("http_requests_total", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert_eq!(
        result_series(&body).len(),
        2,
        "the bare name returns every series, unchanged from slice 01"
    );
}

// NOTE: two US-08 scenarios that lived here asserted that `=~` and `!~`
// were rejected as "regex matchers not yet supported" (a 400). ADR-0046
// supersedes that contract: `=~`/`!~` are now real, fully-anchored regex
// matchers. The superseding behaviour (a 200 filtered matrix for a valid
// pattern, and the NEW "invalid regex matcher" 400 for an uncompilable
// one) is covered end to end by the slice_04 regex suite. The remaining
// US-08 reject scenarios below (unterminated brace, unquoted value, empty
// label name) are unaffected by ADR-0046 and stay green.

// =====================================================================
// US-08 — An unterminated brace is rejected, not treated as a bare name
// (error path)
// =====================================================================

/// @driving_port @US-08
///
/// Given the operator submits a matcher section with no closing brace,
/// When the service parses the selector,
/// Then it returns a 400 status:error and does NOT silently fall back to
/// querying the bare metric name.
#[tokio::test]
async fn an_unterminated_brace_is_rejected_not_treated_as_a_bare_name() {
    let (store, _base) = open_durable_store("matchers-reject-unterminated");
    let t = tenant("acme-prod");
    // Seed data under the bare name: if the parser degraded to a bare-name
    // query it would return this series, so a 200 here would be a silent
    // mis-answer. The 400 assertion guards against exactly that.
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 12.0, &[])],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name=\"checkout\"",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unterminated brace is a 400, never a degraded bare-name 200"
    );
    assert!(prism_accepts_error(&body));
    assert!(
        !prism_accepts_success(&body),
        "the malformed query must never be silently answered as a bare name"
    );
}

// =====================================================================
// US-08 — A matcher value without quotes is rejected (error path)
// =====================================================================

/// @driving_port @US-08
///
/// Given the operator submits a matcher whose value is not quoted,
/// When the service parses the selector,
/// Then it returns a 400 status:error naming the malformed matcher.
#[tokio::test]
async fn a_matcher_value_without_quotes_is_rejected() {
    let (store, _base) = open_durable_store("matchers-reject-unquoted");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{service.name=checkout}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-08 — An empty label name is rejected (boundary error path)
// =====================================================================

/// @driving_port @US-08
///
/// Given the operator submits a matcher with no label name before the
/// operator,
/// When the service parses the selector,
/// Then it returns a 400 status:error.
#[tokio::test]
async fn an_empty_label_name_is_rejected() {
    let (store, _base) = open_durable_store("matchers-reject-empty-name");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("http_requests_total{=\"x\"}", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// NOTE: the US-08 redaction-on-unsupported-regex scenario that lived here
// asserted the now-superseded "regex matchers are unsupported" 400. With
// ADR-0046 the `=~`/`!~` operators are real matchers, so that exact
// scenario no longer has a contract to assert. Its redaction guarantee is
// re-covered, under the NEW invalid-regex 400, by slice_04's
// `an_invalid_regex_rejection_never_leaks_a_header_pattern_or_query`. The
// non-regex parse-400 redaction discipline remains pinned by the inline
// `the_reason_never_echoes_the_raw_query` selector test.
