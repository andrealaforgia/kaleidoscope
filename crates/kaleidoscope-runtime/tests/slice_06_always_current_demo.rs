// Kaleidoscope consolidated runtime — Slice D: always-current demo, live
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

//! # Slice D — "the always-current demo is served with ZERO ingest" (always-current-demo-v0).
//!
//! ADR-0079 wires the read-side [`DemoTraceOverlay`] / [`DemoLogOverlay`] /
//! [`DemoMetricOverlay`] into the consolidated runtime's query routers, so the
//! demo telemetry is SYNTHESISED at query time (now-relative, store-free) for
//! the demo service identity — and the WRITE/ingest path keeps the RAW stores
//! (the demo has no write path). This slice proves both halves end to end on
//! ONE process over EPHEMERAL `127.0.0.1:0` ports:
//!
//!   1. With NOTHING ingested and NOTHING seeded, a demo service + now-window
//!      traces query returns the six demo traces with now-relative timestamps.
//!   2. `error=true` on that query returns EXACTLY the failed checkout (Error
//!      status + the readable "card declined" message) — WHERE.
//!   3. `with_logs` on the demo trace returns its spans AND its correlated
//!      cause log together in ONE response — WHERE + WHY.
//!   4. A metrics query for `request_count` over a now-window returns the demo
//!      `request_count` point.
//!   5. A NON-demo service query returns empty — pure pass-through, nothing
//!      synthesised.
//!   6. The WRITE path is unaffected: a REAL non-demo trace ingested over OTLP
//!      (and its correlated log) reads back through the SAME read side (window,
//!      by-id, with_logs), proving the read-side overlay decorates but never
//!      shadows the raw store the ingest sink writes to.
//!
//! FALSIFIABILITY: before the overlays are wired into `spawn_consolidated`, the
//! demo service query, the error filter, the with_logs view, and the metric
//! query all return EMPTY (nothing ingested) — assertions (1)-(4) FAIL. They go
//! GREEN only when the read side is overlaid. The write-path assertions (6)
//! fail if the overlay shadows (returns synthetic instead of delegating) the
//! raw store for a non-demo identity.

mod common;

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{poll_until, post_otlp, spawn_test_runtime, TENANT_ACME};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{
    metric::Data as MetricData, number_data_point, Metric, NumberDataPoint, ResourceMetrics,
    ScopeMetrics, Sum,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use prost::Message;

/// The demo `service.name` the overlay synthesises under (ADR-0079 identity).
const DEMO_SERVICE: &str = "kaleidoscope-demo";
/// The pinned failed-checkout demo trace id (ADR-0079 / ADR-0077 F3).
const DEMO_FAILED_TRACE_HEX: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
/// The readable failure message on the demo failed-checkout span + its cause log.
const DEMO_ERROR_MESSAGE: &str = "checkout failed: card declined";

/// A REAL, non-demo service used for the write-path-intact assertions — its
/// reads must pass straight through the overlay to the raw store.
const REAL_SERVICE: &str = "checkout-real";
/// A REAL, non-demo trace id (not any demo id) for the write-path read-back.
const REAL_TRACE_BYTES: [u8; 16] = [0x77; 16];
const REAL_TRACE_HEX: &str = "77777777777777777777777777777777";
/// The body of the REAL correlated log (distinct from the demo cause log, so a
/// shadowing/empty bug is caught).
const REAL_LOG_BODY: &str = "real order accepted, no demo here";
/// A REAL, non-demo metric name (NOT `request_count`), so its read passes
/// straight through the overlay to the raw store (no synthesis).
const REAL_METRIC: &str = "real_orders_total";

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

fn scope() -> InstrumentationScope {
    InstrumentationScope {
        name: "kaleidoscope.always-current-demo.test".to_string(),
        version: "0.0.0".to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    }
}

/// Epoch seconds for "now" (the query windows are expressed in seconds).
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

/// A now-centred window in epoch seconds wide enough to cover every demo
/// record (the demo spans sit within ~75s of now).
fn now_window() -> (u64, u64) {
    let now = now_secs();
    (now - 3_600, now + 3_600)
}

/// One REAL server span on [`REAL_TRACE_BYTES`] under [`REAL_SERVICE`],
/// timestamped 10s before now so a now-window covers it. `error` toggles Error
/// status; the correlated log shares the trace id.
fn encode_real_span() -> Vec<u8> {
    let start = (now_secs() - 10) * 1_000_000_000;
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", REAL_SERVICE)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans: vec![Span {
                    trace_id: REAL_TRACE_BYTES.to_vec(),
                    span_id: vec![0x88; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: "POST /real/orders".to_string(),
                    kind: 2, // SPAN_KIND_SERVER
                    start_time_unix_nano: start,
                    end_time_unix_nano: start + 1_000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: Some(Status {
                        message: "real ok".to_string(),
                        code: 1, // STATUS_CODE_OK
                    }),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// One REAL log carrying [`REAL_TRACE_BYTES`] (correlated to the real span) and
/// the distinct [`REAL_LOG_BODY`], timestamped 10s before now.
fn encode_real_correlated_log() -> Vec<u8> {
    let t = (now_secs() - 10) * 1_000_000_000;
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", REAL_SERVICE)],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(scope()),
                log_records: vec![LogRecord {
                    time_unix_nano: t,
                    observed_time_unix_nano: t,
                    severity_number: 9, // INFO
                    severity_text: "INFO".to_string(),
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(REAL_LOG_BODY.to_string())),
                    }),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    flags: 0,
                    trace_id: REAL_TRACE_BYTES.to_vec(),
                    span_id: vec![0x88; 8],
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// One REAL non-demo Sum metric ([`REAL_METRIC`], value 5) under
/// [`REAL_SERVICE`], timestamped 10s before now.
fn encode_real_metric() -> Vec<u8> {
    let t = (now_secs() - 10) * 1_000_000_000;
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", REAL_SERVICE)],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope()),
                metrics: vec![Metric {
                    name: REAL_METRIC.to_string(),
                    description: "real non-demo counter".to_string(),
                    unit: "1".to_string(),
                    metadata: vec![],
                    data: Some(MetricData::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: t,
                            time_unix_nano: t,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(number_data_point::Value::AsInt(5)),
                        }],
                        aggregation_temporality: 2,
                        is_monotonic: true,
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

async fn get(addr: SocketAddr, path: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}{path}"))
        .send()
        .await
        .expect("GET query endpoint over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read response body");
    (status, body)
}

/// The distinct trace_id strings in a bare span-array body.
fn trace_ids(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|spans| {
            let mut ids: Vec<String> = spans
                .iter()
                .filter_map(|s| s["trace_id"].as_str().map(str::to_string))
                .collect();
            ids.sort();
            ids.dedup();
            ids
        })
        .unwrap_or_default()
}

fn array_len(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0)
}

fn field_array_len(body: &str, field: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v[field].as_array().map(|a| a.len()))
        .unwrap_or(0)
}

/// always-current-demo-v0: with ZERO ingest the synthetic demo is served live
/// across traces (WHERE), with_logs (WHERE + WHY) and metrics, a non-demo read
/// passes straight through (empty), and the REAL write path is untouched.
/// @walking_skeleton @driving_port @real-io
///
/// ```gherkin
/// Scenario: The always-current demo is served with zero ingest, write path intact
///   Given the consolidated runtime is running for tenant "acme" with NOTHING ingested or seeded
///   When Andrea queries the demo service traces over a now-window
///   Then the six demo traces come back with now-relative timestamps
///   And error=true returns exactly the failed checkout with a readable message
///   And with_logs on the demo trace returns its spans and its cause log together
///   And a metrics query for request_count returns the demo point
///   And a non-demo service query returns empty (nothing synthesised)
///   And a REAL non-demo trace ingested over OTLP reads back unchanged
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn always_current_demo_is_served_with_zero_ingest_write_path_intact() {
    // Tenant "acme" IS the demo tenant (ADR-0079). NOTHING is ingested or
    // seeded before the demo assertions — the demo must be PRESENT regardless.
    let rt = spawn_test_runtime("always-current-demo", TENANT_ACME).await;
    let traces = rt.traces_addr();
    let metrics = rt.metrics_addr();
    let (start, end) = now_window();

    // (1) Demo service + now-window traces query returns the six demo traces.
    let (status, body) = get(
        traces,
        &format!("/api/v1/traces?service={DEMO_SERVICE}&start={start}&end={end}"),
    )
    .await;
    assert_eq!(status, 200, "demo traces query answers 200; body: {body}");
    let ids = trace_ids(&body);
    assert_eq!(
        ids.len(),
        6,
        "the six demo traces are synthesised with zero ingest; body: {body}"
    );
    assert!(
        ids.iter().any(|id| id == DEMO_FAILED_TRACE_HEX),
        "the pinned failed-checkout demo trace is present; body: {body}"
    );
    // Now-relative: every demo span sits inside the now-window (start..end secs).
    let json: serde_json::Value = serde_json::from_str(&body).expect("span array");
    for span in json.as_array().expect("array") {
        let start_nanos = span["start_time_unix_nano"]
            .as_u64()
            .or_else(|| {
                span["start_time_unix_nano"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or_else(|| panic!("a span carries start_time_unix_nano; body: {body}"));
        let start_s = start_nanos / 1_000_000_000;
        assert!(
            start_s >= start && start_s <= end,
            "demo span start {start_s}s is inside the now-window [{start},{end}]; body: {body}"
        );
    }

    // (2) error=true returns EXACTLY the failed checkout — WHERE.
    let (status, body) = get(
        traces,
        &format!("/api/v1/traces?service={DEMO_SERVICE}&start={start}&end={end}&error=true"),
    )
    .await;
    assert_eq!(status, 200, "demo error filter answers 200; body: {body}");
    let ids = trace_ids(&body);
    assert_eq!(
        ids,
        vec![DEMO_FAILED_TRACE_HEX.to_string()],
        "error=true returns ONLY the failed checkout trace; body: {body}"
    );
    let json: serde_json::Value = serde_json::from_str(&body).expect("span array");
    assert!(
        json.as_array()
            .expect("array")
            .iter()
            .any(|s| s["status"]["code"].as_str() == Some("Error")
                && s["status"]["message"].as_str() == Some(DEMO_ERROR_MESSAGE)),
        "the failed checkout carries Error status + the readable message; body: {body}"
    );

    // (3) with_logs on the demo trace returns the span AND the cause log
    // together — WHERE + WHY in one response.
    let (status, body) = get(
        traces,
        &format!("/api/v1/traces/with_logs?trace_id={DEMO_FAILED_TRACE_HEX}"),
    )
    .await;
    assert_eq!(status, 200, "demo with_logs answers 200; body: {body}");
    let json: serde_json::Value = serde_json::from_str(&body).expect("with_logs json");
    assert_eq!(
        json["trace_id"].as_str(),
        Some(DEMO_FAILED_TRACE_HEX),
        "the combined view is scoped to the demo trace; body: {body}"
    );
    assert!(
        json["spans"]
            .as_array()
            .expect("spans array")
            .iter()
            .any(|s| s["status"]["code"].as_str() == Some("Error")),
        "the failed checkout span (WHERE) rides the combined view; body: {body}"
    );
    assert!(
        json["logs"]
            .as_array()
            .expect("logs array")
            .iter()
            .any(|l| l["body"].as_str() == Some(DEMO_ERROR_MESSAGE)
                && l["trace_id"].as_str() == Some(DEMO_FAILED_TRACE_HEX)),
        "the cause log (WHY) rides the SAME view, correlated by trace id; body: {body}"
    );

    // (4) A metrics query for request_count over a now-window returns the demo
    // point (value 1).
    let (status, body) = get(
        metrics,
        &format!("/api/v1/query_range?query=request_count&start={start}&end={end}"),
    )
    .await;
    assert_eq!(status, 200, "demo metrics query answers 200; body: {body}");
    assert!(
        common::metrics_status_success(&body),
        "demo metrics query is a success; body: {body}"
    );
    assert!(
        common::metrics_contains_value(&body, "1"),
        "the demo request_count point (value 1) is synthesised; body: {body}"
    );

    // (5) A NON-demo service query returns empty — pure pass-through.
    let (status, body) = get(
        traces,
        &format!("/api/v1/traces?service=some-other-service&start={start}&end={end}"),
    )
    .await;
    assert_eq!(
        status, 200,
        "non-demo service query answers 200; body: {body}"
    );
    assert_eq!(
        array_len(&body),
        0,
        "a non-demo service synthesises NOTHING — pure pass-through; body: {body}"
    );

    // ====================================================================
    // (6) WRITE PATH INTACT: ingest a REAL non-demo trace + correlated log
    // over OTLP, then read them back through the SAME (overlaid) read side.
    // The overlay must DELEGATE (not shadow) the raw store for a non-demo
    // identity. Distinct real ids/body so a shadow/empty bug is caught.
    // ====================================================================
    assert_eq!(
        post_otlp(&rt.ingest_http_base(), "traces", encode_real_span()).await,
        200,
        "the REAL non-demo span is ingested on the raw write path"
    );
    assert_eq!(
        post_otlp(&rt.ingest_http_base(), "logs", encode_real_correlated_log()).await,
        200,
        "the REAL correlated log is ingested on the raw write path"
    );
    assert_eq!(
        post_otlp(&rt.ingest_http_base(), "metrics", encode_real_metric()).await,
        200,
        "the REAL non-demo metric is ingested on the raw write path"
    );

    // Window read-back (exercises the read-side trace `query` delegation).
    let real_window = format!("/api/v1/traces?service={REAL_SERVICE}&start={start}&end={end}");
    let (_e, status, body) = poll_until(
        Duration::from_secs(10),
        || get(traces, &real_window),
        |s, b| s == 200 && trace_ids(b).iter().any(|id| id == REAL_TRACE_HEX),
    )
    .await;
    assert_eq!(
        status, 200,
        "real window read-back answers 200; body: {body}"
    );
    assert!(
        trace_ids(&body).iter().any(|id| id == REAL_TRACE_HEX),
        "the REAL trace reads back by window through the overlay (delegation); body: {body}"
    );

    // By-id read-back (exercises the read-side trace `get_trace` delegation).
    let (status, body) = get(
        traces,
        &format!("/api/v1/traces/by_id?trace_id={REAL_TRACE_HEX}"),
    )
    .await;
    assert_eq!(
        status, 200,
        "real by-id read-back answers 200; body: {body}"
    );
    assert!(
        array_len(&body) >= 1,
        "the REAL trace reads back by id through the overlay (delegation); body: {body}"
    );

    // with_logs read-back: the REAL correlated log must come through the
    // read-side log `query` delegation (the demo cause log carries a DIFFERENT
    // trace id, so it is filtered out — only the real log can satisfy this).
    let (status, body) = get(
        traces,
        &format!("/api/v1/traces/with_logs?trace_id={REAL_TRACE_HEX}"),
    )
    .await;
    assert_eq!(
        status, 200,
        "real with_logs read-back answers 200; body: {body}"
    );
    assert!(
        field_array_len(&body, "spans") >= 1,
        "the REAL span rides the combined view (get_trace delegation); body: {body}"
    );
    let json: serde_json::Value = serde_json::from_str(&body).expect("with_logs json");
    assert!(
        json["logs"]
            .as_array()
            .expect("logs array")
            .iter()
            .any(|l| l["body"].as_str() == Some(REAL_LOG_BODY)
                && l["trace_id"].as_str() == Some(REAL_TRACE_HEX)),
        "the REAL correlated log reads back (log query delegation), not shadowed by the demo; body: {body}"
    );

    // Real metric read-back (exercises the read-side metric `query` delegation
    // on a NON-demo metric name: pure pass-through, no synthesis).
    let real_metric_path =
        format!("/api/v1/query_range?query={REAL_METRIC}&start={start}&end={end}");
    let (_e, status, body) = poll_until(
        Duration::from_secs(10),
        || get(metrics, &real_metric_path),
        |s, b| s == 200 && common::metrics_contains_value(b, "5"),
    )
    .await;
    assert_eq!(
        status, 200,
        "real metric read-back answers 200; body: {body}"
    );
    assert!(
        common::metrics_contains_value(&body, "5"),
        "the REAL non-demo metric (value 5) reads back through the overlay (delegation); body: {body}"
    );
}
