//! Internal validation impls.
//!
//! Per ADR-0001 the three `validate_*` functions in `lib.rs` are one-line
//! wrappers around these `pub(crate)` impls.

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use crate::framing::Framing;
use crate::signal::SignalType;
use crate::violation::{empty_input_violation, OtlpViolation};

pub(crate) fn validate_logs(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportLogsServiceRequest, OtlpViolation> {
    if bytes.is_empty() {
        return Err(empty_input_violation(SignalType::Logs, framing));
    }
    unimplemented!("validate_logs: non-empty path deferred to slice 02")
}

pub(crate) fn validate_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportTraceServiceRequest, OtlpViolation> {
    if bytes.is_empty() {
        return Err(empty_input_violation(SignalType::Traces, framing));
    }
    unimplemented!("validate_traces: non-empty path deferred to slice 05")
}

pub(crate) fn validate_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportMetricsServiceRequest, OtlpViolation> {
    if bytes.is_empty() {
        return Err(empty_input_violation(SignalType::Metrics, framing));
    }
    unimplemented!("validate_metrics: non-empty path deferred to slice 06")
}
