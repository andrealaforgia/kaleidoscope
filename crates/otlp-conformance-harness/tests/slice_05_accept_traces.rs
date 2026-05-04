//! Slice 05 — US-05: Accept a minimally valid OTLP traces record.
//!
//! Symmetric with slice 04. Asserts the same shape of public API for a
//! different signal. Per US-05 AC 2 all three reject rules
//! (`EmptyInput`, `ProtobufDecode`, `SignalMismatch`) produce the same
//! shape for traces as for logs, with the asserted signal echoed back as
//! `SignalType::Traces`.

mod common;

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use otlp_conformance_harness::{
    validate_traces, Framing, OtlpViolation, Rule, SignalType, WireTypeRule,
};

// =========================================================================
// Scenario: A minimal traces export request is accepted and returned typed
// =========================================================================

#[test]
fn minimal_traces_export_request_returns_ok() {
    let bytes = common::encode_minimal_traces();
    let result = validate_traces(&bytes, Framing::HttpProtobuf);
    assert!(
        result.is_ok(),
        "expected Ok for a minimal traces export, got {:?}",
        result.err()
    );
}

#[test]
fn minimal_traces_export_request_round_trips_span_name() {
    let bytes = common::encode_minimal_traces();
    let record = validate_traces(&bytes, Framing::HttpProtobuf)
        .expect("a minimal traces export must be accepted");
    let span = &record.resource_spans[0].scope_spans[0].spans[0];
    assert_eq!(span.name, "minimal-span");
}

#[test]
fn accepted_traces_record_is_directly_usable_by_upstream_typed_consumer() {
    let bytes = common::encode_minimal_traces();
    let record = validate_traces(&bytes, Framing::HttpProtobuf)
        .expect("a minimal traces export must be accepted");
    assert_eq!(consume_upstream_traces(&record), 1);
}

fn consume_upstream_traces(record: &ExportTraceServiceRequest) -> usize {
    record.resource_spans.len()
}

// =========================================================================
// Scenario: validate_traces rejects logs bytes with SignalMismatch
// =========================================================================

#[test]
fn validate_traces_rejects_logs_bytes_with_signal_mismatch() {
    let logs_bytes = common::encode_minimal_logs();
    let violation = expect_violation(validate_traces(&logs_bytes, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }) => {
            assert_eq!(observed, SignalType::Logs);
            assert_eq!(asserted, SignalType::Traces);
        }
        other => panic!("expected SignalMismatch, got {other:?}"),
    }
}

// =========================================================================
// Scenario: validate_traces rejects empty input with EmptyInput
// =========================================================================

#[test]
fn validate_traces_rejects_empty_input_with_empty_input_rule() {
    let violation = expect_violation(validate_traces(&[], Framing::HttpProtobuf));
    assert_eq!(violation.rule, Rule::EmptyInput);
    assert_eq!(violation.signal_asserted, SignalType::Traces);
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
