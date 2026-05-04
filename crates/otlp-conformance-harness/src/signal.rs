//! `SignalType` — which OTLP signal the caller asserts the bytes carry.
//!
//! Signal-type inference is explicitly out of scope for v0 (DISCUSS W3, US
//! System Constraint 7). The caller chooses which `validate_*` function to
//! invoke and the harness echoes the asserted signal back through every
//! `OtlpViolation`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SignalType {
    Logs,
    Traces,
    Metrics,
}
