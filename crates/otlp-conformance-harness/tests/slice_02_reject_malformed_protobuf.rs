//! Slice 02 — US-02: Reject malformed protobuf with a structured violation.
//!
//! Per Sentinel's iteration-1 review, the truncation scenario is split into
//! two assertions: one on the byte-locus window (40..=60 inclusive) and one
//! on the named decode-error category in the `observed` field. Both are
//! mutation-resistant against an "always-zero locus" or
//! "generic error string" implementation.

mod common;

use otlp_conformance_harness::{
    validate_logs, validate_traces, ByteOffset, Framing, OtlpViolation, Rule, SignalType,
    WireTypeRule,
};

// =========================================================================
// Truncation scenarios — two assertions, one per `#[test]`
// =========================================================================

#[test]
fn truncated_logs_body_is_rejected_with_protobuf_decode_rule() {
    let real = common::encode_minimal_logs();
    assert!(real.len() > 60, "fixture too small to truncate at byte 50");
    let truncated = common::truncate(&real, 50);

    let violation = expect_violation(validate_logs(&truncated, Framing::HttpProtobuf));
    assert!(
        matches!(violation.rule, Rule::WireType(WireTypeRule::ProtobufDecode)),
        "expected WireType::ProtobufDecode, got {:?}",
        violation.rule
    );
}

#[test]
fn truncated_logs_body_locus_falls_within_40_to_60_inclusive() {
    // US-02 scenario "A truncated OTLP body's byte locus points near the
    // truncation boundary" — iteration-2 fix to the BDD (line 168 of
    // user-stories.md): "the violation's locus is a ByteOffset whose value
    // is between 40 and 60 inclusive". Mutation-resistant against an
    // always-zero locus.
    let real = common::encode_minimal_logs();
    assert!(real.len() > 60, "fixture too small to truncate at byte 50");
    let truncated = common::truncate(&real, 50);

    let violation = expect_violation(validate_logs(&truncated, Framing::HttpProtobuf));
    let offset = match violation.locus {
        ByteOffset::Known(n) => n,
        ByteOffset::Unknown => {
            panic!("expected a known byte offset for a truncation, got Unknown")
        }
        // `ByteOffset` is `#[non_exhaustive]`; the catch-all is required to
        // compile but should never fire for the variants in scope today.
        _ => panic!("unexpected ByteOffset variant"),
    };
    assert!(
        (40..=60).contains(&offset),
        "byte locus {offset} is outside the 40..=60 window required by US-02"
    );
}

#[test]
fn truncated_logs_body_observed_field_names_a_known_decode_category() {
    // US-02 scenario "A truncated OTLP body's `observed` field names a
    // recognisable decode-error category" (iteration-2 fix). Mutation-
    // resistant against a generic "error occurred" string.
    let real = common::encode_minimal_logs();
    let truncated = common::truncate(&real, 50);

    let violation = expect_violation(validate_logs(&truncated, Framing::HttpProtobuf));
    let observed = violation.observed.to_lowercase();
    let categories = [
        "unexpected eof",
        "wire type error",
        "missing length-delimited data",
    ];
    assert!(
        categories.iter().any(|cat| observed.contains(cat)),
        "observed field {:?} contains none of the named decode-error categories {:?}",
        violation.observed,
        categories
    );
}

// =========================================================================
// Invalid-varint scenarios
// =========================================================================

#[test]
fn invalid_varint_is_rejected_with_protobuf_decode_rule() {
    let bytes = common::bad_varint();
    let violation = expect_violation(validate_logs(&bytes, Framing::HttpProtobuf));
    assert!(
        matches!(violation.rule, Rule::WireType(WireTypeRule::ProtobufDecode)),
        "expected WireType::ProtobufDecode, got {:?}",
        violation.rule
    );
}

#[test]
fn invalid_varint_locus_identifies_a_position_within_input() {
    // US-02 scenario "An invalid varint is rejected with ProtobufDecode" —
    // "the violation's locus identifies a position within the input
    // (best-effort byte offset)". `ByteOffset::Unknown` is acceptable here
    // (US-02 AC 2: "if the underlying decoder does not provide one, the
    // violation records `ByteOffset::Unknown`"); a `Known(n)` must be
    // within input bounds.
    let bytes = common::bad_varint();
    let violation = expect_violation(validate_logs(&bytes, Framing::HttpProtobuf));
    if let ByteOffset::Known(n) = violation.locus {
        assert!(
            n <= bytes.len(),
            "locus {n} exceeds input length {}",
            bytes.len()
        );
    }
}

#[test]
fn invalid_varint_observed_field_names_a_known_decode_category() {
    // Iteration-2 non-blocking suggestion 2: extended category set for the
    // varint case, which prost reports specifically as "invalid varint".
    let bytes = common::bad_varint();
    let violation = expect_violation(validate_logs(&bytes, Framing::HttpProtobuf));
    let observed = violation.observed.to_lowercase();
    let categories = [
        "unexpected eof",
        "wire type error",
        "missing length-delimited data",
        "invalid varint",
    ];
    assert!(
        categories.iter().any(|cat| observed.contains(cat)),
        "observed field {:?} contains none of the named decode-error categories {:?}",
        violation.observed,
        categories
    );
}

// =========================================================================
// Bad-tag scenarios
// =========================================================================

#[test]
fn bad_tag_is_rejected_with_protobuf_decode_rule() {
    let bytes = common::bad_tag();
    let violation = expect_violation(validate_logs(&bytes, Framing::HttpProtobuf));
    assert!(
        matches!(violation.rule, Rule::WireType(WireTypeRule::ProtobufDecode)),
        "expected WireType::ProtobufDecode for a bad-tag body, got {:?}",
        violation.rule
    );
}

// =========================================================================
// Public-API encapsulation: prost::DecodeError is not in the public surface
// =========================================================================

#[test]
fn malformed_logs_violation_is_pattern_matchable_without_prost_types() {
    // US-02 scenario "The decode failure does not leak the prost error
    // type into the public API": the caller must be able to pattern-match
    // on the violation entirely in terms of the harness's public types.
    // This test is the structural proof — it compiles and runs without
    // any `use prost::*` on this side.
    let real = common::encode_minimal_logs();
    let truncated = common::truncate(&real, 50);
    let violation = expect_violation(validate_logs(&truncated, Framing::HttpProtobuf));
    match violation.rule {
        Rule::WireType(WireTypeRule::ProtobufDecode) => {}
        other => panic!("expected WireType::ProtobufDecode, got {other:?}"),
    }
    assert_eq!(violation.signal_asserted, SignalType::Logs);
}

// =========================================================================
// Symmetry across signal types — traces also surface ProtobufDecode
// =========================================================================

#[test]
fn truncated_traces_body_is_rejected_with_protobuf_decode_rule() {
    let real = common::encode_minimal_traces();
    assert!(real.len() > 60, "fixture too small to truncate at byte 50");
    let truncated = common::truncate(&real, 50);
    let violation = expect_violation(validate_traces(&truncated, Framing::HttpProtobuf));
    assert!(
        matches!(violation.rule, Rule::WireType(WireTypeRule::ProtobufDecode)),
        "expected WireType::ProtobufDecode for traces, got {:?}",
        violation.rule
    );
    assert_eq!(violation.signal_asserted, SignalType::Traces);
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
