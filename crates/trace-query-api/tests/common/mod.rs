// Kaleidoscope trace-query-api — slice 01 acceptance test helpers
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

//! Shared seeding and request helpers for the slice 01 traces-read suite.
//!
//! These tests drive trace-query-api through its single public driving
//! port (`trace_query_api::router`) using `tower::ServiceExt::oneshot`
//! against the axum `Router`, with no network port bound. This mirrors
//! the proven log-query-api posture (ADR-0047), itself mirroring
//! query-api (ADR-0042). The lib hands back a Router built over a
//! `TraceStore` and a resolved tenant, so the acceptance tests exercise
//! the real handler, real bounds parsing, real required-`service`
//! handling, and real tenant scoping without spawning a server.
//!
//! ASSUMED trace-query-api surface (DELIVER MUST match this, or update
//! these tests in the same slice-01 commit):
//!
//! ```ignore
//! // The single driving port. `tenant: Option<TenantId>` models
//! // fail-closed tenancy AT THE ROUTER SEAM: `Some(t)` is a resolved
//! // tenant; `None` is "no tenant resolvable" and every request is
//! // refused (401). The production binary maps
//! // KALEIDOSCOPE_TRACE_QUERY_TENANT onto this same Option.
//! pub fn router(
//!     store: std::sync::Arc<dyn ray::TraceStore + Send + Sync>,
//!     tenant: Option<aegis::TenantId>,
//! ) -> axum::Router;
//! ```

// Shared integration-test helper module: each `tests/*.rs` file compiles as
// its own crate that includes this module, so a helper unused by one slice
// (e.g. the ephemeral-bind read-auth slice, which drives a real `reqwest`
// GET rather than `oneshot`/`call`) is dead code in that target only. The
// module-level allow keeps the shared helpers clippy-clean across every
// slice that includes them.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use ray::{
    FileBackedTraceStore, IngestReceipt, NoopRecorder, Predicate, ServiceName, Span, SpanBatch,
    SpanEvent, SpanId, SpanKind, SpanLink, SpanStatus, StatusCode as RayStatusCode, TimeRange,
    TraceId, TraceStore, TraceStoreError,
};
use serde_json::Value;
use tower::ServiceExt; // for `oneshot`

/// Tenant constructor in the platform's aegis vocabulary.
pub fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

/// Convert whole seconds to the nanoseconds ray stores. Used only to
/// place seed spans at exact second boundaries so the half-open range
/// assertions are crisp.
pub fn secs_to_nanos(seconds: u64) -> u64 {
    seconds * 1_000_000_000
}

/// A fixed `TraceId` derived from a single byte, repeated 16 times, for
/// stable deterministic IDs in the seed batches.
pub fn trace_id(byte: u8) -> TraceId {
    TraceId([byte; 16])
}

/// A fixed `SpanId` derived from a single byte, repeated 8 times, for
/// stable deterministic IDs in the seed batches.
pub fn span_id(byte: u8) -> SpanId {
    SpanId([byte; 8])
}

/// A minimal Server-kind span carrying `service.name` in its resource
/// attributes, starting at the given second. Slice 01 only.
#[allow(dead_code)]
pub fn span(observed_secs: u64, service: &str, name: &str) -> Span {
    span_with_ids(observed_secs, service, name, 0xAA, observed_secs as u8)
}

/// As `span` but with explicit trace/span byte selectors so multiple
/// distinct spans can share or diverge on identity as the scenario
/// requires.
pub fn span_with_ids(
    observed_secs: u64,
    service: &str,
    name: &str,
    trace_byte: u8,
    span_byte: u8,
) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    let start = secs_to_nanos(observed_secs);
    Span {
        trace_id: trace_id(trace_byte),
        span_id: span_id(span_byte),
        parent_span_id: None,
        name: name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 120_000_000,
        status: SpanStatus::default(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

/// A span placed at an EXACT nanosecond instant (not a whole second),
/// for the half-open boundary arms where a span must sit precisely at
/// `start` or `end`. Slice 01 only.
#[allow(dead_code)]
pub fn span_at_nanos(start_nanos: u64, service: &str, name: &str) -> Span {
    let mut s = span(0, service, name);
    s.start_time_unix_nano = start_nanos;
    s.end_time_unix_nano = start_nanos + 120_000_000;
    s
}

/// A fully-populated span exercising every `Span` field for the
/// field-fidelity arm: name, kind, status (Error + message), span
/// attributes, resource attributes, a populated `parent_span_id`, one
/// event, and one link. Slice 01 only.
#[allow(dead_code)]
pub fn rich_span(observed_secs: u64) -> Span {
    let start = secs_to_nanos(observed_secs);
    let mut attributes = BTreeMap::new();
    attributes.insert("http.route".to_string(), "/orders".to_string());
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    let mut event_attrs = BTreeMap::new();
    event_attrs.insert("exception.type".to_string(), "TimeoutError".to_string());
    let event = SpanEvent {
        time_unix_nano: start + 60_000_000,
        name: "exception".to_string(),
        attributes: event_attrs,
    };
    let mut link_attrs = BTreeMap::new();
    link_attrs.insert("link.kind".to_string(), "follows-from".to_string());
    let link = SpanLink {
        trace_id: trace_id(0xCC),
        span_id: span_id(0xDD),
        attributes: link_attrs,
    };
    Span {
        trace_id: trace_id(0xAA),
        span_id: span_id(0x01),
        parent_span_id: Some(span_id(0x02)),
        name: "place-order".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 120_000_000,
        status: SpanStatus {
            code: RayStatusCode::Error,
            message: "upstream timeout".to_string(),
        },
        attributes,
        resource_attributes: resource,
        events: vec![event],
        links: vec![link],
    }
}

/// Open a fresh durable ray store in a unique tempdir. The walking
/// skeleton and the tenant-isolation arms seed REAL durable storage
/// (real filesystem I/O through `FileBackedTraceStore`, the same
/// adapter the aperture trace path writes through) so the read loop is
/// proven against the real adapter, not an in-memory stand-in.
pub fn open_durable_store(label: &str) -> (Arc<FileBackedTraceStore>, std::path::PathBuf) {
    let mut base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    base.push(format!(
        "trace-query-api-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&base).expect("mkdir");
    base.push("store");
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open store");
    (Arc::new(store), base)
}

/// Seed a batch of spans for a tenant into a real durable store.
pub fn seed(store: &Arc<FileBackedTraceStore>, t: &TenantId, spans: Vec<Span>) {
    store
        .ingest(t, SpanBatch::with_spans(spans))
        .expect("seed durable store");
}

/// Build the GET request the contract pins:
/// `/api/v1/traces?service=&start=&end=` with `start`/`end` in epoch
/// seconds.
#[allow(dead_code)]
pub fn traces_request(service: &str, start: &str, end: &str) -> Request<Body> {
    let uri = format!("/api/v1/traces?service={service}&start={start}&end={end}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// Build a GET request that omits the `service` parameter entirely.
/// Slice 01's structural divergence from logs: the handler must return
/// 400 before touching the store.
#[allow(dead_code)]
pub fn traces_request_without_service(start: &str, end: &str) -> Request<Body> {
    let uri = format!("/api/v1/traces?start={start}&end={end}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// Build a request that carries a forwarded Authorization header, for
/// the redaction arm: the error text must never echo the secret.
#[allow(dead_code)]
pub fn traces_request_with_auth(
    service: &str,
    start: &str,
    end: &str,
    authorization: &str,
) -> Request<Body> {
    let uri = format!("/api/v1/traces?service={service}&start={start}&end={end}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", authorization)
        .body(Body::empty())
        .expect("build request")
}

/// Drive the router with a single request and return the HTTP status
/// plus the parsed JSON body. The one place that touches transport
/// mechanics, keeping the scenario bodies in business terms.
pub async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.expect("router responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let json: Value = serde_json::from_slice(&bytes).expect("body is JSON");
    (status, json)
}

/// The spans array of a success body. The success arm is a BARE JSON
/// array of `Span`s (ADR-0048 Decision 2), not a wrapped envelope.
pub fn spans_array(body: &Value) -> &Vec<Value> {
    body.as_array()
        .expect("success body is a bare JSON array of spans")
}

/// True when the body is the error envelope
/// `{status:"error", error:<str>}` (the error arm reuses the sibling
/// endpoints' shape exactly).
pub fn is_error_envelope(body: &Value) -> bool {
    body.get("status").and_then(Value::as_str) == Some("error")
        && body.get("error").and_then(Value::as_str).is_some()
}

/// The `name` strings of the returned spans, in order, for terse
/// assertions about which spans survived a window. Slice 01 only.
#[allow(dead_code)]
pub fn span_names(body: &Value) -> Vec<String> {
    spans_array(body)
        .iter()
        .filter_map(|s| s["name"].as_str().map(str::to_string))
        .collect()
}

/// The `start_time_unix_nano` of each returned span, in order.
/// Slice 01 only.
#[allow(dead_code)]
pub fn start_times(body: &Value) -> Vec<u64> {
    spans_array(body)
        .iter()
        .filter_map(|s| s["start_time_unix_nano"].as_u64())
        .collect()
}

// --------------------------------------------------------------------
// A driven-port test double that LIES: ingest, query, get_trace, and
// query_with all fail with PersistenceFailed, modelling a durable store
// that opened cleanly but cannot be read mid-flight. The real
// FileBackedTraceStore is hard to make fail on demand, and the
// InMemoryTraceStore never returns PersistenceFailed (store.rs:39), so
// the 500 arm is exercised with this double. It is a test adapter for
// the `TraceStore` driven port, not an internal trace-query-api
// component, so driving the router over it still honours the hexagonal
// boundary (Mandate 1).
//
// CRITICAL: the same double is also used to PROVE the 400 paths
// (missing or empty `service`, malformed window, inverted window) never
// reach the store. If those handlers wrongly touched the store, the
// failing double would lift the response to a 500. A clean 400 therefore
// proves the no-store-query property the contract pins (ADR-0048
// Decision 3).
// --------------------------------------------------------------------

/// A store whose `query` (and every other read/write) always returns
/// `PersistenceFailed`.
pub struct FailingTraceStore;

impl TraceStore for FailingTraceStore {
    fn ingest(
        &self,
        _tenant: &TenantId,
        _batch: SpanBatch,
    ) -> Result<IngestReceipt, TraceStoreError> {
        Err(TraceStoreError::PersistenceFailed {
            reason: "ingest disabled in read service".to_string(),
        })
    }

    fn get_trace(
        &self,
        _tenant: &TenantId,
        _trace_id: &TraceId,
    ) -> Result<Vec<Span>, TraceStoreError> {
        Err(TraceStoreError::PersistenceFailed {
            reason: "backing trace store unreadable".to_string(),
        })
    }

    fn query(
        &self,
        _tenant: &TenantId,
        _service: &ServiceName,
        _range: TimeRange,
    ) -> Result<Vec<Span>, TraceStoreError> {
        Err(TraceStoreError::PersistenceFailed {
            reason: "backing trace store unreadable".to_string(),
        })
    }

    fn query_with(
        &self,
        _tenant: &TenantId,
        _service: &ServiceName,
        _range: TimeRange,
        _predicate: &Predicate,
    ) -> Result<Vec<Span>, TraceStoreError> {
        Err(TraceStoreError::PersistenceFailed {
            reason: "backing trace store unreadable".to_string(),
        })
    }
}
