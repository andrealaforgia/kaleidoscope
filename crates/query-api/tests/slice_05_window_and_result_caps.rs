// Kaleidoscope query-api — slice 05 honest-read-caps acceptance suite
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
//! cap (100_000 matrix entries) on `/api/v1/query_range`.
//!
//! Maps to `docs/feature/honest-read-caps-v0/discuss/user-stories.md`
//! (US-01 window cap on query-api, US-04 result cap on query-api,
//! US-05 redaction on cap reasons for query-api). Contract pinned by
//! ADR-0050 (`docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`).
//!
//! The user-centric outcome: when the operator (or a misconfigured
//! Grafana dashboard) asks for too wide a window or would receive too
//! many series, the metrics read API refuses with a named 400 carrying
//! the existing `{status:"error", error:"..."}` envelope. The refusal
//! is OUT LOUD, never silently truncated, never `X-Truncated`, never a
//! calm empty 200. The store is never touched on the window-cap
//! refusal path; the store IS touched exactly once on the result-cap
//! refusal path (the handler must know the size to refuse) but
//! serialisation is never attempted.
//!
//! For query-api specifically (ADR-0050 Decision 4 / DESIGN D5), the
//! result cap measures the FINAL MATRIX-ENTRY COUNT, after
//! `matrix::to_matrix(rows)` runs, not the upstream raw row count. The
//! count is what the user observes in `data.result.length`; that is
//! the count the cap defends. The DELIVER must implement the check
//! after `to_matrix` and before `success_response`.
//!
//! Boundary discipline (ADR-0050 Decision 1 and 2): the boundary is
//! `>`, never `>=`. A window of exactly `MAX_WINDOW_SECONDS` (86_400)
//! is served; one second wider is refused. A result of exactly
//! `MAX_RESULT_ROWS` (100_000) matrix entries is served; one more is
//! refused.
//!
//! Redaction posture (ADR-0050 Decision 7): the cap-400 body inherits
//! the query-api posture from ADR-0042 Decision 9. The body must never
//! echo the raw window values, the raw query, the raw pattern, nor a
//! forwarded `Authorization` / "SECRET" / "Bearer" value.
//!
//! RED state (behavioural, not compile-level): every scenario in this
//! file COMPILES against the current `query-api` source. The
//! scenarios that expect a 400 fail today because no cap check exists
//! in the handler; the scenarios that expect a 200 pass today because
//! the request happens to be within an unenforced cap. The DELIVER
//! wave adds the two `pub const` and the two `if` arms per ADR-0050,
//! at which point every scenario goes green.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, gauge, open_durable_store, point, prism_accepts_error, prism_accepts_success,
    query_range_request, query_range_request_with_auth, result_series, secs_to_nanos, tenant,
    FailingStore,
};
use pulse::{
    IngestReceipt, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    MetricStoreError, Predicate, TimeRange,
};

// ---------------------------------------------------------------------
// A driven-port test double that returns a configurable, large `Vec`
// of synthetic `(Metric, MetricPoint)` rows from `query`. Each row
// carries a distinct point attribute `seq` so the merged label set
// (`resource_attributes UNION point.attributes UNION {__name__}`) is
// distinct for every row, which means `matrix::to_matrix` folds the
// rows into exactly `count` matrix entries.
//
// The cap that query-api enforces is on the FINAL MATRIX-ENTRY COUNT
// (DESIGN D5; ADR-0050 Decision 4), so seeding `count` rows with
// distinct labels yields `count` matrix entries and the cap-check sees
// exactly the value the user would have observed in the body.
//
// The double is a test adapter for the `pulse::MetricStore` driven
// port, not an internal query-api component, so driving the router
// through `router(...)` over it still honours the hexagonal boundary.
// ---------------------------------------------------------------------

/// A store whose `query` returns exactly `count` synthetic rows for
/// any tenant, metric, and range. Each row carries a distinct `seq`
/// point attribute so the rows fold into `count` distinct matrix
/// entries. Ingest is disabled (this is a read service). `query_with`
/// mirrors `query` (the handler at slice 02 only uses `query`).
struct BulkStore {
    count: usize,
}

impl BulkStore {
    fn new(count: usize) -> Self {
        Self { count }
    }
}

fn synthetic_row(seq: usize) -> (Metric, MetricPoint) {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    let mut attrs = BTreeMap::new();
    // The `seq` attribute makes each merged label set distinct, so
    // `to_matrix` produces one entry per row (DD4 grouping by merged
    // label set).
    attrs.insert("seq".to_string(), format!("{seq}"));
    let metric = Metric {
        name: MetricName::new("synthetic_total"),
        description: "acceptance bulk gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points: Vec::new(),
        resource_attributes: resource,
    };
    let point = MetricPoint {
        time_unix_nano: 1_716_200_000_000_000_000,
        start_time_unix_nano: 0,
        attributes: attrs,
        value: 1.0,
    };
    (metric, point)
}

impl MetricStore for BulkStore {
    fn ingest(
        &self,
        _tenant: &aegis::TenantId,
        _batch: MetricBatch,
    ) -> Result<IngestReceipt, MetricStoreError> {
        Err(MetricStoreError::PersistenceFailed {
            reason: "ingest disabled in read service".to_string(),
        })
    }

    fn query(
        &self,
        _tenant: &aegis::TenantId,
        _metric_name: &MetricName,
        _range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        Ok((0..self.count).map(synthetic_row).collect())
    }

    fn query_with(
        &self,
        _tenant: &aegis::TenantId,
        _metric_name: &MetricName,
        _range: TimeRange,
        _predicate: &Predicate,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        Ok((0..self.count).map(synthetic_row).collect())
    }
}

// =====================================================================
// US-01 — Happy path: a window WITHIN the cap is served normally
// =====================================================================

/// @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has one in-window matching series for
/// "process_cpu_utilization" seeded into a real durable Pulse store,
/// When the operator queries that bare name over a 45-second window
/// (well within the 86_400-second cap),
/// Then she sees the matching series in the Prometheus matrix envelope.
/// The cap is invisible on well-formed queries.
#[tokio::test]
async fn a_query_range_request_within_the_window_cap_is_served_normally() {
    let (store, _base) = open_durable_store("within-window-cap");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 0.40, &[])],
            )]),
        )
        .expect("seed durable store");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("process_cpu_utilization", "1716200000", "1716200045");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        prism_accepts_success(&body),
        "Prism's isPromSuccess must accept the response: {body}"
    );
    assert_eq!(result_series(&body).len(), 1, "the one in-cap series");
}

// =====================================================================
// US-01 — Boundary: window of EXACTLY MAX_WINDOW_SECONDS is served
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given a window of exactly 86_400 seconds (start=0, end=86_400) and
/// a real durable store with no metric under the queried name in that
/// window,
/// When the operator queries a bare metric name over that exact-cap
/// window,
/// Then the response is 200 with the calm empty matrix. The boundary
/// is inclusive: `end - start == MAX_WINDOW_SECONDS` is served (NOT
/// refused).
///
/// This kills a `>` -> `>=` mutant on the window cap check.
#[tokio::test]
async fn a_query_range_request_at_exactly_the_window_cap_is_served() {
    let (store, _base) = open_durable_store("at-window-cap");
    let t = tenant("acme-prod");
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    // The bare-name query (no `{...}`) is the simplest path through
    // selector parsing.
    let request = query_range_request("process_cpu_utilization", "0", "86400");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a window of exactly 86_400 seconds is at the cap and served, not refused"
    );
    assert!(
        prism_accepts_success(&body),
        "the boundary case is a calm matrix success, never a cap-error: {body}"
    );
    assert!(
        result_series(&body).is_empty(),
        "no series, but a calm 200 empty matrix at the boundary"
    );
}

// =====================================================================
// US-01 — Window over the cap is refused BEFORE the store is touched
// =====================================================================

/// @driving_port @US-01
///
/// Given a window of 86_401 seconds (start=0, end=86401), one second
/// over the cap, and a LyingStore whose `query` always returns
/// `PersistenceFailed`,
/// When the operator queries a bare metric name over that window,
/// Then the response is 400 with the `{status:"error", error:...}`
/// envelope, the `error` string names "window" and a value-of-cap
/// substring like "exceeds 86400", and the LyingStore's `query` was
/// NEVER called (proven by the absence of the 500 that would lift if
/// the lying store had been touched).
///
/// This is the carpaccio taste-test 1 from ADR-0050: the cap fires
/// BEFORE the store. A mutant that swapped check and store would lift
/// the response to a 500 and fail this scenario.
#[tokio::test]
async fn a_window_one_second_over_the_cap_is_refused_before_the_store() {
    let store: Arc<dyn MetricStore + Send + Sync> = Arc::new(FailingStore);
    let router = query_api::router(store, Some(tenant("acme-prod")), None);
    let request = query_range_request("process_cpu_utilization", "0", "86401");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a window over the cap is a 400, never a 500: the store is NEVER touched"
    );
    assert!(
        prism_accepts_error(&body),
        "the cap refusal is the existing error envelope Prism's isPromError accepts: {body}"
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
// US-04 — Boundary: result of EXACTLY MAX_RESULT_ROWS series is served
// =====================================================================

/// @driving_port @US-04
///
/// Given a store that returns exactly 100_000 rows, each with a
/// distinct merged label set (so `to_matrix` folds them into exactly
/// 100_000 matrix entries),
/// When the operator queries the synthetic metric over a within-cap
/// window,
/// Then the response is 200 and `data.result` carries all 100_000
/// matrix entries. The boundary is inclusive: `result.len() ==
/// MAX_RESULT_ROWS` is served (NOT refused).
///
/// This kills a `>` -> `>=` mutant on the result-cap check.
#[tokio::test]
async fn a_result_at_exactly_the_result_cap_is_served() {
    let store: Arc<dyn MetricStore + Send + Sync> = Arc::new(BulkStore::new(100_000));
    let router = query_api::router(store, Some(tenant("acme-prod")), None);
    let request = query_range_request("synthetic_total", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a result of exactly 100_000 matrix entries is at the cap and served, not refused"
    );
    assert!(prism_accepts_success(&body));
    assert_eq!(
        result_series(&body).len(),
        100_000,
        "all 100_000 matrix entries are returned at the boundary"
    );
}

// =====================================================================
// US-04 — A result one entry OVER the cap is refused, never truncated
// =====================================================================

/// @driving_port @US-04
///
/// Given a store that returns 100_001 rows with distinct merged label
/// sets (so `to_matrix` produces 100_001 matrix entries),
/// When the operator queries the synthetic metric over a within-cap
/// window,
/// Then the response is 400 with `{status:"error", error:...}`, the
/// error names "result" and the cap value, and the response is NEVER
/// a truncated 200, NEVER an `X-Truncated` 200, NEVER a calm empty.
///
/// The store IS touched here (the cap fires AFTER the store; the
/// handler must know the size to refuse) but serialisation of the
/// matrix never starts: the body is the error envelope, not the
/// success envelope.
#[tokio::test]
async fn a_result_one_entry_over_the_cap_is_refused_with_a_named_400() {
    let store: Arc<dyn MetricStore + Send + Sync> = Arc::new(BulkStore::new(100_001));
    let router = query_api::router(store, Some(tenant("acme-prod")), None);
    let request = query_range_request("synthetic_total", "1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a result over the cap is a named 400, never a truncated 200"
    );
    assert!(
        prism_accepts_error(&body),
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
        !prism_accepts_success(&body),
        "the refusal must NEVER look like a calm success envelope: {body}"
    );
}

// =====================================================================
// US-05 — Redaction: cap-refused body echoes nothing sensitive
// =====================================================================

/// @driving_port @US-05
///
/// Given the operator's request carries a forwarded
/// `Authorization: Bearer SECRET` header AND a year-long window AND a
/// query "cpu_seconds_total",
/// When the query-api returns the window-cap 400,
/// Then the response body contains NONE of:
///   - the raw end value ("31536000"),
///   - the raw query text ("cpu_seconds_total"),
///   - the literal "SECRET",
///   - the literal "Bearer".
///
/// Mirrors the existing redaction posture (ADR-0042 Decision 9)
/// inherited by the new cap reason (ADR-0050 Decision 7).
#[tokio::test]
async fn the_cap_refused_body_never_echoes_raw_values_or_a_credential() {
    let store: Arc<dyn MetricStore + Send + Sync> = Arc::new(FailingStore);
    let router = query_api::router(store, Some(tenant("acme-prod")), None);
    let request =
        query_range_request_with_auth("cpu_seconds_total", "0", "31536000", "Bearer SECRET");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let rendered = body.to_string();
    assert!(
        !rendered.contains("31536000"),
        "the body must not echo the raw end value: {rendered}"
    );
    assert!(
        !rendered.contains("cpu_seconds_total"),
        "the body must not echo the raw query text: {rendered}"
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
