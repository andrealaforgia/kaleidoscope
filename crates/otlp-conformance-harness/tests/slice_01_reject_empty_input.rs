//! Slice 01 — US-01: Reject empty input with a structured violation.
//!
//! This is the project-level walking skeleton (DISCUSS D2). It bootstraps
//! the full Cargo workspace, the public surface, and the simplest possible
//! round-trip — a zero-length byte sequence handed to each of the three
//! `validate_*` functions, expected to round-trip as
//! `Err(OtlpViolation { rule: Rule::EmptyInput, ... })`.
//!
//! Mandate Single-Then-Per-Fact: each scenario from the user story is split
//! so a mutation can only kill one assertion at a time. Mandate Hexagonal:
//! every test invokes the public `validate_*` functions directly — never
//! `crate::decode::*` or `crate::validate::*` (those are crate-private).

mod common;

use otlp_conformance_harness::{
    validate_logs, validate_metrics, validate_traces, ByteOffset, Framing, OtlpViolation, Rule,
    SignalType,
};

// =========================================================================
// Scenario: Empty input is rejected with the EmptyInput rule (logs)
// =========================================================================

#[test]
fn empty_logs_input_returns_err_with_empty_input_rule() {
    let result = validate_logs(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.rule, Rule::EmptyInput);
}

#[test]
fn empty_logs_input_echoes_signal_logs_in_violation() {
    let result = validate_logs(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.signal_asserted, SignalType::Logs);
}

#[test]
fn empty_logs_input_echoes_framing_http_protobuf_in_violation() {
    let result = validate_logs(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.framing_asserted, Framing::HttpProtobuf);
}

#[test]
fn empty_logs_input_records_byte_locus_at_zero() {
    let result = validate_logs(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    // Per US-01's elevator pitch and Solution: the locus is `ByteOffset(0)`
    // for empty input — there is nowhere else for it to be.
    assert_eq!(violation.locus, ByteOffset::Known(0));
}

// =========================================================================
// Scenario: Empty input rejection is the same shape across all signal types
// =========================================================================

#[test]
fn empty_traces_input_returns_err_with_empty_input_rule() {
    let result = validate_traces(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.rule, Rule::EmptyInput);
}

#[test]
fn empty_traces_input_echoes_signal_traces_in_violation() {
    let result = validate_traces(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.signal_asserted, SignalType::Traces);
}

#[test]
fn empty_metrics_input_returns_err_with_empty_input_rule() {
    let result = validate_metrics(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.rule, Rule::EmptyInput);
}

#[test]
fn empty_metrics_input_echoes_signal_metrics_in_violation() {
    let result = validate_metrics(&[], Framing::HttpProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.signal_asserted, SignalType::Metrics);
}

// =========================================================================
// Scenario: Grpc framing is also rejected on empty input
// =========================================================================

#[test]
fn empty_logs_input_with_grpc_framing_echoes_grpc_in_violation() {
    let result = validate_logs(&[], Framing::GrpcProtobuf);
    let violation = expect_violation(result);
    assert_eq!(violation.framing_asserted, Framing::GrpcProtobuf);
    assert_eq!(violation.rule, Rule::EmptyInput);
}

// =========================================================================
// Scenario: The harness produces no side effects when rejecting empty input
//
// US-01 AC 4: "The harness writes nothing to stdout, stderr, or any
// logging facade when handling empty input (assertion observed across all
// three channels)." Three separate `#[test]` functions, one per channel,
// so a mutation can only fail one at a time.
// =========================================================================

#[test]
fn empty_input_rejection_writes_nothing_to_stdout() {
    let (result, observations) =
        common::observe_silence(|| validate_logs(&[], Framing::HttpProtobuf));
    expect_violation(result);
    assert!(
        observations.stdout.is_empty(),
        "stdout was written to during empty-input rejection: {:?}",
        String::from_utf8_lossy(&observations.stdout)
    );
}

#[test]
fn empty_input_rejection_writes_nothing_to_stderr() {
    let (result, observations) =
        common::observe_silence(|| validate_logs(&[], Framing::HttpProtobuf));
    expect_violation(result);
    assert!(
        observations.stderr.is_empty(),
        "stderr was written to during empty-input rejection: {:?}",
        String::from_utf8_lossy(&observations.stderr)
    );
}

#[test]
fn empty_input_rejection_emits_no_log_records() {
    let (result, observations) =
        common::observe_silence(|| validate_logs(&[], Framing::HttpProtobuf));
    expect_violation(result);
    assert!(
        observations.log_records.is_empty(),
        "log records emitted during empty-input rejection: {:?}",
        observations.log_records
    );
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
