// Kaleidoscope log-query-api — slice 03 severity-filter acceptance suite
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

//! Severity filter on the logs read path — `?min_severity=` drops
//! records below the requested OTel floor BEFORE the result cap.
//!
//! Maps to `docs/feature/log-query-severity-filter-v0/discuss/user-stories.md`
//! (US-01 walking skeleton; US-02 default unchanged; US-03 boundary
//! inclusive; US-04 filter BEFORE cap; US-05 unknown-severity 400 with
//! redaction). Contract pinned by
//! `docs/product/architecture/adr-0052-log-query-severity-filter.md`.
//!
//! The user-centric outcome: Sara, on-call SRE for tenant "acme-prod",
//! mid-incident, GETs `/api/v1/logs?start=&end=&min_severity=WARN` and
//! finally receives ONLY the WARN and ERROR records she needs to triage;
//! INFO heartbeats stay on the platform side of the wire. A typo
//! (`WARNING`) is refused with the existing 400 envelope without echoing
//! the raw value. The existing window cap and result cap from ADR-0050
//! continue to behave exactly as before for the unfiltered path; with a
//! filter present the cap measures the post-filter row count, so a
//! narrowed read receives all matching records up to the cap.
//!
//! Every scenario drives log-query-api through its single public driving
//! port `log_query_api::router(store, tenant)` via `oneshot` against a
//! REAL durable `FileBackedLogStore` (the seeded scenarios), a synthetic
//! `BulkSeverityLogStore` for the high-cardinality filter-BEFORE-cap arm
//! (mirrors `slice_02_caps.rs:86` BulkLogStore), or a counting failing
//! store for the no-store-call assertion on the unknown-severity 400.
//!
//! RED state (DISTILL Mandate 7): the suite COMPILES against the current
//! `log-query-api` surface (the `LogsParams::min_severity` field is the
//! scaffold added in this DISTILL, marker `SCAFFOLD: true`). The handler
//! IGNORES the field today, so the scenarios fail behaviourally:
//!   - the walking skeleton receives ALL six records, not the three at-or
//!     above WARN;
//!   - the boundary scenarios receive the unfiltered set;
//!   - the filter-BEFORE-cap arm receives 200_000 records and trips the
//!     existing result cap with a 400;
//!   - the unknown-severity arm receives 200 with the unfiltered records
//!     instead of the named 400.
//!
//! DELIVER lands the parse + wire and the scenarios go green one at a
//! time per the established outer-loop convention.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, logs_request, open_durable_store, records_array, seed, tenant,
};
use lumen::{
    IngestReceipt, LogBatch, LogRecord, LogStore, LogStoreError, Predicate, SeverityNumber,
    TimeRange,
};

// ---------------------------------------------------------------------
// Local test helpers — severity-aware record seeding.
//
// `common::record` builds INFO records only. The severity-filter slice
// needs records at every level on the OTel ladder, so this file carries
// a small `record_with_severity` helper that mirrors `common::record`
// but takes the severity explicitly. Kept local to slice 03 so the
// shared helpers stay unchanged.
// ---------------------------------------------------------------------

fn record_with_severity(observed_secs: u64, severity: SeverityNumber, body: &str) -> LogRecord {
    let mut resource = std::collections::BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    let severity_text = severity_text_for(severity);
    LogRecord {
        observed_time_unix_nano: observed_secs * 1_000_000_000,
        severity_number: severity,
        severity_text,
        body: body.to_string(),
        attributes: std::collections::BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn severity_text_for(sev: SeverityNumber) -> String {
    if sev == SeverityNumber::TRACE {
        "TRACE".to_string()
    } else if sev == SeverityNumber::DEBUG {
        "DEBUG".to_string()
    } else if sev == SeverityNumber::INFO {
        "INFO".to_string()
    } else if sev == SeverityNumber::WARN {
        "WARN".to_string()
    } else if sev == SeverityNumber::ERROR {
        "ERROR".to_string()
    } else if sev == SeverityNumber::FATAL {
        "FATAL".to_string()
    } else {
        "UNSPECIFIED".to_string()
    }
}

/// Build the GET request for the logs endpoint with a `min_severity`
/// parameter. Mirrors `common::logs_request` but appends the new
/// optional parameter on the query string. Kept local because the
/// slice-01 / slice-02 shared helper deliberately ignores severity
/// (those suites must remain unchanged).
fn logs_request_with_min_severity(
    start: &str,
    end: &str,
    min_severity: &str,
) -> axum::http::Request<axum::body::Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}&min_severity={min_severity}");
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Six records at three severities seeded into the durable store for
/// the walking skeleton, the default-unchanged, the boundary inclusive
/// and the case-insensitive scenarios. All records are inside the
/// canonical window `[1716200000s, 1716200060s)` and the
/// observed-time order is ascending so the response order is crisp.
fn seed_mixed_severity_window(store: &Arc<lumen::FileBackedLogStore>, t: &aegis::TenantId) {
    seed(
        store,
        t,
        vec![
            record_with_severity(1_716_200_005, SeverityNumber::INFO, "checkout: heartbeat 1"),
            record_with_severity(1_716_200_010, SeverityNumber::INFO, "checkout: heartbeat 2"),
            record_with_severity(
                1_716_200_015,
                SeverityNumber::WARN,
                "checkout: slow upstream a",
            ),
            record_with_severity(
                1_716_200_020,
                SeverityNumber::ERROR,
                "checkout: payment timeout",
            ),
            record_with_severity(1_716_200_025, SeverityNumber::INFO, "checkout: heartbeat 3"),
            record_with_severity(
                1_716_200_030,
                SeverityNumber::WARN,
                "checkout: slow upstream b",
            ),
        ],
    );
}

fn severity_numbers(body: &serde_json::Value) -> Vec<i64> {
    records_array(body)
        .iter()
        .filter_map(|r| r["severity_number"].as_i64())
        .collect()
}

// ---------------------------------------------------------------------
// A driven-port test double whose `query_with` honours
// `Predicate::min_severity` on a high-cardinality synthetic feed.
//
// Mirrors the `BulkLogStore` pattern from `slice_02_caps.rs:86`: the
// double exists so the cap-interaction arm can assert that 50_000
// matching records are returned WITHOUT seeding 200_000 records into
// the durable store, which would be slow and memory-heavy in CI.
//
// `query` always returns the FULL 200_000 records (mirrors today's
// unfiltered behaviour, which trips the existing result cap with a
// 400). `query_with` returns 200_000 records when the predicate is
// empty, but only the 50_000 ERROR records when the predicate has a
// min_severity floor at-or-above ERROR. This is the cleanest way to
// pin filter-BEFORE-cap behaviour: the scenario asks for
// `min_severity=ERROR`, the handler MUST call `query_with` with that
// predicate, and the double then returns 50_000 — well under the
// 100_000 cap, so a 200 with 50_000 records is the right answer.
//
// The double is a test adapter for the `lumen::LogStore` driven port,
// not an internal log-query-api component, so driving the router
// through `router(...)` over it still honours the hexagonal boundary
// (Mandate 1).
// ---------------------------------------------------------------------

struct BulkSeverityLogStore {
    total_count: usize,
    error_count: usize,
}

impl BulkSeverityLogStore {
    fn new(total_count: usize, error_count: usize) -> Self {
        Self {
            total_count,
            error_count,
        }
    }
}

fn synthetic_record(seq: usize, severity: SeverityNumber) -> LogRecord {
    let mut resource = std::collections::BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: 1_716_200_000_000_000_000 + (seq as u64),
        severity_number: severity,
        severity_text: severity_text_for(severity),
        body: "synthetic".to_string(),
        attributes: std::collections::BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

impl LogStore for BulkSeverityLogStore {
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
        // The full synthetic feed: `error_count` ERROR records first,
        // then the rest as INFO. Order is by sequence number so the
        // observed_time order remains ascending.
        let records: Vec<LogRecord> = (0..self.total_count)
            .map(|seq| {
                let severity = if seq < self.error_count {
                    SeverityNumber::ERROR
                } else {
                    SeverityNumber::INFO
                };
                synthetic_record(seq, severity)
            })
            .collect();
        Ok(records)
    }

    fn query_with(
        &self,
        tenant: &aegis::TenantId,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        // Honour the predicate on the synthetic feed. Empty predicate
        // returns the full feed; a min_severity at or above ERROR
        // returns only the ERROR records.
        let all = self.query(tenant, range)?;
        Ok(all.into_iter().filter(|r| predicate.matches(r)).collect())
    }
}

// ---------------------------------------------------------------------
// A counting failing store. `query` and `query_with` both fail with
// PersistenceFailed and increment a call counter, so a scenario can
// assert the store was NEVER touched on the unknown-severity 400 path
// (US-05 no-store-call mutation target from ADR-0052 Verification).
// ---------------------------------------------------------------------

struct CountingFailingLogStore {
    query_calls: AtomicUsize,
    query_with_calls: AtomicUsize,
}

impl CountingFailingLogStore {
    fn new() -> Self {
        Self {
            query_calls: AtomicUsize::new(0),
            query_with_calls: AtomicUsize::new(0),
        }
    }

    fn total_store_calls(&self) -> usize {
        self.query_calls.load(Ordering::SeqCst) + self.query_with_calls.load(Ordering::SeqCst)
    }
}

impl LogStore for CountingFailingLogStore {
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
        self.query_calls.fetch_add(1, Ordering::SeqCst);
        Err(LogStoreError::PersistenceFailed {
            reason: "backing log store unreadable".to_string(),
        })
    }

    fn query_with(
        &self,
        _tenant: &aegis::TenantId,
        _range: TimeRange,
        _predicate: &Predicate,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        self.query_with_calls.fetch_add(1, Ordering::SeqCst);
        Err(LogStoreError::PersistenceFailed {
            reason: "backing log store unreadable".to_string(),
        })
    }
}

// =====================================================================
// US-01 — Walking skeleton: a min-severity floor drops records below it
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has six records seeded into a REAL durable
/// Lumen store inside the window [1716200000s, 1716200060s): three INFO
/// heartbeats, two WARN slow-upstream notices, one ERROR payment-timeout,
/// When the on-call SRE GETs the logs endpoint for "acme-prod" over
/// [1716200000, 1716200060) with `min_severity=WARN`,
/// Then she sees exactly three records (the two WARN and the one ERROR)
/// in ascending observed_time order, and no INFO heartbeat appears in
/// the response.
///
/// This is the demo-able outcome of the slice: the platform delivers the
/// "WARN or worse" subset Sara needs to triage, with the INFO noise
/// stripped server-side. It seeds REAL durable storage (the same
/// `FileBackedLogStore` adapter the gateway writes through) so the
/// skeleton proves wiring, the predicate-carrying `query_with` read, the
/// `>=` floor on `SeverityNumber`, and the bare-array shape end to end.
#[tokio::test]
async fn operator_filters_to_warn_and_above_during_an_incident() {
    let (store, _base) = open_durable_store("ws-min-severity-warn");
    let t = tenant("acme-prod");
    seed_mixed_severity_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_min_severity("1716200000", "1716200060", "WARN");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let records = records_array(&body);
    assert_eq!(
        records.len(),
        3,
        "exactly three records survive the WARN floor: two WARN and one ERROR; got {body}"
    );
    let severities = severity_numbers(&body);
    assert!(
        severities
            .iter()
            .all(|s| *s >= SeverityNumber::WARN.0 as i64),
        "no record has severity_number below WARN (13): {severities:?}"
    );
    let times: Vec<u64> = records
        .iter()
        .filter_map(|r| r["observed_time_unix_nano"].as_u64())
        .collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(
        times, sorted,
        "the three records are in ascending observed_time order"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("heartbeat"),
        "no INFO heartbeat record appears in the response: {rendered}"
    );
}

// =====================================================================
// US-02 — Default unchanged: parameter absent returns every in-window
// record (backward-compatibility contract)
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has the same six mixed-severity records
/// inside the window,
/// When the on-call SRE GETs the logs endpoint over the window with NO
/// `min_severity` parameter,
/// Then she sees all six in-window records, behaving exactly as before
/// the severity filter shipped.
///
/// This pins the backward-compatibility promise: an existing client
/// (Marcus's 30-second poll script) that does NOT send the new
/// parameter receives byte-equal records to the slice-prior response.
#[tokio::test]
async fn parameter_absent_returns_every_in_window_record_unchanged() {
    let (store, _base) = open_durable_store("default-unchanged");
    let t = tenant("acme-prod");
    seed_mixed_severity_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        records_array(&body).len(),
        6,
        "every in-window record is returned when the parameter is absent: {body}"
    );
}

// =====================================================================
// US-03 — Boundary inclusive: == floor is INCLUDED at WARN, INFO, ERROR
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given tenant "acme-prod" has the same six mixed-severity records,
/// When the on-call SRE queries the endpoint with `min_severity=WARN`,
/// Then exactly three records are returned (two WARN at severity 13 and
/// one ERROR at 17): the WARN records at exactly the floor are
/// INCLUDED. The boundary is `>=`, NOT `>`. Kills a `>=` -> `>` mutant.
#[tokio::test]
async fn boundary_inclusive_warn_keeps_records_at_exactly_warn() {
    let (store, _base) = open_durable_store("boundary-warn-inclusive");
    let t = tenant("acme-prod");
    seed_mixed_severity_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_min_severity("1716200000", "1716200060", "WARN");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        records_array(&body).len(),
        3,
        "WARN floor: two WARN + one ERROR are returned (== floor is included)"
    );
}

/// @driving_port @real-io @US-03
///
/// Given tenant "acme-prod" has the same six mixed-severity records,
/// When the on-call SRE queries the endpoint with `min_severity=INFO`,
/// Then all six in-window records are returned: every INFO (9), every
/// WARN (13), and every ERROR (17). INFO at exactly the floor is
/// INCLUDED.
#[tokio::test]
async fn boundary_inclusive_info_keeps_every_record() {
    let (store, _base) = open_durable_store("boundary-info-inclusive");
    let t = tenant("acme-prod");
    seed_mixed_severity_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_min_severity("1716200000", "1716200060", "INFO");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        records_array(&body).len(),
        6,
        "INFO floor: all six in-window records are returned"
    );
}

/// @driving_port @real-io @US-03
///
/// Given tenant "acme-prod" has the same six mixed-severity records,
/// When the on-call SRE queries the endpoint with `min_severity=ERROR`,
/// Then exactly one record is returned (the single ERROR at 17): every
/// INFO and WARN record is excluded.
#[tokio::test]
async fn boundary_inclusive_error_keeps_only_error() {
    let (store, _base) = open_durable_store("boundary-error-inclusive");
    let t = tenant("acme-prod");
    seed_mixed_severity_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_min_severity("1716200000", "1716200060", "ERROR");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let records = records_array(&body);
    assert_eq!(
        records.len(),
        1,
        "ERROR floor: only the single ERROR record is returned"
    );
    assert_eq!(
        records[0]["severity_number"].as_i64(),
        Some(SeverityNumber::ERROR.0 as i64),
        "the returned record is at severity ERROR (17)"
    );
}

// =====================================================================
// US-04 — Filter BEFORE the result cap (ADR-0052 Decision 4 / FLAG 3)
// =====================================================================

/// @driving_port @US-04
///
/// Given a synthetic store carries 200_000 records (50_000 ERROR +
/// 150_000 INFO) inside an in-cap window for tenant "acme-prod",
/// When the on-call SRE GETs the logs endpoint with `min_severity=ERROR`,
/// Then the response is HTTP 200 with exactly 50_000 records: the
/// filter ran BEFORE the result cap, so the cap measured the
/// post-filter row count (50_000, well under MAX_RESULT_ROWS=100_000)
/// and did NOT fire.
///
/// This pins ADR-0052 Decision 4 (filter BEFORE cap) observably. A
/// mutant that ran the cap first would lift this to a cap-400 against
/// the 200_000 upstream row count and fail this scenario. The double
/// mirrors the `slice_02_caps.rs:86` `BulkLogStore` pattern; the cap
/// IS in play (200_000 > 100_000 on the unfiltered path), so the
/// scenario is a genuine cap-interaction test, not a small-fixture
/// stand-in.
#[tokio::test]
async fn filter_runs_before_the_result_cap_so_narrowed_reads_are_served() {
    let store: Arc<dyn LogStore + Send + Sync> =
        Arc::new(BulkSeverityLogStore::new(200_000, 50_000));
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request_with_min_severity("1716200000", "1716200060", "ERROR");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the filter ran BEFORE the cap; 50_000 ERROR records are under the cap and served"
    );
    assert_eq!(
        records_array(&body).len(),
        50_000,
        "exactly 50_000 ERROR records are returned (filter happened first; cap did not fire)"
    );
}

// =====================================================================
// US-05 — Unknown severity is a redacted 400; the store is NEVER touched
// =====================================================================

/// @driving_port @US-05
///
/// Given the on-call SRE fat-fingers `min_severity=WARNING` (which is
/// NOT one of the six OTel names; ADR-0052 Decision 2 rejects aliases),
/// When the endpoint validates the severity parameter,
/// Then the response is HTTP 400 with the existing error envelope
/// `{status:"error", error:"unknown severity"}`, the body does NOT
/// contain the literal substring "WARNING" (redaction; ADR-0047
/// Decision 1 / ADR-0050 Decision 7), and the store is NEVER touched
/// on this path (proven by a counting failing store whose call
/// counters stay at zero — a 500 here would mean the store was
/// touched).
///
/// This pins ADR-0052 Decision 5 (severity parse BEFORE the store call)
/// and Decision 6 (redaction inherited). Kills a mutant that echoes the
/// raw value into the reason text and a mutant that calls the store
/// before parsing the severity.
#[tokio::test]
async fn an_unknown_severity_name_is_a_redacted_400_without_touching_the_store() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_with_min_severity("1716200000", "1716200060", "WARNING");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unknown severity name is a 400, never a 500: the store is NEVER touched"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "unknown severity",
        "the reason text names the class verbatim"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("WARNING"),
        "the body must never echo the raw severity parameter value: {rendered}"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the unknown-severity 400 path"
    );
}

// =====================================================================
// US-01 — Case-insensitive: `warn`, `WARN`, `Warn` produce identical
// filtered results (ADR-0052 Decision 2)
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given tenant "acme-prod" has the same six mixed-severity records,
/// When the on-call SRE issues three queries with `min_severity=warn`,
/// `min_severity=WARN`, and `min_severity=Warn`,
/// Then all three responses are HTTP 200 and carry the SAME three
/// filtered records (the two WARN and the one ERROR): the parser maps
/// the six OTel names case-insensitively (ADR-0052 Decision 2). Kills
/// a mutant that compares with `eq` instead of `eq_ignore_ascii_case`.
#[tokio::test]
async fn the_severity_name_match_is_case_insensitive_on_the_six_otel_names() {
    let (store, _base) = open_durable_store("case-insensitive-warn");
    let t = tenant("acme-prod");
    seed_mixed_severity_window(&store, &t);

    let mut bodies: Vec<serde_json::Value> = Vec::new();
    for case_form in ["warn", "WARN", "Warn"] {
        let router = log_query_api::router(
            store.clone() as Arc<dyn LogStore + Send + Sync>,
            Some(t.clone()),
        );
        let request = logs_request_with_min_severity("1716200000", "1716200060", case_form);
        let (status, body) = call(router, request).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "case form {case_form:?} is accepted: {body}"
        );
        bodies.push(body);
    }

    let first = &bodies[0];
    let first_len = records_array(first).len();
    assert_eq!(
        first_len, 3,
        "the lowercase warn form yields the three WARN+ERROR records"
    );
    for (case_form, body) in [("WARN", &bodies[1]), ("Warn", &bodies[2])] {
        assert_eq!(
            body, first,
            "case form {case_form:?} produces an identical response to lowercase warn"
        );
    }
}
