// Kaleidoscope log-query-api — slice 01 acceptance test helpers
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

//! Shared seeding and request helpers for the slice 01 logs-read suite.
//!
//! These tests drive log-query-api through its single public driving port
//! (`log_query_api::router`) using `tower::ServiceExt::oneshot` against
//! the axum `Router`, with no network port bound. This mirrors the proven
//! query-api posture (ADR-0042 Decision 1, reproduced for logs by
//! ADR-0047): the lib hands back a Router built over a `LogStore` and a
//! resolved tenant, so the acceptance tests exercise the real handler,
//! real bounds parsing, and real tenant scoping without spawning a
//! server. The composition root (the thin binary, with its env-var tenant
//! resolution and the Earned-Trust probe) is a separate concern the
//! crafter covers at the binary boundary.
//!
//! ASSUMED log-query-api surface (the crafter MUST match this, or update
//! these tests in the same slice-01 commit):
//!
//! ```ignore
//! // The single driving port. `tenant: Option<TenantId>` models
//! // fail-closed tenancy AT THE ROUTER SEAM: `Some(t)` is a resolved
//! // tenant; `None` is "no tenant resolvable" and every request is
//! // refused (401). The production binary maps
//! // KALEIDOSCOPE_LOG_QUERY_TENANT onto this same Option.
//! pub fn router(
//!     store: std::sync::Arc<dyn lumen::LogStore + Send + Sync>,
//!     tenant: Option<aegis::TenantId>,
//! ) -> axum::Router;
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use lumen::{
    FileBackedLogStore, IngestReceipt, LogBatch, LogRecord, LogStore, LogStoreError, NoopRecorder,
    Predicate, SeverityNumber, TimeRange,
};
use serde_json::Value;
use tower::ServiceExt; // for `oneshot`

/// Tenant constructor in the platform's aegis vocabulary.
pub fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

/// Convert whole seconds to the nanoseconds Lumen stores. Used only to
/// place seed records at exact second boundaries so the half-open range
/// assertions are crisp.
pub fn secs_to_nanos(seconds: u64) -> u64 {
    seconds * 1_000_000_000
}

/// A minimal INFO-level record carrying a `service.name` resource
/// attribute, observed at the given nanosecond timestamp.
pub fn record(observed_secs: u64, service: &str, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    LogRecord {
        observed_time_unix_nano: secs_to_nanos(observed_secs),
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

/// A record placed at an EXACT nanosecond instant (not a whole second),
/// for the half-open boundary arms where a record must sit precisely at
/// `start` or `end`. Slice 01 only.
#[allow(dead_code)]
pub fn record_at_nanos(observed_nanos: u64, service: &str, body: &str) -> LogRecord {
    let mut r = record(0, service, body);
    r.observed_time_unix_nano = observed_nanos;
    r
}

/// A fully-populated record exercising every `LogRecord` field, for the
/// field-fidelity arm: a chosen severity number and text, record
/// attributes, resource attributes, and a populated trace id and span id.
/// Slice 01 only.
#[allow(dead_code)]
pub fn rich_record(observed_secs: u64) -> LogRecord {
    let mut attributes = BTreeMap::new();
    attributes.insert("http.status_code".to_string(), "503".to_string());
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: secs_to_nanos(observed_secs),
        severity_number: SeverityNumber::ERROR,
        severity_text: "ERROR".to_string(),
        body: "db pool exhausted".to_string(),
        attributes,
        resource_attributes: resource,
        trace_id: Some([
            0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19,
        ]),
        span_id: Some([0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21]),
    }
}

/// Open a fresh durable Lumen store in a unique tempdir. The walking
/// skeleton and the tenant-isolation arms seed REAL durable storage (real
/// filesystem I/O through `FileBackedLogStore`, the same adapter the
/// gateway writes through) so the read loop is proven against the real
/// adapter, not an in-memory stand-in.
pub fn open_durable_store(label: &str) -> (Arc<FileBackedLogStore>, std::path::PathBuf) {
    let mut base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    base.push(format!(
        "log-query-api-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&base).expect("mkdir");
    base.push("store");
    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open store");
    (Arc::new(store), base)
}

/// Seed a batch of records for a tenant into a real durable store.
pub fn seed(store: &Arc<FileBackedLogStore>, t: &TenantId, records: Vec<LogRecord>) {
    store
        .ingest(t, LogBatch::with_records(records))
        .expect("seed durable store");
}

/// Build the GET request the contract pins: `/api/v1/logs?start=&end=`
/// with `start`/`end` in epoch seconds.
pub fn logs_request(start: &str, end: &str) -> Request<Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// Build a request that also carries a forwarded Authorization header, for
/// the redaction arm: the error text must never echo the secret.
pub fn logs_request_with_auth(start: &str, end: &str, authorization: &str) -> Request<Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", authorization)
        .body(Body::empty())
        .expect("build request")
}

/// Drive the router with a single request and return the HTTP status plus
/// the parsed JSON body. The one place that touches transport mechanics,
/// keeping the scenario bodies in business terms.
pub async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.expect("router responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let json: Value = serde_json::from_slice(&bytes).expect("body is JSON");
    (status, json)
}

/// The records array of a success body. The success arm is a BARE JSON
/// array of `LogRecord`s (ADR-0047 Decision 1), not a wrapped envelope.
pub fn records_array(body: &Value) -> &Vec<Value> {
    body.as_array()
        .expect("success body is a bare JSON array of records")
}

/// True when the body is the error envelope `{status:"error", error:<str>}`
/// (the error arm borrows the metrics endpoint's shape exactly).
pub fn is_error_envelope(body: &Value) -> bool {
    body.get("status").and_then(Value::as_str) == Some("error")
        && body.get("error").and_then(Value::as_str).is_some()
}

/// The `body` strings of the returned records, in order, for terse
/// assertions about which records survived a window. Slice 01 only.
#[allow(dead_code)]
pub fn record_bodies(body: &Value) -> Vec<String> {
    records_array(body)
        .iter()
        .filter_map(|r| r["body"].as_str().map(str::to_string))
        .collect()
}

/// The `observed_time_unix_nano` of each returned record, in order.
/// Slice 01 only.
#[allow(dead_code)]
pub fn observed_times(body: &Value) -> Vec<u64> {
    records_array(body)
        .iter()
        .filter_map(|r| r["observed_time_unix_nano"].as_u64())
        .collect()
}

// --------------------------------------------------------------------
// A driven-port test double that LIES: ingest and query both fail with
// PersistenceFailed, modelling a durable store that opened cleanly but
// cannot be read mid-flight. The real FileBackedLogStore is hard to make
// fail on demand, and the InMemoryLogStore never returns PersistenceFailed
// (store.rs:49), so the 500 arm is exercised with this double. It is a
// test adapter for the `LogStore` driven port, not an internal
// log-query-api component, so driving the router over it still honours the
// hexagonal boundary (Mandate 1).
// --------------------------------------------------------------------

/// A store whose `query` always returns `PersistenceFailed`.
pub struct FailingLogStore;

impl LogStore for FailingLogStore {
    fn ingest(&self, _tenant: &TenantId, _batch: LogBatch) -> Result<IngestReceipt, LogStoreError> {
        Err(LogStoreError::PersistenceFailed {
            reason: "ingest disabled in read service".to_string(),
        })
    }

    fn query(
        &self,
        _tenant: &TenantId,
        _range: TimeRange,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        Err(LogStoreError::PersistenceFailed {
            reason: "backing log store unreadable".to_string(),
        })
    }

    fn query_with(
        &self,
        _tenant: &TenantId,
        _range: TimeRange,
        _predicate: &Predicate,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        Err(LogStoreError::PersistenceFailed {
            reason: "backing log store unreadable".to_string(),
        })
    }
}
