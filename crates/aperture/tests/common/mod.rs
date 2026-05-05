//! Shared test helpers for the slice-level acceptance tests.
//!
//! These helpers mirror the harness's `tests/common/mod.rs` shape:
//! deterministic in-process encoders for OTLP messages, light fixtures
//! for starting an Aperture instance against ephemeral ports, and
//! shared assertion helpers for stderr-line matching.
//!
//! The DISTILL state of `aperture::*` is `unimplemented!()` for every
//! function, so every helper that calls into the production crate
//! panics at runtime. That panic is the canonical RED state — DELIVER
//! drives one panic away per slice, in order.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use prost::Message;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{
    metric::Data as MetricData, number_data_point, Gauge, Metric, NumberDataPoint, ResourceMetrics,
    ScopeMetrics, Sum,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::RecordingSink;
use aperture::Handle;

// =========================================================================
// Test instance lifecycle
// =========================================================================
//
// Aperture is a service. Every acceptance test starts a real instance
// on the loopback interface with ephemeral ports (so tests can run in
// parallel without contending for 4317/4318) and exercises it through
// real `tonic` and `axum` clients.
//
// The fixture below constructs the configuration, spawns the instance,
// waits for `/readyz` to return 200, and returns a `TestInstance`
// that owns the handle and the recording sink. Drop semantics: the
// `TestInstance` triggers graceful shutdown on drop, with a 5 s
// deadline. Tests that want to assert deadline-exceeded shutdown drive
// the shutdown manually.

/// A live Aperture instance bound to ephemeral loopback ports, fronted
/// by a [`RecordingSink`] the test can interrogate.
pub struct TestInstance {
    pub handle: Handle,
    pub sink: Arc<RecordingSink>,
}

impl TestInstance {
    /// The address the gRPC listener bound to (ephemeral port).
    pub fn grpc_addr(&self) -> SocketAddr {
        self.handle.grpc_addr()
    }

    /// The address the HTTP listener bound to (ephemeral port).
    pub fn http_addr(&self) -> SocketAddr {
        self.handle.http_addr()
    }

    /// The base URL of the HTTP listener (no trailing slash). Tests
    /// append `/v1/logs`, `/healthz`, etc.
    pub fn http_base_url(&self) -> String {
        format!("http://{}", self.handle.http_addr())
    }

    /// The gRPC channel target string the integration tests pass to
    /// `tonic::transport::Channel::from_shared`.
    pub fn grpc_endpoint(&self) -> String {
        format!("http://{}", self.handle.grpc_addr())
    }
}

/// Start an Aperture instance with the given configuration and a
/// fresh [`RecordingSink`]. Awaits readiness before returning.
pub async fn start_with_recording_sink(config: Config) -> TestInstance {
    let sink = Arc::new(RecordingSink::new());
    let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
    let handle = aperture::spawn(config, sink_dyn)
        .await
        .expect("aperture::spawn failed during test setup");
    handle
        .wait_until_ready()
        .await
        .expect("aperture readiness probe never reached Ready");
    TestInstance { handle, sink }
}

/// Start an Aperture instance with default ephemeral-port configuration
/// and a fresh [`RecordingSink`]. The most common entry point for the
/// slice tests.
pub async fn start_default() -> TestInstance {
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().expect("loopback ipv4 parses"))
        .http_bind_addr("127.0.0.1:0".parse().expect("loopback ipv4 parses"))
        .build()
        .expect("default test config builds");
    start_with_recording_sink(config).await
}

// =========================================================================
// OTLP message encoders
// =========================================================================
//
// Hand-crafted with `prost::Message::encode_to_vec` from the upstream
// `opentelemetry-proto` types. The shape matches the bare-minimum
// case the OTel SDK would emit. These bytes are accepted by the real
// harness's `validate_*` functions; the harness is what production
// code calls, so the tests are exercising the same wire format
// production sees.
//
// Note: `service.name` is parameterised so cap/saturation tests can
// distinguish records by source; the resource_count is always one
// (one resource per request).

/// Encode a minimal but conformant `ExportLogsServiceRequest`.
pub fn encode_logs_request(service_name: &str, record_count: usize) -> Vec<u8> {
    let mut log_records = Vec::with_capacity(record_count);
    for i in 0..record_count {
        log_records.push(LogRecord {
            time_unix_nano: 1_700_000_000_000_000_000 + i as u64,
            observed_time_unix_nano: 1_700_000_000_000_000_000 + i as u64,
            severity_number: 9, // INFO
            severity_text: "INFO".to_string(),
            body: Some(AnyValue {
                value: Some(any_value::Value::StringValue(format!("record-{i}"))),
            }),
            attributes: vec![],
            dropped_attributes_count: 0,
            flags: 0,
            trace_id: vec![],
            span_id: vec![],
        });
    }

    let req = ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", service_name)],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "aperture.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };
    req.encode_to_vec()
}

/// Encode a minimal but conformant `ExportTraceServiceRequest` with
/// `span_count` spans on a single resource.
pub fn encode_traces_request(service_name: &str, span_count: usize) -> Vec<u8> {
    let mut spans = Vec::with_capacity(span_count);
    for i in 0..span_count {
        spans.push(Span {
            trace_id: vec![1; 16],
            span_id: vec![(i & 0xFF) as u8; 8],
            trace_state: String::new(),
            parent_span_id: vec![],
            flags: 0,
            name: format!("span-{i}"),
            kind: 1, // SPAN_KIND_INTERNAL
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_000_000_010,
            attributes: vec![],
            dropped_attributes_count: 0,
            events: vec![],
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: None,
        });
    }

    let req = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", service_name)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "aperture.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };
    req.encode_to_vec()
}

/// Encode a minimal but conformant `ExportMetricsServiceRequest` with
/// one sum data point and one gauge data point. Used by the metrics
/// slice for the "two data points" UAT.
pub fn encode_metrics_request(service_name: &str) -> Vec<u8> {
    let req = ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", service_name)],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "aperture.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                metrics: vec![
                    Metric {
                        name: "request_count".to_string(),
                        description: "minimal sum metric".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(MetricData::Sum(Sum {
                            data_points: vec![NumberDataPoint {
                                attributes: vec![],
                                start_time_unix_nano: 1_700_000_000_000_000_000,
                                time_unix_nano: 1_700_000_000_000_000_010,
                                exemplars: vec![],
                                flags: 0,
                                value: Some(number_data_point::Value::AsInt(42)),
                            }],
                            aggregation_temporality: 2,
                            is_monotonic: true,
                        })),
                    },
                    Metric {
                        name: "current_temperature".to_string(),
                        description: "minimal gauge metric".to_string(),
                        unit: "Cel".to_string(),
                        metadata: vec![],
                        data: Some(MetricData::Gauge(Gauge {
                            data_points: vec![NumberDataPoint {
                                attributes: vec![],
                                start_time_unix_nano: 1_700_000_000_000_000_000,
                                time_unix_nano: 1_700_000_000_000_000_010,
                                exemplars: vec![],
                                flags: 0,
                                value: Some(number_data_point::Value::AsDouble(21.5)),
                            }],
                        })),
                    },
                ],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };
    req.encode_to_vec()
}

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

// =========================================================================
// HTTP-protobuf helpers
// =========================================================================
//
// Aperture is a service; the HTTP arm is a real axum listener on a
// loopback port. Tests POST `application/x-protobuf` bodies through
// `reqwest`. The helpers below sit between the body-encoders above and
// the response-shape assertions in each slice file.

/// Convenience wrapper around `reqwest::Client::post` that posts a
/// protobuf body to `{base}/v1/{signal}` with the canonical
/// `application/x-protobuf` content type.
pub async fn post_otlp_protobuf(
    client: &reqwest::Client,
    base_url: &str,
    signal: &str,
    body: Vec<u8>,
) -> reqwest::Response {
    client
        .post(format!("{base_url}/v1/{signal}"))
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("HTTP POST against Aperture's loopback listener should not fail")
}

// =========================================================================
// Stderr observation
// =========================================================================
//
// Several scenarios assert "stderr line contains X" or "stderr line
// with event=Y". DELIVER may wire up a configurable stderr observer
// inside the production crate (so tests can subscribe directly to the
// tracing subscriber rather than parsing JSON out of file descriptors).
// At DISTILL we declare the helper surface as the seam — the tests
// reference these helpers by name; DELIVER lands the implementation.

/// A captured structured-log line. Re-exported from
/// `aperture::testing::StderrEvent` so the assertion helpers below can
/// take a slice of the alias-friendly type.
pub type StderrEvent = aperture::testing::StderrEvent;

/// Subscribe to stderr events emitted by an Aperture instance for the
/// duration of the closure. Returns the captured events alongside the
/// closure's result.
///
/// Delegates to the production seam
/// `aperture::testing::stderr_capture` (see DESIGN
/// `component-design.md > Observability design`).
pub async fn capture_stderr_events<F, Fut, R>(f: F) -> (R, Vec<StderrEvent>)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = R>,
{
    aperture::testing::stderr_capture(f).await
}

/// Assert that the captured stderr events contain at least one event
/// with the given `event=` field. Returns the first matching event so
/// the caller can drill in on its other fields.
pub fn expect_stderr_event<'a>(events: &'a [StderrEvent], event_name: &str) -> &'a StderrEvent {
    events
        .iter()
        .find(|e| e.event == event_name)
        .unwrap_or_else(|| {
            panic!(
                "expected stderr event with event={event_name}, got events: {:?}",
                events.iter().map(|e| &e.event).collect::<Vec<_>>()
            )
        })
}

/// Assert NONE of the captured stderr events match the given event name.
pub fn expect_no_stderr_event(events: &[StderrEvent], event_name: &str) {
    if let Some(found) = events.iter().find(|e| e.event == event_name) {
        panic!("expected no stderr event with event={event_name}; found one: {found:?}");
    }
}

// =========================================================================
// Wait helpers
// =========================================================================

/// Poll `predicate` every 25 ms for up to `deadline`. Useful for
/// scenarios that need to await an asynchronous side-effect (a stderr
/// line landing, a `/readyz` flip, an in-flight count reaching zero).
pub async fn wait_for<F: Fn() -> bool>(predicate: F, deadline: Duration) {
    let started = std::time::Instant::now();
    while !predicate() {
        if started.elapsed() > deadline {
            panic!("wait_for predicate did not become true within {deadline:?}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
