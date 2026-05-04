//! Internal validation impls.
//!
//! Per ADR-0001 the three `validate_*` functions in `lib.rs` are one-line
//! wrappers around these `pub(crate)` impls. Stubs panic with
//! `unimplemented!()` so the acceptance tests are RED until DELIVER drives
//! them green.

#![allow(dead_code, unused_variables)]

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use crate::framing::Framing;
use crate::violation::OtlpViolation;

pub(crate) fn validate_logs(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportLogsServiceRequest, OtlpViolation> {
    unimplemented!(
        "validate_logs: implementation deferred to DELIVER wave (slices 01-04)"
    )
}

pub(crate) fn validate_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportTraceServiceRequest, OtlpViolation> {
    unimplemented!(
        "validate_traces: implementation deferred to DELIVER wave (slice 05)"
    )
}

pub(crate) fn validate_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportMetricsServiceRequest, OtlpViolation> {
    unimplemented!(
        "validate_metrics: implementation deferred to DELIVER wave (slice 06)"
    )
}
