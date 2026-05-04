//! Slice 06 — US-06: Accept a minimally valid OTLP metrics record.
//!
//! US-06 AC 5 (iteration-2 fix to user-stories.md line 583) names the
//! three exact function signatures: at end of slice-06 the public API
//! exposes exactly those three functions, all returning the same
//! `OtlpViolation` type on the error path. The
//! `signature_lock_compiles_*` tests below are the structural proof — they
//! depend on the harness's three functions being callable with exactly
//! the locked signatures and on the upstream return types being the
//! locked paths. Any signature drift is a compile-time failure.

mod common;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use otlp_conformance_harness::{
    validate_logs, validate_metrics, validate_traces, Framing, OtlpViolation, Rule, SignalType,
    WireTypeRule,
};

// =========================================================================
// Scenario: A minimal metrics export request is accepted and returned typed
// =========================================================================

#[test]
fn minimal_metrics_export_request_returns_ok() {
    let bytes = common::encode_minimal_metrics();
    let result = validate_metrics(&bytes, Framing::HttpProtobuf);
    assert!(
        result.is_ok(),
        "expected Ok for a minimal metrics export, got {:?}",
        result.err()
    );
}

#[test]
fn minimal_metrics_export_request_contains_sum_and_gauge() {
    // US-06 AC 3 + scenario 1: the minimal metrics fixture must include
    // both a sum data point and a gauge data point.
    let bytes = common::encode_minimal_metrics();
    let record = validate_metrics(&bytes, Framing::HttpProtobuf)
        .expect("a minimal metrics export must be accepted");
    let metrics = &record.resource_metrics[0].scope_metrics[0].metrics;
    let mut has_sum = false;
    let mut has_gauge = false;
    use opentelemetry_proto::tonic::metrics::v1::metric::Data;
    for m in metrics {
        match &m.data {
            Some(Data::Sum(_)) => has_sum = true,
            Some(Data::Gauge(_)) => has_gauge = true,
            _ => {}
        }
    }
    assert!(
        has_sum,
        "minimal metrics fixture must contain a sum data point"
    );
    assert!(
        has_gauge,
        "minimal metrics fixture must contain a gauge data point"
    );
}

#[test]
fn accepted_metrics_record_is_directly_usable_by_upstream_typed_consumer() {
    let bytes = common::encode_minimal_metrics();
    let record = validate_metrics(&bytes, Framing::HttpProtobuf)
        .expect("a minimal metrics export must be accepted");
    assert_eq!(consume_upstream_metrics(&record), 1);
}

fn consume_upstream_metrics(record: &ExportMetricsServiceRequest) -> usize {
    record.resource_metrics.len()
}

// =========================================================================
// Scenario: validate_metrics rejects traces bytes with SignalMismatch
// =========================================================================

#[test]
fn validate_metrics_rejects_traces_bytes_with_signal_mismatch() {
    let traces_bytes = common::encode_minimal_traces();
    let violation = expect_violation(validate_metrics(&traces_bytes, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }) => {
            assert_eq!(observed, SignalType::Traces);
            assert_eq!(asserted, SignalType::Metrics);
        }
        other => panic!("expected SignalMismatch, got {other:?}"),
    }
}

// =========================================================================
// Scenario: validate_metrics covers the three reject rules symmetrically
// =========================================================================

#[test]
fn validate_metrics_rejects_empty_input_with_empty_input_rule() {
    let violation = expect_violation(validate_metrics(&[], Framing::HttpProtobuf));
    assert_eq!(violation.rule, Rule::EmptyInput);
    assert_eq!(violation.signal_asserted, SignalType::Metrics);
}

#[test]
fn validate_metrics_rejects_malformed_protobuf_with_protobuf_decode_rule() {
    let bytes = common::bad_varint();
    let violation = expect_violation(validate_metrics(&bytes, Framing::HttpProtobuf));
    assert!(
        matches!(violation.rule, Rule::WireType(WireTypeRule::ProtobufDecode)),
        "expected ProtobufDecode for malformed metrics, got {:?}",
        violation.rule
    );
    assert_eq!(violation.signal_asserted, SignalType::Metrics);
}

// =========================================================================
// Scenario: Public API signature lock — US-06 AC 5
//
// "At end of slice-06 the public API exposes exactly three functions
// with the following signatures: validate_logs(...) -> Result<...,
// OtlpViolation>; validate_traces(...) -> Result<..., OtlpViolation>;
// validate_metrics(...) -> Result<..., OtlpViolation>. All three return
// the same OtlpViolation type on the error path."
//
// The three tests below are structural proofs: they only compile if the
// signatures are exactly the locked ones and if the error type is
// shared across all three functions.
// =========================================================================

#[test]
fn signature_lock_compiles_for_validate_logs() {
    let f: fn(&[u8], Framing) -> Result<ExportLogsServiceRequest, OtlpViolation> = validate_logs;
    let _ = f;
}

#[test]
fn signature_lock_compiles_for_validate_traces() {
    let f: fn(&[u8], Framing) -> Result<ExportTraceServiceRequest, OtlpViolation> = validate_traces;
    let _ = f;
}

#[test]
fn signature_lock_compiles_for_validate_metrics() {
    let f: fn(&[u8], Framing) -> Result<ExportMetricsServiceRequest, OtlpViolation> =
        validate_metrics;
    let _ = f;
}

#[test]
fn signature_lock_uses_same_violation_type_across_all_three_functions() {
    // Take an Err from each function and put them in the same Vec —
    // possible only if all three Err types are the same OtlpViolation.
    let logs_err = validate_logs(&[], Framing::HttpProtobuf).err();
    let traces_err = validate_traces(&[], Framing::HttpProtobuf).err();
    let metrics_err = validate_metrics(&[], Framing::HttpProtobuf).err();
    let errs: Vec<Option<OtlpViolation>> = vec![logs_err, traces_err, metrics_err];
    assert_eq!(errs.len(), 3);
    for e in &errs {
        assert!(e.is_some(), "all three empty inputs must yield Err");
    }
}

// =========================================================================
// Helpers
// =========================================================================

fn expect_violation<T: std::fmt::Debug>(result: Result<T, OtlpViolation>) -> OtlpViolation {
    match result {
        Ok(record) => panic!("expected violation, got Ok({record:?})"),
        Err(v) => v,
    }
}
