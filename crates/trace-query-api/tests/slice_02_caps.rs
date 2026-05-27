// Kaleidoscope trace-query-api — slice 02 honest-read-caps acceptance suite
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

//! Honest read-side caps — window cap (86_400 seconds) and result-size
//! cap (100_000 spans) on `/api/v1/traces`.
//!
//! Maps to `docs/feature/honest-read-caps-v0/discuss/user-stories.md`
//! (US-03 window cap on trace-query-api, US-04 result cap on
//! trace-query-api, US-05 redaction on cap reasons for traces). Contract
//! pinned by ADR-0050 (`docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`).
//!
//! The user-centric outcome: when the operator (or a misconfigured
//! script) asks for too wide a window or would receive too many spans,
//! the trace read API refuses with a named 400 carrying the existing
//! `{status:"error", error:"..."}` envelope. The refusal is OUT LOUD,
//! never silently truncated, never `X-Truncated`, never a calm empty
//! 200. The store is never touched on the window-cap refusal path; the
//! store is touched exactly once on the result-cap refusal path (the
//! handler must know the size to refuse) but serialisation is never
//! attempted.
//!
//! Boundary discipline (ADR-0050 Decision 1 and 2): the boundary is
//! `>`, never `>=`. A window of exactly `MAX_WINDOW_SECONDS` (86_400)
//! is served; one second wider is refused. A result of exactly
//! `MAX_RESULT_ROWS` (100_000) is served; one row more is refused.
//!
//! Handler order preserved (ADR-0050 Decision 4 / DESIGN D5): the
//! existing missing-service 400 STILL FIRES FIRST on a request whose
//! `service` is absent, even when the window also exceeds the cap. The
//! operator fixes the most-required-field failure first.
//!
//! Redaction posture (ADR-0050 Decision 7): the cap-400 body inherits
//! the stricter trace-query-api posture. The body must contain neither
//! the raw window values, nor the raw `service`, nor "SECRET", nor
//! "Bearer", nor a forwarded `Authorization` value, anywhere.
//!
//! RED state (behavioural, not compile-level): every scenario in this
//! file COMPILES against the current `trace-query-api` source. The
//! scenarios that expect a 400 fail today because no cap check exists
//! in the handler; the scenarios that expect a 200 pass today because
//! the request happens to be within an unenforced cap. The DELIVER wave
//! adds the two `pub const` and the two `if` arms per ADR-0050, at
//! which point every scenario goes green.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, open_durable_store, seed, span_with_ids, spans_array, tenant,
    traces_request, traces_request_with_auth, traces_request_without_service, FailingTraceStore,
};
use ray::{
    IngestReceipt, Predicate, ServiceName, Span, SpanBatch, SpanKind, SpanStatus, TimeRange,
    TraceId, TraceStore, TraceStoreError,
};

// ---------------------------------------------------------------------
// A driven-port test double that returns a configurable, large `Vec`
// of synthetic spans from `query`. Used to exercise the result-size
// cap arm without seeding the durable store with 100_000+ records,
// which would be slow and memory-heavy in CI. The cap fires AFTER the
// store query, so the store IS touched here (a clean 400 still proves
// the cap fired at the right seam: serialisation never starts because
// the body is `{status:"error", ...}`, not a JSON array of spans).
//
// The double is a test adapter for the `ray::TraceStore` driven port,
// not an internal trace-query-api component, so driving the router
// through `router(...)` over it still honours the hexagonal boundary.
// ---------------------------------------------------------------------

/// A store whose `query` returns exactly `count` synthetic spans for
/// any tenant, service, and range. Ingest is disabled (this is a read
/// service). `get_trace` and `query_with` are inert.
struct BulkTraceStore {
    count: usize,
}

impl BulkTraceStore {
    fn new(count: usize) -> Self {
        Self { count }
    }
}

fn synthetic_span(seq: usize) -> Span {
    let trace_byte = (seq & 0xFF) as u8;
    let span_byte = ((seq >> 8) & 0xFF) as u8;
    Span {
        trace_id: TraceId([trace_byte; 16]),
        span_id: ray::SpanId([span_byte; 8]),
        parent_span_id: None,
        name: "synthetic".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: 1_716_200_000_000_000_000,
        end_time_unix_nano: 1_716_200_000_120_000_000,
        status: SpanStatus::default(),
        attributes: std::collections::BTreeMap::new(),
        resource_attributes: {
            let mut r = std::collections::BTreeMap::new();
            r.insert("service.name".to_string(), "checkout".to_string());
            r
        },
        events: Vec::new(),
        links: Vec::new(),
    }
}

impl TraceStore for BulkTraceStore {
    fn ingest(
        &self,
        _tenant: &aegis::TenantId,
        _batch: SpanBatch,
    ) -> Result<IngestReceipt, TraceStoreError> {
        Err(TraceStoreError::PersistenceFailed {
            reason: "ingest disabled in read service".to_string(),
        })
    }

    fn get_trace(
        &self,
        _tenant: &aegis::TenantId,
        _trace_id: &TraceId,
    ) -> Result<Vec<Span>, TraceStoreError> {
        Ok(Vec::new())
    }

    fn query(
        &self,
        _tenant: &aegis::TenantId,
        _service: &ServiceName,
        _range: TimeRange,
    ) -> Result<Vec<Span>, TraceStoreError> {
        Ok((0..self.count).map(synthetic_span).collect())
    }

    fn query_with(
        &self,
        _tenant: &aegis::TenantId,
        _service: &ServiceName,
        _range: TimeRange,
        _predicate: &Predicate,
    ) -> Result<Vec<Span>, TraceStoreError> {
        Ok((0..self.count).map(synthetic_span).collect())
    }
}

// =====================================================================
// US-03 — Happy path: a window WITHIN the cap is served normally
// =====================================================================

/// @driving_port @real-io @adapter-integration @US-03
///
/// Given tenant "acme-prod" has one in-window span for service
/// "checkout" seeded into a real durable ray store,
/// When the operator queries that service over a 60-second window
/// (well within the 86_400-second cap),
/// Then she sees the matching span in a bare JSON array, with no cap
/// refusal in sight. The cap is invisible on well-formed queries.
#[tokio::test]
async fn a_traces_request_within_the_window_cap_is_served_normally() {
    let (store, _base) = open_durable_store("within-window-cap");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![span_with_ids(
            1_716_200_005,
            "checkout",
            "place-order",
            0xAA,
            0x01,
        )],
    );

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        spans_array(&body).len(),
        1,
        "the one matching in-window span is returned"
    );
}

// =====================================================================
// US-03 — Boundary: window of EXACTLY MAX_WINDOW_SECONDS is served
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given a window of exactly 86_400 seconds (start=0, end=86_400) and
/// a real durable store with no spans inside that window,
/// When the operator queries service "checkout" over that exact-cap
/// window,
/// Then the response is 200 with the calm empty bare array. The
/// boundary is inclusive: `end - start == MAX_WINDOW_SECONDS` is
/// served (NOT refused).
///
/// This kills a `>` -> `>=` mutant on the window cap check.
#[tokio::test]
async fn a_traces_request_at_exactly_the_window_cap_is_served() {
    let (store, _base) = open_durable_store("at-window-cap");
    let t = tenant("acme-prod");
    // The store is empty for this window; the cap-check, if any, sees
    // a 86_400-second window and lets the request through; the store
    // is queried and returns an empty array.
    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_request("checkout", "0", "86400");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a window of exactly 86_400 seconds is at the cap and served, not refused"
    );
    assert!(
        spans_array(&body).is_empty(),
        "the empty window is the calm empty bare array: {body}"
    );
}

// =====================================================================
// US-03 — Window over the cap is refused BEFORE the store is touched
// =====================================================================

/// @driving_port @US-03
///
/// Given a window of 86_401 seconds (start=0, end=86401), one second
/// over the cap, and a LyingTraceStore whose `query` always returns
/// `PersistenceFailed`,
/// When the operator queries service "checkout" over that window,
/// Then the response is 400 with the `{status:"error", error:...}`
/// envelope, the `error` string names "window" and a value-of-cap
/// substring like "exceeds 86400" (the cap class is named, the request
/// value is not echoed), and the LyingTraceStore's `query` was NEVER
/// called (proven by the absence of the 500 that would lift if the
/// lying store had been touched).
///
/// This is the carpaccio taste-test 1 from ADR-0050: the cap fires
/// BEFORE the store. A mutant that swapped check and store would lift
/// the response to a 500 and fail this scenario.
#[tokio::test]
async fn a_window_one_second_over_the_cap_is_refused_before_the_store() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request("checkout", "0", "86401");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a window over the cap is a 400, never a 500: the store is NEVER touched"
    );
    assert!(
        is_error_envelope(&body),
        "the cap refusal is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        message.contains("window"),
        "the cap reason names the window class: {message}"
    );
    assert!(
        message.contains("exceeds") && message.contains("86400"),
        "the cap reason names the value of the cap, not the request value: {message}"
    );
}

// =====================================================================
// US-03 — The missing-service 400 still fires FIRST, even on over-window
// =====================================================================

/// @driving_port @US-03
///
/// Given a request with NO `service` parameter AND a year-long window
/// (well over the cap), and a LyingTraceStore,
/// When the operator sends the request,
/// Then the response is 400 with the existing
/// `invalid request: service is required` envelope (the handler-order
/// precondition), NOT the new cap-400 reason. The store is NEVER
/// touched either way. Handler order is preserved: tenant -> service
/// -> parse -> window-cap -> store -> result-cap.
#[tokio::test]
async fn the_missing_service_400_still_fires_before_the_window_cap_400() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request_without_service("0", "31536000");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(is_error_envelope(&body));
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        message.contains("service"),
        "the existing missing-service reason fires first: {message}"
    );
    assert!(
        message.contains("required"),
        "the existing missing-service reason names the missing field: {message}"
    );
}

// =====================================================================
// US-04 — Boundary: result of EXACTLY MAX_RESULT_ROWS is served
// =====================================================================

/// @driving_port @US-04
///
/// Given a store that returns exactly 100_000 synthetic spans for an
/// in-window query,
/// When the operator queries service "checkout" over a within-cap
/// window,
/// Then the response is 200 and carries all 100_000 spans. The
/// boundary is inclusive: `spans.len() == MAX_RESULT_ROWS` is served
/// (NOT refused).
///
/// This kills a `>` -> `>=` mutant on the result-cap check.
#[tokio::test]
async fn a_result_at_exactly_the_result_cap_is_served() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(BulkTraceStore::new(100_000));
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a result of exactly 100_000 spans is at the cap and served, not refused"
    );
    assert_eq!(
        spans_array(&body).len(),
        100_000,
        "all 100_000 spans are returned at the boundary"
    );
}

// =====================================================================
// US-04 — A result one row OVER the cap is refused, never truncated
// =====================================================================

/// @driving_port @US-04
///
/// Given a store that returns 100_001 synthetic spans for an in-window
/// query,
/// When the operator queries service "checkout" over a within-cap
/// window,
/// Then the response is 400 with `{status:"error", error:...}`, the
/// error names "result" and the cap value, and the response is NEVER
/// a truncated 200, NEVER an `X-Truncated` 200, NEVER a calm empty.
///
/// The store IS touched here (the cap fires AFTER the store; the
/// handler must know the size to refuse) but serialisation is never
/// attempted: the body is the error envelope, not a JSON array.
#[tokio::test]
async fn a_result_one_row_over_the_cap_is_refused_with_a_named_400() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(BulkTraceStore::new(100_001));
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request("checkout", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a result over the cap is a named 400, never a truncated 200"
    );
    assert!(
        is_error_envelope(&body),
        "the result-cap refusal is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert!(
        message.contains("result"),
        "the cap reason names the result class: {message}"
    );
    assert!(
        message.contains("exceeds") && message.contains("100000"),
        "the cap reason names the value of the cap: {message}"
    );
    assert!(
        body.as_array().is_none(),
        "the refusal must never look like a bare JSON array of spans: {body}"
    );
}

// =====================================================================
// US-05 — Redaction: cap-refused body echoes nothing sensitive (strict)
// =====================================================================

/// @driving_port @US-05
///
/// Given the operator's request carries a forwarded
/// `Authorization: Bearer SECRET` header AND requests a service named
/// "checkout-with-secret-shape" AND a year-long window,
/// When the trace read API returns the window-cap 400,
/// Then the response body contains NONE of:
///   - the raw start value ("0") or end value ("31536000"),
///   - the raw `service` value ("checkout-with-secret-shape"),
///   - the literal "SECRET" anywhere,
///   - the literal "Bearer" anywhere.
///
/// This is the stricter trace-query-api posture inherited by the new
/// cap reasons (ADR-0050 Decision 7).
#[tokio::test]
async fn the_cap_refused_body_never_echoes_raw_values_or_a_credential() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_request_with_auth(
        "checkout-with-secret-shape",
        "0",
        "31536000",
        "Bearer SECRET",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let rendered = body.to_string();
    assert!(
        !rendered.contains("31536000"),
        "the body must not echo the raw end value: {rendered}"
    );
    assert!(
        !rendered.contains("checkout-with-secret-shape"),
        "the body must not echo the raw service value: {rendered}"
    );
    assert!(
        !rendered.contains("SECRET"),
        "the body must not contain SECRET anywhere (stricter trace posture): {rendered}"
    );
    assert!(
        !rendered.contains("Bearer"),
        "the body must not contain Bearer anywhere (stricter trace posture): {rendered}"
    );
}
