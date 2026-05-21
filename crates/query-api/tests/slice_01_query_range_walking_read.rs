// Kaleidoscope query-api — slice 01 acceptance suite
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

//! Slice 01 — query_range walking read.
//!
//! Maps to `docs/feature/query-range-api-v0/slices/slice-01-query-range-walking-read.md`.
//! Stories: US-01 (serve matrix), US-02 (calm empty), US-03 (reject
//! unparseable), US-04 (fail-closed tenancy), US-05 (scope boundary).
//!
//! The user-centric outcome: an on-call operator queries a metric by
//! name over a recent range and sees its time series, read out of the
//! durable Pulse store and shaped so Prism's own validators render it.
//! Empty and error arms are distinct and calm. Every query is scoped to
//! one fail-closed tenant.
//!
//! All scenarios drive query-api through the single public driving port
//! `query_api::router(store, tenant, static_dir)` via `oneshot` — this
//! suite passes `None` for `static_dir` (API-only, no static serving;
//! the same-origin static fallback is covered by the slice_02 suite of
//! prism-backend-wiring-v0) (see
//! `common/mod.rs`), and assert the exact JSON shape Prism's
//! `isPromSuccess` / `isPromError` accept. Until the crate exists this
//! file does not compile, which is the intended RED state for the
//! slice-01 outer loop.
//!
//! One-at-a-time outer loop: the walking skeleton is enabled; every
//! following scenario is `#[ignore]`d and gets enabled one at a time as
//! the crafter drives each inward.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, gauge, open_durable_store, point, prism_accepts_error, prism_accepts_success,
    query_range_request, query_range_request_with_auth, result_series, secs_to_nanos, tenant,
    FailingStore,
};
use pulse::{MetricBatch, MetricStore};

// =====================================================================
// US-01 — Walking skeleton: operator sees a metric plotted over a range
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has "process_cpu_utilization" with four
/// points across a minute on the "checkout" service,
/// When the operator queries that bare name over a range covering the
/// first three points,
/// Then they see one Prometheus matrix series, folded with __name__ and
/// service.name, carrying the in-range points as [seconds, "value"], and
/// Prism's success validator accepts the response.
///
/// This is the demo-able read-loop closure. It seeds a REAL durable
/// Pulse store on the filesystem (the same adapter the gateway writes
/// through) so the skeleton proves wiring, path resolution, and the
/// matrix shape end to end, not an in-memory stand-in.
#[tokio::test]
async fn operator_sees_a_metric_plotted_over_a_range() {
    let (store, _base) = open_durable_store("walking-read");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 0.40, &[]),
                    point(secs_to_nanos(1_716_200_015), 0.55, &[]),
                    point(secs_to_nanos(1_716_200_030), 0.61, &[]),
                    point(secs_to_nanos(1_716_200_045), 0.58, &[]),
                ],
            )]),
        )
        .expect("seed durable store");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("process_cpu_utilization", "1716200000", "1716200045");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        prism_accepts_success(&body),
        "Prism's isPromSuccess must accept the response: {body}"
    );
    assert_eq!(body["data"]["resultType"], "matrix", "resultType is matrix");

    let series = result_series(&body);
    assert_eq!(series.len(), 1, "one series for one label set");
    let only = &series[0];
    assert_eq!(
        only["metric"]["__name__"], "process_cpu_utilization",
        "__name__ label is the metric name"
    );
    assert_eq!(
        only["metric"]["service.name"], "checkout",
        "resource attribute folds into the label set"
    );
    // Half-open [start, end): the point at exactly end (1716200045) is
    // excluded, so three of the four points appear.
    assert_eq!(
        only["values"],
        serde_json::json!([
            [1_716_200_000u64, "0.4"],
            [1_716_200_015u64, "0.55"],
            [1_716_200_030u64, "0.61"],
        ]),
        "in-range points as [seconds_number, value_string]"
    );
}

// =====================================================================
// US-01 — Two series under one metric name (edge case)
// =====================================================================

/// @driving_port @US-01
///
/// Given a metric whose points carry distinct route labels,
/// When the operator queries it over a covering range,
/// Then the operator sees one matrix series per distinct label set.
#[tokio::test]
async fn points_split_into_one_series_per_label_set() {
    let (store, _base) = open_durable_store("two-series");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "http_server_active_requests",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 3.0, &[("route", "/cart")]),
                    point(secs_to_nanos(1_716_200_015), 7.0, &[("route", "/pay")]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("http_server_active_requests", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 2, "one series per distinct label set");
    let routes: Vec<&str> = series
        .iter()
        .filter_map(|s| s["metric"]["route"].as_str())
        .collect();
    assert!(routes.contains(&"/cart"), "the /cart series is present");
    assert!(routes.contains(&"/pay"), "the /pay series is present");
}

// =====================================================================
// US-01 — Half-open range includes start, excludes end (boundary)
// =====================================================================

/// @driving_port @US-01
///
/// Given a metric with a point at exactly the range start and another
/// at exactly the range end,
/// When the operator queries over [start, end),
/// Then the start point is included and the end point is excluded.
#[tokio::test]
async fn half_open_range_includes_start_and_excludes_end() {
    let (store, _base) = open_durable_store("half-open");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "queue_depth",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 5.0, &[]),
                    point(secs_to_nanos(1_716_200_060), 9.0, &[]),
                ],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("queue_depth", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let series = result_series(&body);
    assert_eq!(series.len(), 1);
    assert_eq!(
        series[0]["values"],
        serde_json::json!([[1_716_200_000u64, "5"]]),
        "start point included, end point excluded by the half-open range"
    );
}

// =====================================================================
// US-01 — NaN encodes as the string "NaN" (boundary)
// =====================================================================

/// @driving_port @US-01
///
/// Given a metric point whose value is NaN,
/// When the operator queries it,
/// Then the values pair carries the JSON string "NaN" (which Prism's
/// parseValue maps back to Number.NaN).
#[tokio::test]
async fn nan_value_encodes_as_the_string_nan() {
    let (store, _base) = open_durable_store("nan");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "gc_pause_seconds",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), f64::NAN, &[])],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("gc_pause_seconds", "1716200000", "1716200015");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let series = result_series(&body);
    assert_eq!(
        series[0]["values"][0][1], "NaN",
        "NaN is encoded as the string \"NaN\""
    );
}

// =====================================================================
// US-01 — Whole-valued float renders without a trailing .0 (boundary)
// =====================================================================

/// @driving_port @US-01
///
/// Given a metric point whose value is 0.0,
/// When the operator queries it,
/// Then the value renders as the minimal-decimal string "0".
#[tokio::test]
async fn whole_valued_float_renders_without_trailing_zero() {
    let (store, _base) = open_durable_store("minimal-decimal");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "gc_pause_seconds",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 0.0, &[])],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("gc_pause_seconds", "1716200000", "1716200015");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let series = result_series(&body);
    assert_eq!(
        series[0]["values"][0][1], "0",
        "0.0 renders as the minimal-decimal string \"0\""
    );
}

// =====================================================================
// US-02 — Calm empty: unknown metric
// =====================================================================

/// @driving_port @US-02
///
/// Given the tenant has no metric under the queried name,
/// When the operator queries it over any range,
/// Then they see a calm success with an empty result array, not an
/// error. Prism's success validator accepts an empty result.
#[tokio::test]
async fn unknown_metric_returns_a_calm_empty_result() {
    let (store, _base) = open_durable_store("empty-unknown");
    let t = tenant("acme-prod");
    // Nothing seeded under this name.
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("htp_server_requests", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        prism_accepts_success(&body),
        "empty is a calm success, not an error: {body}"
    );
    assert_eq!(body["data"]["resultType"], "matrix");
    assert!(
        result_series(&body).is_empty(),
        "unknown metric yields an empty result array"
    );
}

// =====================================================================
// US-02 — Calm empty: known metric, range before its first point
// =====================================================================

/// @driving_port @US-02
///
/// Given a known metric whose earliest point is later than the queried
/// range,
/// When the operator queries the earlier range,
/// Then they see a calm empty success.
#[tokio::test]
async fn known_metric_with_no_points_in_range_returns_empty() {
    let (store, _base) = open_durable_store("empty-out-of-range");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "disk_io_bytes",
                "checkout",
                vec![point(secs_to_nanos(1_716_300_000), 1024.0, &[])],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("disk_io_bytes", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    assert!(
        result_series(&body).is_empty(),
        "no points fall in the earlier range"
    );
}

// =====================================================================
// US-03 — Reject a function call with a readable reason (error path)
// =====================================================================

/// @driving_port @US-03
///
/// Given the operator pastes a rate() function call,
/// When the service parses the selector,
/// Then it returns a 400 status:error naming functions as unsupported,
/// which Prism's error validator accepts and shows verbatim.
#[tokio::test]
async fn a_function_call_is_rejected_with_a_readable_reason() {
    let (store, _base) = open_durable_store("reject-function");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("rate(http_requests_total[5m])", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        prism_accepts_error(&body),
        "Prism's isPromError must accept the response: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        message.contains("unsupported"),
        "the error names the query as unsupported: {message}"
    );
}

// =====================================================================
// US-03 — Reject a binary operator (error path)
// =====================================================================

/// @driving_port @US-03
///
/// Given the operator submits a binary-operator expression,
/// When the service parses the selector,
/// Then it returns a 400 status:error.
#[tokio::test]
async fn a_binary_operator_is_rejected() {
    let (store, _base) = open_durable_store("reject-operator");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("cpu_seconds_total / node_count", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-03 — Reject an empty query (boundary error path)
// =====================================================================

/// @driving_port @US-03
///
/// Given the operator submits an empty query string,
/// When the service parses the selector,
/// Then it returns a 400 status:error asking for a metric name.
#[tokio::test]
async fn an_empty_query_is_rejected() {
    let (store, _base) = open_durable_store("reject-empty");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-03 — A rejection never leaks a forwarded header value (security)
// =====================================================================

/// @driving_port @US-03
///
/// Given the operator's request carries a forwarded Authorization
/// header with a secret,
/// When the service returns a status:error for an unsupported query,
/// Then the error text never contains the secret.
#[tokio::test]
async fn a_rejection_never_leaks_a_forwarded_header_value() {
    let (store, _base) = open_durable_store("redaction");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request_with_auth(
        "rate(http_requests_total[5m])",
        "1716200000",
        "1716200060",
        "Bearer SECRET",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        !message.contains("SECRET"),
        "the error text must not echo the forwarded secret: {message}"
    );
}

// =====================================================================
// US-01 — Malformed time bounds are rejected (error path)
// =====================================================================

/// @driving_port @US-01
///
/// Given the operator supplies a non-numeric start,
/// When the service parses the time bounds,
/// Then it returns a 400 status:error naming the invalid bounds.
#[tokio::test]
async fn a_non_numeric_start_is_rejected() {
    let (store, _base) = open_durable_store("bad-start");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("process_cpu_utilization", "not-a-number", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-01 — Inverted time bounds are rejected (error path)
// =====================================================================

/// @driving_port @US-01
///
/// Given the operator supplies an end earlier than the start,
/// When the service parses the time bounds,
/// Then it returns a 400 status:error.
#[tokio::test]
async fn inverted_time_bounds_are_rejected() {
    let (store, _base) = open_durable_store("inverted-bounds");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("process_cpu_utilization", "1716200060", "1716200000");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-05 — Range-vector selector rejected (scope boundary)
// =====================================================================

/// @driving_port @US-05
///
/// Given the operator submits a range-vector selector,
/// When the service parses the selector,
/// Then it returns a 400 status:error naming range vectors as
/// unsupported at v0.
#[tokio::test]
async fn a_range_vector_selector_is_rejected() {
    let (store, _base) = open_durable_store("reject-range-vector");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("http_requests_total[5m]", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-05 — Aggregation rejected (scope boundary)
// =====================================================================

/// @driving_port @US-05
///
/// Given the operator submits an aggregation,
/// When the service parses the selector,
/// Then it returns a 400 status:error.
#[tokio::test]
async fn an_aggregation_is_rejected() {
    let (store, _base) = open_durable_store("reject-aggregation");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("sum(process_cpu_utilization)", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(prism_accepts_error(&body));
}

// =====================================================================
// US-05 — A label matcher is now ACCEPTED (scope boundary moved by
// ADR-0044, which refines ADR-0042 Decision 3)
// =====================================================================

/// @driving_port @US-05
///
/// Given the operator submits a well-formed label-matcher selector,
/// When the service parses the selector,
/// Then the matcher is accepted (a success envelope), not the slice-01
/// 400. ADR-0044 (Accepted) refines ADR-0042 Decision 3 to support `=`
/// and `!=` label matchers, so the old "any `{` is a 400" scope boundary
/// no longer holds; the filtering behaviour itself is pinned by the
/// `slice_03_label_matchers` suite.
#[tokio::test]
async fn a_label_matcher_is_accepted_after_adr_0044() {
    let (store, _base) = open_durable_store("accept-matcher");
    let t = tenant("acme-prod");
    // Nothing seeded under this name, so a valid matcher selector yields
    // a calm-empty success, never the old 400.
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request(
        "http_requests_total{job=\"checkout\"}",
        "1716200000",
        "1716200060",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        prism_accepts_success(&body),
        "a well-formed label matcher is accepted after ADR-0044: {body}"
    );
}

// =====================================================================
// US-05 — A bare name with surrounding whitespace is accepted (boundary)
// =====================================================================

/// @driving_port @US-05
///
/// Given the operator submits a bare metric name padded with
/// surrounding whitespace,
/// When the service parses the selector,
/// Then the whitespace is trimmed and the query is honoured, returning
/// that metric's series.
#[tokio::test]
async fn a_bare_name_with_surrounding_whitespace_is_accepted() {
    let (store, _base) = open_durable_store("trim-whitespace");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 0.4, &[])],
            )]),
        )
        .expect("seed");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("  process_cpu_utilization  ", "1716200000", "1716200015");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(prism_accepts_success(&body));
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "the trimmed name is queried and matches");
    assert_eq!(series[0]["metric"]["__name__"], "process_cpu_utilization");
}

// =====================================================================
// US-04 — Fail-closed: no resolvable tenant refuses the request
// =====================================================================

/// @driving_port @US-04
///
/// Given the service was built with no resolvable tenant (the binary's
/// KALEIDOSCOPE_QUERY_TENANT is unset or empty, modelled here as a
/// `None` tenant at the router seam),
/// When the operator queries any metric,
/// Then the request is refused and no metric data is returned.
#[tokio::test]
async fn a_request_with_no_resolvable_tenant_is_refused() {
    let (store, _base) = open_durable_store("fail-closed");
    // The same store holds data, but the router resolves NO tenant.
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, None, None);
    let request = query_range_request("process_cpu_utilization", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_ne!(
        status,
        StatusCode::OK,
        "a request with no resolvable tenant is refused, never served"
    );
    // The refusal is an honest status:error body, never a fabricated
    // empty success.
    assert!(
        prism_accepts_error(&body),
        "fail-closed refusal is a status:error, not a calm empty: {body}"
    );
}

// =====================================================================
// US-04 — Tenant isolation: another tenant's data never appears
// =====================================================================

/// @driving_port @real-io @adapter-integration @US-04
///
/// Given two tenants each hold an identically-named metric in the same
/// durable store,
/// When the operator queries through a router scoped to one tenant,
/// Then only that tenant's series is returned and the other tenant's
/// points never appear.
#[tokio::test]
async fn a_query_returns_only_the_resolved_tenants_data() {
    let (store, _base) = open_durable_store("tenant-isolation");
    let acme = tenant("acme-prod");
    let globex = tenant("globex-prod");
    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 0.4, &[])],
            )]),
        )
        .expect("seed acme");
    store
        .ingest(
            &globex,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "billing",
                vec![point(secs_to_nanos(1_716_200_000), 0.9, &[])],
            )]),
        )
        .expect("seed globex");

    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(acme),
        None,
    );
    let request = query_range_request("process_cpu_utilization", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "only acme-prod's series is returned");
    assert_eq!(
        series[0]["metric"]["service.name"], "checkout",
        "the returned series is acme-prod's (service.name=checkout)"
    );
    // globex-prod's value 0.9 / service billing must be entirely absent.
    let rendered = body.to_string();
    assert!(
        !rendered.contains("billing") && !rendered.contains("0.9"),
        "no globex-prod point or label leaks across the tenant boundary: {rendered}"
    );
}

// =====================================================================
// US-01 — Persistence failure surfaces as a server error (5xx)
// =====================================================================

/// @driving_port @infrastructure-failure @US-01
///
/// Given the backing store can be opened but fails to read,
/// When the operator queries a metric,
/// Then the service returns a 5xx server error with a status:error body,
/// never a fabricated empty success that would mislead the operator.
#[tokio::test]
async fn a_persistence_failure_surfaces_as_a_server_error() {
    let store: Arc<dyn MetricStore + Send + Sync> = Arc::new(FailingStore);
    let t = tenant("acme-prod");
    let router = query_api::router(store, Some(t), None);
    let request = query_range_request("process_cpu_utilization", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert!(
        status.is_server_error(),
        "a persistence read failure is a 5xx, got {status}"
    );
    assert!(
        prism_accepts_error(&body),
        "the 5xx carries a status:error body, never a fabricated empty: {body}"
    );
    // The empty-success shape must NOT be served on a read failure.
    assert!(
        !prism_accepts_success(&body),
        "a persistence failure must never masquerade as a calm empty success"
    );
}
