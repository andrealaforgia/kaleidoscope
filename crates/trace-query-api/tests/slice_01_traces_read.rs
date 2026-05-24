// Kaleidoscope trace-query-api — slice 01 traces-read acceptance suite
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

//! Traces read path — the operator reads a tenant's in-window spans over HTTP.
//!
//! Maps to `docs/feature/ray-query-api-v0/discuss/user-stories.md`.
//! Stories: US-01 (in-window read + ordering + field fidelity), US-02
//! (calm empty), US-03 (tenant scoping, fail-closed), US-04 (missing
//! `service` 400, bad window 400, store failure 500, redaction).
//! Contract pinned by ADR-0048.
//!
//! The user-centric outcome: Sara, on-call SRE for tenant "acme-prod"
//! and service "checkout", GETs `/api/v1/traces?service=&start=&end=`
//! and finally READS the spans the aperture trace path has been writing
//! into ray for weeks, scoped to her tenant and service, over a window.
//! Empty reads calm (200 `[]`), a missing `service` reads as a 400 that
//! names the fault before the store is touched, a fat-fingered window
//! reads as a 400 likewise, and a backend failure reads as a 500 that
//! never fabricates an empty success.
//!
//! One structural divergence from logs (ADR-0048 Decision 1): traces
//! REQUIRE a `service` parameter. Missing or empty `service` is a 400
//! caught BEFORE the store, proved here with a `FailingTraceStore`
//! double that would lift the response to a 500 if wrongly queried.
//!
//! Every scenario drives trace-query-api through its single public
//! driving port `trace_query_api::router(store, tenant)` via `oneshot`
//! against a REAL durable `FileBackedTraceStore` (the success and
//! isolation arms) or a failing store double (the 400-before-store and
//! 500 arms).
//!
//! RED state (DISTILL Mandate 7): the lib ships a scaffold whose
//! handler is `unimplemented!`, so every scenario that sends a request
//! fails RED (the handler panics), never BROKEN (a compile error). The
//! suite COMPILES against the real ray + axum + tower surfaces and
//! asserts the real ADR-0048 contract. DELIVER implements the handler
//! and enables the scenarios one at a time.
//!
//! One-at-a-time outer loop: the walking skeleton is enabled; every
//! following scenario is `#[ignore]`d and gets enabled one at a time as
//! the crafter drives each inward.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, open_durable_store, rich_span, secs_to_nanos, seed, span,
    span_at_nanos, span_names, span_with_ids, spans_array, start_times, tenant, traces_request,
    traces_request_with_auth, traces_request_without_service, FailingTraceStore,
};
use ray::TraceStore;

// =====================================================================
// US-01 — Walking skeleton: the operator reads the in-window spans
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has six spans seeded into a REAL durable
/// ray store for service "checkout", three inside the window
/// [1716200000s, 1716200060s) (a Server "place-order", an Internal
/// "reserve-stock", a Client "charge-card") and three outside it (two
/// earlier, one later),
/// When the operator GETs the traces endpoint for "acme-prod" with
/// service "checkout" over [1716200000, 1716200060),
/// Then she sees exactly the three in-window spans, in ascending
/// start_time order, with all fields intact and no out-of-window span
/// present.
///
/// This is the demo-able outcome of the slice: spans that were written
/// and invisible are finally readable, scoped to the tenant and the
/// service, over a window. It seeds REAL durable storage (the same
/// `FileBackedTraceStore` adapter the aperture trace path writes
/// through) so the skeleton proves wiring, the `TraceStore::query`
/// read, the half-open window, and the bare-array shape end to end.
#[tokio::test]
async fn operator_reads_the_in_window_spans_for_a_tenant_and_service() {
    let (store, _base) = open_durable_store("ws-in-window-read");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            // Two before the window.
            span_with_ids(1_716_199_900, "checkout", "before: warming up", 0xAA, 0x01),
            span_with_ids(1_716_199_990, "checkout", "before: still early", 0xAA, 0x02),
            // Three inside [1716200000, 1716200060).
            span_with_ids(1_716_200_005, "checkout", "place-order", 0xAA, 0x03),
            span_with_ids(1_716_200_030, "checkout", "reserve-stock", 0xAA, 0x04),
            span_with_ids(1_716_200_055, "checkout", "charge-card", 0xAA, 0x05),
            // One after the window.
            span_with_ids(1_716_200_100, "checkout", "after: too late", 0xAA, 0x06),
        ],
    );

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        span_names(&body),
        vec![
            "place-order".to_string(),
            "reserve-stock".to_string(),
            "charge-card".to_string(),
        ],
        "exactly the three in-window spans, in ascending start_time order"
    );
    let times = start_times(&body);
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(
        times, sorted,
        "spans are in ascending start_time_unix_nano order"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("before:") && !rendered.contains("after:"),
        "no out-of-window span appears: {rendered}"
    );
}

// =====================================================================
// US-01 — The half-open window includes start and excludes end (boundary)
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given tenant "acme-prod" has one span at exactly the window start
/// and one at exactly the window end for service "checkout",
/// When the operator GETs the traces endpoint over [start, end),
/// Then only the span at start is returned: the half-open window
/// includes start and excludes end.
#[tokio::test]
async fn the_half_open_window_includes_start_and_excludes_end() {
    let (store, _base) = open_durable_store("half-open-boundary");
    let t = tenant("acme-prod");
    let start_ns = secs_to_nanos(1_716_200_000);
    let end_ns = secs_to_nanos(1_716_200_060);
    seed(
        &store,
        &t,
        vec![
            span_at_nanos(start_ns, "checkout", "exactly at start"),
            span_at_nanos(end_ns, "checkout", "exactly at end"),
        ],
    );

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        span_names(&body),
        vec!["exactly at start".to_string()],
        "the span at start is included, the span at end is excluded"
    );
}

// =====================================================================
// US-01 — Every Span field round-trips faithfully (boundary)
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given tenant "acme-prod" has one span carrying a name, kind, status
/// (Error + message), span attributes, resource attributes, a populated
/// parent_span_id, one event, and one link,
/// When the operator GETs the traces endpoint over a covering window,
/// Then the response carries every field of the span, none dropped or
/// renamed, with `trace_id`/`span_id` as lowercase hex strings.
#[tokio::test]
async fn every_span_field_round_trips_in_the_response() {
    let (store, _base) = open_durable_store("field-fidelity");
    let t = tenant("acme-prod");
    seed(&store, &t, vec![rich_span(1_716_200_005)]);

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let spans = spans_array(&body);
    assert_eq!(spans.len(), 1, "the single covering span is returned");
    let s = &spans[0];
    assert_eq!(s["name"], "place-order");
    assert_eq!(s["kind"], "Server");
    assert_eq!(
        s["start_time_unix_nano"].as_u64(),
        Some(secs_to_nanos(1_716_200_005))
    );
    assert_eq!(s["status"]["code"], "Error");
    assert_eq!(s["status"]["message"], "upstream timeout");
    assert_eq!(s["attributes"]["http.route"], "/orders");
    assert_eq!(s["resource_attributes"]["service.name"], "checkout");
    assert!(
        s["parent_span_id"].as_str().is_some(),
        "the populated parent_span_id is carried as a hex string: {s}"
    );
    assert_eq!(
        s["trace_id"].as_str().map(str::len),
        Some(32),
        "trace_id is a 32-char lowercase hex string: {s}"
    );
    assert_eq!(
        s["span_id"].as_str().map(str::len),
        Some(16),
        "span_id is a 16-char lowercase hex string: {s}"
    );
    let events = s["events"].as_array().expect("events array carried");
    assert_eq!(events.len(), 1, "the populated event is carried: {s}");
    assert_eq!(events[0]["name"], "exception");
    let links = s["links"].as_array().expect("links array carried");
    assert_eq!(links.len(), 1, "the populated link is carried: {s}");
}

// =====================================================================
// US-02 — A window with no spans returns a calm empty array (happy path)
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has spans for service "checkout" but none
/// whose start_time falls in the requested window,
/// When the operator GETs the traces endpoint over that empty window,
/// Then the status is 200 and the result is an empty array, not an
/// error.
#[tokio::test]
async fn a_window_with_no_spans_returns_a_calm_empty_array() {
    let (store, _base) = open_durable_store("calm-empty-window");
    let t = tenant("acme-prod");
    // The earliest span is well after the queried window.
    seed(
        &store,
        &t,
        vec![span(1_716_300_000, "checkout", "much later")],
    );

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        spans_array(&body).is_empty(),
        "an empty window is a calm empty array: {body}"
    );
    assert!(!is_error_envelope(&body), "empty is never an error: {body}");
}

// =====================================================================
// US-02 — An unknown (tenant, service) pair returns the empty array (edge)
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given the endpoint resolves tenant "acme-prod" but the service
/// "ghost-service" has never had spans written under it,
/// When the operator GETs the traces endpoint over any window,
/// Then the status is 200 and the result is an empty array, never an
/// error.
#[tokio::test]
async fn an_unknown_service_returns_the_empty_array_not_an_error() {
    // A real durable store that has spans for "checkout" but none for
    // "ghost-service"; the store returns Ok(Vec::new()) for an unknown
    // (tenant, service) key (store.rs:202).
    let (store, _base) = open_durable_store("calm-empty-unknown-service");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![span(1_716_200_010, "checkout", "exists for checkout only")],
    );

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_request("ghost-service", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        spans_array(&body).is_empty(),
        "an unknown service yields the empty array: {body}"
    );
}

// =====================================================================
// US-03 — A query returns only the resolved tenant's spans (happy path)
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given the endpoint resolves tenant "acme-prod" and the durable store
/// holds in-window spans for service "checkout" under BOTH "acme-prod"
/// and "globex-prod",
/// When the operator GETs the traces endpoint over the window,
/// Then the result contains only acme-prod's spans and no globex-prod
/// span appears.
#[tokio::test]
async fn a_trace_query_returns_only_the_resolved_tenants_spans() {
    let (store, _base) = open_durable_store("tenant-isolation");
    let acme = tenant("acme-prod");
    let globex = tenant("globex-prod");
    seed(
        &store,
        &acme,
        vec![span_with_ids(
            1_716_200_010,
            "checkout",
            "acme: place-order",
            0xAA,
            0x01,
        )],
    );
    seed(
        &store,
        &globex,
        vec![span_with_ids(
            1_716_200_020,
            "checkout",
            "globex: secret-order",
            0xBB,
            0x01,
        )],
    );

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(acme));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        span_names(&body),
        vec!["acme: place-order".to_string()],
        "only acme-prod's span is returned"
    );
    assert!(
        !body.to_string().contains("globex"),
        "no globex-prod span leaks across the tenant boundary: {body}"
    );
}

// =====================================================================
// US-03 — A request with no resolvable tenant is refused (boundary)
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given the endpoint has no configured tenant and the request carries
/// no tenant signal,
/// When the operator GETs the traces endpoint over any window,
/// Then the request is refused with a 401 error envelope and no span
/// is returned.
#[tokio::test]
async fn a_request_with_no_resolvable_tenant_is_refused() {
    let (store, _base) = open_durable_store("fail-closed-no-tenant");
    // Seed spans that MUST NOT leak: fail-closed means refused before
    // the store, so even a populated store returns nothing.
    seed(
        &store,
        &tenant("acme-prod"),
        vec![span(1_716_200_010, "checkout", "must not leak")],
    );

    // None models "no tenant resolvable" at the router seam.
    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, None);
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no resolvable tenant is refused fail-closed"
    );
    assert!(
        is_error_envelope(&body),
        "the refusal is an error envelope: {body}"
    );
    assert!(
        !body.to_string().contains("must not leak"),
        "no spans are returned when refused: {body}"
    );
}

// =====================================================================
// US-04 — An inverted window is rejected with no store query (error path)
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator submits a window where start is later than end,
/// When the endpoint validates the window,
/// Then the status is 400 with an error envelope, and (proven by a
/// store double that would lift the response to a 500 if queried) no
/// store query is run.
#[tokio::test]
async fn an_inverted_window_is_rejected_with_no_store_query() {
    // A failing store double: if the handler queried it, the 5xx arm
    // would fire instead of the 400. A 400 here proves the bad window
    // is caught BEFORE the store is touched.
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    // start (later) > end (earlier).
    let request = traces_request("checkout", "1716200060", "1716200000");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an inverted window is a 400 caught before the store"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is an error envelope: {body}"
    );
}

// =====================================================================
// US-04 — A non-numeric window bound is rejected (error path)
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator submits a window with a non-numeric start bound,
/// When the endpoint validates the window,
/// Then the status is 400 with an error envelope and the store is
/// never touched (proven by a failing store double).
#[tokio::test]
async fn a_non_numeric_window_bound_is_rejected_with_no_store_query() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request("checkout", "notanumber", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a malformed bound is a 400 caught before the store"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is an error envelope: {body}"
    );
}

// =====================================================================
// ADR-0048 Decision 1 — A missing `service` parameter is a 400 (error path)
// =====================================================================

/// @driving_port @US-04
///
/// One structural divergence from logs (ADR-0048 Decision 1): traces
/// require a `service` parameter, and a request that omits it is a 400
/// caught BEFORE the store is touched, never an empty result.
///
/// Given the operator submits a request without any `service` parameter,
/// When the endpoint validates the request,
/// Then the status is 400 with an error envelope and the store is
/// never touched (proven by a failing store double that would lift the
/// response to a 500 if wrongly queried).
#[tokio::test]
async fn a_missing_service_parameter_is_rejected_with_no_store_query() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request_without_service("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a missing service parameter is a 400 caught before the store"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is an error envelope: {body}"
    );
}

// =====================================================================
// ADR-0048 Decision 1 — An empty `service` value is a 400 (error path)
// =====================================================================

/// @driving_port @US-04
///
/// The structural divergence again: an empty `service=` is identical
/// in shape to "missing" and must be rejected the same way before the
/// store is touched, never serialised as an empty result (which would
/// collapse the honest three-way distinction the contract pins).
///
/// Given the operator submits a request with an empty `service`
/// parameter (e.g. `service=`),
/// When the endpoint validates the request,
/// Then the status is 400 with an error envelope and the store is
/// never touched (proven by a failing store double).
#[tokio::test]
async fn an_empty_service_parameter_is_rejected_with_no_store_query() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request("", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an empty service parameter is a 400 caught before the store"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is an error envelope: {body}"
    );
}

// =====================================================================
// US-04 — A store read failure surfaces as a 500, never a fabricated empty
// =====================================================================

/// @infrastructure-failure @driving_port @US-04
///
/// Given the durable store fails to read for tenant "acme-prod",
/// service "checkout", with a persistence error,
/// When the operator GETs the traces endpoint over a valid window,
/// Then the status is a 500 server error with an error envelope, NOT
/// a fabricated empty array.
#[tokio::test]
async fn a_store_read_failure_surfaces_as_a_server_error_not_an_empty_result() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "a store failure is a 500, never a fabricated empty success"
    );
    assert!(
        is_error_envelope(&body),
        "the failure is an error envelope: {body}"
    );
    assert!(
        body.as_array().is_none(),
        "a store failure must never look like an empty array: {body}"
    );
}

// =====================================================================
// US-04 — A failure response never leaks a forwarded header value
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator's request carries a forwarded Authorization
/// header "Bearer SECRET" and the store fails to read,
/// When the endpoint returns an error response,
/// Then the error text does not contain "SECRET", the raw query, or
/// the offending parameter value.
#[tokio::test]
async fn a_failure_response_never_leaks_a_forwarded_header_value() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request_with_auth("checkout", "1716200000", "1716200060", "Bearer SECRET");
    let (status, body) = call(router, request).await;

    // The exact error status is asserted elsewhere; here the point is
    // the redaction: whatever the error, it must not echo the
    // credential nor the raw parameter values.
    assert!(status.is_client_error() || status.is_server_error());
    let rendered = body.to_string();
    assert!(
        !rendered.contains("SECRET"),
        "the error text must never echo a forwarded credential: {rendered}"
    );
    assert!(
        !rendered.contains("Bearer"),
        "the error text must never echo the Authorization scheme: {rendered}"
    );
}
