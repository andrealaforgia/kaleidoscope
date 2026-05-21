//! # `otlp-conformance-harness`
//!
//! A CC0-1.0 Rust crate that validates byte sequences against the
//! OpenTelemetry OTLP wire specification. Phase-0 leaf dependency for
//! Kaleidoscope. Consumed by every later Kaleidoscope component (Aperture,
//! Codex, Spark, Pulse, Lumen, Ray, Strata) and by third-party OTel
//! implementers.
//!
//! ## Public surface (locked by ADR-0001 and US-06 AC 5)
//!
//! Three free functions, plus the named types and the spec-version
//! constant. The crate does **not** re-export any `opentelemetry_proto`
//! type — consumers depend on `opentelemetry-proto` directly so the
//! dependency edge is visible in their `Cargo.toml`.
//!
//! ## Implementation status
//!
//! The three validators are implemented and green. The acceptance
//! slices in `tests/slice_*.rs` cover empty input, malformed protobuf,
//! signal mismatch, and the accept paths for logs, traces and metrics,
//! with the public surface locked by `slice_07_lock_the_contract`. The
//! corpus vectors live under `tests/vectors/{logs,metrics,traces}/`.

#![forbid(unsafe_code)]

mod decode;
mod framing;
mod signal;
mod validate;
mod violation;

pub use framing::Framing;
pub use signal::SignalType;
pub use violation::{ByteOffset, OtlpViolation, Rule, WireTypeRule};

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

/// Version of the OpenTelemetry specification this crate targets. Sourced
/// from `[package.metadata.kaleidoscope.otlp]` and re-exported here per
/// ADR-0003 and `shared-artifacts-registry.md > otlp_spec_version`. The
/// corpus runner refuses to run vectors whose declared spec version
/// differs from this constant.
pub const OTLP_SPEC_VERSION: &str = "1.5.0";

/// Validates a byte sequence as an OTLP logs export request.
///
/// On success, returns the upstream
/// [`ExportLogsServiceRequest`](opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest)
/// unchanged (US-04 AC 2 — the harness does not wrap or rename the
/// upstream type).
///
/// On failure, returns an [`OtlpViolation`] naming the rule, the byte
/// locus, the expected and observed states, and the signal/framing the
/// caller asserted.
pub fn validate_logs(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportLogsServiceRequest, OtlpViolation> {
    validate::validate_logs(bytes, framing)
}

/// Validates a byte sequence as an OTLP traces export request. See
/// [`validate_logs`] for the contract; the only difference is the typed
/// return on the accept path
/// ([`ExportTraceServiceRequest`](opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest))
/// and the asserted signal echoed in any [`OtlpViolation`].
pub fn validate_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportTraceServiceRequest, OtlpViolation> {
    validate::validate_traces(bytes, framing)
}

/// Validates a byte sequence as an OTLP metrics export request. See
/// [`validate_logs`] for the contract; the typed return on the accept
/// path is
/// [`ExportMetricsServiceRequest`](opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest).
pub fn validate_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportMetricsServiceRequest, OtlpViolation> {
    validate::validate_metrics(bytes, framing)
}
