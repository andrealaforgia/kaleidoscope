//! Internal decode dispatch.
//!
//! Per ADR-0001 this module is `pub(crate)` only; the public surface in
//! `lib.rs` delegates here. Decode functions consume the byte sequence
//! through `prost::Message::decode` for the asserted signal's descriptor
//! and translate any `prost::DecodeError` into a harness-owned
//! `OtlpViolation { rule: WireType(ProtobufDecode), ... }`.

use prost::Message;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use crate::framing::Framing;
use crate::signal::SignalType;
use crate::violation::{protobuf_decode_violation, OtlpViolation};

/// Field number of the only known top-level field on every
/// `Export*ServiceRequest`: `resource_logs` / `resource_spans` /
/// `resource_metrics`. A non-empty body whose first tag references any
/// other field number is malformed — prost would silently skip the
/// unknown field and produce a vacuous record, defeating the harness's
/// rejection contract (US-02 bad-tag scenario).
const RESOURCE_FIELD_NUMBER: u32 = 1;

pub(crate) fn decode_logs(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportLogsServiceRequest, OtlpViolation> {
    decode_with_strict_top_level(bytes, framing, SignalType::Logs, |b| {
        ExportLogsServiceRequest::decode(b)
    })
}

pub(crate) fn decode_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportTraceServiceRequest, OtlpViolation> {
    decode_with_strict_top_level(bytes, framing, SignalType::Traces, |b| {
        ExportTraceServiceRequest::decode(b)
    })
}

pub(crate) fn decode_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportMetricsServiceRequest, OtlpViolation> {
    decode_with_strict_top_level(bytes, framing, SignalType::Metrics, |b| {
        ExportMetricsServiceRequest::decode(b)
    })
}

/// Run `prost_decode` on `bytes` after a strict top-level-tag check that
/// rejects bodies whose first wire tag references an unknown field
/// number. Any prost error is translated to a harness-owned
/// `ProtobufDecode` violation with the asserted signal and framing.
fn decode_with_strict_top_level<T>(
    bytes: &[u8],
    framing: Framing,
    signal: SignalType,
    prost_decode: impl FnOnce(&[u8]) -> Result<T, prost::DecodeError>,
) -> Result<T, OtlpViolation> {
    if let Err(prost_err) = first_tag_references_resource_field(bytes) {
        return Err(protobuf_decode_violation(
            signal,
            framing,
            bytes.len(),
            prost_err,
        ));
    }
    prost_decode(bytes)
        .map_err(|prost_err| protobuf_decode_violation(signal, framing, bytes.len(), prost_err))
}

/// Verify the first wire tag in `bytes` is `(RESOURCE_FIELD_NUMBER,
/// LengthDelimited)`-shaped at field-number granularity. The wire-type
/// match itself is delegated to prost's per-message decoder so failure
/// modes ("wire type error") arrive through the standard prost path.
fn first_tag_references_resource_field(bytes: &[u8]) -> Result<(), prost::DecodeError> {
    let mut buf = bytes;
    let (field_number, _wire_type) = prost::encoding::decode_key(&mut buf)?;
    if field_number != RESOURCE_FIELD_NUMBER {
        return Err(prost::DecodeError::new(format!(
            "wire type error: unknown top-level field {field_number}"
        )));
    }
    Ok(())
}
