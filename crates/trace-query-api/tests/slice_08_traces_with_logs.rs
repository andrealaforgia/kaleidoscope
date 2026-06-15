// Kaleidoscope trace-query-api — slice 08 combined traces+logs suite
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

//! Combined trace+logs — `/api/v1/traces/with_logs?trace_id=<32-hex>`
//! returns ONE trace's spans together with its correlated logs in a SINGLE
//! response, so an operator observes "a trace and its logs" in one call
//! without client-side stitching (experimentable-stack-v0).
//!
//! Every scenario drives trace-query-api through its public driving port
//! `trace_query_api::router_with_logs(trace_store, log_store, tenant)` via
//! tower `oneshot` against REAL in-memory `TraceStore` / `LogStore` fakes
//! (the driven ports), or failing doubles for the 500 arms. No internal
//! type is touched: the assertions are on the HTTP status and the observable
//! JSON body shape. Port-to-port at the crate boundary.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::http::StatusCode;

use aegis::TenantId;
use common::{call, is_error_envelope, tenant};
use lumen::{
    InMemoryLogStore, IngestReceipt as LumenReceipt, LogBatch, LogRecord, LogStore, LogStoreError,
    NoopRecorder as LumenNoop, Predicate as LumenPredicate, SeverityNumber,
    TimeRange as LumenTimeRange,
};
use ray::{
    InMemoryTraceStore, NoopRecorder as RayNoop, Span, SpanBatch, SpanId, SpanKind, SpanStatus,
    StatusCode as RayStatusCode, TraceId, TraceStore,
};
use serde_json::Value;

// ---------------------------------------------------------------------
// Fixtures — build the two driven-port fakes and seed them.
// ---------------------------------------------------------------------

/// A span on `trace_byte`*16 / `span_byte`*8, at `secs`, with `status`,
/// carrying `service.name` so the store's by-service index is also exercised.
fn span(secs: u64, trace_byte: u8, span_byte: u8, name: &str, status: SpanStatus) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
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

fn error_status(message: &str) -> SpanStatus {
    SpanStatus {
        code: RayStatusCode::Error,
        message: message.to_string(),
    }
}

fn log_on_trace(observed: u64, body: &str, trace_byte: Option<u8>) -> LogRecord {
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::ERROR,
        severity_text: "ERROR".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: BTreeMap::new(),
        trace_id: trace_byte.map(|b| [b; 16]),
        span_id: None,
    }
}

fn trace_store_with(spans: Vec<Span>, t: &TenantId) -> Arc<dyn TraceStore + Send + Sync> {
    let store = InMemoryTraceStore::new(Box::new(RayNoop));
    store
        .ingest(t, SpanBatch::with_spans(spans))
        .expect("seed trace store");
    Arc::new(store)
}

fn log_store_with(records: Vec<LogRecord>, t: &TenantId) -> Arc<dyn LogStore + Send + Sync> {
    let store = InMemoryLogStore::new(Box::new(LumenNoop));
    store
        .ingest(t, LogBatch::with_records(records))
        .expect("seed log store");
    Arc::new(store)
}

fn with_logs_request(trace_id: &str) -> axum::http::Request<axum::body::Body> {
    let uri = format!("/api/v1/traces/with_logs?trace_id={trace_id}");
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

fn with_logs_request_without_trace_id() -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/traces/with_logs")
        .body(axum::body::Body::empty())
        .expect("build request")
}

fn spans_of(body: &Value) -> &Vec<Value> {
    body["spans"].as_array().expect("spans is a JSON array")
}

fn logs_of(body: &Value) -> &Vec<Value> {
    body["logs"].as_array().expect("logs is a JSON array")
}

// =====================================================================
// (a)+(b) Happy path — a trace with two spans (one Error-status) AND its
// correlated log come back TOGETHER in one response, scoped to that
// trace; logs and spans bearing a DIFFERENT trace_id are excluded.
// =====================================================================

#[tokio::test]
async fn combined_returns_the_trace_spans_and_its_correlated_logs_together() {
    let t = tenant("acme-prod");
    // Trace ab… has two spans (one Error). Trace cd… is a different trace
    // whose span and log must NOT appear in the ab… response.
    let trace_store = trace_store_with(
        vec![
            span(
                100,
                0xAB,
                0x01,
                "place-order",
                error_status("upstream timeout"),
            ),
            span(101, 0xAB, 0x02, "charge-card", SpanStatus::default()),
            span(102, 0xCD, 0x03, "other-trace", SpanStatus::default()),
        ],
        &t,
    );
    let log_store = log_store_with(
        vec![
            log_on_trace(100_000, "card declined", Some(0xAB)),
            log_on_trace(101_000, "unrelated trace", Some(0xCD)),
            log_on_trace(102_000, "no trace id", None),
        ],
        &t,
    );

    let router = trace_query_api::router_with_logs(trace_store, log_store, Some(t));
    let (status, body) = call(
        router,
        with_logs_request("abababababababababababababababab"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "the combined lookup is 200: {body}");
    assert_eq!(
        body["trace_id"].as_str(),
        Some("abababababababababababababababab"),
        "the response echoes the canonical lowercase trace_id: {body}"
    );

    // Both spans of trace ab… are present; the cd… span is excluded.
    let spans = spans_of(&body);
    assert_eq!(
        spans.len(),
        2,
        "both spans of the trace are returned: {body}"
    );
    for s in spans {
        assert_eq!(
            s["trace_id"].as_str(),
            Some("abababababababababababababababab"),
            "every returned span belongs to the requested trace: {s}"
        );
    }
    // The Error-status span survives with its status (status included).
    let error_present = spans.iter().any(|s| {
        s["status"]["code"].as_str() == Some("Error")
            && s["status"]["message"].as_str() == Some("upstream timeout")
    });
    assert!(error_present, "the Error-status span is carried: {body}");

    // Exactly the one correlated log is present; the cd… and the
    // no-trace-id logs are excluded.
    let logs = logs_of(&body);
    assert_eq!(logs.len(), 1, "only the correlated log is returned: {body}");
    assert_eq!(logs[0]["body"].as_str(), Some("card declined"), "{body}");
    assert_eq!(
        logs[0]["trace_id"].as_str(),
        Some("abababababababababababababababab"),
        "the log renders the same trace_id string as the spans: {body}"
    );

    let rendered = body.to_string();
    assert!(
        !rendered.contains("unrelated trace") && !rendered.contains("no trace id"),
        "logs bearing a different (or no) trace_id are excluded: {rendered}"
    );
    assert!(
        !rendered.contains("other-trace"),
        "spans of a different trace are excluded: {rendered}"
    );
}

// =====================================================================
// (e) Unknown trace_id — the calm object with empty spans + empty logs,
// HTTP 200, never a 404 or an error envelope.
// =====================================================================

#[tokio::test]
async fn combined_unknown_trace_id_returns_empty_spans_and_empty_logs() {
    let t = tenant("acme-prod");
    // The stores hold a different trace; the query asks for one never seen.
    let trace_store = trace_store_with(
        vec![span(100, 0xAB, 0x01, "place-order", SpanStatus::default())],
        &t,
    );
    let log_store = log_store_with(vec![log_on_trace(100_000, "card declined", Some(0xAB))], &t);

    let router = trace_query_api::router_with_logs(trace_store, log_store, Some(t));
    let (status, body) = call(
        router,
        with_logs_request("00000000000000000000000000000000"),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "an unknown trace_id is the calm 200 object, never a 404: {body}"
    );
    assert_ne!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body["trace_id"].as_str(),
        Some("00000000000000000000000000000000"),
        "{body}"
    );
    assert!(spans_of(&body).is_empty(), "spans is empty []: {body}");
    assert!(logs_of(&body).is_empty(), "logs is empty []: {body}");
    assert!(
        !is_error_envelope(&body),
        "the empty arm is the object shape, not an error envelope: {body}"
    );
}

// =====================================================================
// (c) Missing trace_id — 400 "invalid trace_id", store never touched.
// =====================================================================

#[tokio::test]
async fn combined_missing_trace_id_returns_400() {
    let t = tenant("acme-prod");
    // Failing stores prove the no-store-call property: a leaked read would
    // lift the response to 500.
    let trace_store: Arc<dyn TraceStore + Send + Sync> = Arc::new(common::FailingTraceStore);
    let log_store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);

    let router = trace_query_api::router_with_logs(trace_store, log_store, Some(t));
    let (status, body) = call(router, with_logs_request_without_trace_id()).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a missing trace_id is a 400, never a 500: {body}"
    );
    assert!(is_error_envelope(&body), "{body}");
    assert_eq!(body["error"].as_str(), Some("invalid trace_id"), "{body}");
}

// =====================================================================
// (d) Malformed trace_id — wrong length / non-hex — 400 with no echo,
// store never touched.
// =====================================================================

#[tokio::test]
async fn combined_malformed_trace_id_returns_400_with_no_echo() {
    let t = tenant("acme-prod");
    // 31 chars (short), 33 chars (long), and a 32-char value whose last
    // character is non-hex. All collapse to the one literal class label.
    let malformed = [
        "0123456789abcdef0123456789abcde",   // 31
        "0123456789abcdef0123456789abcdef0", // 33
        "0123456789abcdef0123456789abcdeg",  // 32, non-hex 'g'
    ];
    for raw in malformed {
        let trace_store: Arc<dyn TraceStore + Send + Sync> = Arc::new(common::FailingTraceStore);
        let log_store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);
        let router = trace_query_api::router_with_logs(trace_store, log_store, Some(t.clone()));
        let (status, body) = call(router, with_logs_request(raw)).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "malformed trace_id {raw:?} is a 400, never a 500: {body}"
        );
        assert!(is_error_envelope(&body), "{body}");
        assert_eq!(
            body["error"].as_str(),
            Some("invalid trace_id"),
            "the single literal class label, no clever diagnostic: {body}"
        );
        assert!(
            !body.to_string().contains(raw),
            "the body must never echo the raw trace_id value: {body}"
        );
    }
}

// =====================================================================
// Uppercase hex — accepted case-insensitively; the response echoes the
// canonical lowercase id (kills a case-sensitive parse mutant).
// =====================================================================

#[tokio::test]
async fn combined_uppercase_trace_id_resolves_and_echoes_lowercase() {
    let t = tenant("acme-prod");
    let trace_store = trace_store_with(
        vec![span(100, 0xAB, 0x01, "place-order", SpanStatus::default())],
        &t,
    );
    let log_store = log_store_with(vec![log_on_trace(100_000, "card declined", Some(0xAB))], &t);

    let router = trace_query_api::router_with_logs(trace_store, log_store, Some(t));
    let (status, body) = call(
        router,
        with_logs_request("ABABABABABABABABABABABABABABABAB"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "uppercase hex is accepted: {body}");
    assert_eq!(
        body["trace_id"].as_str(),
        Some("abababababababababababababababab"),
        "the canonical lowercase id is echoed for an uppercase query: {body}"
    );
    assert_eq!(spans_of(&body).len(), 1, "{body}");
    assert_eq!(logs_of(&body).len(), 1, "{body}");
}

// =====================================================================
// Auth/tenancy — no resolvable tenant is refused (401), identical to the
// existing trace endpoints; the stores are never touched.
// =====================================================================

#[tokio::test]
async fn combined_missing_tenant_returns_401_with_no_store_call() {
    let trace_store: Arc<dyn TraceStore + Send + Sync> = Arc::new(common::FailingTraceStore);
    let log_store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);

    let router = trace_query_api::router_with_logs(trace_store, log_store, None);
    let (status, body) = call(
        router,
        with_logs_request("abababababababababababababababab"),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no resolvable tenant returns 401; the stores are never touched: {body}"
    );
    assert!(is_error_envelope(&body), "{body}");
}

// =====================================================================
// Trace-store failure — 500. Kills a mutant dropping the get_trace error
// arm. The log store is healthy, isolating the trace-store path.
// =====================================================================

#[tokio::test]
async fn combined_trace_store_failure_returns_500() {
    let t = tenant("acme-prod");
    let trace_store: Arc<dyn TraceStore + Send + Sync> = Arc::new(common::FailingTraceStore);
    let log_store = log_store_with(vec![log_on_trace(100_000, "card declined", Some(0xAB))], &t);

    let router = trace_query_api::router_with_logs(trace_store, log_store, Some(t));
    let (status, body) = call(
        router,
        with_logs_request("abababababababababababababababab"),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "an unreadable trace store is a 500: {body}"
    );
    assert!(is_error_envelope(&body), "{body}");
}

// =====================================================================
// Log-store failure — 500. Kills a mutant dropping the logs_for_trace
// error arm. The trace store is healthy, isolating the log-store path.
// =====================================================================

#[tokio::test]
async fn combined_log_store_failure_returns_500() {
    let t = tenant("acme-prod");
    let trace_store = trace_store_with(
        vec![span(100, 0xAB, 0x01, "place-order", SpanStatus::default())],
        &t,
    );
    let log_store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);

    let router = trace_query_api::router_with_logs(trace_store, log_store, Some(t));
    let (status, body) = call(
        router,
        with_logs_request("abababababababababababababababab"),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "an unreadable log store is a 500: {body}"
    );
    assert!(is_error_envelope(&body), "{body}");
}

// ---------------------------------------------------------------------
// A log-store double that LIES on every read, for the 500 arm and the
// no-store-call proofs (a leaked read lifts a 400/401 to 500).
// ---------------------------------------------------------------------

struct FailingLogStore;

impl LogStore for FailingLogStore {
    fn ingest(&self, _tenant: &TenantId, _batch: LogBatch) -> Result<LumenReceipt, LogStoreError> {
        Err(LogStoreError::PersistenceFailed {
            reason: "ingest disabled in read service".to_string(),
        })
    }

    fn query(
        &self,
        _tenant: &TenantId,
        _range: LumenTimeRange,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        Err(LogStoreError::PersistenceFailed {
            reason: "backing log store unreadable".to_string(),
        })
    }

    fn query_with(
        &self,
        _tenant: &TenantId,
        _range: LumenTimeRange,
        _predicate: &LumenPredicate,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        Err(LogStoreError::PersistenceFailed {
            reason: "backing log store unreadable".to_string(),
        })
    }
}
