//! Internal decode dispatch.
//!
//! Per ADR-0001 this module is `pub(crate)` only; the public surface in
//! `lib.rs` delegates here. The DELIVER wave fills in the empty-check, the
//! prost decode, and the signal-mismatch fallback. DISTILL leaves
//! `unimplemented!()` so the acceptance tests fail with a panic in the
//! expected RED state.

#![allow(dead_code, unused_variables)]

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use crate::framing::Framing;
use crate::violation::OtlpViolation;

pub(crate) fn decode_logs(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportLogsServiceRequest, OtlpViolation> {
    unimplemented!("decode_logs: implementation deferred to DELIVER wave")
}

pub(crate) fn decode_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportTraceServiceRequest, OtlpViolation> {
    unimplemented!("decode_traces: implementation deferred to DELIVER wave")
}

pub(crate) fn decode_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportMetricsServiceRequest, OtlpViolation> {
    unimplemented!("decode_metrics: implementation deferred to DELIVER wave")
}
