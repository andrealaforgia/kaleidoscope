// Kaleidoscope consolidated runtime — shared acceptance-test helpers
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

//! Shared helpers for the in-process live-visibility acceptance suite.
//!
//! Every scenario drives the RUNNING consolidated runtime through its driving
//! ports: it `spawn_consolidated(..)`s the composition root in the TEST
//! process on EPHEMERAL `127.0.0.1:0` ports (never the fixed
//! 4317/4318/9090/9091/9092 — the fixed-port flake, project memory
//! `aperture_fixed_port_4317_flake`), POSTs a real OTLP body to the ingest
//! HTTP listener, then GETs the query routers over loopback — all in ONE
//! process, with NO second process and NO store drop/reopen between send and
//! query. That single-process write-then-read is the load-bearing proof the
//! sink and router hold the SAME `Arc<Store>` (ADR-0076 Enforcement).
//!
//! ## DISTILL RED-not-BROKEN
//!
//! `kaleidoscope_runtime::spawn_consolidated` is a `__SCAFFOLD__` panic until
//! DELIVER, so `spawn_test_runtime` panics at runtime. That panic is the
//! canonical RED state (mirroring aperture's DISTILL `tests/common/mod.rs`):
//! the crate COMPILES, the scenarios FAIL on the missing live runtime, NOT on
//! a compile error. Scenarios are `#[ignore]`d to keep trunk green; run them
//! with `cargo test -p kaleidoscope-runtime -- --ignored` to observe RED.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aegis::{load_catalogue, Validator, ValidatorConfig};
use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};
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
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;

/// The local-experiment tenant the runtime is configured for.
pub const TENANT_ACME: &str = "acme";
/// A second tenant used as the cross-tenant negative control.
pub const TENANT_GLOBEX: &str = "globex";

/// A concrete instant for the ingested telemetry, in epoch SECONDS. Query
/// windows are expressed in epoch seconds (every query router parses
/// start/end as epoch seconds); the OTLP bodies stamp `T_SECONDS * 1e9` nanos,
/// so a seconds window bracketing `T_SECONDS` always covers the record.
pub const T_SECONDS: u64 = 1_717_000_000;
/// `T_SECONDS` in nanoseconds — the OTLP `time_unix_nano` the bodies carry.
pub const T_NANOS: u64 = T_SECONDS * 1_000_000_000;

/// The metric name the story sends and queries back.
pub const METRIC_NAME: &str = "request_count";
/// The log body the story sends and queries back.
pub const LOG_BODY: &str = "checkout failed: card declined";
/// The trace id (32 hex chars = 16 bytes) the story sends and queries back.
pub const TRACE_ID_HEX: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
/// `TRACE_ID_HEX` as raw bytes for the OTLP span.
pub const TRACE_ID_BYTES: [u8; 16] = [
    0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e, 0x47, 0x36,
];
/// The span name + the service the trace is filed under (the window route
/// requires a `service` parameter).
pub const SPAN_NAME: &str = "GET /api/v1/query_range";
pub const SERVICE_NAME: &str = "checkout";

// JWT constants for the optional-read-auth fail-closed scenario.
pub const AUTH_ISSUER: &str = "acme-observability";
pub const AUTH_AUDIENCE: &str = "kaleidoscope-query";
pub const AUTH_SECRET: &[u8] = b"consolidated-runtime-read-auth-test-secret-not-for-production";

// =========================================================================
// Pillar root
// =========================================================================

/// A fresh, empty pillar root under the OS temp dir, unique per call.
pub fn fresh_pillar_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let pid = std::process::id();
    let mut path = std::env::temp_dir();
    path.push(format!("kal-consolidated-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&path).expect("mkdir pillar root");
    path
}

// =========================================================================
// Runtime lifecycle (the driving entry — RED until DELIVER)
// =========================================================================

/// A running consolidated runtime plus the pillar root it owns (kept alive so
/// the temp dir is not reclaimed mid-test).
pub struct TestRuntime {
    pub runtime: RunningRuntime,
    _pillar_root: PathBuf,
}

impl TestRuntime {
    pub fn ingest_http_base(&self) -> String {
        format!("http://{}", self.runtime.ingest_http_addr)
    }
    pub fn metrics_addr(&self) -> SocketAddr {
        self.runtime.metrics_query_addr
    }
    pub fn logs_addr(&self) -> SocketAddr {
        self.runtime.logs_query_addr
    }
    pub fn traces_addr(&self) -> SocketAddr {
        self.runtime.traces_query_addr
    }
    pub fn ingest_grpc_addr(&self) -> SocketAddr {
        self.runtime.ingest_grpc_addr
    }
}

/// Spawn a consolidated runtime on EPHEMERAL ports for `tenant`, with a fresh
/// empty pillar root. RED-not-BROKEN: panics inside `spawn_consolidated` until
/// DELIVER wires the composition.
pub async fn spawn_test_runtime(label: &str, tenant: &str) -> TestRuntime {
    let pillar_root = fresh_pillar_root(label);
    let config = ConsolidatedConfig::for_ephemeral_test(pillar_root.clone(), tenant);
    let runtime = spawn_consolidated(config)
        .await
        .expect("consolidated runtime spawns on ephemeral ports");
    TestRuntime {
        runtime,
        _pillar_root: pillar_root,
    }
}

/// Spawn with a custom config (the isolation / read-auth / fail-closed
/// scenarios tweak tenants, auth, or a deliberately-occupied port).
pub async fn spawn_test_runtime_with(_label: &str, config: ConsolidatedConfig) -> TestRuntime {
    let pillar_root = config.pillar_root.clone();
    let runtime = spawn_consolidated(config)
        .await
        .expect("consolidated runtime spawns on ephemeral ports");
    TestRuntime {
        runtime,
        _pillar_root: pillar_root,
    }
}

/// Try to spawn, returning the `Result` so the fail-closed scenario can assert
/// a refusal rather than a successful boot.
pub async fn try_spawn(config: ConsolidatedConfig) -> Result<RunningRuntime, String> {
    spawn_consolidated(config).await.map_err(|e| e.to_string())
}

// =========================================================================
// OTLP body encoders (one signal each, stamped at T_NANOS, no tenant.id ->
// filed under the runtime's default ingest tenant)
// =========================================================================

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

/// One `request_count` Sum data point with value `1` at `T_NANOS`.
pub fn encode_metric_request_count_one() -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope()),
                metrics: vec![Metric {
                    name: METRIC_NAME.to_string(),
                    description: "minimal sum metric".to_string(),
                    unit: "1".to_string(),
                    metadata: vec![],
                    data: Some(MetricData::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: T_NANOS,
                            time_unix_nano: T_NANOS,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(number_data_point::Value::AsInt(1)),
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

/// One log record carrying [`LOG_BODY`] at `T_NANOS`.
pub fn encode_log_checkout_failed() -> Vec<u8> {
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
                        value: Some(any_value::Value::StringValue(LOG_BODY.to_string())),
                    }),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    flags: 0,
                    trace_id: vec![],
                    span_id: vec![],
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// One server span on trace [`TRACE_ID_HEX`], service [`SERVICE_NAME`], around
/// `T_NANOS`.
pub fn encode_trace_span() -> Vec<u8> {
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
                    status: None,
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

// =========================================================================
// Driving the listeners over loopback
// =========================================================================

/// POST an OTLP protobuf body to the ingest HTTP listener
/// (`{base}/v1/{signal}`) and return the HTTP status. Mirrors aperture's
/// `post_otlp_protobuf`.
pub async fn post_otlp(base_url: &str, signal: &str, body: Vec<u8>) -> u16 {
    reqwest::Client::new()
        .post(format!("{base_url}/v1/{signal}"))
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("POST OTLP body to the ingest HTTP listener over loopback")
        .status()
        .as_u16()
}

/// GET `/api/v1/query_range` for [`METRIC_NAME`] over a window covering T.
pub async fn get_query_range(addr: SocketAddr) -> (u16, String) {
    let (start, end) = window_seconds();
    let url =
        format!("http://{addr}/api/v1/query_range?query={METRIC_NAME}&start={start}&end={end}");
    get(&url).await
}

/// GET `/api/v1/logs` over a window covering T.
pub async fn get_logs(addr: SocketAddr) -> (u16, String) {
    let (start, end) = window_seconds();
    get(&format!(
        "http://{addr}/api/v1/logs?start={start}&end={end}"
    ))
    .await
}

/// GET `/api/v1/traces` (window route) over a window covering T, for the
/// seeded service.
pub async fn get_traces_window(addr: SocketAddr) -> (u16, String) {
    let (start, end) = window_seconds();
    get(&format!(
        "http://{addr}/api/v1/traces?service={SERVICE_NAME}&start={start}&end={end}"
    ))
    .await
}

/// GET `/api/v1/traces/by_id` (point-lookup route) for a trace id.
pub async fn get_trace_by_id(addr: SocketAddr, trace_id: &str) -> (u16, String) {
    get(&format!(
        "http://{addr}/api/v1/traces/by_id?trace_id={trace_id}"
    ))
    .await
}

fn window_seconds() -> (u64, u64) {
    (T_SECONDS - 3_600, T_SECONDS + 3_600)
}

async fn get(url: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("GET query endpoint over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read query response body");
    (status, body)
}

// =========================================================================
// Response shape assertions (business outcomes, not transport details)
// =========================================================================

/// The number of result series in a `query_range` success body
/// (`data.result.length`).
pub fn metrics_result_len(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["data"]["result"].as_array().map(|a| a.len()))
        .unwrap_or(0)
}

/// Whether a `query_range` body reports `status: success`.
pub fn metrics_status_success(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .map(|v| v["status"] == "success")
        .unwrap_or(false)
}

/// Whether the metrics body carries a point with the string value `value` in
/// any series' `values` array (Prometheus matrix shape `[[ts, "v"]]`).
pub fn metrics_contains_value(body: &str, value: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["data"]["result"].as_array().cloned())
        .map(|series| {
            series.iter().any(|s| {
                s["values"]
                    .as_array()
                    .map(|vals| vals.iter().any(|pt| pt[1] == value))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// The number of records/spans in a logs or traces success body (a bare JSON
/// array).
pub fn array_len(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0)
}

// =========================================================================
// Freshness polling (the live-visibility loop tolerates async accept)
// =========================================================================

/// Poll `f` until it returns a body satisfying `done`, or `timeout` elapses.
/// Returns the elapsed time to first success (the freshness measurement) and
/// the final `(status, body)`.
pub async fn poll_until<F, Fut>(
    timeout: Duration,
    mut f: F,
    done: impl Fn(u16, &str) -> bool,
) -> (Duration, u16, String)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = (u16, String)>,
{
    let start = Instant::now();
    loop {
        let (status, body) = f().await;
        if done(status, &body) {
            return (start.elapsed(), status, body);
        }
        if start.elapsed() >= timeout {
            return (start.elapsed(), status, body);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

// =========================================================================
// Optional read-auth validator (the fail-closed-when-configured scenario)
// =========================================================================

/// Build a read-auth validator (audience `kaleidoscope-query`) over a
/// catalogue holding `acme` and `globex`, via the production `load_catalogue`
/// (real TOML I/O), mirroring the read-auth slice tests.
pub fn read_auth_validator() -> Arc<Validator> {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let cat_path =
        std::env::temp_dir().join(format!("kal-consolidated-read-auth-cat-{stamp}.toml"));
    std::fs::write(
        &cat_path,
        format!("[[tenants]]\nid = \"{TENANT_ACME}\"\n\n[[tenants]]\nid = \"{TENANT_GLOBEX}\"\n"),
    )
    .expect("write catalogue");
    let catalogue = load_catalogue(&cat_path).expect("load catalogue");
    let _ = std::fs::remove_file(&cat_path);
    Arc::new(Validator::new(ValidatorConfig {
        issuer: AUTH_ISSUER.to_string(),
        audience: AUTH_AUDIENCE.to_string(),
        hs256_key: AUTH_SECRET.to_vec(),
        catalogue,
    }))
}

/// Bind and KEEP a loopback TcpListener, returning its address and the live
/// listener. The fail-closed-startup scenario hands this occupied address to
/// the runtime as one of the five binds so the bind conflicts and startup must
/// refuse (no half-up process).
pub async fn occupy_loopback_port() -> (SocketAddr, tokio::net::TcpListener) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback port to occupy");
    let addr = listener.local_addr().expect("read back occupied addr");
    (addr, listener)
}
