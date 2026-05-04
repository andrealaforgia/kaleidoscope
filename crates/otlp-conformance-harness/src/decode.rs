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
use crate::violation::{protobuf_decode_violation, signal_mismatch_violation, OtlpViolation};

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
    match decode_strict::<ExportLogsServiceRequest>(bytes) {
        Ok(record) => Ok(record),
        Err(prost_err) => Err(reject_with_signal_mismatch_fallback(
            bytes,
            framing,
            SignalType::Logs,
            prost_err,
        )),
    }
}

pub(crate) fn decode_traces(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportTraceServiceRequest, OtlpViolation> {
    match decode_strict::<ExportTraceServiceRequest>(bytes) {
        Ok(record) => Ok(record),
        Err(prost_err) => Err(reject_with_signal_mismatch_fallback(
            bytes,
            framing,
            SignalType::Traces,
            prost_err,
        )),
    }
}

pub(crate) fn decode_metrics(
    bytes: &[u8],
    framing: Framing,
) -> Result<ExportMetricsServiceRequest, OtlpViolation> {
    match decode_strict::<ExportMetricsServiceRequest>(bytes) {
        Ok(record) => Ok(record),
        Err(prost_err) => Err(reject_with_signal_mismatch_fallback(
            bytes,
            framing,
            SignalType::Metrics,
            prost_err,
        )),
    }
}

/// Apply the US-03 alternative-decode fallback: when decoding into the
/// asserted signal failed, try the other two signals; if exactly one of
/// them succeeds, surface `SignalMismatch { observed, asserted }`. If
/// none succeed, the failure stays as `ProtobufDecode` carrying the
/// original prost error.
fn reject_with_signal_mismatch_fallback(
    bytes: &[u8],
    framing: Framing,
    asserted: SignalType,
    prost_err: prost::DecodeError,
) -> OtlpViolation {
    if let Some(observed) = first_alternative_signal_that_decodes(bytes, asserted) {
        return signal_mismatch_violation(observed, asserted, framing);
    }
    protobuf_decode_violation(asserted, framing, bytes.len(), prost_err)
}

/// Return the first `SignalType` (other than `asserted`) whose strict
/// decoder accepts `bytes`. The search order is fixed (Logs, Traces,
/// Metrics) so the chosen "observed" is deterministic for a given input.
fn first_alternative_signal_that_decodes(bytes: &[u8], asserted: SignalType) -> Option<SignalType> {
    for candidate in [SignalType::Logs, SignalType::Traces, SignalType::Metrics] {
        if candidate == asserted {
            continue;
        }
        if signal_decodes_bytes(candidate, bytes) {
            return Some(candidate);
        }
    }
    None
}

fn signal_decodes_bytes(signal: SignalType, bytes: &[u8]) -> bool {
    match signal {
        SignalType::Logs => decode_strict::<ExportLogsServiceRequest>(bytes).is_ok(),
        SignalType::Traces => decode_strict::<ExportTraceServiceRequest>(bytes).is_ok(),
        SignalType::Metrics => decode_strict::<ExportMetricsServiceRequest>(bytes).is_ok(),
    }
}

/// Strictly decode `bytes` as the requested message type, refusing
/// otherwise-prost-permissive bodies whose first wire tag references an
/// unknown top-level field number. Returns the underlying
/// `prost::DecodeError` on any failure so callers may translate it into
/// the harness's violation taxonomy.
fn decode_strict<M: Message + Default>(bytes: &[u8]) -> Result<M, prost::DecodeError> {
    first_tag_references_resource_field(bytes)?;
    M::decode(bytes)
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
