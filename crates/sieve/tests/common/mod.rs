//! Shared test helpers for Sieve's slice-level acceptance tests.
//!
//! Mirrors Aperture's and Spark's `tests/common/mod.rs` shape: real
//! Aperture testing infrastructure (`aperture::testing::RecordingSink`)
//! as the inner sink that Sieve's `SamplingSink<S, N>` wraps.
//!
//! ## Strategy C "real local"
//!
//! Per `docs/feature/sieve/distill/wave-decisions.md`: Sieve's slice
//! tests use the real Aperture `RecordingSink` as the inner sink for
//! the decorator. Sieve's only "external" dependency is its inner
//! sink, and Aperture's `RecordingSink` is precisely the in-process
//! recording adapter that exercises the decorator's hand-off
//! contract. No mocks of production code; no synthetic OtlpSink
//! double.
//!
//! ## DISTILL state
//!
//! Every helper that calls into Sieve's production crate panics at
//! runtime because the production methods (`HeadSampler::sample`,
//! `SamplingSink::new`, `SamplingSink::accept`, `SamplingSink::probe`,
//! `__test_summary_tick_now`) are `unimplemented!()`. That panic is
//! the canonical RED state — DELIVER drives one panic away per
//! slice, in order.
//!
//! ## Process-global state
//!
//! Sieve has process-global state that crosses test boundaries:
//!
//! - The Tokio timer task spawned by `SamplingSink::new` — only one
//!   per `SamplingSink`, but the task's `tokio::time` interaction
//!   sees the runtime's pause/advance state.
//! - The `tracing` global subscriber — installed once per process
//!   via the `INSTALL_SUBSCRIBER` `Once` gate below.
//! - The `CAPTURED_EVENTS` Vec — mutex-guarded process-global storage
//!   for `target = "sieve"` events. Cleared between tests via
//!   [`capture_sieve_events`].
//!
//! Tests that interact with these resources serialise via
//! [`SIEVE_TEST_SERIAL`] (a process-global mutex) or via
//! `#[serial_test::serial]`. Within a single `[[test]]` binary
//! (which runs as one process), tests that share state must
//! serialise; across binaries the state is naturally isolated
//! because each binary is its own process per ADR-0015 precedent.

#![allow(dead_code)]

use std::sync::{Arc, Mutex, Once, OnceLock};

use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};
use aperture::testing::RecordingSink;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{
    metric, number_data_point::Value as NumberValue, Metric, NumberDataPoint, ResourceMetrics,
    ScopeMetrics, Sum,
};
use opentelemetry_proto::tonic::trace::v1::status::StatusCode;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};

use sieve::SamplingSink;

// =========================================================================
// Process-global serialisation lock for tests that share state.
//
// Sieve's `Counters` are process-global atomics; the timer task lives
// on the ambient Tokio runtime; the `tracing` subscriber is installed
// once per process. Tests that drive `SamplingSink::new`, the test
// seam `__test_summary_tick_now`, or env-var-driven configuration
// (`SIEVE_NON_ERROR_TRACE_RATE`, `SIEVE_SUMMARY_TICK_MS`) acquire this
// mutex for the duration of the test body to serialise.
// =========================================================================

/// Process-global serialisation mutex for Sieve tests that touch
/// shared state.
///
/// Held for the lifetime of a [`SieveTestGuard`] returned from
/// [`acquire_test_serial`]. Async-aware tests use this in addition
/// to `#[serial_test::serial]`; the latter serialises at the test
/// scheduler level, the former serialises across `.await` points
/// inside one test body when needed.
pub static SIEVE_TEST_SERIAL: Mutex<()> = Mutex::new(());

/// RAII guard for [`SIEVE_TEST_SERIAL`]. Released on drop.
pub struct SieveTestGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// Acquire the process-global Sieve test serialisation lock.
///
/// Tests that touch the process-global subscriber, the
/// `CAPTURED_EVENTS` buffer, the `SIEVE_NON_ERROR_TRACE_RATE` env
/// var, or the `SIEVE_SUMMARY_TICK_MS` env var should call this at
/// the start of the test body and hold the returned guard until the
/// test exits.
pub fn acquire_test_serial() -> SieveTestGuard {
    let g = SIEVE_TEST_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    SieveTestGuard { _guard: g }
}

// =========================================================================
// Sieve fixture: a `SamplingSink` over the real Aperture
// `RecordingSink`.
//
// The slice tests exercise the decorator + the sampler against the
// real Aperture `OtlpSink + Probe` contract. `RecordingSink` records
// every accepted record; the slice tests assert against the
// recorded set.
// =========================================================================

/// A live `SamplingSink<RecordingSink, HeadSampler>` plus a handle
/// to the inner `RecordingSink` so tests can interrogate what
/// reached the inner sink.
///
/// At DISTILL `SamplingSink::new` panics with `unimplemented!()` so
/// constructing this fixture panics. That is the RED state — DELIVER
/// slice 01 makes the constructor real.
pub struct SieveFixture {
    pub sink: SamplingSink<RecordingSink, sieve::HeadSampler>,
    pub recording: Arc<RecordingSink>,
}

/// Spawn a `SamplingSink` over a fresh `RecordingSink` at the given
/// non-error rate.
///
/// Panics if `rate` is out-of-range (the constructor returns
/// `Err(SieveConfigError::RateOutOfRange)` and the test panics on
/// `.expect`). Tests that intentionally pass an out-of-range value
/// should call `HeadSampler::new` directly and assert on the error.
pub fn spawn_sampling_sink_with_recording_inner(rate: f64) -> SieveFixture {
    let recording_inner = RecordingSink::new();
    let sampler = sieve::HeadSampler::new(rate)
        .unwrap_or_else(|e| panic!("HeadSampler::new({rate}) must succeed for fixture: {e}"));
    // Hold an Arc<RecordingSink> so the test can `drain()` after the
    // sink consumes records. The decorator owns the inner sink by
    // value; we rebuild a second copy here for inspection. At
    // DISTILL the constructor panics on `unimplemented!()` so tests
    // panic before reaching the recording inspection — which is the
    // canonical RED state.
    let _recording_arc: Arc<RecordingSink> = Arc::new(recording_inner);
    let recording_for_sink = RecordingSink::new();
    let sink = SamplingSink::new(recording_for_sink, sampler);
    // The decorator owns its inner sink; the test side keeps a
    // separate `RecordingSink` reference to assert against. DELIVER
    // slice 01 + 05 will replace this stand-in with a shared
    // `Arc<RecordingSink>` constructed by the decorator's `new` in a
    // way the slice tests can reach.
    SieveFixture {
        sink,
        recording: _recording_arc,
    }
}

// =========================================================================
// Deterministic-seed trace_id builder.
//
// Slice 03 and slice 04 need 10 000 distinct trace_ids whose
// distribution under `xxh3_64` is uniform enough to deliver the ±3%
// band. The simplest way to get that without a `rand` runtime
// dependency is a counter: `trace_id` is `u128::to_be_bytes(i as
// u128)` for `i in 0..N`. The hash function does the work of
// distributing the otherwise-sequential keys uniformly across
// `[0.0, 1.0]`.
// =========================================================================

/// Build a deterministic 16-byte `trace_id` from a 64-bit seed.
///
/// The seed is splatted into the lower 8 bytes; the upper 8 bytes
/// carry a fixed marker (`0xCAFEBABE_DEADBEEF`) so the test
/// fixtures are visually distinguishable in debug output. Slice 03
/// uses `seed in 0..10_000` to produce 10 000 distinct trace_ids
/// whose `xxh3_64` mapping is uniform.
pub fn fixture_trace_id(seed: u64) -> [u8; 16] {
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&0xCAFE_BABE_DEAD_BEEF_u64.to_be_bytes());
    id[8..].copy_from_slice(&seed.to_be_bytes());
    id
}

// =========================================================================
// Span / trace fixture builders.
//
// Sieve's `TraceView` borrows a `&[Span]`. The slice tests build
// fixture `Vec<Span>`s and pass them through the
// `__test_trace_view(trace_id, &spans)` seam.
// =========================================================================

/// Construct a fixture span carrying the given `status.code`.
///
/// `OK` → `StatusCode::Ok` (the OTLP "ok" code, code 1).
/// `ERROR` → `StatusCode::Error` (code 2).
/// `UNSET` → `StatusCode::Unset` (code 0; default).
pub fn fixture_span_with_status(trace_id: [u8; 16], status: StatusCode) -> Span {
    Span {
        trace_id: trace_id.to_vec(),
        span_id: vec![0; 8],
        trace_state: String::new(),
        parent_span_id: Vec::new(),
        flags: 0,
        name: "fixture-span".to_string(),
        kind: 0,
        start_time_unix_nano: 0,
        end_time_unix_nano: 0,
        attributes: Vec::new(),
        dropped_attributes_count: 0,
        events: Vec::new(),
        dropped_events_count: 0,
        links: Vec::new(),
        dropped_links_count: 0,
        status: Some(Status {
            message: String::new(),
            code: status as i32,
        }),
    }
}

/// Construct an error-bearing single-span fixture trace at the
/// given trace_id.
pub fn fixture_error_trace(trace_id: [u8; 16]) -> Vec<Span> {
    vec![fixture_span_with_status(trace_id, StatusCode::Error)]
}

/// Construct an all-OK single-span fixture trace at the given
/// trace_id.
pub fn fixture_ok_trace(trace_id: [u8; 16]) -> Vec<Span> {
    vec![fixture_span_with_status(trace_id, StatusCode::Ok)]
}

/// Construct an all-UNSET single-span fixture trace at the given
/// trace_id. `UNSET` is OTLP's default `status.code` value; some
/// SDKs do not set the code on success spans.
pub fn fixture_unset_trace(trace_id: [u8; 16]) -> Vec<Span> {
    vec![fixture_span_with_status(trace_id, StatusCode::Unset)]
}

/// Construct a multi-span trace where exactly one span carries
/// `status.code == ERROR` and the rest are OK. Slice 02 exercises
/// the "12 spans, one error" scenario from US-SI-02.
pub fn fixture_multi_span_one_error(trace_id: [u8; 16], total_spans: usize) -> Vec<Span> {
    let mut spans = Vec::with_capacity(total_spans);
    for i in 0..total_spans {
        let status = if i == 0 {
            StatusCode::Error
        } else {
            StatusCode::Ok
        };
        spans.push(fixture_span_with_status(trace_id, status));
    }
    spans
}

// =========================================================================
// `ExportTraceServiceRequest` envelope builders.
//
// The decorator's `accept(SinkRecord::Traces(req))` path consumes the
// upstream OTLP envelope. Slice 01 / 05 / 06 build envelopes that
// mix multiple traces and assert against the decorator's grouping
// pass and per-trace decision.
// =========================================================================

/// Wrap a single trace's spans in a single-`ResourceSpans`,
/// single-`ScopeSpans` envelope. The simplest possible envelope.
pub fn envelope_with_one_trace(spans: Vec<Span>) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: None,
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Wrap many fixture traces (each its own `Vec<Span>`) in a single
/// `ExportTraceServiceRequest` envelope, all under one
/// `ResourceSpans` / `ScopeSpans`. Slice 06 uses this to mix
/// kept and dropped traces in one batch.
pub fn envelope_with_many_traces(traces: Vec<Vec<Span>>) -> ExportTraceServiceRequest {
    let mut all_spans = Vec::new();
    for spans in traces {
        all_spans.extend(spans);
    }
    envelope_with_one_trace(all_spans)
}

/// Build a fixture `ExportLogsServiceRequest` carrying a single log
/// record. Slice 05 uses this to assert pass-through behaviour.
pub fn envelope_with_one_log() -> ExportLogsServiceRequest {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: None,
            scope_logs: vec![ScopeLogs {
                scope: None,
                log_records: vec![LogRecord {
                    time_unix_nano: 0,
                    observed_time_unix_nano: 0,
                    severity_number: 9,
                    severity_text: "INFO".to_string(),
                    body: Some(AnyValue {
                        value: Some(
                            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                                "checkout completed for user U-1234".to_string(),
                            ),
                        ),
                    }),
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(
                                opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                                    "checkout-service".to_string(),
                                ),
                            ),
                        }),
                    }],
                    dropped_attributes_count: 0,
                    flags: 0,
                    trace_id: Vec::new(),
                    span_id: Vec::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Build a fixture `ExportMetricsServiceRequest` carrying a single
/// counter metric. Slice 05 uses this to assert pass-through.
pub fn envelope_with_one_metric() -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: None,
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "orders_completed_total".to_string(),
                    description: "Total orders completed.".to_string(),
                    unit: "1".to_string(),
                    metadata: Vec::new(),
                    data: Some(metric::Data::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: Vec::new(),
                            start_time_unix_nano: 0,
                            time_unix_nano: 0,
                            exemplars: Vec::new(),
                            flags: 0,
                            value: Some(NumberValue::AsInt(42)),
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
}

// =========================================================================
// `target = "sieve"` tracing-event capture.
//
// Slice 06 asserts the DEBUG per-decision events and the periodic
// INFO summary use `target = "sieve"` and carry the right field set.
// The capture mechanism is process-global (the global subscriber is
// install-once per process) and lives behind a `Once` gate; tests
// inside one binary that need concurrent captures must serialise.
// =========================================================================

/// A captured `tracing` event from `target = "sieve"`.
#[derive(Debug, Clone)]
pub struct SieveEvent {
    pub level: String,
    pub message: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

impl SieveEvent {
    pub fn message_contains(&self, needle: &str) -> bool {
        self.message.contains(needle)
    }

    pub fn field_str(&self, name: &str) -> Option<&str> {
        self.fields.get(name).and_then(|v| v.as_str())
    }

    pub fn field_u64(&self, name: &str) -> Option<u64> {
        self.fields.get(name).and_then(|v| v.as_u64())
    }

    pub fn field_f64(&self, name: &str) -> Option<f64> {
        self.fields.get(name).and_then(|v| v.as_f64())
    }
}

/// Process-global capture buffer.
pub static CAPTURED_EVENTS: Mutex<Vec<SieveEvent>> = Mutex::new(Vec::new());

/// Process-global subscriber install gate.
pub static INSTALL_SUBSCRIBER: Once = Once::new();

/// Process-global handle for the captured-events level filter.
/// Reserved for future use; Sieve's events are captured via the
/// per-event `target` check inside the layer.
static _CAPTURE_HANDLE: OnceLock<()> = OnceLock::new();

/// RAII guard that begins a capture session on construction and
/// clears the buffer on drop.
pub struct CaptureGuard {
    _private: (),
}

impl CaptureGuard {
    /// Snapshot the captured events so far. Cloned out from under
    /// the mutex; tests assert against the snapshot.
    pub fn events(&self) -> Vec<SieveEvent> {
        CAPTURED_EVENTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURED_EVENTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }
}

/// Begin capturing `target = "sieve"` events for the lifetime of
/// the returned guard.
///
/// The first call in a test process installs a `tracing_subscriber`
/// `Registry` with a [`SieveCaptureLayer`] as the global default.
/// Subsequent calls clear the buffer and return a fresh guard.
pub fn capture_sieve_events() -> CaptureGuard {
    INSTALL_SUBSCRIBER.call_once(install_sieve_capture_subscriber);
    CAPTURED_EVENTS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clear();
    CaptureGuard { _private: () }
}

fn install_sieve_capture_subscriber() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::Registry;

    let _ = Registry::default().with(SieveCaptureLayer).try_init();
}

struct SieveCaptureLayer;

impl<S> tracing_subscriber::Layer<S> for SieveCaptureLayer
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != "sieve" {
            return;
        }
        let mut visitor = SieveEventVisitor::default();
        event.record(&mut visitor);
        let level = event.metadata().level().to_string();
        let message = visitor.message.unwrap_or_default();
        let fields = visitor.fields;
        CAPTURED_EVENTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(SieveEvent {
                level,
                message,
                fields,
            });
    }
}

#[derive(Default)]
struct SieveEventVisitor {
    message: Option<String>,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl tracing::field::Visit for SieveEventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_owned());
        } else {
            self.fields.insert(
                field.name().to_owned(),
                serde_json::Value::String(value.to_owned()),
            );
        }
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .insert(field.name().to_owned(), serde_json::Value::Bool(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.insert(
            field.name().to_owned(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.insert(
            field.name().to_owned(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        let n = serde_json::Number::from_f64(value).unwrap_or_else(|| serde_json::Number::from(0));
        self.fields
            .insert(field.name().to_owned(), serde_json::Value::Number(n));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let formatted = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(formatted);
        } else {
            self.fields.insert(
                field.name().to_owned(),
                serde_json::Value::String(formatted),
            );
        }
    }
}

/// Assert that the captured events contain at least one event whose
/// message contains the given substring. Returns the first match.
/// Panics with a diagnostic if none match.
pub fn expect_sieve_event_with_message(events: &[SieveEvent], needle: &str) -> SieveEvent {
    events
        .iter()
        .find(|e| e.message_contains(needle))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "expected a captured sieve event whose message contains {needle:?}; \
                 got events: {:?}",
                events.iter().map(|e| &e.message).collect::<Vec<_>>()
            )
        })
}

/// Assert that NONE of the captured events match the given
/// substring. Panics if any match is found.
pub fn expect_no_sieve_event_with_message(events: &[SieveEvent], needle: &str) {
    if let Some(found) = events.iter().find(|e| e.message_contains(needle)) {
        panic!("expected no captured sieve event matching {needle:?}; found: {found:?}");
    }
}

// =========================================================================
// Async accept helper — the thinnest possible call into the
// decorator's `OtlpSink::accept` for the slice tests.
//
// Wraps the hand-rolled `Pin<Box<dyn Future>>` shape into an
// `async fn` so the slice tests can write `accept(...).await` rather
// than `accept(...).await` against the boxed future.
// =========================================================================

/// Call the decorator's `accept` and return the result.
pub async fn accept<S, N>(sink: &SamplingSink<S, N>, record: SinkRecord) -> Result<(), SinkError>
where
    S: OtlpSink + Probe,
    N: sieve::Sampler,
{
    sink.accept(record).await
}

/// Call the decorator's `probe` and return the result.
pub async fn probe<S, N>(sink: &SamplingSink<S, N>) -> Result<(), ProbeError>
where
    S: OtlpSink + Probe,
    N: sieve::Sampler,
{
    sink.probe().await
}
