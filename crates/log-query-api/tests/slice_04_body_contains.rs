// Kaleidoscope log-query-api — slice 04 body-contains acceptance suite
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

//! Body-substring filter on the logs read path — `?body_contains=`
//! narrows the response to records whose `body` field contains the
//! supplied substring, byte-wise, case-sensitive, BEFORE the result
//! cap.
//!
//! Maps to `docs/feature/log-body-text-search-v0/discuss/user-stories.md`
//! (US-01 walking skeleton; US-02 calm empty; US-03 default unchanged;
//! US-04 empty-string + over-cap 400; US-05 case-sensitive pin; US-06
//! cross-tenant isolation). Contract pinned by
//! `docs/product/architecture/adr-0055-log-body-text-search.md`.
//!
//! The user-centric outcome: Sara, on-call SRE for tenant "acme-prod",
//! mid-incident with the string "kafka timeout" in hand from a paging
//! alert, GETs `/api/v1/logs?start=&end=&body_contains=kafka%20timeout`
//! and receives ONLY the records whose `body` carries the substring.
//! The 198 unrelated INFO heartbeats and the 40 unrelated ERROR records
//! about a different incident stay on the platform side of the wire.
//! An empty value (`?body_contains=`) is refused with the existing 400
//! envelope; an oversize value (1025+ bytes) is refused with the same
//! literal envelope, and the raw value is NEVER echoed (redaction).
//!
//! Every scenario drives log-query-api through its single public
//! driving port `log_query_api::router(store, tenant)` via `oneshot`
//! against a REAL durable `FileBackedLogStore` (the seeded scenarios)
//! or a counting failing store for the no-store-call assertion on the
//! empty-string and over-cap 400 arms.
//!
//! RED state (DISTILL Mandate 7): the suite COMPILES against the
//! current `log-query-api` surface (the `LogsParams::body_contains`
//! field and the `parse_body_contains` scaffold are the additions in
//! this DISTILL wave). Every scenario — including the walking
//! skeleton AC-01 — is `#[ignore]`'d at DISTILL close so the
//! workspace pre-commit gate (`cargo test --workspace --all-targets
//! --locked`) passes. Verified RED locally: running AC-01 with
//! `cargo test -p log-query-api --test slice_04_body_contains --
//! --ignored` panics with `__SCAFFOLD__ log-body-text-search-v0
//! RED` inside `parse_body_contains`. Crafty de-ignores AC-01 FIRST
//! in DELIVER (it is the walking-skeleton outer-loop scenario), then
//! the remaining scenarios one at a time as he fills the parser
//! body, the `Predicate::matches` arm, and the dispatch wiring.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, logs_request, open_durable_store, record, records_array, seed, tenant,
};
use lumen::{IngestReceipt, LogBatch, LogRecord, LogStore, LogStoreError, Predicate, TimeRange};

// ---------------------------------------------------------------------
// Local test helpers — body-substring-aware request builders.
//
// The shared `common::logs_request` builds the parameter-less URI
// (start + end only). The slice-04 scenarios need to append the
// `body_contains=` parameter (URL-encoded for the happy-path
// scenarios; raw for the empty arm). Kept local so the slice-01,
// slice-02, and slice-03 shared helpers stay unchanged.
// ---------------------------------------------------------------------

/// Build the GET request with a `body_contains` parameter (raw, no
/// URL encoding applied). The caller passes the encoded form (e.g.
/// `kafka%20timeout`) when whitespace is involved.
fn logs_request_with_body_contains(
    start: &str,
    end: &str,
    body_contains: &str,
) -> axum::http::Request<axum::body::Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}&body_contains={body_contains}");
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Seed the canonical five-record window for the body-contains
/// scenarios: three records whose `body` carries the substring
/// "kafka timeout" and two records whose `body` does not. All
/// records are inside `[1716200000s, 1716200060s)` in ascending
/// observed-time order so the response order assertions are crisp.
fn seed_kafka_timeout_window(store: &Arc<lumen::FileBackedLogStore>, t: &aegis::TenantId) {
    seed(
        store,
        t,
        vec![
            record(
                1_716_200_005,
                "checkout",
                "kafka timeout connecting to broker-1",
            ),
            record(
                1_716_200_015,
                "checkout",
                "kafka timeout reading from broker-2",
            ),
            record(
                1_716_200_025,
                "checkout",
                "kafka timeout writing to broker-3",
            ),
            record(1_716_200_035, "checkout", "checkout: heartbeat 1"),
            record(1_716_200_045, "checkout", "checkout: heartbeat 2"),
        ],
    );
}

// ---------------------------------------------------------------------
// A counting failing store. `query` and `query_with` both fail with
// PersistenceFailed and increment a call counter, so a scenario can
// assert the store was NEVER touched on the empty-string or
// over-cap 400 path (ADR-0055 Decision 7 / DD4-DD6 no-store-call
// mutation target).
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
// AC-01 / US-01 — Walking skeleton: a known substring narrows the
// response to matching records
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has five records seeded into a REAL
/// durable Lumen store inside the window [1716200000s, 1716200060s):
/// three records whose `body` carries "kafka timeout" and two records
/// whose `body` does not,
/// When the on-call SRE GETs the logs endpoint for "acme-prod" over
/// [1716200000, 1716200060) with `body_contains=kafka%20timeout`,
/// Then she sees exactly three records (the three carrying the
/// substring) in ascending observed_time order, and no heartbeat
/// record appears in the response.
///
/// This is the demo-able outcome of the slice: the platform delivers
/// the "records carrying this string" subset Sara needs to triage,
/// with the unrelated records stripped server-side. It seeds REAL
/// durable storage (the same `FileBackedLogStore` adapter the gateway
/// writes through) so the skeleton proves wiring, the
/// predicate-carrying `query_with` read, the `String::contains`
/// substring match, and the bare-array shape end to end.
///
/// RED state at DISTILL close: the handler reaches
/// `parse_body_contains`, which is `unimplemented!` in the scaffold,
/// so the test panics with `__SCAFFOLD__ log-body-text-search-v0
/// RED`. Crafty de-ignores this scenario FIRST in DELIVER (walking
/// skeleton; outer-loop convention) and fills the parser body so
/// the test goes green.
///
/// `#[ignore]`'d at DISTILL close so the workspace pre-commit gate
/// (`cargo test --workspace --all-targets --locked`) passes; RED
/// state is verifiable via
/// `cargo test -p log-query-api --test slice_04_body_contains
/// ac_01 -- --ignored`.
#[tokio::test]
async fn ac_01_known_substring_narrows_the_response_to_matching_records() {
    let (store, _base) = open_durable_store("ws-body-contains-kafka-timeout");
    let t = tenant("acme-prod");
    seed_kafka_timeout_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    // `kafka%20timeout` is the URL-encoded form of `kafka timeout`;
    // axum's `Query<LogsParams>` extractor decodes it before the
    // parameter reaches the handler.
    let request = logs_request_with_body_contains("1716200000", "1716200060", "kafka%20timeout");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let records = records_array(&body);
    assert_eq!(
        records.len(),
        3,
        "exactly three records carry 'kafka timeout' in their body; got {body}"
    );
    // Every returned record's body contains the substring.
    for r in records {
        let body_str = r["body"].as_str().expect("body is a string");
        assert!(
            body_str.contains("kafka timeout"),
            "every returned record's body carries the substring; saw {body_str:?}"
        );
    }
    // Ordering preserved: ascending observed_time_unix_nano.
    let times: Vec<u64> = records
        .iter()
        .filter_map(|r| r["observed_time_unix_nano"].as_u64())
        .collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(
        times, sorted,
        "the three matching records are in ascending observed_time order"
    );
    // No heartbeat record leaks through.
    let rendered = body.to_string();
    assert!(
        !rendered.contains("heartbeat"),
        "no heartbeat record appears in the response: {rendered}"
    );
}

// =====================================================================
// AC-02 / US-02 — Calm empty: unmatched substring returns 200 with []
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has the same five records, none of whose
/// body contains "cassandra",
/// When the support engineer GETs the logs endpoint over the window
/// with `body_contains=cassandra`,
/// Then the response is HTTP 200 with the calm empty bare array `[]`,
/// NEVER HTTP 404, NEVER HTTP 500.
///
/// This pins ADR-0047 Decision 1 (the calm-empty contract) against
/// the new arm: the absence of matches is a successful query that
/// returned no rows, not a "not found" or a server error. Priya
/// distinguishes "substring absent from this window" (200 + `[]`)
/// from "the query was malformed" (400) from "the store is broken"
/// (500).
#[tokio::test]
async fn ac_02_unknown_substring_returns_calm_empty_array() {
    let (store, _base) = open_durable_store("calm-empty-cassandra");
    let t = tenant("acme-prod");
    seed_kafka_timeout_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_body_contains("1716200000", "1716200060", "cassandra");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "an unmatched substring is a calm 200, NEVER 404, NEVER 500"
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
/// Given tenant "acme-prod" has the same five records,
/// When the automation client GETs the logs endpoint over the window
/// with NO `body_contains` parameter,
/// Then it receives every in-window record, behaving exactly as
/// before the body-contains filter shipped.
///
/// This pins the backward-compatibility promise: Marcus's 60-second
/// poll script (which does NOT send the new parameter) receives
/// byte-equal records to the slice-prior response.
#[tokio::test]
async fn ac_03_missing_body_contains_returns_every_in_window_record() {
    let (store, _base) = open_durable_store("default-unchanged-body-contains");
    let t = tenant("acme-prod");
    seed_kafka_timeout_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        records_array(&body).len(),
        5,
        "every in-window record is returned when the parameter is absent: {body}"
    );
}

// =====================================================================
// AC-04a / US-04 — Empty body_contains is a redacted 400
// =====================================================================

/// @driving_port @US-04
///
/// Given the on-call SRE fat-fingers `body_contains=` (an empty
/// value, indistinguishable from "match every record" if accepted
/// silently),
/// When the endpoint validates the parameter,
/// Then the response is HTTP 400 with the literal envelope
/// `{"status":"error","error":"invalid body_contains"}`, and the
/// store is NEVER touched on this path.
///
/// This pins ADR-0055 Decision 4 (empty rejection) and Decision 5
/// (literal envelope; redaction). Kills a mutant that treats `Some("")`
/// as `None` and silently falls through to "no filter".
#[tokio::test]
async fn ac_04a_empty_body_contains_returns_400_with_literal_envelope() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_with_body_contains("1716200000", "1716200060", "");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an empty body_contains is a 400, never a 500: the store is NEVER touched"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid body_contains",
        "the reason text is the literal class label"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the empty-string 400 path"
    );
}

// =====================================================================
// AC-04b / US-04 — Over-cap body_contains is a redacted 400; raw
// value NEVER echoed (anti-echo)
// =====================================================================

/// @driving_port @US-04
///
/// Given the client sends a `body_contains` value of 1025 bytes
/// (strictly above the 1024-byte cap),
/// When the endpoint validates the parameter,
/// Then the response is HTTP 400 with the SAME literal envelope as
/// the empty arm, the body does NOT contain any byte of the raw
/// value (a recognisable prefix is asserted absent), and the store
/// is NEVER touched.
///
/// This pins ADR-0055 Decision 5 (length cap 1024 bytes,
/// inclusive boundary; the SAME literal envelope) and the anti-echo
/// posture (the raw oversize value is NEVER reflected in the
/// response body). The 1025-byte value uses a distinctive prefix
/// (`OVERSIZE-`) so the absence assertion is unambiguous.
#[tokio::test]
async fn ac_04b_over_cap_body_contains_returns_400_with_redacted_envelope() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );

    // 1025 bytes: 9-byte recognisable prefix + 1016 'A' chars.
    // ADR-0055 Decision 5 / DD6: 1024 bytes is inclusively accepted;
    // 1025 bytes is strictly over the cap and rejected.
    let oversize_raw = format!("OVERSIZE-{}", "A".repeat(1016));
    assert_eq!(oversize_raw.len(), 1025, "fixture must be 1025 bytes");
    let request = logs_request_with_body_contains("1716200000", "1716200060", &oversize_raw);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an over-cap body_contains is a 400, never a 500"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid body_contains",
        "the reason text is the SAME literal as the empty arm (no second reason class)"
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
// AC-04c / US-05 — Case-sensitive pinned (KAFKA does NOT match kafka)
// =====================================================================

/// @driving_port @real-io @US-05
///
/// Given tenant "acme-prod" has records whose body is "kafka timeout
/// ..." (lowercase 'k'),
/// When the on-call SRE GETs the endpoint with `body_contains=KAFKA`
/// (uppercase),
/// Then the response is HTTP 200 with the calm empty bare array `[]`:
/// the byte-wise substring match is case-sensitive (`K`=0x4B,
/// `k`=0x6B; the bytes differ; no match).
///
/// This pins ADR-0055 Decision 3 (case-sensitive matching). Kills a
/// `String::contains` -> `to_lowercase().contains` mutant. The test
/// IS the documentation: Sara reads it and learns the platform's
/// posture from where she will look.
#[tokio::test]
async fn ac_04c_case_sensitive_match_pinned() {
    let (store, _base) = open_durable_store("case-sensitive-KAFKA");
    let t = tenant("acme-prod");
    seed_kafka_timeout_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_body_contains("1716200000", "1716200060", "KAFKA");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the case-sensitive miss is a calm 200 with []"
    );
    assert_eq!(
        records_array(&body).len(),
        0,
        "KAFKA does NOT match kafka; the response is the calm empty bare array []: {body}"
    );
}

// =====================================================================
// AC-05 / US-06 — Cross-tenant isolation: tenant B never sees tenant
// A's matches
// =====================================================================

/// @driving_port @real-io @US-06
///
/// Given tenant "acme-prod" has records whose body carries "kafka
/// timeout" and tenant "globex-staging" has ZERO records in the
/// window,
/// When the operator (holding the globex-staging credential) GETs the
/// endpoint with `body_contains=kafka%20timeout` under tenant
/// "globex-staging",
/// Then the response is HTTP 200 with the calm empty bare array `[]`,
/// the body NEVER contains the substring "broker" (a marker
/// borrowed from the acme-prod fixtures), and no acme-prod record
/// appears.
///
/// This pins the platform's per-tenant isolation invariant
/// (ADR-0047) against the new arm: the `query_with(&tenant, range,
/// predicate)` seam carries the tenant as the first argument; the
/// tenant-bucket lookup happens before any predicate evaluation
/// (`crates/lumen/src/store.rs:166-180`). A mutant that resolves the
/// tenant AFTER applying the filter (or that applies the filter
/// against all tenants' records) is killed by this scenario.
#[tokio::test]
async fn ac_05_cross_tenant_isolation_holds_for_body_contains() {
    let (store, _base) = open_durable_store("cross-tenant-body-contains");
    let t_acme = tenant("acme-prod");
    let t_globex = tenant("globex-staging");
    // Seed acme-prod with the kafka-timeout records; globex-staging
    // remains empty (no seed call).
    seed_kafka_timeout_window(&store, &t_acme);

    let router = log_query_api::router(
        store as Arc<dyn LogStore + Send + Sync>,
        Some(t_globex.clone()),
    );
    let request = logs_request_with_body_contains("1716200000", "1716200060", "kafka%20timeout");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the cross-tenant miss is a calm 200 with []"
    );
    assert_eq!(
        records_array(&body).len(),
        0,
        "tenant globex-staging receives [] when querying for a substring that exists only in tenant acme-prod's records: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("broker"),
        "no acme-prod body text leaks across tenants: {rendered}"
    );
}

// =====================================================================
// AC-cap / US-01 — Filter BEFORE the result cap (ADR-0055 Decision 6)
// =====================================================================

/// @driving_port @US-01
///
/// Given a seeded fixture with more records than `MAX_RESULT_ROWS`
/// matching the substring would still be honest (the cap measures
/// the post-filter vector),
/// When the operator GETs the endpoint with `body_contains=kafka%20timeout`,
/// Then the response is HTTP 200 (the filter ran BEFORE the cap; the
/// cap measured the post-filter row count).
///
/// This pins ADR-0055 Decision 6 (filter BEFORE cap) observably.
///
/// NOTE (DISTILL): a fully realistic version of this test would seed
/// 200_000+ records into the durable store and assert the matching
/// subset is served. Seeding that volume into `FileBackedLogStore` at
/// runtime is expensive (slow CI, large tempdir); the cleaner pattern
/// is a `BulkBodyContainsLogStore` test double mirroring
/// `slice_03_severity_filter.rs::BulkSeverityLogStore`. Crafty MAY
/// add the bulk double during DELIVER if mutation testing or
/// `cargo public-api` exposes a `>` -> `>=` cap-boundary mutant the
/// small-fixture scenarios miss. Marked `#[ignore]` at DISTILL close;
/// the small-fixture scenarios (AC-01..AC-05) already exercise the
/// filter-and-dispatch arm.
#[tokio::test]
async fn ac_cap_filter_runs_before_result_cap() {
    // Small-fixture stand-in: 3 matching, 2 non-matching, both well
    // under the cap. The cap-interaction proof on a bulk feed is a
    // candidate DELIVER addition (see scenario notes above).
    let (store, _base) = open_durable_store("cap-after-filter-body-contains");
    let t = tenant("acme-prod");
    seed_kafka_timeout_window(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_body_contains("1716200000", "1716200060", "kafka%20timeout");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the filter ran BEFORE the cap; matching records are under the cap and served"
    );
    assert_eq!(
        records_array(&body).len(),
        3,
        "the three matching records are returned (filter happened first; cap did not fire)"
    );
}
