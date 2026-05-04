//! Slice 03 — US-03: Reject valid protobuf of the wrong signal type.
//!
//! Per US-03 AC 2 (iteration-2 fix to user-stories.md line 300): when the
//! bytes match the asserted signal, the harness returns `Ok(record)`
//! immediately and the returned record is the typed upstream value (not
//! an intermediate state, surrogate, or harness-local wrapper). When the
//! bytes match a different signal, the harness surfaces
//! `WireType::SignalMismatch { observed, asserted }`. When the bytes
//! decode as none of the three signals, the failure stays as
//! `WireType::ProtobufDecode` (not `SignalMismatch`).

mod common;

use otlp_conformance_harness::{
    validate_logs, validate_metrics, validate_traces, Framing, OtlpViolation, Rule, SignalType,
    WireTypeRule,
};

// =========================================================================
// Scenario: Traces bytes handed to validate_logs produce SignalMismatch
// =========================================================================

#[test]
fn traces_bytes_to_validate_logs_yield_signal_mismatch() {
    let traces_bytes = common::encode_minimal_traces();
    let violation = expect_violation(validate_logs(&traces_bytes, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }) => {
            assert_eq!(observed, SignalType::Traces);
            assert_eq!(asserted, SignalType::Logs);
        }
        other => panic!("expected WireType::SignalMismatch, got {other:?}"),
    }
}

// =========================================================================
// Scenario: Metrics bytes handed to validate_logs produce SignalMismatch
// =========================================================================

#[test]
fn metrics_bytes_to_validate_logs_yield_signal_mismatch() {
    let metrics_bytes = common::encode_minimal_metrics();
    let violation = expect_violation(validate_logs(&metrics_bytes, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }) => {
            assert_eq!(observed, SignalType::Metrics);
            assert_eq!(asserted, SignalType::Logs);
        }
        other => panic!("expected WireType::SignalMismatch, got {other:?}"),
    }
}

// =========================================================================
// Scenario: Logs bytes handed to validate_traces produce SignalMismatch
// =========================================================================

#[test]
fn logs_bytes_to_validate_traces_yield_signal_mismatch() {
    let logs_bytes = common::encode_minimal_logs();
    let violation = expect_violation(validate_traces(&logs_bytes, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }) => {
            assert_eq!(observed, SignalType::Logs);
            assert_eq!(asserted, SignalType::Traces);
        }
        other => panic!("expected WireType::SignalMismatch, got {other:?}"),
    }
}

// =========================================================================
// Scenario: Traces bytes handed to validate_metrics produce SignalMismatch
// =========================================================================

#[test]
fn traces_bytes_to_validate_metrics_yield_signal_mismatch() {
    let traces_bytes = common::encode_minimal_traces();
    let violation = expect_violation(validate_metrics(&traces_bytes, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }) => {
            assert_eq!(observed, SignalType::Traces);
            assert_eq!(asserted, SignalType::Metrics);
        }
        other => panic!("expected WireType::SignalMismatch, got {other:?}"),
    }
}

// =========================================================================
// Scenario: Bytes that decode as none of the three signals stay as
// ProtobufDecode (NOT SignalMismatch)
// =========================================================================

#[test]
fn undecodable_bytes_stay_as_protobuf_decode_not_signal_mismatch() {
    let bytes = common::bad_varint();
    let violation = expect_violation(validate_logs(&bytes, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::ProtobufDecode) => {}
        Rule::WireType(WireTypeRule::SignalMismatch { .. }) => {
            panic!(
                "undecodable bytes must surface ProtobufDecode, not SignalMismatch — \
                 the alternative-decode fallback should have failed for all three signals"
            )
        }
        other => panic!("expected WireType::ProtobufDecode, got {other:?}"),
    }
}

// =========================================================================
// Scenario: A matching signal returns Ok with the typed upstream value
//
// US-03 AC 2 (iteration-2 fix): "on a matching signal the harness returns
// `Ok(record)` immediately and the returned record is the typed upstream
// value (not an intermediate state, surrogate, or harness-local wrapper).
// Verifiable by a Cargo unit test that pattern-matches on the return
// value." The pattern-match is structural: the variable's compile-time
// type is the upstream `ExportLogsServiceRequest`.
// =========================================================================

#[test]
fn matching_logs_signal_returns_typed_upstream_record() {
    use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;

    let logs_bytes = common::encode_minimal_logs();
    let result = validate_logs(&logs_bytes, Framing::HttpProtobuf);
    let record: ExportLogsServiceRequest = match result {
        Ok(r) => r,
        Err(e) => panic!("expected Ok with the upstream record, got Err({e:?})"),
    };
    // The record must carry the resource_logs we encoded — which proves
    // the harness produced the actual upstream message, not a substitute.
    assert!(!record.resource_logs.is_empty(), "decoded record is empty");
}

// =========================================================================
// Helpers
// =========================================================================

fn expect_violation<T: std::fmt::Debug>(
    result: Result<T, OtlpViolation>,
) -> OtlpViolation {
    match result {
        Ok(record) => panic!("expected violation, got Ok({record:?})"),
        Err(v) => v,
    }
}
