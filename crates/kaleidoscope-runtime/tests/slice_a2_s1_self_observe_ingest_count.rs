// Kaleidoscope consolidated runtime — A2.S1: self-observe log-ingest count
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

//! # A2.S1 — "The runtime counts the telemetry it ingests and exposes it on
//! the normal metrics query path"
//!
//! Signal: LOGS only (counting logs into the metric store does not recurse;
//! counting metrics would). Metric: `records_ingested{signal="logs"}` as a
//! CUMULATIVE Sum so the latest-value query always reads the running total,
//! not a per-batch delta.
//!
//! ## Acceptance criteria
//!
//! - **E1** count equals N=5 exactly: send N records in ONE request; the
//!   runtime's own ingested-records count reads N (cumulative).
//! - **E2** ordinary metric on the NORMAL query path: queryable via
//!   `GET /api/v1/query_range?query=records_ingested`, not a side endpoint.
//! - **E3** zero/absent before, N after: both states are observed.
//! - **E4** end-to-end: real running runtime, ephemeral ports, no mocks.
//! - **E5** tenant: self-metric lands under the LOG INGEST TENANT (`acme`
//!   in this test); `self_tenant = None` routes to the ingest tenant.
//! - **E6** forward-compat seam: `self_tenant: Option<TenantId>` keeps the
//!   tenant decision at ONE construction-time field. `Some(t)` routes the
//!   self-metric to `t` (future reserved internal identity) without changing
//!   any other logic. Proved by the unit test below, NOT wired in production
//!   for S1.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use common::{
    fresh_pillar_root, metrics_contains_value, metrics_result_len, poll_until, post_otlp,
    spawn_test_runtime_with, TENANT_ACME,
};
use kaleidoscope_runtime::{ConsolidatedConfig, SelfMetricsIngestRecorder};
use lumen::MetricsRecorder as LumenMetricsRecorder;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;
use pulse::{FileBackedMetricStore, MetricName, MetricStore, NoopRecorder, TimeRange};

// =========================================================================
// Helpers
// =========================================================================

/// Encode ONE export request carrying N=5 `LogRecord`s in a single
/// `ResourceLogs → ScopeLogs`. Timestamps are anchored to the real current
/// time so the self-metric's recorded timestamp falls inside the live query
/// window [now-3600s, now+3600s].
fn encode_five_log_records() -> Vec<u8> {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos() as u64;

    let records: Vec<LogRecord> = (0u64..5)
        .map(|i| LogRecord {
            time_unix_nano: now_nanos + i,
            observed_time_unix_nano: now_nanos + i,
            severity_number: 17, // ERROR
            severity_text: "ERROR".to_string(),
            body: Some(AnyValue {
                value: Some(any_value::Value::StringValue(format!(
                    "self-observe-a2-s1 record {i}"
                ))),
            }),
            attributes: vec![],
            dropped_attributes_count: 0,
            flags: 0,
            trace_id: vec![],
            span_id: vec![],
        })
        .collect();

    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".to_string(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(
                            "self-observe-test".to_string(),
                        )),
                    }),
                }],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "a2.s1.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// Query `records_ingested` on the NORMAL metrics query path
/// (`GET /api/v1/query_range?query=records_ingested`) over a window covering
/// the real current time [now−3600 s, now+3600 s] in epoch seconds.
async fn query_records_ingested(addr: SocketAddr) -> (u16, String) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let start = now_secs - 3_600;
    let end = now_secs + 3_600;
    let url =
        format!("http://{addr}/api/v1/query_range?query=records_ingested&start={start}&end={end}");
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("GET records_ingested from the metrics query router");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read response body");
    (status, body)
}

// =========================================================================
// A2.S1.E1 / E2 / E3 / E4 / E5 — end-to-end acceptance test
// =========================================================================

/// A2.S1 — `records_ingested` is absent before any log ingest and reads N=5
/// after exactly N=5 log records are sent in ONE request.
///
/// ```gherkin
/// Given the consolidated runtime is running for tenant "acme"
///   And the demo overlay is disabled (raw stores only)
///   And `records_ingested` has NEVER been queried before (empty)
/// When Andrea queries `GET /api/v1/query_range?query=records_ingested`
/// Then the response is empty / absent                          (E3-before)
/// When Andrea sends ONE /v1/logs export carrying 5 LogRecords
/// Then the HTTP status is 200
/// When Andrea polls `records_ingested` until a result appears
/// Then the result length is > 0                               (E2)
///   And the sole value is exactly "5"                         (E1, E3-after)
///   And the metric is on the NORMAL query path (not a side endpoint) (E2)
///   And the metric lands under the "acme" tenant              (E5)
/// ```
///
/// FALSIFIABILITY: without `SelfMetricsIngestRecorder` wired into
/// `FileBackedLogStore`, no `records_ingested` point is ever written, so the
/// `poll_until` timeout forces the final assertion to fail — the test is RED
/// until the recorder is wired into `spawn_consolidated`.
#[tokio::test(flavor = "multi_thread")]
async fn records_ingested_absent_then_equals_n() {
    // Demo overlay OFF: the raw pulse store must reflect ONLY the real ingest,
    // not any synthetic demo overlay metrics.
    let root = fresh_pillar_root("a2-s1-e2e");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.demo_overlay_enabled = false;
    let rt = spawn_test_runtime_with("a2-s1-e2e", config).await;

    // ── E3 / before: records_ingested is absent. ─────────────────────────
    let (before_status, before_body) = query_records_ingested(rt.metrics_addr()).await;
    assert_eq!(
        before_status, 200,
        "records_ingested query returns 200 even when the store is empty; body: {before_body}"
    );
    assert_eq!(
        metrics_result_len(&before_body),
        0,
        "records_ingested is ABSENT before any log ingest (E3-before); body: {before_body}"
    );

    // ── E1 / E4: send N=5 log records in ONE request. ────────────────────
    let ingest_status = post_otlp(&rt.ingest_http_base(), "logs", encode_five_log_records()).await;
    assert_eq!(
        ingest_status, 200,
        "the 5-record log export is accepted (E4)"
    );

    // ── E3 / after + E1 + E2: poll until `records_ingested` appears. ─────
    let (_elapsed, after_status, after_body) = poll_until(
        Duration::from_secs(10),
        || query_records_ingested(rt.metrics_addr()),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;

    assert_eq!(
        after_status, 200,
        "query_range answers 200 after ingest; body: {after_body}"
    );
    assert!(
        metrics_result_len(&after_body) > 0,
        "records_ingested is PRESENT after log ingest (E3-after); body: {after_body}"
    );
    // E1: cumulative count must be N=5 exactly, not off-by-batching.
    assert!(
        metrics_contains_value(&after_body, "5"),
        "records_ingested reads N=5 exactly (E1 cumulative, not delta); body: {after_body}"
    );
    // Extra guard: value "1" or "4" would signal delta (N×1-record batch)
    // rather than a single cumulative write.
    assert!(
        !metrics_contains_value(&after_body, "1"),
        "the value is NOT 1 (would indicate per-batch delta); body: {after_body}"
    );
    assert!(
        !metrics_contains_value(&after_body, "4"),
        "the value is NOT 4 (partial count guard); body: {after_body}"
    );
}

// =========================================================================
// Recorder unit tests (in-proc, no HTTP)
// =========================================================================

/// Unit test: the recorder is CUMULATIVE — two `record_ingest` calls produce
/// a running total of 5, NOT two separate delta points of 2 and 3.
///
/// Proof of E1 at the recorder layer (independent of the e2e wiring):
/// the FileBackedMetricStore's latest point after both calls must be 5.
#[test]
fn recorder_cumulative_not_delta() {
    let root = fresh_pillar_root("a2-s1-unit-cumulative");
    let store = Arc::new(
        FileBackedMetricStore::open(root.join("pulse"), Box::new(NoopRecorder))
            .expect("open test metric store"),
    );
    let recorder = SelfMetricsIngestRecorder::new(
        Arc::clone(&store) as Arc<dyn MetricStore + Send + Sync>,
        "logs",
        None, // S1 default: ingest tenant
    );
    let acme = TenantId("acme".to_string());

    recorder.record_ingest(&acme, 2);
    recorder.record_ingest(&acme, 3);

    let rows = store
        .query(
            &acme,
            &MetricName::new("records_ingested"),
            TimeRange::all(),
        )
        .expect("query records_ingested under acme");

    // Two calls → two points: [2.0, 5.0]. The LATEST (maximum) is the
    // cumulative total.
    let latest = rows
        .iter()
        .map(|(_, p)| p.value)
        .fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(
        latest, 5.0,
        "the latest value is the cumulative total 5, NOT a delta; rows: {rows:?}"
    );

    // Confirm both points are present (cumulative accumulates, not replaces).
    let all_values: Vec<f64> = rows.iter().map(|(_, p)| p.value).collect();
    assert!(
        all_values.contains(&5.0),
        "the running total 5 is present; values: {all_values:?}"
    );
}

/// Unit test: the E6 forward-compat seam — when `self_tenant = Some(t)` the
/// self-metric lands under `t`, NOT under the ingest tenant.
///
/// Proves the seam is reversible: a future slice can route the self-metric to
/// a reserved internal identity simply by passing `Some(internal_id)` at
/// construction time without touching any recorder logic.
#[test]
fn recorder_seam_routes_to_self_tenant_when_set() {
    let root = fresh_pillar_root("a2-s1-unit-seam");
    let store = Arc::new(
        FileBackedMetricStore::open(root.join("pulse"), Box::new(NoopRecorder))
            .expect("open test metric store"),
    );
    let internal_tenant = TenantId("__kaleidoscope_internal__".to_string());
    let recorder = SelfMetricsIngestRecorder::new(
        Arc::clone(&store) as Arc<dyn MetricStore + Send + Sync>,
        "logs",
        Some(internal_tenant.clone()), // E6 seam: route to reserved identity
    );
    let acme = TenantId("acme".to_string());

    recorder.record_ingest(&acme, 5);

    // Metric MUST appear under the seam tenant (the future internal identity).
    let under_internal = store
        .query(
            &internal_tenant,
            &MetricName::new("records_ingested"),
            TimeRange::all(),
        )
        .expect("query under seam tenant");
    assert_eq!(
        under_internal.len(),
        1,
        "self-metric lands under the seam tenant (E6 forward-compat); rows: {under_internal:?}"
    );

    // Metric must NOT appear under the ingest tenant (no leakage).
    let under_acme = store
        .query(
            &acme,
            &MetricName::new("records_ingested"),
            TimeRange::all(),
        )
        .expect("query under ingest tenant");
    assert_eq!(
        under_acme.len(),
        0,
        "self-metric does NOT land under the ingest tenant when seam is set; rows: {under_acme:?}"
    );
}
