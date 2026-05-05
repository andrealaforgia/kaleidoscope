//! Shared test helpers for the seven slice-level acceptance test suites.
//!
//! Cargo's integration-test convention: `tests/common/mod.rs` is a module
//! that every `tests/slice_*.rs` file may declare via
//! `mod common;`. The helpers below are kept minimal — anything that
//! grows business logic of its own would belong in production code, not
//! in a test helper, per Mandate 3 of the test-design-mandates skill.

#![allow(dead_code)]

use std::fmt::Write as FmtWrite;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Once;

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

// =========================================================================
// Bytes synthesised in-process for the slice tests
// =========================================================================
//
// Where the upstream OpenTelemetry SDK is the realistic source of bytes
// (slices 04/05/06 happy paths, slice 03 signal-mismatch round-trip), the
// SDK's prost-generated message types are used directly via
// `Message::encode_to_vec`. The shape of each minimal message is the
// same as what `opentelemetry-sdk` would emit for the bare-minimum
// case — a single resource, a single scope, a single record, with the
// required attributes (service.name on the resource, an instrumentation
// scope name, and the bare per-record fields).

/// Encode a minimal but conformant `ExportLogsServiceRequest`.
pub fn encode_minimal_logs() -> Vec<u8> {
    let req = ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "kaleidoscope-corpus-fixture")],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "kaleidoscope.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: vec![LogRecord {
                    time_unix_nano: 1_700_000_000_000_000_000,
                    observed_time_unix_nano: 1_700_000_000_000_000_000,
                    severity_number: 9, // INFO
                    severity_text: "INFO".to_string(),
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(
                            "minimal log record for corpus".to_string(),
                        )),
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
    };
    req.encode_to_vec()
}

/// Encode a minimal but conformant `ExportTraceServiceRequest`.
pub fn encode_minimal_traces() -> Vec<u8> {
    let req = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "kaleidoscope-corpus-fixture")],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "kaleidoscope.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                spans: vec![Span {
                    trace_id: vec![1; 16],
                    span_id: vec![1; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: "minimal-span".to_string(),
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
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };
    req.encode_to_vec()
}

/// Encode a minimal but conformant `ExportMetricsServiceRequest` with
/// one sum data point and one gauge data point (US-06 AC: minimal must
/// include at least a sum and a gauge).
pub fn encode_minimal_metrics() -> Vec<u8> {
    let resource = Resource {
        attributes: vec![string_kv("service.name", "kaleidoscope-corpus-fixture")],
        dropped_attributes_count: 0,
    };
    let scope = InstrumentationScope {
        name: "kaleidoscope.test".to_string(),
        version: "0.0.0".to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    };
    let req = ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(resource),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope),
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
                            aggregation_temporality: 2, // CUMULATIVE
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
// Hand-crafted malformed bytes for slice 02
// =========================================================================

/// Truncate a real OTLP body at the given byte offset. Used by US-02 to
/// produce a "truncated at byte 50" sequence whose decode failure should
/// fall in the 40..=60 byte-locus window.
pub fn truncate(bytes: &[u8], at: usize) -> Vec<u8> {
    let cut = at.min(bytes.len());
    bytes[..cut].to_vec()
}

/// Produce a byte sequence whose first varint (after the tag byte at
/// offset 0) is invalid: a sequence of bytes all with the continuation
/// bit set, never terminating. This drives `prost` into an
/// "invalid varint" / "unexpected EOF in varint" error around byte 7
/// (the maximum varint length is 10 bytes; we emit 9 continuation bytes
/// after the tag so the error surface is well-defined).
pub fn bad_varint() -> Vec<u8> {
    // Tag byte 0x08 = field 1, wire type 0 (varint). Then nine
    // continuation bytes (0x80..). Total 10 bytes; prost reports
    // failure at or near byte 7-9.
    vec![0x08, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80]
}

/// Produce a byte sequence whose first tag references an undefined
/// field number for `ExportLogsServiceRequest`. Field numbers in the
/// upstream proto for that message are 1..=1 (only `resource_logs`);
/// using field number 99999 with wire type 2 (length-delimited) and a
/// zero length forces prost into a "wire type mismatch" or
/// "unknown field" handling code path.
pub fn bad_tag() -> Vec<u8> {
    // Tag = (field_number << 3) | wire_type. field 99999, wire 2 ->
    // 99999 << 3 | 2 = 799994. Encoded as varint:
    //   799994 = 0xC353A => low 7 bits 0x3A then 0x6A then 0x30 ...
    // We'll construct it via prost-style varint encoding manually:
    let mut v = Vec::new();
    let mut n: u64 = (99999u64 << 3) | 2;
    while n >= 0x80 {
        v.push(((n & 0x7F) | 0x80) as u8);
        n >>= 7;
    }
    v.push(n as u8);
    // Length-delimited body: claim 5 bytes, supply 5 bytes of garbage.
    v.push(5);
    v.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC, 0xFB]);
    v
}

// =========================================================================
// No-side-effects observation
// =========================================================================
//
// The no-side-effects scenarios (US-01 scenario 3, US-04 scenario 3)
// require asserting nothing was written to stdout, stderr, or any
// logger. We use `gag` to redirect the OS-level stdout/stderr file
// descriptors, capturing any write at all (including from C code),
// and a captured `log` backend for the logging facade.

use log::{LevelFilter, Metadata, Record};
use std::sync::Mutex;

static LOGGER_INIT: Once = Once::new();
static CAPTURED_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct CapturingLogger;

impl log::Log for CapturingLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &Record<'_>) {
        if let Ok(mut buf) = CAPTURED_LOG.lock() {
            buf.push(format!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            ));
        }
    }
    fn flush(&self) {}
}

static LOGGER: CapturingLogger = CapturingLogger;

/// Install the capturing logger on first use. Idempotent. Subsequent
/// calls do nothing because `set_logger` may only succeed once per
/// process; the no-side-effects scenarios only need the logger to exist.
pub fn install_capturing_logger() {
    LOGGER_INIT.call_once(|| {
        // If another test already installed a logger, this returns Err,
        // which is fine for our purposes — every test in this binary
        // shares the capture.
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(LevelFilter::Trace);
    });
}

/// Drain the captured log buffer and return all records emitted since the
/// last drain. Tests assert the returned Vec is empty.
pub fn drained_log_records() -> Vec<String> {
    CAPTURED_LOG
        .lock()
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default()
}

/// Result of running a closure with stdout, stderr, and the log facade
/// observed. `stdout` and `stderr` are the bytes captured at the OS
/// level; `log_records` is the list of `log` crate records emitted.
pub struct Observations {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub log_records: Vec<String>,
}

impl Observations {
    /// True if no observation captured any output.
    pub fn is_silent(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty() && self.log_records.is_empty()
    }
}

/// Process-wide lock for stdout/stderr redirection. `gag::BufferRedirect`
/// redirects the OS-level file descriptors, which is inherently a
/// process-global resource — `cargo test`'s default thread-parallelism
/// causes redirects to clash unless every observer holds this lock.
static OBSERVE_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` while observing stdout, stderr, and the `log` facade. Returns
/// the function's result alongside the observations.
///
/// Implementation note: `gag::BufferRedirect` redirects the OS-level
/// stdout/stderr file descriptors for the duration of the closure, then
/// reads what was written when the redirect is dropped. The capturing
/// logger is process-wide; we drain its buffer at the start so we only
/// observe records emitted during `f`. The redirect is process-global,
/// so this function holds `OBSERVE_LOCK` for its entire duration to
/// serialise concurrent observers.
pub fn observe_silence<R, F>(f: F) -> (R, Observations)
where
    F: FnOnce() -> R,
{
    install_capturing_logger();
    // Take the global redirect lock — every other observer in this
    // binary blocks here until we release.
    let _guard = OBSERVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Drain anything left from earlier tests so we measure only `f`.
    let _ = drained_log_records();

    use gag::BufferRedirect;
    let mut out_redir = BufferRedirect::stdout().expect("redirect stdout");
    let mut err_redir = BufferRedirect::stderr().expect("redirect stderr");

    let result = f();

    let mut stdout = Vec::new();
    out_redir
        .read_to_end(&mut stdout)
        .expect("read stdout buffer");
    drop(out_redir);

    let mut stderr = Vec::new();
    err_redir
        .read_to_end(&mut stderr)
        .expect("read stderr buffer");
    drop(err_redir);

    let log_records = drained_log_records();

    (
        result,
        Observations {
            stdout,
            stderr,
            log_records,
        },
    )
}

// =========================================================================
// Vector-corpus reading (slice 07 helpers)
// =========================================================================

/// Resolve a path under `tests/vectors/`. Test binaries are launched with
/// `CARGO_MANIFEST_DIR` set to the crate root, so the vectors live at a
/// stable path relative to that.
pub fn vector_path(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("vectors");
    p.push(rel);
    p
}

/// Read a vector's bytes from disk.
pub fn read_vector(rel: &str) -> Vec<u8> {
    let path = vector_path(rel);
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!("read vector {}: {e}", path.display());
    })
}

/// Hex-encoded SHA-256 of the given bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        let _ = FmtWrite::write_fmt(&mut s, format_args!("{b:02x}"));
    }
    s
}
