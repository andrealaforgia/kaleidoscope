//! Violation type and rule taxonomy.
//!
//! Per ADR-0002:
//! - `Rule` is a nested enum: `EmptyInput | WireType(WireTypeRule)`.
//! - Every public enum and `OtlpViolation` itself carry `#[non_exhaustive]`.
//! - `OtlpViolation` implements `std::error::Error` with a single-line
//!   `Display` and a `source()` chain wrapping `prost::DecodeError` via a
//!   crate-private boxed trait object.
//! - `expected` and `observed` use `Cow<'static, str>` so static literals
//!   pay no allocations and prost-derived diagnostics pay one allocation.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use crate::framing::Framing;
use crate::signal::SignalType;

/// Position within a byte sequence where a violation was detected.
///
/// `Known(n)` carries a best-effort offset; `Unknown` is recorded when the
/// underlying decoder does not provide one (US-02 technical notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ByteOffset {
    Known(usize),
    Unknown,
}

/// The closed set of violation rules. Adding a variant is a minor-version
/// bump under `#[non_exhaustive]`'s additive-evolution rules.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rule {
    /// The byte sequence had length zero (US-01).
    EmptyInput,
    /// A wire-level violation: the bytes did not match the expected
    /// protobuf descriptor (US-02) or matched a different signal than the
    /// one asserted (US-03).
    WireType(WireTypeRule),
}

/// Wire-type sub-rules. Nested under `Rule::WireType` so future rule
/// families (semantic-conventions checks, framing-level checks) get their
/// own namespaces.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireTypeRule {
    /// `prost` refused to decode the bytes for the asserted signal's
    /// descriptor (US-02).
    ProtobufDecode,
    /// The bytes decoded cleanly as a different OTLP signal than the one
    /// the caller asserted (US-03).
    SignalMismatch {
        observed: SignalType,
        asserted: SignalType,
    },
}

/// The harness's only error type. Returned by every `validate_*` function
/// on the reject path. Per ADR-0002 the public surface carries the rule,
/// the byte locus, the asserted signal/framing, and human-readable
/// expected/observed strings; the `prost::DecodeError` (when present) is
/// reachable through `std::error::Error::source()` only.
#[derive(Debug)]
#[non_exhaustive]
pub struct OtlpViolation {
    pub rule: Rule,
    pub locus: ByteOffset,
    pub expected: Cow<'static, str>,
    pub observed: Cow<'static, str>,
    pub signal_asserted: SignalType,
    pub framing_asserted: Framing,
    /// Crate-private causal chain. Set only when wrapping a
    /// `prost::DecodeError`. Consumers walking the chain see
    /// `&dyn std::error::Error`, never `&prost::DecodeError`, satisfying
    /// US-02 AC 3.
    pub(crate) source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl fmt::Display for OtlpViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "otlp violation: rule={} signal={:?} framing={:?} locus={} expected={:?} observed={:?}",
            DisplayRule(&self.rule),
            self.signal_asserted,
            self.framing_asserted,
            DisplayLocus(self.locus),
            self.expected.as_ref(),
            self.observed.as_ref(),
        )
    }
}

struct DisplayRule<'a>(&'a Rule);

impl fmt::Display for DisplayRule<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Rule::EmptyInput => f.write_str("EmptyInput"),
            Rule::WireType(WireTypeRule::ProtobufDecode) => f.write_str("WireType::ProtobufDecode"),
            Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }) => {
                write!(
                    f,
                    "WireType::SignalMismatch{{observed={observed:?}, asserted={asserted:?}}}"
                )
            }
        }
    }
}

struct DisplayLocus(ByteOffset);

impl fmt::Display for DisplayLocus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ByteOffset::Known(n) => write!(f, "byte {n}"),
            ByteOffset::Unknown => f.write_str("unknown"),
        }
    }
}

impl Error for OtlpViolation {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|b| b as &(dyn Error + 'static))
    }
}

/// Build the canonical `Rule::EmptyInput` violation for a zero-length body
/// asserted under `signal` and `framing`. Used by every `validate_*`
/// function on the empty-input path.
pub(crate) fn empty_input_violation(
    signal: SignalType,
    framing: Framing,
) -> OtlpViolation {
    OtlpViolation {
        rule: Rule::EmptyInput,
        locus: ByteOffset::Known(0),
        expected: Cow::Borrowed("non-empty OTLP body"),
        observed: Cow::Borrowed("0 bytes"),
        signal_asserted: signal,
        framing_asserted: framing,
        source: None,
    }
}

/// Build the `Rule::WireType(WireTypeRule::ProtobufDecode)` violation that
/// wraps a `prost::DecodeError`. The `observed` field carries a
/// harness-owned category drawn from a closed taxonomy, derived from the
/// underlying prost description; the raw prost diagnostic is preserved
/// only via `Error::source()`. The byte locus is best-effort: prost does
/// not carry an offset, so the harness records the input length as the
/// position at which decoding ran out (US-02 technical notes).
pub(crate) fn protobuf_decode_violation(
    signal: SignalType,
    framing: Framing,
    input_len: usize,
    prost_err: prost::DecodeError,
) -> OtlpViolation {
    let observed = classify_prost_decode_error(&prost_err);
    OtlpViolation {
        rule: Rule::WireType(WireTypeRule::ProtobufDecode),
        locus: ByteOffset::Known(input_len),
        expected: Cow::Borrowed(
            "valid protobuf wire bytes per opentelemetry-proto descriptor",
        ),
        observed,
        signal_asserted: signal,
        framing_asserted: framing,
        source: Some(Box::new(prost_err)),
    }
}

/// Build the `Rule::WireType(WireTypeRule::SignalMismatch { observed,
/// asserted })` violation for bytes that decode cleanly as a different
/// OTLP signal than the one the caller invoked. Per US-03 the byte
/// locus is `Known(0)` — the mismatch is intrinsic to the body, not
/// localised to any byte within it.
pub(crate) fn signal_mismatch_violation(
    observed: SignalType,
    asserted: SignalType,
    framing: Framing,
) -> OtlpViolation {
    OtlpViolation {
        rule: Rule::WireType(WireTypeRule::SignalMismatch { observed, asserted }),
        locus: ByteOffset::Known(0),
        expected: Cow::Borrowed("OTLP body matching the asserted signal"),
        observed: Cow::Borrowed(
            "OTLP body matching a different signal than asserted",
        ),
        signal_asserted: asserted,
        framing_asserted: framing,
        source: None,
    }
}

/// Map a `prost::DecodeError`'s free-form description to one of the
/// harness's named decode-error categories. The categories are exactly
/// those US-02's UAT scenarios name; consumers may rely on the
/// substring stability for log-greppable diagnostics.
fn classify_prost_decode_error(err: &prost::DecodeError) -> Cow<'static, str> {
    let raw = err.to_string();
    let lower = raw.to_lowercase();
    if lower.contains("buffer underflow")
        || lower.contains("unexpected end")
        || lower.contains("eof")
    {
        return Cow::Borrowed("unexpected EOF in length-delimited field");
    }
    if lower.contains("invalid varint") {
        return Cow::Borrowed("invalid varint");
    }
    if lower.contains("invalid wire type")
        || lower.contains("wire type mismatch")
        || lower.contains("wire type")
    {
        return Cow::Borrowed("wire type error");
    }
    if lower.contains("length delimiter") || lower.contains("missing length-delimited") {
        return Cow::Borrowed("missing length-delimited data");
    }
    Cow::Owned(raw)
}

// =========================================================================
// Inner-loop unit tests
// =========================================================================
//
// These tests exercise the violation-construction helpers and the
// `Display` impl directly. The functions are their own driving ports —
// calling them from the test IS port-to-port testing at the domain scope
// (TDD methodology skill, "Pure domain functions ARE their own driving
// ports").

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_empty_input_renders_single_line_with_named_components() {
        let v = empty_input_violation(SignalType::Logs, Framing::HttpProtobuf);
        let line = format!("{v}");
        assert!(!line.contains('\n'), "Display must be single-line");
        assert!(line.starts_with("otlp violation: "), "missing prefix: {line}");
        assert!(line.contains("rule=EmptyInput"), "missing rule: {line}");
        assert!(line.contains("signal=Logs"), "missing signal: {line}");
        assert!(line.contains("framing=HttpProtobuf"), "missing framing: {line}");
        assert!(line.contains("locus=byte 0"), "missing locus: {line}");
        assert!(
            line.contains("expected=\"non-empty OTLP body\""),
            "missing expected: {line}"
        );
        assert!(line.contains("observed=\"0 bytes\""), "missing observed: {line}");
    }

    #[test]
    fn display_protobuf_decode_renders_nested_rule_and_input_len_locus() {
        let prost_err = prost::DecodeError::new("buffer underflow");
        let v = protobuf_decode_violation(
            SignalType::Traces,
            Framing::GrpcProtobuf,
            42,
            prost_err,
        );
        let line = format!("{v}");
        assert!(line.contains("rule=WireType::ProtobufDecode"), "{line}");
        assert!(line.contains("signal=Traces"), "{line}");
        assert!(line.contains("framing=GrpcProtobuf"), "{line}");
        assert!(line.contains("locus=byte 42"), "{line}");
        assert!(
            line.contains("observed=\"unexpected EOF in length-delimited field\""),
            "{line}"
        );
    }

    #[test]
    fn display_signal_mismatch_renders_observed_and_asserted() {
        let v = signal_mismatch_violation(
            SignalType::Traces,
            SignalType::Logs,
            Framing::HttpProtobuf,
        );
        let line = format!("{v}");
        assert!(
            line.contains("rule=WireType::SignalMismatch{observed=Traces, asserted=Logs}"),
            "{line}"
        );
        assert!(line.contains("signal=Logs"), "{line}");
        assert!(line.contains("locus=byte 0"), "{line}");
    }

    #[test]
    fn display_unknown_locus_renders_unknown_keyword() {
        let v = OtlpViolation {
            rule: Rule::EmptyInput,
            locus: ByteOffset::Unknown,
            expected: Cow::Borrowed("expected"),
            observed: Cow::Borrowed("observed"),
            signal_asserted: SignalType::Metrics,
            framing_asserted: Framing::HttpProtobuf,
            source: None,
        };
        let line = format!("{v}");
        assert!(line.contains("locus=unknown"), "{line}");
    }

    #[test]
    fn source_chain_exposes_prost_decode_error_under_dyn_error() {
        let prost_err = prost::DecodeError::new("invalid varint");
        let v = protobuf_decode_violation(
            SignalType::Logs,
            Framing::HttpProtobuf,
            10,
            prost_err,
        );
        let chain: &dyn Error = &v;
        let inner = chain.source().expect("source must be set for ProtobufDecode");
        assert!(inner.to_string().contains("invalid varint"));
    }

    #[test]
    fn source_chain_is_none_for_violations_without_a_prost_cause() {
        let v = empty_input_violation(SignalType::Logs, Framing::HttpProtobuf);
        let chain: &dyn Error = &v;
        assert!(chain.source().is_none());
    }

    #[test]
    fn classify_prost_decode_error_recognises_buffer_underflow() {
        let err = prost::DecodeError::new("buffer underflow");
        assert_eq!(
            classify_prost_decode_error(&err),
            Cow::Borrowed::<str>("unexpected EOF in length-delimited field")
        );
    }

    #[test]
    fn classify_prost_decode_error_recognises_invalid_varint() {
        let err = prost::DecodeError::new("invalid varint");
        assert_eq!(
            classify_prost_decode_error(&err),
            Cow::Borrowed::<str>("invalid varint")
        );
    }

    #[test]
    fn classify_prost_decode_error_recognises_wire_type_failures() {
        let err = prost::DecodeError::new("invalid wire type: Varint (expected LengthDelimited)");
        assert_eq!(
            classify_prost_decode_error(&err),
            Cow::Borrowed::<str>("wire type error")
        );
    }

    #[test]
    fn classify_prost_decode_error_falls_through_to_raw_for_unrecognised_descriptions() {
        let err = prost::DecodeError::new("brand new failure mode never seen before");
        let classified = classify_prost_decode_error(&err);
        assert!(classified.contains("brand new failure mode"));
    }
}
