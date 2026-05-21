// Kaleidoscope query-api — slice 01 acceptance test helpers
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

//! Shared seeding and request helpers for the slice 01 acceptance suite.
//!
//! These tests drive query-api through its single public driving port
//! (`query_api::router`) using `tower::ServiceExt::oneshot` against the
//! axum `Router`, with no network port bound. This matches the design's
//! testability seam (DD1 / ADR-0042 Decision 1): the lib hands back a
//! Router built over a `MetricStore` and a resolved tenant, so the
//! acceptance tests exercise the real handler, real selector, real
//! matrix translation, and real tenant scoping without spawning a
//! server. The composition root (the thin binary, with its env-var
//! tenant resolution and the Earned-Trust probe) is a separate concern
//! the crafter covers at the binary boundary.
//!
//! ASSUMED query-api surface (the crafter MUST match this, or update
//! these tests in the same slice-01 commit):
//!
//! ```ignore
//! // The single driving port. `tenant: Option<TenantId>` models
//! // fail-closed tenancy AT THE ROUTER SEAM: `Some(t)` is a resolved
//! // tenant; `None` is "no tenant resolvable" and every request is
//! // refused. The production binary maps KALEIDOSCOPE_QUERY_TENANT
//! // (set/non-empty -> Some, unset/empty -> None) onto this same
//! // Option, so the fail-closed behaviour is identical in tests and
//! // in production without any process-global env mutation here.
//! pub fn router(
//!     store: std::sync::Arc<dyn pulse::MetricStore + Send + Sync>,
//!     tenant: Option<aegis::TenantId>,
//! ) -> axum::Router;
//! ```
//!
//! If the crafter prefers a builder (`QueryApi::new(store,
//! tenant).router()`), keep the same two inputs and the same
//! `Option<TenantId>` fail-closed semantics; only the call sites in
//! these helpers change.

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    MetricStoreError, NoopRecorder, TimeRange,
};
use serde_json::Value;
use tower::ServiceExt; // for `oneshot`

/// Tenant constructor in the platform's aegis vocabulary.
pub fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

/// One gauge point with optional point-level attributes.
pub fn point(time_unix_nano: u64, value: f64, attrs: &[(&str, &str)]) -> MetricPoint {
    let mut attributes = BTreeMap::new();
    for (k, v) in attrs {
        attributes.insert((*k).to_string(), (*v).to_string());
    }
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes,
        value,
    }
}

/// A gauge metric carrying one resource attribute `service.name`.
pub fn gauge(name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(name),
        description: "acceptance gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

/// Open a fresh durable Pulse store in a unique tempdir. The walking
/// skeleton and the tenant-isolation arms seed REAL durable storage
/// (real filesystem I/O through `FileBackedMetricStore`) so the read
/// loop is proven against the same adapter the gateway writes through,
/// not an in-memory stand-in.
pub fn open_durable_store(label: &str) -> (Arc<FileBackedMetricStore>, std::path::PathBuf) {
    let mut base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    base.push(format!("query-api-{label}-{}-{nanos}", std::process::id()));
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open store");
    (Arc::new(store), base)
}

/// Convert seconds to the nanoseconds Pulse stores. Mirrors the
/// conversion the handler performs; used only to place seed points at
/// exact second boundaries so the half-open range assertions are crisp.
pub fn secs_to_nanos(seconds: u64) -> u64 {
    seconds * 1_000_000_000
}

/// Build the GET request the contract pins, with the four query
/// parameters in the order Prism's `buildUrl` emits them.
pub fn query_range_request(query: &str, start: &str, end: &str) -> Request<Body> {
    let encoded_query = encode(query);
    let uri = format!("/api/v1/query_range?query={encoded_query}&start={start}&end={end}&step=15s");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// Build a request that also carries a forwarded Authorization header,
/// for the redaction arm (the error text must never echo the secret).
/// Used by the slice_01 redaction arm; the slice_02 binary compiles
/// `common` too but does not need it, so allow dead code per-binary.
#[allow(dead_code)]
pub fn query_range_request_with_auth(
    query: &str,
    start: &str,
    end: &str,
    authorization: &str,
) -> Request<Body> {
    let encoded_query = encode(query);
    let uri = format!("/api/v1/query_range?query={encoded_query}&start={start}&end={end}&step=15s");
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", authorization)
        .body(Body::empty())
        .expect("build request")
}

/// Minimal percent-encoding for the query parameter so selectors that
/// contain spaces, braces, brackets, parentheses, and slashes survive
/// the URI builder intact. Only the bytes that actually need escaping
/// for these tests are encoded.
fn encode(raw: &str) -> String {
    let mut out = String::new();
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b':' | b'-' | b'.' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Drive the router with a single request and return the HTTP status
/// plus the parsed JSON body. This is the one place that touches the
/// transport mechanics, keeping the scenario bodies in business terms.
pub async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.expect("router responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let json: Value = serde_json::from_slice(&bytes).expect("body is JSON");
    (status, json)
}

// --------------------------------------------------------------------
// Prism contract validators, reproduced verbatim from
// apps/prism/src/lib/promql/queryRange.ts so the acceptance suite
// asserts the EXACT shape Prism's client will accept. If either of
// these returns false, Prism would not render the response, which is
// the real failure these tests guard against.
// --------------------------------------------------------------------

/// Mirror of Prism's `isPromSuccess`: status === 'success' AND
/// Array.isArray(data.result).
pub fn prism_accepts_success(body: &Value) -> bool {
    body.get("status").and_then(Value::as_str) == Some("success")
        && body
            .get("data")
            .and_then(|d| d.get("result"))
            .map(Value::is_array)
            == Some(true)
}

/// Mirror of Prism's `isPromError`: status === 'error' AND typeof
/// error === 'string'. Used by the slice_01 error arms; the slice_02
/// binary compiles `common` too but does not need it, so allow dead
/// code per-binary.
#[allow(dead_code)]
pub fn prism_accepts_error(body: &Value) -> bool {
    body.get("status").and_then(Value::as_str) == Some("error")
        && body.get("error").and_then(Value::as_str).is_some()
}

/// Convenience: the `data.result` array of a success body.
pub fn result_series(body: &Value) -> &Vec<Value> {
    body.get("data")
        .and_then(|d| d.get("result"))
        .and_then(Value::as_array)
        .expect("success body has data.result array")
}

// --------------------------------------------------------------------
// A driven-port test double that LIES: open succeeds, read fails. Used
// for the persistence-failure (5xx) arm. This is a test adapter for the
// `MetricStore` driven port, not an internal query-api component, so
// driving query-api through `router(...)` over it still honours the
// hexagonal boundary (Mandate 1).
// --------------------------------------------------------------------

/// A store whose `query` always returns `PersistenceFailed`, modelling
/// a durable store that opened cleanly but cannot be read mid-flight.
pub struct FailingStore;

impl MetricStore for FailingStore {
    fn ingest(
        &self,
        _tenant: &TenantId,
        _batch: MetricBatch,
    ) -> Result<pulse::IngestReceipt, MetricStoreError> {
        Err(MetricStoreError::PersistenceFailed {
            reason: "ingest disabled in read service".to_string(),
        })
    }

    fn query(
        &self,
        _tenant: &TenantId,
        _metric_name: &MetricName,
        _range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        Err(MetricStoreError::PersistenceFailed {
            reason: "backing store unreadable".to_string(),
        })
    }

    fn query_with(
        &self,
        _tenant: &TenantId,
        _metric_name: &MetricName,
        _range: TimeRange,
        _predicate: &pulse::Predicate,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        Err(MetricStoreError::PersistenceFailed {
            reason: "backing store unreadable".to_string(),
        })
    }
}
