//! `Decision` and `KeepReason` enums — the sealed observable
//! outcomes a sampler can produce, plus the structured reason carried
//! by the DEBUG event.
//!
//! Per ADR-0018 §"Why sealed `Decision` plus separate `KeepReason`":
//! the routing contract (Keep / Drop) is separated from the
//! observability contract (why a trace was kept). The decorator owns
//! the observability concern; the sampler stays at one method.

/// Sampling decision: keep the trace or drop it.
///
/// Sealed two-variant enum. Observability metadata (the reason a
/// trace was kept) is NOT carried here; it travels via the
/// [`KeepReason`] enum on the DEBUG tracing event the decorator
/// emits.
///
/// `Copy + Eq + Hash` so the decorator can route on the value without
/// `match`-and-discard ergonomic friction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decision {
    /// Forward this trace to the inner sink.
    Keep,
    /// Drop this trace; do not forward to the inner sink.
    Drop,
}

/// Reason a trace was kept. Carried only by DEBUG tracing events;
/// NOT returned from [`crate::Sampler::sample`].
///
/// `#[non_exhaustive]` so a future variant (e.g. tail-sampling's
/// "kept because of latency outlier") is an additive, non-breaking
/// change. Per ADR-0018 §"Cons" of Option A.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeepReason {
    /// At least one span in the trace carried `status.code == ERROR`.
    /// The error-bias rule kicks in and the trace is always kept,
    /// regardless of the configured rate.
    ErrorBearing,
    /// The trace_id-keyed `xxh3_64` hash, mapped into `[0.0, 1.0]`,
    /// fell strictly below the configured rate. The trace is kept by
    /// the rate-based rule.
    Sampled,
}
