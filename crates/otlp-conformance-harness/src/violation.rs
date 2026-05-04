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
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = (
            &self.rule,
            &self.locus,
            &self.expected,
            &self.observed,
            &self.signal_asserted,
            &self.framing_asserted,
        );
        unimplemented!(
            "Display for OtlpViolation: implementation deferred to DELIVER wave"
        )
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
