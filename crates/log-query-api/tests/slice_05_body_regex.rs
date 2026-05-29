// Kaleidoscope log-query-api — slice 05 body-regex acceptance suite
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

//! Body-regex filter on the logs read path — `?body_regex=`
//! narrows the response to records whose `body` field is matched
//! by the supplied regular expression (`Regex::is_match`,
//! unanchored, byte-wise case-sensitive by default), BEFORE the
//! result cap.
//!
//! Maps to `docs/feature/log-body-regex-search-v0/discuss/user-stories.md`
//! (US-01 walking skeleton; US-02 calm empty; US-03 default
//! unchanged; US-04a invalid syntax 400; US-04b empty 400; US-04c
//! over-cap 400; US-05 case-sensitive pin; US-06 mutual exclusion;
//! US-07 cross-tenant isolation) and the conjunctive composition
//! with `min_severity` scenario named in
//! `application-architecture.md` § Composition. Contract pinned by
//! `docs/product/architecture/adr-0056-log-body-regex-search.md`.
//!
//! The user-centric outcome: Maria Santos, on-call SRE for tenant
//! "acme-prod", mid-incident with a regex covering several shapes
//! of a kafka-timeout failure family, GETs
//! `/api/v1/logs?start=&end=&body_regex=kafka.%2A(timeout|timed%20out)`
//! and receives ONLY the records whose `body` matches the regex —
//! every shape of the same failure in ONE request, no client-side
//! union of three `body_contains` queries. An empty value, an
//! over-cap value, or an unparseable value is refused with the
//! existing 400 envelope; the raw value is NEVER echoed; sending
//! BOTH `body_contains` and `body_regex` is refused with a new
//! literal reason that is its own redaction class.
//!
//! Every scenario drives log-query-api through its single public
//! driving port `log_query_api::router(store, tenant)` via
//! `oneshot` against a REAL durable `FileBackedLogStore` (the
//! seeded scenarios) or a counting failing store for the
//! no-store-call assertion on the four 400 arms.
//!
//! RED state (DISTILL Mandate 7): the suite COMPILES against the
//! current `log-query-api` surface (the `LogsParams::body_regex`
//! field, `MAX_BODY_REGEX_LEN`, the `parse_body_regex` scaffold,
//! the mutual-exclusion check, the 6-arm dispatch, and the new
//! `Predicate::body_regex` builder are the additions in this
//! DISTILL wave). Every scenario — including the walking
//! skeleton AC-01 — is `#[ignore]`'d at DISTILL close so the
//! workspace pre-commit gate (`cargo test --workspace --all-targets
//! --locked`) passes. Verified RED locally: running AC-01 with
//! `cargo test -p log-query-api --test slice_05_body_regex --
//! --ignored` panics with `__SCAFFOLD__ log-body-regex-search-v0
//! RED` inside `parse_body_regex` (and inside
//! `Predicate::matches` for the scenarios that bypass the parse
//! helper). Crafty de-ignores AC-01 FIRST in DELIVER (it is the
//! walking-skeleton outer-loop scenario), then the remaining
//! scenarios one at a time as he fills the parser body, the
//! `Predicate::matches` arm, and the dispatch wiring.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, logs_request, open_durable_store, record, records_array, seed, tenant,
};
use lumen::{
    IngestReceipt, LogBatch, LogRecord, LogStore, LogStoreError, Predicate, SeverityNumber,
    TimeRange,
};

// ---------------------------------------------------------------------
// Local test helpers — body-regex-aware request builders and a
// severity-aware seeder for the conjunctive-composition scenario.
// ---------------------------------------------------------------------

/// Build a GET request with a `body_regex` parameter (raw; caller
/// passes the URL-encoded form when special characters are
/// involved).
fn logs_request_with_body_regex(
    start: &str,
    end: &str,
    body_regex: &str,
) -> axum::http::Request<axum::body::Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}&body_regex={body_regex}");
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Build a GET request that carries BOTH `body_contains` and
/// `body_regex` (the mutual-exclusion arm).
fn logs_request_with_body_contains_and_body_regex(
    start: &str,
    end: &str,
    body_contains: &str,
    body_regex: &str,
) -> axum::http::Request<axum::body::Body> {
    let uri = format!(
        "/api/v1/logs?start={start}&end={end}&body_contains={body_contains}&body_regex={body_regex}"
    );
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Build a GET request that carries BOTH `min_severity` and
/// `body_regex` (the conjunctive-composition arm).
fn logs_request_with_min_severity_and_body_regex(
    start: &str,
    end: &str,
    min_severity: &str,
    body_regex: &str,
) -> axum::http::Request<axum::body::Body> {
    let uri = format!(
        "/api/v1/logs?start={start}&end={end}&min_severity={min_severity}&body_regex={body_regex}"
    );
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Seed the canonical seven-record window for the body-regex
/// scenarios: five records whose `body` matches the regex
/// `kafka.*(timeout|timed out)` and two records ("ok") that do not.
/// All records are inside `[1716200000s, 1716200060s)` in ascending
/// observed-time order so the response order assertions are crisp.
fn seed_kafka_family_window(store: &Arc<lumen::FileBackedLogStore>, t: &aegis::TenantId) {
    seed(
        store,
        t,
        vec![
            record(1_716_200_005, "checkout", "kafka connect timeout 1"),
            record(1_716_200_010, "checkout", "kafka connect timeout 2"),
            record(1_716_200_015, "checkout", "kafka connect timeout 3"),
            record(1_716_200_020, "checkout", "kafka connection timed out 1"),
            record(1_716_200_025, "checkout", "kafka connection timed out 2"),
            record(1_716_200_030, "checkout", "ok"),
            record(1_716_200_035, "checkout", "ok"),
        ],
    );
}

/// A record at INFO severity. Mirrors `common::record` (which is
/// INFO by default) but kept locally for the severity-aware seeder.
fn info_record(observed_secs: u64, service: &str, body: &str) -> LogRecord {
    let mut r = record(observed_secs, service, body);
    r.severity_number = SeverityNumber::INFO;
    r.severity_text = "INFO".to_string();
    r
}

/// A record at WARN severity, otherwise identical shape to
/// `common::record`.
fn warn_record(observed_secs: u64, service: &str, body: &str) -> LogRecord {
    let mut r = record(observed_secs, service, body);
    r.severity_number = SeverityNumber::WARN;
    r.severity_text = "WARN".to_string();
    r
}

/// Seed a mixed-severity window for the conjunctive AC-COMBO
/// scenario:
/// - record A: INFO + matches `kafka.*timeout` (excluded by min_severity)
/// - record B: WARN + matches `kafka.*timeout` (kept; satisfies BOTH)
/// - record C: INFO + does NOT match (excluded by both filters)
/// - record D: WARN + does NOT match (excluded by body_regex)
/// - record E: WARN + matches `kafka.*timeout` (kept; satisfies BOTH)
fn seed_mixed_severity_window(store: &Arc<lumen::FileBackedLogStore>, t: &aegis::TenantId) {
    seed(
        store,
        t,
        vec![
            info_record(1_716_200_005, "checkout", "kafka connect timeout A"),
            warn_record(1_716_200_010, "checkout", "kafka connect timeout B"),
            info_record(1_716_200_015, "checkout", "checkout: heartbeat C"),
            warn_record(1_716_200_020, "checkout", "redis: noisy warning D"),
            warn_record(1_716_200_025, "checkout", "kafka connect timeout E"),
        ],
    );
}

// ---------------------------------------------------------------------
// A counting failing store. `query` and `query_with` both fail with
// PersistenceFailed and increment a call counter, so a scenario can
// assert the store was NEVER touched on any of the four 400 paths
// (empty / over-cap / invalid-syntax / mutual-exclusion).
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
// AC-01 / US-01 — Walking skeleton: a known regex pattern matches the
// full failure family in one request
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has seven records seeded into a REAL
/// durable Lumen store inside the window [1716200000s, 1716200060s):
/// five records whose `body` matches the regex
/// `kafka.*(timeout|timed out)` (three "kafka connect timeout N" and
/// two "kafka connection timed out N") and two records whose `body`
/// is "ok",
/// When the on-call SRE GETs the logs endpoint for "acme-prod" over
/// [1716200000, 1716200060) with
/// `body_regex=kafka.%2A(timeout|timed%20out)` (URL-encoded
/// `kafka.*(timeout|timed out)`),
/// Then she sees exactly five records (the five matching the regex)
/// in ascending observed_time order, and no "ok" record appears in
/// the response.
///
/// This is the demo-able outcome of the slice: the platform delivers
/// every shape of the kafka-timeout failure family in one request,
/// with the unrelated records stripped server-side. It seeds REAL
/// durable storage (the same `FileBackedLogStore` adapter the
/// gateway writes through) so the skeleton proves wiring, the
/// predicate-carrying `query_with` read, the `Regex::is_match`
/// match call, and the bare-array shape end to end.
///
/// RED state at DISTILL close: the handler reaches
/// `parse_body_regex`, which is `unimplemented!` in the scaffold,
/// so the test panics with
/// `__SCAFFOLD__ log-body-regex-search-v0 RED`. Crafty
/// de-ignores this scenario FIRST in DELIVER (walking skeleton;
/// outer-loop convention) and fills the parser body and the
/// predicate arm so the test goes green.
#[tokio::test]
async fn ac_01_known_pattern_matches_failure_family() {
    let (store, _base) = open_durable_store("ws-body-regex-kafka-family");
    let t = tenant("acme-prod");
    seed_kafka_family_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    // `kafka.%2A(timeout|timed%20out)` is the URL-encoded form of
    // `kafka.*(timeout|timed out)`; axum's `Query<LogsParams>`
    // extractor decodes it before the parameter reaches the
    // handler.
    let request =
        logs_request_with_body_regex("1716200000", "1716200060", "kafka.%2A(timeout|timed%20out)");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let records = records_array(&body);
    assert_eq!(
        records.len(),
        5,
        "exactly five records match kafka.*(timeout|timed out); got {body}"
    );
    // No "ok" record leaks through.
    let rendered = body.to_string();
    assert!(
        !rendered.contains("\"ok\""),
        "no non-matching 'ok' record appears in the response: {rendered}"
    );
    // Ordering preserved: ascending observed_time_unix_nano.
    let times: Vec<u64> = records
        .iter()
        .filter_map(|r| r["observed_time_unix_nano"].as_u64())
        .collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(
        times, sorted,
        "the five matching records are in ascending observed_time order"
    );
}

// =====================================================================
// AC-02 / US-02 — Calm empty: a valid pattern matching no record
// returns 200 with []
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has the same seven records, none of
/// whose body matches `cassandra.*timeout`,
/// When the support engineer GETs the logs endpoint over the window
/// with `body_regex=cassandra.%2Atimeout`,
/// Then the response is HTTP 200 with the calm empty bare array
/// `[]`, NEVER HTTP 404, NEVER HTTP 500.
///
/// Pins ADR-0047 Decision 1 (the calm-empty contract) against the
/// new arm: the absence of matches is a successful query that
/// returned no rows, not a "not found" or a server error.
#[tokio::test]
async fn ac_02_unknown_pattern_returns_empty() {
    let (store, _base) = open_durable_store("calm-empty-body-regex-cassandra");
    let t = tenant("acme-prod");
    seed_kafka_family_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_body_regex("1716200000", "1716200060", "cassandra.%2Atimeout");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "an unmatched valid pattern is a calm 200, NEVER 404, NEVER 500"
    );
    assert_eq!(
        records_array(&body).len(),
        0,
        "the response is the calm empty bare array []: {body}"
    );
}

// =====================================================================
// AC-03 / US-03 — Default unchanged: parameter absent returns every
// in-window record (backward-compatibility contract)
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given tenant "acme-prod" has the same seven records,
/// When the automation client GETs the logs endpoint over the window
/// with NO `body_regex` parameter,
/// Then it receives every in-window record, behaving exactly as
/// before the body-regex filter shipped.
///
/// Pins the backward-compatibility promise: Marcus's 60-second poll
/// script (which does NOT send the new parameter) receives
/// byte-equal records to the slice-prior response.
#[tokio::test]
async fn ac_03_missing_body_regex_returns_all() {
    let (store, _base) = open_durable_store("default-unchanged-body-regex");
    let t = tenant("acme-prod");
    seed_kafka_family_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        records_array(&body).len(),
        7,
        "every in-window record is returned when the parameter is absent: {body}"
    );
}

// =====================================================================
// AC-04a / US-04a — Invalid regex syntax is a redacted 400
// =====================================================================

/// @driving_port @US-04a
///
/// Given the on-call SRE fat-fingers `body_regex=[` (an unclosed
/// character class that the `regex` crate refuses to compile),
/// When the endpoint validates the parameter,
/// Then the response is HTTP 400 with the literal envelope
/// `{"status":"error","error":"invalid body_regex"}`, the body does
/// NOT echo the raw pattern, and the store is NEVER touched on this
/// path.
///
/// Pins ADR-0056 Decision 3 (handler-side compile, fail-fast 400)
/// and the redaction posture. Kills a mutant that catches
/// `Regex::new` errors and falls through to "no filter" instead of
/// returning 400.
#[tokio::test]
async fn ac_04a_invalid_regex_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    // `[` is the literal raw character (no encoding required). The
    // unclosed bracket is rejected by `Regex::new`.
    let request = logs_request_with_body_regex("1716200000", "1716200060", "[");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unparseable body_regex is a 400, never a 500: the store is NEVER touched"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid body_regex",
        "the reason text is the literal class label"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the invalid-syntax 400 path"
    );
}

// =====================================================================
// AC-04b / US-04b — Empty body_regex is a redacted 400
// =====================================================================

/// @driving_port @US-04b
///
/// Given the on-call SRE fat-fingers `body_regex=` (an empty value;
/// `Regex::new("")` returns `Ok` matching every position, which
/// would silently match every record),
/// When the endpoint validates the parameter,
/// Then the response is HTTP 400 with the literal envelope
/// `{"status":"error","error":"invalid body_regex"}`, and the store
/// is NEVER touched on this path.
///
/// Pins ADR-0056 Decision 6 (empty rejection). Kills a mutant that
/// treats `Some("")` as `None` and silently falls through to "no
/// filter".
#[tokio::test]
async fn ac_04b_empty_string_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_with_body_regex("1716200000", "1716200060", "");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an empty body_regex is a 400, never a 500: the store is NEVER touched"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid body_regex",
        "the reason text is the SAME literal as the invalid-syntax arm"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the empty-string 400 path"
    );
}

// =====================================================================
// AC-04c / US-04c — Over-cap body_regex is a redacted 400; raw value
// NEVER echoed (anti-echo)
// =====================================================================

/// @driving_port @US-04c
///
/// Given the client sends a `body_regex` value of 1025 bytes
/// (strictly above the 1024-byte cap),
/// When the endpoint validates the parameter,
/// Then the response is HTTP 400 with the SAME literal envelope as
/// the empty and invalid-syntax arms, the body does NOT contain any
/// byte of the raw value (a recognisable prefix is asserted absent),
/// and the store is NEVER touched.
///
/// Pins ADR-0056 Decision 5 (length cap 1024 bytes, inclusive
/// boundary; same literal envelope) and the anti-echo posture. The
/// 1025-byte value uses a distinctive prefix (`OVERSIZE-`) so the
/// absence assertion is unambiguous. The pattern body
/// (`A` repeated) is a valid regex on its own, so the cap rejection
/// must precede the compile call (DD3 / parse-helper-spec order:
/// empty -> over-cap -> compile).
#[tokio::test]
async fn ac_04c_length_over_cap_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );

    // 1025 bytes: 9-byte recognisable prefix + 1016 'A' chars.
    // 1024 is inclusively accepted; 1025 is strictly over the cap
    // and rejected. `A`-repeated is a valid regex if it reached the
    // compile call, so a green 400 here proves the cap rejection
    // ran BEFORE the compile.
    let oversize_raw = format!("OVERSIZE-{}", "A".repeat(1016));
    assert_eq!(oversize_raw.len(), 1025, "fixture must be 1025 bytes");
    let request = logs_request_with_body_regex("1716200000", "1716200060", &oversize_raw);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an over-cap body_regex is a 400, never a 500"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid body_regex",
        "the reason text is the SAME literal as the empty and invalid-syntax arms"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("OVERSIZE-"),
        "the body must NEVER echo the raw oversize value: {rendered}"
    );
    assert!(
        !rendered.contains("1025"),
        "the body must NEVER echo the length: {rendered}"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the over-cap 400 path"
    );
}

// =====================================================================
// AC-05 / US-05 — Case-sensitive matching pinned (KAFKA does NOT match
// records whose body is "kafka error")
// =====================================================================

/// @driving_port @real-io @US-05
///
/// Given tenant "acme-prod" has records whose body is "kafka error"
/// (lowercase 'k'),
/// When the on-call SRE GETs the endpoint with `body_regex=KAFKA`
/// (uppercase),
/// Then the response is HTTP 200 with the calm empty bare array
/// `[]`: the regex match is case-sensitive by default
/// (`K`=0x4B, `k`=0x6B; the bytes differ; no match).
///
/// Pins ADR-0056 Decision (case-sensitive default per PIN 2 in
/// user-stories.md) and the operator escape hatch (an operator who
/// wants case-insensitive matching uses the inline `(?i)` flag, a
/// separate behaviour outside this scenario's scope; the helper-level
/// inline test in `parse-helper-spec.md` § Test surface item 9
/// pins the inline-flag arm).
#[tokio::test]
async fn ac_05_case_sensitive_default() {
    let (store, _base) = open_durable_store("case-sensitive-body-regex-KAFKA");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            record(1_716_200_005, "checkout", "kafka error 1"),
            record(1_716_200_010, "checkout", "kafka error 2"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_body_regex("1716200000", "1716200060", "KAFKA");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the case-sensitive miss is a calm 200 with []"
    );
    assert_eq!(
        records_array(&body).len(),
        0,
        "KAFKA does NOT match lowercase 'kafka'; the response is the calm empty bare array []: {body}"
    );
}

// =====================================================================
// AC-06 / US-06 — Mutual exclusion: BOTH body_contains AND body_regex
// returns a redacted 400 with the new literal reason
// =====================================================================

/// @driving_port @US-06
///
/// Given the client's URL-construction bug appends BOTH
/// `body_contains=foo` AND `body_regex=bar` to the same request,
/// When the endpoint validates the parameters,
/// Then the response is HTTP 400 with the literal envelope
/// `{"status":"error","error":"specify body_regex or body_contains, not both"}`,
/// the body does NOT echo either raw value, and the store is NEVER
/// touched on this path.
///
/// Pins ADR-0056 Decision 7 / DD4: a deliberate error answer
/// (intersection? union? error?) at slice 01, NOT a quiet AND-default
/// or last-one-wins. Kills a mutant that drops the check and
/// silently AND-composes both body filters. Both `foo` and `bar`
/// are syntactically valid `body_contains` (under-cap) and
/// `body_regex` (syntactically valid regex) values; the rejection
/// is exclusively about their joint presence.
#[tokio::test]
async fn ac_06_mutual_exclusion_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request =
        logs_request_with_body_contains_and_body_regex("1716200000", "1716200060", "foo", "bar");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "BOTH body_contains and body_regex is a 400, never a 500"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "specify body_regex or body_contains, not both",
        "the reason text is the NEW literal class label for mutual exclusion (distinct from 'invalid body_regex')"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("\"foo\""),
        "the body must NEVER echo the raw body_contains value: {rendered}"
    );
    assert!(
        !rendered.contains("\"bar\""),
        "the body must NEVER echo the raw body_regex value: {rendered}"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the mutual-exclusion 400 path"
    );
}

// =====================================================================
// AC-07 / US-07 — Cross-tenant isolation: tenant B never sees tenant
// A's regex matches
// =====================================================================

/// @driving_port @real-io @US-07
///
/// Given tenant "acme-prod" has records whose body matches
/// `kafka.*timeout` and tenant "globex-staging" has ZERO records in
/// the window,
/// When the operator (holding the globex-staging credential) GETs
/// the endpoint with `body_regex=kafka.%2Atimeout` under tenant
/// "globex-staging",
/// Then the response is HTTP 200 with the calm empty bare array
/// `[]`, the body NEVER contains the substring "connect" (a marker
/// borrowed from the acme-prod fixtures), and no acme-prod record
/// appears.
///
/// Pins the platform's per-tenant isolation invariant (ADR-0047)
/// against the new arm: the `query_with(&tenant, range, predicate)`
/// seam carries the tenant as the first argument; the tenant-bucket
/// lookup happens before any predicate evaluation. A mutant that
/// resolves the tenant AFTER applying the filter (or that applies
/// the filter against all tenants' records) is killed by this
/// scenario.
#[tokio::test]
async fn ac_07_cross_tenant_isolation() {
    let (store, _base) = open_durable_store("cross-tenant-body-regex");
    let t_acme = tenant("acme-prod");
    let t_globex = tenant("globex-staging");
    // Seed acme-prod with the kafka-timeout family; globex-staging
    // remains empty (no seed call).
    seed_kafka_family_window(&store, &t_acme);

    let router = log_query_api::router(
        store as Arc<dyn LogStore + Send + Sync>,
        Some(t_globex.clone()),
    );
    let request = logs_request_with_body_regex("1716200000", "1716200060", "kafka.%2Atimeout");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the cross-tenant miss is a calm 200 with []"
    );
    assert_eq!(
        records_array(&body).len(),
        0,
        "tenant globex-staging receives [] when querying for a pattern that matches only tenant acme-prod's records: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("connect"),
        "no acme-prod body text leaks across tenants: {rendered}"
    );
}

// =====================================================================
// AC-COMBO / Conjunctive composition — min_severity AND body_regex
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given tenant "acme-prod" has a mixed-severity window of five
/// records:
/// - INFO "kafka connect timeout A" (matches body_regex, fails severity)
/// - WARN "kafka connect timeout B" (matches BOTH)
/// - INFO "checkout: heartbeat C" (fails BOTH)
/// - WARN "redis: noisy warning D" (matches severity, fails body_regex)
/// - WARN "kafka connect timeout E" (matches BOTH)
/// When the SRE GETs the endpoint with `min_severity=WARN` AND
/// `body_regex=kafka.%2Atimeout`,
/// Then the response is HTTP 200 with exactly two records (the WARN
/// records matching the regex; records B and E) in ascending
/// observed_time order, and neither the INFO record matching the
/// regex (A) nor the WARN record failing the regex (D) appears.
///
/// Pins ADR-0056 Decision 9 / application-architecture.md §
/// Composition (conjunctive AND between min_severity and
/// body_regex). Kills a mutant that swaps AND for OR in
/// `Predicate::matches` and a mutant that short-circuits one filter
/// when the other is absent.
#[tokio::test]
async fn ac_combo_severity_x_regex() {
    let (store, _base) = open_durable_store("combo-severity-x-body-regex");
    let t = tenant("acme-prod");
    seed_mixed_severity_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_min_severity_and_body_regex(
        "1716200000",
        "1716200060",
        "WARN",
        "kafka.%2Atimeout",
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let records = records_array(&body);
    assert_eq!(
        records.len(),
        2,
        "exactly two records satisfy BOTH min_severity=WARN AND body_regex=kafka.*timeout; got {body}"
    );
    // No INFO records appear (rule out the INFO match on record A).
    let rendered = body.to_string();
    assert!(
        !rendered.contains("\"INFO\""),
        "no INFO record appears in the response (record A excluded by min_severity): {rendered}"
    );
    // No noisy-warning record (rule out record D, which is WARN
    // but does NOT match the regex).
    assert!(
        !rendered.contains("noisy warning"),
        "no non-matching WARN record appears in the response (record D excluded by body_regex): {rendered}"
    );
    // Ordering preserved: ascending observed_time_unix_nano.
    let times: Vec<u64> = records
        .iter()
        .filter_map(|r| r["observed_time_unix_nano"].as_u64())
        .collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(
        times, sorted,
        "the two matching records are in ascending observed_time order"
    );
}
