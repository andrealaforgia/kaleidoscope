// Kaleidoscope log-query-api — slice 06 pagination acceptance suite
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

//! Pagination on the logs read path — `?limit=` and `?offset=`
//! bound HOW MANY of the already-matched, already-ordered records are
//! returned and from WHICH position, as a handler-side slice over the
//! `Vec<LogRecord>` the store returns in stable
//! `observed_time_unix_nano` order, WITHIN the existing 100000-row cap.
//!
//! Maps to `docs/feature/log-query-pagination-v0/discuss/user-stories.md`
//! (US-01 limit returns the first N; US-02 offset skips to the next
//! page; US-03 default unchanged; US-04 pagination honesty;
//! US-05a invalid limit 400; US-05b invalid offset 400; US-05c over-cap
//! limit 400; US-06 composes with filters; US-07 cross-tenant
//! isolation). Contract pinned by
//! `docs/product/architecture/adr-0057-log-query-pagination.md` and
//! `docs/feature/log-query-pagination-v0/design/`.
//!
//! The user-centric outcome: Maria Santos, on-call SRE for tenant
//! "acme-prod", mid-incident, asks for a bounded first page
//! (`?limit=50`) instead of pulling the whole 100000-row block and
//! trimming client-side with `jq '.[:50]'`; she then scrolls forward
//! one page at a time (`?limit=50&offset=50`). A `limit` of `0`,
//! negative, non-numeric, or over the 100000-row cap is refused with
//! the existing 400 envelope; a negative or non-numeric `offset` is the
//! same; an `offset` past the end of the result set is a calm empty
//! page `[]` (HTTP 200), NEVER 404. Pagination is the LAST stage of the
//! pipeline: it operates over the post-filter, per-tenant, ordered
//! vector, so filter-before-page and tenant-scope-before-page are
//! automatic.
//!
//! Every scenario drives log-query-api through its single public
//! driving port `log_query_api::router(store, tenant)` via `oneshot`
//! against a REAL durable `FileBackedLogStore` (the seeded scenarios)
//! or a counting failing store for the no-store-call assertions on the
//! invalid-`limit` / invalid-`offset` 400 arms.
//!
//! RED state (DISTILL Mandate 7): the suite COMPILES against the
//! current `log-query-api` surface (the new `LogsParams::limit` and
//! `LogsParams::offset` fields, the `parse_limit` / `parse_offset`
//! scaffolds, and the handler-side page slice are the additions in
//! this DISTILL wave). Every scenario is `#[ignore]`'d at DISTILL close
//! so the workspace pre-commit gate (`cargo test --workspace
//! --all-targets --locked`) passes. Verified RED locally: running any
//! pagination scenario with `cargo test -p log-query-api --test
//! slice_06_pagination -- --ignored` panics with
//! `__SCAFFOLD__ log-query-pagination-v0 RED` inside `parse_limit` or
//! `parse_offset`. The slice-prior scenarios (slice_01..05) stay green
//! because the no-pagination dispatch path is byte-unchanged (the
//! `(None, None)` fast path never calls either parse helper). Crafty
//! de-ignores `ac_01_limit_returns_first_n` FIRST in DELIVER (the
//! demo-able first page), then the remaining scenarios one at a time as
//! he fills the parser bodies and the slice expression.

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
// Local test helpers — pagination-aware request builders and a
// deterministic ten-record seeder.
// ---------------------------------------------------------------------

/// Build a GET request with optional `limit` and/or `offset` query-string
/// parameters appended to the canonical window request. `None` omits the
/// parameter entirely (so the `(None, None)` backward-compat path is
/// reachable); `Some(raw)` appends the raw (already URL-safe) value.
fn logs_request_paginated(
    start: &str,
    end: &str,
    limit: Option<&str>,
    offset: Option<&str>,
) -> axum::http::Request<axum::body::Body> {
    let mut uri = format!("/api/v1/logs?start={start}&end={end}");
    if let Some(l) = limit {
        uri.push_str(&format!("&limit={l}"));
    }
    if let Some(o) = offset {
        uri.push_str(&format!("&offset={o}"));
    }
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Build a GET request carrying a `min_severity` floor alongside an
/// optional `limit` (the compose-with-filter scenario).
fn logs_request_with_min_severity_and_limit(
    start: &str,
    end: &str,
    min_severity: &str,
    limit: &str,
) -> axum::http::Request<axum::body::Body> {
    let uri =
        format!("/api/v1/logs?start={start}&end={end}&min_severity={min_severity}&limit={limit}");
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// The canonical window used by every seeded scenario.
const WINDOW_START: &str = "1716200000";
const WINDOW_END: &str = "1716200600";

/// Seed ten records for `acme-prod` inside `[1716200000s, 1716200600s)`
/// in ascending observed-time order, each with a DISTINCT body so the
/// page-membership assertions are crisp. The body carries its 1-based
/// position ("rec-01" .. "rec-10") so a page can be checked for exactly
/// which records it contains, in order.
fn seed_ten_records(store: &Arc<lumen::FileBackedLogStore>, t: &aegis::TenantId) {
    seed(
        store,
        t,
        vec![
            record(1_716_200_005, "checkout", "rec-01"),
            record(1_716_200_010, "checkout", "rec-02"),
            record(1_716_200_015, "checkout", "rec-03"),
            record(1_716_200_020, "checkout", "rec-04"),
            record(1_716_200_025, "checkout", "rec-05"),
            record(1_716_200_030, "checkout", "rec-06"),
            record(1_716_200_035, "checkout", "rec-07"),
            record(1_716_200_040, "checkout", "rec-08"),
            record(1_716_200_045, "checkout", "rec-09"),
            record(1_716_200_050, "checkout", "rec-10"),
        ],
    );
}

/// Seed five records for `acme-prod` (the offset-past-end scenario).
fn seed_five_records(store: &Arc<lumen::FileBackedLogStore>, t: &aegis::TenantId) {
    seed(
        store,
        t,
        vec![
            record(1_716_200_005, "checkout", "rec-01"),
            record(1_716_200_010, "checkout", "rec-02"),
            record(1_716_200_015, "checkout", "rec-03"),
            record(1_716_200_020, "checkout", "rec-04"),
            record(1_716_200_025, "checkout", "rec-05"),
        ],
    );
}

/// A record at WARN severity (otherwise identical to `common::record`,
/// which is INFO by default), for the compose-with-filter scenario.
fn warn_record(observed_secs: u64, service: &str, body: &str) -> LogRecord {
    let mut r = record(observed_secs, service, body);
    r.severity_number = SeverityNumber::WARN;
    r.severity_text = "WARN".to_string();
    r
}

/// Seed a mixed-severity window for the compose-with-filter scenario:
/// INFO and WARN records interleaved in ascending observed-time order so
/// that `min_severity=WARN` keeps a known, ordered subset. The WARN
/// bodies are "warn-01" .. "warn-04"; the INFO bodies are "info-*".
fn seed_mixed_severity(store: &Arc<lumen::FileBackedLogStore>, t: &aegis::TenantId) {
    seed(
        store,
        t,
        vec![
            record(1_716_200_005, "checkout", "info-a"),
            warn_record(1_716_200_010, "checkout", "warn-01"),
            record(1_716_200_015, "checkout", "info-b"),
            warn_record(1_716_200_020, "checkout", "warn-02"),
            warn_record(1_716_200_025, "checkout", "warn-03"),
            record(1_716_200_030, "checkout", "info-c"),
            warn_record(1_716_200_035, "checkout", "warn-04"),
        ],
    );
}

/// The `body` strings of the returned records, in order.
fn bodies(body: &serde_json::Value) -> Vec<String> {
    records_array(body)
        .iter()
        .filter_map(|r| r["body"].as_str().map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------------
// A counting failing store. `query` and `query_with` both fail with
// PersistenceFailed and increment a call counter, so a scenario can
// assert the store was NEVER touched on an invalid-`limit` /
// invalid-`offset` 400 path.
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
// AC-01 / US-01 — Walking skeleton: a limit returns the first N records
// in order
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has ten records seeded into a REAL durable
/// Lumen store inside the window [1716200000s, 1716200600s) in ascending
/// observed-time order ("rec-01" .. "rec-10"),
/// When the on-call SRE GETs the logs endpoint for "acme-prod" over the
/// window with `limit=3`,
/// Then she sees exactly the FIRST three records ("rec-01", "rec-02",
/// "rec-03") in ascending observed_time order, and none of the
/// fourth-through-tenth records appears.
///
/// This is the demo-able outcome of the slice: the platform delivers a
/// bounded first page instead of the whole block, so Maria can eyeball
/// the leading edge of the result set before committing to a larger
/// pull. It seeds REAL durable storage (the same `FileBackedLogStore`
/// adapter the gateway writes through) so the skeleton proves wiring,
/// the read, the stable order, and the handler-side slice end to end.
///
/// RED state at DISTILL close: the handler reaches `parse_limit`, which
/// is `unimplemented!` in the scaffold, so the test panics with
/// `__SCAFFOLD__ log-query-pagination-v0 RED`. Crafty de-ignores this
/// scenario FIRST in DELIVER (walking skeleton; outer-loop convention)
/// and fills the parse helper bodies and the slice expression.
#[tokio::test]
async fn ac_01_limit_returns_first_n() {
    let (store, _base) = open_durable_store("pagination-limit-first-n");
    let t = tenant("acme-prod");
    seed_ten_records(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, Some("3"), None);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        bodies(&body),
        vec!["rec-01", "rec-02", "rec-03"],
        "the page is exactly the first three records in ascending observed_time order: {body}"
    );
}

// =====================================================================
// AC-02 / US-02 — offset skips to the next page
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has the same ten records in ascending
/// observed-time order,
/// When the on-call SRE GETs the logs endpoint over the window with
/// `limit=3` and `offset=3`,
/// Then she sees exactly the FOURTH, FIFTH, and SIXTH records
/// ("rec-04", "rec-05", "rec-06") in order, and neither the first three
/// (already seen) nor the last four appears.
///
/// Pins US-02: `offset` skips the first N records of the ordered result
/// set before `limit` is applied. `offset` counts records consumed (the
/// second page of size 3 is `offset=3`), so the boundary is clean.
#[tokio::test]
async fn ac_02_offset_skips() {
    let (store, _base) = open_durable_store("pagination-offset-skips");
    let t = tenant("acme-prod");
    seed_ten_records(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, Some("3"), Some("3"));
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        bodies(&body),
        vec!["rec-04", "rec-05", "rec-06"],
        "the second page is exactly the fourth-through-sixth records in order: {body}"
    );
}

// =====================================================================
// AC-03 / US-03 — missing pagination preserves today's behaviour
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given tenant "acme-prod" has the same ten records,
/// When the automation client GETs the logs endpoint over the window
/// with NEITHER `limit` NOR `offset`,
/// Then it receives every in-window record (all ten) in order, behaving
/// exactly as before pagination shipped.
///
/// Pins the backward-compatibility promise (US-03): the `(None, None)`
/// dispatch path is byte-unchanged; no default page size is injected.
/// This scenario stays GREEN even against the scaffold once de-ignored,
/// because the no-pagination fast path NEVER calls `parse_limit` or
/// `parse_offset` (it is the slice-prior code path).
#[tokio::test]
async fn ac_03_missing_pagination_returns_all() {
    let (store, _base) = open_durable_store("pagination-missing-returns-all");
    let t = tenant("acme-prod");
    seed_ten_records(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    // No limit, no offset — the slice-prior request shape.
    let request = logs_request(WINDOW_START, WINDOW_END);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        bodies(&body),
        vec![
            "rec-01", "rec-02", "rec-03", "rec-04", "rec-05", "rec-06", "rec-07", "rec-08",
            "rec-09", "rec-10",
        ],
        "every in-window record is returned when neither parameter is present: {body}"
    );
}

// =====================================================================
// AC-04 / US-04 — pagination honesty: pages partition the set with no
// duplicate and no gap
// =====================================================================

/// @driving_port @real-io @property @US-04
///
/// Given tenant "acme-prod" has the same ten records in a fixed, stable
/// order,
/// When the pages (offset=0, limit=5) and (offset=5, limit=5) are
/// fetched in turn,
/// Then each page is HTTP 200, the ordered concatenation of the two
/// pages equals all ten records exactly, no record appears in more than
/// one page, and no in-window record is absent from the union.
///
/// This is the slice's central correctness promise (US-04): for a fixed
/// result set the page slices partition it cleanly, resting on the
/// store's stable ascending observed-time order. The `@property` tag
/// signals the universal invariant (a clean partition for any page size
/// dividing the set); the crafter may pin it with a generator in the
/// inner loop, but the observable contract is this two-page union.
#[tokio::test]
async fn ac_04_pagination_honesty() {
    let t = tenant("acme-prod");

    // Page 1: offset=0, limit=5.
    let (store1, _b1) = open_durable_store("pagination-honesty-page1");
    seed_ten_records(&store1, &t);
    let router1 = log_query_api::router(store1 as Arc<dyn LogStore + Send + Sync>, Some(t.clone()));
    let (status1, body1) = call(
        router1,
        logs_request_paginated(WINDOW_START, WINDOW_END, Some("5"), Some("0")),
    )
    .await;
    assert_eq!(status1, StatusCode::OK);
    let page1 = bodies(&body1);

    // Page 2: offset=5, limit=5.
    let (store2, _b2) = open_durable_store("pagination-honesty-page2");
    seed_ten_records(&store2, &t);
    let router2 = log_query_api::router(store2 as Arc<dyn LogStore + Send + Sync>, Some(t));
    let (status2, body2) = call(
        router2,
        logs_request_paginated(WINDOW_START, WINDOW_END, Some("5"), Some("5")),
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    let page2 = bodies(&body2);

    // No overlap: the two pages share no record.
    for r in &page1 {
        assert!(
            !page2.contains(r),
            "record {r} appears in BOTH pages (duplicate): page1={page1:?} page2={page2:?}"
        );
    }
    // Union equals the full set in order, with no gap.
    let mut union = page1.clone();
    union.extend(page2.clone());
    assert_eq!(
        union,
        vec![
            "rec-01", "rec-02", "rec-03", "rec-04", "rec-05", "rec-06", "rec-07", "rec-08",
            "rec-09", "rec-10",
        ],
        "the ordered concatenation of the two pages equals all ten records with no duplicate and no gap"
    );
}

// =====================================================================
// AC-05a / US-05a — limit=0 is a redacted 400
// =====================================================================

/// @driving_port @US-05a
///
/// Given the handler resolves a valid tenant "acme-prod" and the window
/// parses within the cap,
/// When the SRE GETs the logs endpoint over the window with `limit=0`
/// (an uninitialised page-size variable in her script),
/// Then the response is HTTP 400 with the literal envelope
/// `{"status":"error","error":"invalid limit"}`, and the store is NEVER
/// touched on this path.
///
/// Pins ADR-0057 Decision 5 / PIN 6: `limit=0` is INVALID (a page of
/// zero records carries no information an absent request would not).
/// Kills a mutant that treats `0` as a valid empty-page request.
#[tokio::test]
async fn ac_05a_invalid_limit_zero_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, Some("0"), None);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "limit=0 is a 400, never a 500 and never a calm empty 200"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid limit",
        "the reason text is the literal class label"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the limit=0 400 path"
    );
}

// =====================================================================
// AC-05b / US-05a — non-numeric limit is a redacted 400
// =====================================================================

/// @driving_port @US-05a
///
/// Given the handler resolves a valid tenant "acme-prod" and the window
/// parses within the cap,
/// When the SRE GETs the logs endpoint over the window with `limit=abc`,
/// Then the response is HTTP 400 with the SAME literal envelope
/// `{"status":"error","error":"invalid limit"}`, and the store is NEVER
/// touched.
///
/// Pins ADR-0057 Decision 5: a non-numeric `limit` is rejected with the
/// same literal as the zero and negative arms. Kills a mutant that falls
/// through to "no limit" on a parse failure.
#[tokio::test]
async fn ac_05b_invalid_limit_nonnumeric_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, Some("abc"), None);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a non-numeric limit is a 400"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid limit",
        "the reason text is the SAME literal as the zero arm"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the non-numeric limit 400 path"
    );
}

// =====================================================================
// AC-05c / US-05a — negative limit is a redacted 400 (raw value never
// echoed)
// =====================================================================

/// @driving_port @US-05a
///
/// Given the handler resolves a valid tenant "acme-prod" and the window
/// parses within the cap,
/// When the SRE GETs the logs endpoint over the window with `limit=-5`,
/// Then the response is HTTP 400 with the SAME literal envelope, the
/// body NEVER contains the substring "-5", and the store is NEVER
/// touched.
///
/// Pins ADR-0057 Decision 5 and the anti-echo posture: a negative
/// `limit` is the same parse-failure arm as non-numeric (a leading `-`
/// is non-parseable as `usize`); the raw value is NEVER reflected.
#[tokio::test]
async fn ac_05c_invalid_limit_negative_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, Some("-5"), None);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "a negative limit is a 400");
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(message, "invalid limit");
    let rendered = body.to_string();
    assert!(
        !rendered.contains("-5"),
        "the body must NEVER echo the raw negative limit value: {rendered}"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the negative limit 400 path"
    );
}

// =====================================================================
// AC-05d / US-05c — limit strictly over the cap is a redacted 400
// (raw value never echoed; boundary inclusive at the cap)
// =====================================================================

/// @driving_port @US-05c
///
/// Given the handler resolves a valid tenant "acme-prod" and the window
/// parses within the cap,
/// When the SRE GETs the logs endpoint over the window with
/// `limit=100001` (strictly above the 100000-row cap),
/// Then the response is HTTP 400 with the literal envelope
/// `{"status":"error","error":"invalid limit"}`, the body NEVER
/// contains the substring "100001" (anti-echo: the raw value is a
/// client input, not a platform constant), and the store is NEVER
/// touched.
///
/// Pins ADR-0057 Decision 2 / FLAG 2: a `limit` over the cap is
/// REJECTED, not clamped. The boundary is INCLUSIVE (`limit=100000` is
/// served); `100001` is strictly over and refused. Kills a `>` -> `>=`
/// mutant on the over-cap check, and a clamp-to-cap mutant.
#[tokio::test]
async fn ac_05d_limit_over_cap_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, Some("100001"), None);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a limit strictly over the 100000-row cap is a 400, never a silent clamp"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid limit",
        "the reason text is the SAME literal as the other limit arms"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("100001"),
        "the body must NEVER echo the raw over-cap limit value: {rendered}"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the over-cap limit 400 path"
    );
}

// =====================================================================
// AC-06a / US-05b — non-numeric offset is a redacted 400
// =====================================================================

/// @driving_port @US-05b
///
/// Given the handler resolves a valid tenant "acme-prod" and the window
/// parses within the cap,
/// When the SRE GETs the logs endpoint over the window with
/// `offset=xyz`,
/// Then the response is HTTP 400 with the literal envelope
/// `{"status":"error","error":"invalid offset"}` (a DISTINCT literal
/// from "invalid limit"), and the store is NEVER touched.
///
/// Pins ADR-0057 Decision 5 / US-05b: a non-numeric `offset` is
/// rejected. The reason literal is "invalid offset", its own redaction
/// class, distinct from "invalid limit". Kills a mutant that conflates
/// the two parse helpers' reason texts.
#[tokio::test]
async fn ac_06a_invalid_offset_nonnumeric_returns_400() {
    let store = Arc::new(CountingFailingLogStore::new());
    let router = log_query_api::router(
        store.clone() as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("acme-prod")),
    );
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, None, Some("xyz"));
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a non-numeric offset is a 400"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid offset",
        "the reason text is the offset literal, DISTINCT from 'invalid limit'"
    );
    assert_eq!(
        store.total_store_calls(),
        0,
        "neither query nor query_with was called on the non-numeric offset 400 path"
    );
}

// =====================================================================
// AC-06b / US-02 — offset past the end is a calm empty page, not a 400
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has five records in the window in ascending
/// observed-time order,
/// When the SRE GETs the logs endpoint over the window with
/// `offset=100` (far past the five-record end),
/// Then the response is HTTP 200 with the calm empty bare array `[]`,
/// NEVER HTTP 404 and NEVER HTTP 400.
///
/// Pins ADR-0057 Decision 5 / PIN 4: an `offset` past the end of the
/// result set is a well-formed request that legitimately has no rows,
/// served as the same calm empty `[]` the contract uses for a filter
/// that matches nothing. The offset is `Ok(100)` (no upper cap); the
/// empty page is the slice's job (`skip(100)` over five records yields
/// an empty iterator), NOT a parse error. Kills a mutant that returns
/// 404 or 400 for an over-large offset.
#[tokio::test]
async fn ac_06b_offset_past_end_returns_empty() {
    let (store, _base) = open_durable_store("pagination-offset-past-end");
    let t = tenant("acme-prod");
    seed_five_records(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, None, Some("100"));
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "an offset past the end is a calm 200 with [], NEVER 404, NEVER 400"
    );
    assert_eq!(
        records_array(&body).len(),
        0,
        "the response is the calm empty bare array []: {body}"
    );
}

// =====================================================================
// AC-07 / US-06 — pagination composes with a filter (page of the
// post-filter set)
// =====================================================================

/// @driving_port @real-io @US-06
///
/// Given tenant "acme-prod" has a mixed-severity window of seven records
/// (three INFO, four WARN: "warn-01" .. "warn-04") in ascending
/// observed-time order,
/// When the SRE GETs the logs endpoint over the window with
/// `min_severity=WARN` AND `limit=2`,
/// Then the response is HTTP 200 with exactly the FIRST TWO
/// WARN-or-above records ("warn-01", "warn-02") in order, and no INFO
/// record appears.
///
/// Pins ADR-0057 / US-06: pagination is the LAST stage of the pipeline
/// (tenant -> window -> filters -> order -> page), so `limit` applies to
/// the POST-FILTER, ordered set, NOT the raw window. The first two
/// records of the WINDOW are INFO; if pagination applied before the
/// filter the page would be empty or contain INFO records. Kills a
/// mutant that paginates before filtering.
#[tokio::test]
async fn ac_07_pagination_composes_with_filter() {
    let (store, _base) = open_durable_store("pagination-composes-with-filter");
    let t = tenant("acme-prod");
    seed_mixed_severity(&store, &t);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request_with_min_severity_and_limit(WINDOW_START, WINDOW_END, "WARN", "2");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        bodies(&body),
        vec!["warn-01", "warn-02"],
        "the page is the first two WARN-or-above records of the POST-FILTER set, in order: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("info-"),
        "no INFO record appears in the response (filter applied before the page): {rendered}"
    );
}

// =====================================================================
// AC-08 / US-07 — cross-tenant isolation holds under pagination
// =====================================================================

/// @driving_port @real-io @US-07
///
/// Given tenant "acme-prod" has ten records in the window and tenant
/// "globex-staging" has ZERO records in the window,
/// When the operator (holding the globex-staging credential) GETs the
/// logs endpoint over the window with `limit=2` and `offset=0` under
/// tenant "globex-staging",
/// Then the response is HTTP 200 with the calm empty bare array `[]`,
/// the body NEVER contains the substring "rec-" (a marker borrowed from
/// the acme-prod fixtures), and no acme-prod record appears.
///
/// Pins the platform's per-tenant isolation invariant (ADR-0047)
/// against the new pagination arm: the page slice operates over the
/// per-tenant scoped vector the `query`/`query_with(&tenant, ...)` seam
/// returns (the tenant is the first argument; the bucket lookup precedes
/// any slice). A mutant that paginates a cross-tenant shared vector, or
/// applies the slice before the tenant scope, is killed by this
/// scenario.
#[tokio::test]
async fn ac_08_cross_tenant_isolation() {
    let (store, _base) = open_durable_store("pagination-cross-tenant");
    let t_acme = tenant("acme-prod");
    let t_globex = tenant("globex-staging");
    // Seed acme-prod with ten records; globex-staging remains empty.
    seed_ten_records(&store, &t_acme);

    let router = log_query_api::router(
        store as Arc<dyn LogStore + Send + Sync>,
        Some(t_globex.clone()),
    );
    let request = logs_request_paginated(WINDOW_START, WINDOW_END, Some("2"), Some("0"));
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the cross-tenant miss is a calm 200 with []"
    );
    assert_eq!(
        records_array(&body).len(),
        0,
        "tenant globex-staging receives [] when paginating an empty scoped set: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("rec-"),
        "no acme-prod record leaks across tenants under pagination: {rendered}"
    );
}
