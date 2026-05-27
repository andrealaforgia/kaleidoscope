// Kaleidoscope log-query-api — slice 02 honest-read-caps acceptance suite
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
//! cap (100_000 log records) on `/api/v1/logs`.
//!
//! Maps to `docs/feature/honest-read-caps-v0/discuss/user-stories.md`
//! (US-02 window cap on log-query-api, US-04 result cap on
//! log-query-api, US-05 redaction on cap reasons for logs). Contract
//! pinned by ADR-0050 (`docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`).
//!
//! The user-centric outcome: when the on-call SRE (or a misconfigured
//! script) asks for too wide a window or would receive too many log
//! records, the log read API refuses with a named 400 carrying the
//! existing `{status:"error", error:"..."}` envelope. The refusal is
//! OUT LOUD, never silently truncated, never `X-Truncated`, never a
//! calm empty 200. The store is never touched on the window-cap
//! refusal path; the store is touched exactly once on the result-cap
//! refusal path (the handler must know the size to refuse) but
//! serialisation is never attempted.
//!
//! Boundary discipline (ADR-0050 Decision 1 and 2): the boundary is
//! `>`, never `>=`. A window of exactly `MAX_WINDOW_SECONDS` (86_400)
//! is served; one second wider is refused. A result of exactly
//! `MAX_RESULT_ROWS` (100_000) is served; one record more is refused.
//!
//! Redaction posture (ADR-0050 Decision 7): the cap-400 body inherits
//! the log-query-api posture from ADR-0047 Decision 1. The body must
//! never echo the raw window values nor a forwarded `Authorization` /
//! "SECRET" / "Bearer" value.
//!
//! RED state (behavioural, not compile-level): every scenario in this
//! file COMPILES against the current `log-query-api` source. The
//! scenarios that expect a 400 fail today because no cap check exists
//! in the handler; the scenarios that expect a 200 pass today because
//! the request happens to be within an unenforced cap. The DELIVER
//! wave adds the two `pub const` and the two `if` arms per ADR-0050,
//! at which point every scenario goes green.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, logs_request, logs_request_with_auth, open_durable_store, record,
    records_array, seed, tenant, FailingLogStore,
};
use lumen::{
    IngestReceipt, LogBatch, LogRecord, LogStore, LogStoreError, Predicate, SeverityNumber,
    TimeRange,
};

// ---------------------------------------------------------------------
// A driven-port test double that returns a configurable, large `Vec`
// of synthetic log records from `query`. Used to exercise the
// result-size cap arm without seeding the durable store with 100_000+
// records, which would be slow and memory-heavy in CI. The cap fires
// AFTER the store query, so the store IS touched here (a clean 400
// still proves the cap fired at the right seam: serialisation never
// starts because the body is `{status:"error", ...}`, not a JSON array
// of LogRecord).
//
// The double is a test adapter for the `lumen::LogStore` driven port,
// not an internal log-query-api component, so driving the router
// through `router(...)` over it still honours the hexagonal boundary.
// ---------------------------------------------------------------------

/// A store whose `query` returns exactly `count` synthetic log
/// records for any tenant and range. Ingest is disabled (this is a
/// read service). `query_with` mirrors `query`.
struct BulkLogStore {
    count: usize,
}

impl BulkLogStore {
    fn new(count: usize) -> Self {
        Self { count }
    }
}

fn synthetic_record(seq: usize) -> LogRecord {
    let mut resource = std::collections::BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        // Ascending timestamps so the store contract's "ascending
        // observed_time order" is honoured even on the synthetic feed.
        observed_time_unix_nano: 1_716_200_000_000_000_000 + (seq as u64),
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: "synthetic".to_string(),
        attributes: std::collections::BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

impl LogStore for BulkLogStore {
    fn ingest(
        &self,
        _tenant: &aegis::TenantId,
        _batch: LogBatch,
    ) -> Result<IngestReceipt, LogStoreError> {
        Err(LogStoreError::PersistenceFailed {
            reason: "ingest disabled in read service".to_string(),
        })
    }

    fn query(
        &self,
        _tenant: &aegis::TenantId,
        _range: TimeRange,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        Ok((0..self.count).map(synthetic_record).collect())
    }

    fn query_with(
        &self,
        _tenant: &aegis::TenantId,
        _range: TimeRange,
        _predicate: &Predicate,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        Ok((0..self.count).map(synthetic_record).collect())
    }
}

// =====================================================================
// US-02 — Happy path: a window WITHIN the cap is served normally
// =====================================================================

/// @driving_port @real-io @adapter-integration @US-02
///
/// Given tenant "acme-prod" has one in-window log record seeded into a
/// real durable Lumen store,
/// When the on-call SRE queries the logs endpoint over a 60-second
/// window (well within the 86_400-second cap),
/// Then she sees the matching record in a bare JSON array. The cap is
/// invisible on well-formed queries.
#[tokio::test]
async fn a_logs_request_within_the_window_cap_is_served_normally() {
    let (store, _base) = open_durable_store("within-window-cap");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![record(1_716_200_005, "checkout", "checkout: started")],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        records_array(&body).len(),
        1,
        "the one matching in-window record is returned"
    );
}

// =====================================================================
// US-02 — Boundary: window of EXACTLY MAX_WINDOW_SECONDS is served
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given a window of exactly 86_400 seconds (start=0, end=86_400) and
/// a real durable store with no records inside that window,
/// When the on-call SRE queries the logs endpoint over that exact-cap
/// window,
/// Then the response is 200 with the calm empty bare array. The
/// boundary is inclusive: `end - start == MAX_WINDOW_SECONDS` is
/// served (NOT refused).
///
/// This kills a `>` -> `>=` mutant on the window cap check.
#[tokio::test]
async fn a_logs_request_at_exactly_the_window_cap_is_served() {
    let (store, _base) = open_durable_store("at-window-cap");
    let t = tenant("acme-prod");
    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("0", "86400");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a window of exactly 86_400 seconds is at the cap and served, not refused"
    );
    assert!(
        records_array(&body).is_empty(),
        "the empty window is the calm empty bare array: {body}"
    );
}

// =====================================================================
// US-02 — Window over the cap is refused BEFORE the store is touched
// =====================================================================

/// @driving_port @US-02
///
/// Given a window of 86_401 seconds (start=0, end=86401), one second
/// over the cap, and a LyingLogStore whose `query` always returns
/// `PersistenceFailed`,
/// When the on-call SRE queries the logs endpoint over that window,
/// Then the response is 400 with the `{status:"error", error:...}`
/// envelope, the `error` string names "window" and a value-of-cap
/// substring like "exceeds 86400", and the LyingLogStore's `query` was
/// NEVER called (proven by the absence of the 500 that would lift if
/// the lying store had been touched).
///
/// This is the carpaccio taste-test 1 from ADR-0050: the cap fires
/// BEFORE the store. A mutant that swapped check and store would lift
/// the response to a 500 and fail this scenario.
#[tokio::test]
async fn a_window_one_second_over_the_cap_is_refused_before_the_store() {
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request("0", "86401");
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
// US-04 — Boundary: result of EXACTLY MAX_RESULT_ROWS is served
// =====================================================================

/// @driving_port @US-04
///
/// Given a store that returns exactly 100_000 synthetic log records
/// for an in-window query,
/// When the on-call SRE queries the logs endpoint over a within-cap
/// window,
/// Then the response is 200 and carries all 100_000 records. The
/// boundary is inclusive: `records.len() == MAX_RESULT_ROWS` is served
/// (NOT refused).
///
/// This kills a `>` -> `>=` mutant on the result-cap check.
#[tokio::test]
async fn a_result_at_exactly_the_result_cap_is_served() {
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(BulkLogStore::new(100_000));
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a result of exactly 100_000 records is at the cap and served, not refused"
    );
    assert_eq!(
        records_array(&body).len(),
        100_000,
        "all 100_000 records are returned at the boundary"
    );
}

// =====================================================================
// US-04 — A result one record OVER the cap is refused, never truncated
// =====================================================================

/// @driving_port @US-04
///
/// Given a store that returns 100_001 synthetic log records for an
/// in-window query,
/// When the on-call SRE queries the logs endpoint over a within-cap
/// window,
/// Then the response is 400 with `{status:"error", error:...}`, the
/// error names "result" and the cap value, and the response is NEVER
/// a truncated 200, NEVER an `X-Truncated` 200, NEVER a calm empty.
///
/// The store IS touched here (the cap fires AFTER the store; the
/// handler must know the size to refuse) but serialisation is never
/// attempted: the body is the error envelope, not a JSON array.
#[tokio::test]
async fn a_result_one_record_over_the_cap_is_refused_with_a_named_400() {
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(BulkLogStore::new(100_001));
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request("1716200000", "1716200060");
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
        "the refusal must never look like a bare JSON array of records: {body}"
    );
}

// =====================================================================
// US-05 — Redaction: cap-refused body echoes no raw window or credential
// =====================================================================

/// @driving_port @US-05
///
/// Given the operator's request carries a forwarded
/// `Authorization: Bearer SECRET` header AND a year-long window,
/// When the log read API returns the window-cap 400,
/// Then the response body contains NONE of:
///   - the raw end value ("31536000"),
///   - the literal "SECRET",
///   - the literal "Bearer".
///
/// Mirrors the existing redaction posture (ADR-0047 Decision 1)
/// inherited by the new cap reason (ADR-0050 Decision 7).
#[tokio::test]
async fn the_cap_refused_body_never_echoes_raw_values_or_a_credential() {
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request_with_auth("0", "31536000", "Bearer SECRET");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let rendered = body.to_string();
    assert!(
        !rendered.contains("31536000"),
        "the body must not echo the raw end value: {rendered}"
    );
    assert!(
        !rendered.contains("SECRET"),
        "the body must not contain SECRET (forwarded credential): {rendered}"
    );
    assert!(
        !rendered.contains("Bearer"),
        "the body must not contain Bearer (forwarded credential): {rendered}"
    );
}
