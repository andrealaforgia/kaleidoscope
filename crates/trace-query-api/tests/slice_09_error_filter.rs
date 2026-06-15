// Kaleidoscope trace-query-api — slice 09 traces listing error filter
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

//! Traces listing error filter — `GET /api/v1/traces?...&error=true` returns
//! ONLY the spans of traces that contain at least one Error-status span,
//! within the same service + time-window scope that already applies, so a
//! newcomer can REACH the failed trace and tell it IS the failed one without
//! opening every trace (experimentable-stack-v0).
//!
//! `error=false` or `error` absent is EXACTLY today's behaviour (no
//! error filtering). A malformed `error` value is a 400 caught BEFORE the
//! store, consistent with the existing service/window param validation.
//!
//! Every scenario drives trace-query-api through its public driving port
//! `trace_query_api::router(store, tenant)` via tower `oneshot` against a REAL
//! in-memory `TraceStore` fake (the happy/scope arms), or a failing double for
//! the malformed-value-before-store proof. Port-to-port at the crate boundary:
//! assertions are on the HTTP status and the observable JSON span array.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use axum::body::Body;
use axum::http::{Request, StatusCode};

use common::{call, is_error_envelope, span_names, spans_array, tenant, traces_request};
use ray::{
    InMemoryTraceStore, NoopRecorder, Span, SpanBatch, SpanId, SpanKind, SpanStatus,
    StatusCode as RayStatusCode, TraceId, TraceStore,
};

// ---------------------------------------------------------------------
// Fixtures — build the driven-port fake and seed it with spans whose
// trace_id, service, time, and status the scenarios pin.
// ---------------------------------------------------------------------

const WINDOW_START: &str = "1716200000";
const WINDOW_END: &str = "1716200060";

fn error_status(message: &str) -> SpanStatus {
    SpanStatus {
        code: RayStatusCode::Error,
        message: message.to_string(),
    }
}

fn ok_status() -> SpanStatus {
    SpanStatus {
        code: RayStatusCode::Ok,
        message: String::new(),
    }
}

/// A span on `trace_byte`*16 / `span_byte`*8, at `secs`, filed under
/// `service`, with `status`. Carries `service.name` so the store's
/// by-service index is exercised exactly as production.
fn span(
    secs: u64,
    service: &str,
    trace_byte: u8,
    span_byte: u8,
    name: &str,
    status: SpanStatus,
) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    let start = secs * 1_000_000_000;
    Span {
        trace_id: TraceId([trace_byte; 16]),
        span_id: SpanId([span_byte; 8]),
        parent_span_id: None,
        name: name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 1_000_000,
        status,
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

fn store_with(spans: Vec<Span>, t: &TenantId) -> Arc<dyn TraceStore + Send + Sync> {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    store
        .ingest(t, SpanBatch::with_spans(spans))
        .expect("seed trace store");
    Arc::new(store)
}

/// The listing request the contract pins, with an explicit `error` value.
fn traces_request_with_error(service: &str, start: &str, end: &str, error: &str) -> Request<Body> {
    let uri = format!("/api/v1/traces?service={service}&start={start}&end={end}&error={error}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// The trace_id (lowercase hex) of every span in the response, in order.
fn trace_ids(body: &serde_json::Value) -> Vec<String> {
    spans_array(body)
        .iter()
        .filter_map(|s| s["trace_id"].as_str().map(str::to_string))
        .collect()
}

const FAILED_HEX: &str = "abababababababababababababababab";
const HEALTHY_HEX: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

/// A window holding one FAILED trace (0xAB: an Error "place-order" + a
/// healthy "charge-card" — same trace) and one fully-HEALTHY trace (0xCD:
/// one Ok "healthy-order"), all for service "checkout" inside the window.
fn failed_and_healthy_traces() -> (TenantId, Vec<Span>) {
    let t = tenant("acme-prod");
    let spans = vec![
        span(
            1_716_200_010,
            "checkout",
            0xAB,
            0x01,
            "place-order",
            error_status("upstream timeout"),
        ),
        span(
            1_716_200_011,
            "checkout",
            0xAB,
            0x02,
            "charge-card",
            ok_status(),
        ),
        span(
            1_716_200_012,
            "checkout",
            0xCD,
            0x03,
            "healthy-order",
            ok_status(),
        ),
    ];
    (t, spans)
}

// =====================================================================
// (a)+(b) error=true returns ONLY the failed trace's spans — and ALL of
// them (the non-error span of the failed trace too), so the trace is
// reachable in full; the healthy trace is excluded.
// =====================================================================

#[tokio::test]
async fn error_true_returns_all_spans_of_failed_traces_and_excludes_healthy() {
    let (t, spans) = failed_and_healthy_traces();
    let store = store_with(spans, &t);

    let router = trace_query_api::router(store, Some(t));
    let request = traces_request_with_error("checkout", WINDOW_START, WINDOW_END, "true");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "the listing is 200: {body}");

    // Both spans of the failed trace come back — not just the error span —
    // so the failed trace is reachable in full (behaviour b).
    assert_eq!(
        span_names(&body),
        vec!["place-order".to_string(), "charge-card".to_string()],
        "exactly the failed trace's two spans, error AND healthy span: {body}"
    );

    // Every returned span belongs to the failed trace; the healthy trace is
    // gone (behaviour a).
    assert!(
        trace_ids(&body).iter().all(|id| id == FAILED_HEX),
        "every returned span belongs to the failed trace: {body}"
    );
    assert!(
        !body.to_string().contains("healthy-order") && !body.to_string().contains(HEALTHY_HEX),
        "no span of a healthy trace appears: {body}"
    );

    // The failed trace is distinguishable AS failed: its Error status rides.
    let error_present = spans_array(&body).iter().any(|s| {
        s["status"]["code"].as_str() == Some("Error")
            && s["status"]["message"].as_str() == Some("upstream timeout")
    });
    assert!(error_present, "the Error-status span is carried: {body}");
}

// =====================================================================
// error=true is case-insensitive (TRUE / True), and still filters.
// Kills a case-sensitive parse mutant.
// =====================================================================

#[tokio::test]
async fn error_true_is_case_insensitive() {
    for value in ["TRUE", "True", "tRuE"] {
        let (t, spans) = failed_and_healthy_traces();
        let store = store_with(spans, &t);
        let router = trace_query_api::router(store, Some(t));
        let request = traces_request_with_error("checkout", WINDOW_START, WINDOW_END, value);
        let (status, body) = call(router, request).await;

        assert_eq!(status, StatusCode::OK, "error={value} is 200: {body}");
        assert_eq!(
            span_names(&body),
            vec!["place-order".to_string(), "charge-card".to_string()],
            "error={value} filters to the failed trace case-insensitively: {body}"
        );
    }
}

// =====================================================================
// (c) error=false and error absent return the unfiltered list, EXACTLY
// today's behaviour (every in-window span for the service, healthy and
// failed alike). Kills a mutant that filters when error is off.
// =====================================================================

#[tokio::test]
async fn error_off_returns_the_unfiltered_list() {
    // Each request consumes a fresh store/router (the fake is moved in).
    let cases: Vec<Option<&str>> = vec![Some("false"), Some("FALSE"), None];
    for case in cases {
        let (t, spans) = failed_and_healthy_traces();
        let store = store_with(spans, &t);
        let router = trace_query_api::router(store, Some(t));
        let request = match case {
            Some(value) => traces_request_with_error("checkout", WINDOW_START, WINDOW_END, value),
            None => traces_request("checkout", WINDOW_START, WINDOW_END),
        };
        let (status, body) = call(router, request).await;

        assert_eq!(status, StatusCode::OK, "error={case:?} is 200: {body}");
        // All three in-window spans (both traces) come back — no filtering.
        assert_eq!(
            span_names(&body),
            vec![
                "place-order".to_string(),
                "charge-card".to_string(),
                "healthy-order".to_string(),
            ],
            "error={case:?} leaves the listing unfiltered: {body}"
        );
    }
}

// =====================================================================
// (d) error=true filters WITHIN the existing service + time-window scope:
// a failed trace outside the window, and a failed trace under a different
// service, are both excluded — only the in-scope failed trace survives.
// =====================================================================

#[tokio::test]
async fn error_true_filters_within_the_service_and_window_scope() {
    let t = tenant("acme-prod");
    let store = store_with(
        vec![
            // In-scope failed trace (service checkout, inside the window).
            span(
                1_716_200_010,
                "checkout",
                0xAB,
                0x01,
                "in-scope-failure",
                error_status("boom"),
            ),
            // Failed trace OUTSIDE the window (same service) — excluded by
            // the window the listing already applies.
            span(
                1_716_300_000,
                "checkout",
                0xEF,
                0x02,
                "out-of-window-failure",
                error_status("late boom"),
            ),
            // Failed trace under a DIFFERENT service, inside the window —
            // excluded by the required service scope.
            span(
                1_716_200_015,
                "payments",
                0x99,
                0x03,
                "other-service-failure",
                error_status("other boom"),
            ),
        ],
        &t,
    );

    let router = trace_query_api::router(store, Some(t));
    let request = traces_request_with_error("checkout", WINDOW_START, WINDOW_END, "true");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "the listing is 200: {body}");
    assert_eq!(
        span_names(&body),
        vec!["in-scope-failure".to_string()],
        "only the in-scope failed trace survives the service+window scope: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("out-of-window-failure") && !rendered.contains("other-service-failure"),
        "failed traces outside the window or service are excluded: {rendered}"
    );
}

// =====================================================================
// (e) A malformed `error` value is a 400 caught BEFORE the store, with no
// echo of the raw value. Proven by a failing store double: a leaked query
// would lift the response to a 500.
// =====================================================================

#[tokio::test]
async fn a_malformed_error_value_is_rejected_with_no_store_query() {
    for raw in ["maybe", "1", "yes", ""] {
        let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(common::FailingTraceStore);
        let router = trace_query_api::router(store, Some(tenant("acme-prod")));
        let request = traces_request_with_error("checkout", WINDOW_START, WINDOW_END, raw);
        let (status, body) = call(router, request).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "a malformed error value {raw:?} is a 400 caught before the store: {body}"
        );
        assert!(
            is_error_envelope(&body),
            "the rejection is an error envelope: {body}"
        );
        assert!(
            !body.to_string().contains(raw) || raw.is_empty(),
            "the error text never echoes the raw value: {body}"
        );
    }
}
