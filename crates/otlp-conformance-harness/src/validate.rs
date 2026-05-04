//! Internal validation impls.
//!
//! Per ADR-0001 the three `validate_*` functions in `lib.rs` are one-line
//! wrappers around these `pub(crate)` impls.

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use crate::decode;
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
    decode::decode_logs(bytes, framing)
}

pub(crate) fn validate_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportTraceServiceRequest, OtlpViolation> {
    if bytes.is_empty() {
        return Err(empty_input_violation(SignalType::Traces, framing));
    }
    decode::decode_traces(bytes, framing)
}

pub(crate) fn validate_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportMetricsServiceRequest, OtlpViolation> {
    if bytes.is_empty() {
        return Err(empty_input_violation(SignalType::Metrics, framing));
    }
    decode::decode_metrics(bytes, framing)
}
