// Kaleidoscope consolidated runtime — Slice 5: SPA origin serves trace routes
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

//! # Slice 5 — "the SPA origin serves the trace query routes, same-origin" (experimentable-stack-v0).
//!
//! ADR-0077 F4 made the metrics router (:9090) ALSO serve Prism's static
//! bundle same-origin, so the browser-served SPA reaches `/api/v1/query_range`
//! with a relative path and no CORS. But the TRACE query routes lived ONLY on
//! the standalone traces listener (:9092), so the SPA — served on the metrics
//! origin — could not reach the trace data its linked view needs: a
//! `GET /api/v1/traces/with_logs` on the metrics origin fell through to the SPA
//! `index.html` static fallback (200 text/html, NOT the trace JSON), and a
//! cross-origin call to :9092 has no CORS.
//!
//! This slice proves the metrics/SPA origin now ALSO serves the trace query
//! routes, same-origin, WITHOUT breaking metrics or the SPA static fallback.
//! The whole loop runs in ONE process on EPHEMERAL `127.0.0.1:0` ports with a
//! REAL Prism-bundle directory on the filesystem (a temp dir holding an
//! `index.html`): every assertion below drives the SAME port that serves
//! `query_range` and the SPA (`metrics_addr`), never the standalone :9092.
//!
//! The four observable outcomes asserted, all on the ONE metrics/SPA origin:
//!   1. `GET /api/v1/traces/with_logs?trace_id=...` -> the trace JSON carrying
//!      BOTH the error span and the correlated log (the linked-view surface).
//!   2. `GET /api/v1/traces?service=...&error=true` -> the failed trace's spans.
//!   3. `GET /api/v1/query_range` -> the metric still answers (no regression).
//!   4. `GET /<client-route>` (unknown non-API path) -> the SPA `index.html`
//!      (the static fallback still catches client-side routes).
//!
//! FALSIFIABILITY: before the trace routes are merged onto the metrics router,
//! outcomes (1) and (2) fall through to the SPA static fallback and return the
//! `index.html` HTML (no `spans`/`logs` JSON) — outcomes (1)/(2) FAIL while
//! (3)/(4) still pass, which is exactly the regression-free RED that isolates
//! the new same-origin gateway behaviour.

mod common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use common::{
    encode_metric_request_count_one, fresh_pillar_root, metrics_contains_value,
    metrics_status_success, poll_until, post_otlp, spawn_test_runtime_with, SERVICE_NAME,
    SPAN_NAME, TENANT_ACME, TRACE_ID_BYTES, TRACE_ID_HEX, T_NANOS, T_SECONDS,
};
use kaleidoscope_runtime::ConsolidatedConfig;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use prost::Message;

/// The cause log body correlated to the error span (same trace id).
const CAUSE_LOG_BODY: &str = "checkout failed: card declined";
/// A marker baked into the temp Prism `index.html`, so the SPA-fallback
/// assertion proves the bytes came from the static bundle (NOT a route).
const SPA_MARKER: &str = "<!doctype html><title>KALEIDOSCOPE_SPA_MARKER</title>";

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
        name: "kaleidoscope.consolidated.test".to_string(),
        version: "0.0.0".to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    }
}

/// One ERROR-status server span on [`TRACE_ID_BYTES`] around `T_NANOS`.
fn encode_error_span() -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans: vec![Span {
                    trace_id: TRACE_ID_BYTES.to_vec(),
                    span_id: vec![0x11; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: SPAN_NAME.to_string(),
                    kind: 2, // SPAN_KIND_SERVER
                    start_time_unix_nano: T_NANOS,
                    end_time_unix_nano: T_NANOS + 1_000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: Some(Status {
                        message: "upstream timeout".to_string(),
                        code: 2, // STATUS_CODE_ERROR
                    }),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// One ERROR log carrying [`TRACE_ID_BYTES`] — the cause line correlated to
/// the error span above.
fn encode_correlated_log() -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(scope()),
                log_records: vec![LogRecord {
                    time_unix_nano: T_NANOS,
                    observed_time_unix_nano: T_NANOS,
                    severity_number: 17, // ERROR
                    severity_text: "ERROR".to_string(),
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(CAUSE_LOG_BODY.to_string())),
                    }),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    flags: 0,
                    trace_id: TRACE_ID_BYTES.to_vec(),
                    span_id: vec![0x11; 8],
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// A fresh temp dir holding a Prism-bundle `index.html` carrying [`SPA_MARKER`].
/// This is the REAL static bundle the metrics/SPA router serves; the SPA
/// fallback assertion proves an unknown non-API path returns these bytes.
fn fresh_prism_bundle() -> PathBuf {
    let dir = fresh_pillar_root("prism-bundle");
    std::fs::write(dir.join("index.html"), SPA_MARKER).expect("write prism index.html");
    dir
}

/// GET `path` against the metrics/SPA origin over loopback.
async fn get(addr: SocketAddr, path: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}{path}"))
        .send()
        .await
        .expect("GET against the metrics/SPA origin over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read response body");
    (status, body)
}

/// The number of elements in a named JSON array field (0 if absent / not JSON).
fn count(body: &str, field: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v[field].as_array().map(|a| a.len()))
        .unwrap_or(0)
}

/// experimentable-stack-v0: the metrics/SPA origin (:9090) ALSO serves the
/// trace query routes same-origin, so the browser-served SPA reaches the trace
/// data with relative paths and no CORS — without breaking metrics or the SPA
/// static fallback.
/// @driving_port @real-io
///
/// ```gherkin
/// Scenario: The SPA origin serves the trace query routes same-origin, live
///   Given the consolidated runtime is running for tenant "acme" with a Prism bundle served on the metrics origin
///   And Andrea sends an OTLP error span and a correlated log on trace "4bf9...4736"
///   And Andrea sends an OTLP metric "request_count"
///   When Andrea GETs "/api/v1/traces/with_logs" for "4bf9...4736" on the metrics origin
///   Then the response carries the error span and the cause log together as JSON
///   And "/api/v1/traces?service=checkout&error=true" on that origin returns the failed trace
///   And "/api/v1/query_range" on that origin still returns the metric
///   And an unknown non-API path on that origin still returns the SPA index.html
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn metrics_spa_origin_also_serves_trace_query_routes_same_origin() {
    let root = fresh_pillar_root("spa-origin-traces");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.static_dir = Some(fresh_prism_bundle());
    let rt = spawn_test_runtime_with("spa-origin-traces", config).await;

    // The SPA origin is the SAME port that serves query_range and the bundle.
    let origin = rt.metrics_addr();

    // Ingest the error span, the correlated log, and a metric over the REAL
    // ingest HTTP path.
    assert_eq!(
        post_otlp(&rt.ingest_http_base(), "traces", encode_error_span()).await,
        200,
        "the error span is ingested"
    );
    assert_eq!(
        post_otlp(&rt.ingest_http_base(), "logs", encode_correlated_log()).await,
        200,
        "the correlated log is ingested"
    );
    assert_eq!(
        post_otlp(
            &rt.ingest_http_base(),
            "metrics",
            encode_metric_request_count_one(),
        )
        .await,
        200,
        "the metric is ingested"
    );

    // (1) The combined trace+logs route answers JSON on the SPA origin, NOT the
    // index.html fallback. Poll until both the span and the log are visible
    // through the ONE combined call on the metrics origin.
    let with_logs_path = format!("/api/v1/traces/with_logs?trace_id={TRACE_ID_HEX}");
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || get(origin, &with_logs_path),
        |s, b| s == 200 && count(b, "spans") > 0 && count(b, "logs") > 0,
    )
    .await;
    assert_eq!(
        status, 200,
        "with_logs answers 200 on the metrics/SPA origin; body: {body}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&body).expect("with_logs body is JSON on the SPA origin, not HTML");
    assert_eq!(
        json["trace_id"].as_str(),
        Some(TRACE_ID_HEX),
        "the combined response is scoped to the requested trace_id; body: {body}"
    );
    assert!(
        json["spans"]
            .as_array()
            .expect("spans array")
            .iter()
            .any(|s| s["status"]["code"].as_str() == Some("Error")),
        "the error span rides on the SPA origin's combined response; body: {body}"
    );
    assert!(
        json["logs"]
            .as_array()
            .expect("logs array")
            .iter()
            .any(|l| l["body"].as_str() == Some(CAUSE_LOG_BODY)
                && l["trace_id"].as_str() == Some(TRACE_ID_HEX)),
        "the cause log rides, correlated by trace_id, on the SPA origin; body: {body}"
    );

    // (2) The failed-trace find surface answers on the SPA origin.
    let (start, end) = (T_SECONDS - 3_600, T_SECONDS + 3_600);
    let (status, body) = get(
        origin,
        &format!("/api/v1/traces?service={SERVICE_NAME}&start={start}&end={end}&error=true"),
    )
    .await;
    assert_eq!(
        status, 200,
        "the error=true listing answers 200 on the SPA origin; body: {body}"
    );
    let spans: serde_json::Value =
        serde_json::from_str(&body).expect("traces listing is a JSON array on the SPA origin");
    let spans = spans.as_array().expect("bare span array");
    assert!(
        spans
            .iter()
            .any(|s| s["trace_id"].as_str() == Some(TRACE_ID_HEX)
                && s["status"]["code"].as_str() == Some("Error")),
        "the failed trace is reachable AS failed on the SPA origin; body: {body}"
    );

    // (3) Metrics still answer on the SAME origin (no regression).
    let (start, end) = (T_SECONDS - 3_600, T_SECONDS + 3_600);
    let query_range_path =
        format!("/api/v1/query_range?query=request_count&start={start}&end={end}");
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || get(origin, &query_range_path),
        |s, b| s == 200 && metrics_contains_value(b, "1"),
    )
    .await;
    assert_eq!(
        status, 200,
        "query_range still answers 200 on the metrics origin; body: {body}"
    );
    assert!(
        metrics_status_success(&body),
        "query_range status is success; body: {body}"
    );
    assert!(
        metrics_contains_value(&body, "1"),
        "the metric still comes back on the metrics origin; body: {body}"
    );

    // (4) The SPA static fallback still catches unknown non-API client routes.
    let (status, body) = get(origin, "/some/client/route").await;
    assert_eq!(
        status, 200,
        "an unknown non-API path is the SPA index.html (200, not 404); body: {body}"
    );
    assert!(
        body.contains("KALEIDOSCOPE_SPA_MARKER"),
        "the SPA index.html is still served for client routes; body: {body}"
    );
}
