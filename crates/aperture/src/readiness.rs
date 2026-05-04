//! Readiness state machine — drives `/readyz` and (Slice 08) the
//! drain orchestrator.
//!
//! See `docs/feature/aperture/design/component-design.md >
//! app::readiness::ReadinessState` for the design contract.
//!
//! ## State machine
//!
//! ```text
//! Starting → Ready → Draining
//! ```
//!
//! There is no path from `Draining` back to `Ready` (a draining
//! process never recovers; it exits). `Starting` flips to `Ready` only
//! once both the gRPC and HTTP listeners have signalled they are bound.
//!
//! Slice 02 lights up `mark_grpc_bound`, `mark_http_bound`, and the
//! `/readyz` read path. Slice 08 lands `flip_to_draining` and the
//! `event=readiness_changed ready=false` emit.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use crate::observability::event;

/// Readiness phase a `/readyz` probe can observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ReadinessPhase {
    Starting = 0,
    Ready = 1,
    #[allow(dead_code)] // Slice 08 lights up the draining transition.
    Draining = 2,
}

/// Shared readiness state. Cheap to clone (Arc-wrapped); axum handler
/// state and the composition root both hold one.
pub(crate) struct ReadinessState {
    inner: AtomicU8,
    grpc_bound: AtomicBool,
    http_bound: AtomicBool,
}

pub(crate) type SharedReadinessState = Arc<ReadinessState>;

impl ReadinessState {
    /// Construct a fresh readiness state in the `Starting` phase.
    pub(crate) fn new() -> SharedReadinessState {
        Arc::new(Self {
            inner: AtomicU8::new(ReadinessPhase::Starting as u8),
            grpc_bound: AtomicBool::new(false),
            http_bound: AtomicBool::new(false),
        })
    }

    /// Read the current readiness phase. `/readyz` calls this on every
    /// probe.
    pub(crate) fn current(&self) -> ReadinessPhase {
        match self.inner.load(Ordering::Acquire) {
            0 => ReadinessPhase::Starting,
            1 => ReadinessPhase::Ready,
            2 => ReadinessPhase::Draining,
            // The byte is private; only `Self` writes it. An out-of-range
            // value is an internal-invariant violation. Treat as
            // Starting (the most conservative answer for a probe).
            _ => ReadinessPhase::Starting,
        }
    }

    /// Signal that the gRPC listener is bound. Idempotent. Triggers a
    /// recompute of the overall readiness phase.
    pub(crate) fn mark_grpc_bound(&self) {
        self.grpc_bound.store(true, Ordering::Release);
        self.recompute_ready();
    }

    /// Signal that the HTTP listener is bound. Idempotent. Triggers a
    /// recompute of the overall readiness phase.
    pub(crate) fn mark_http_bound(&self) {
        self.http_bound.store(true, Ordering::Release);
        self.recompute_ready();
    }

    /// Promote `Starting` to `Ready` once both listeners are bound.
    /// A `Draining` instance is sticky — it never transitions back.
    fn recompute_ready(&self) {
        if self.current() == ReadinessPhase::Draining {
            return;
        }
        let both_bound =
            self.grpc_bound.load(Ordering::Acquire) && self.http_bound.load(Ordering::Acquire);
        if !both_bound {
            return;
        }
        // CAS Starting -> Ready so the transition fires exactly once
        // and the `event=readiness_changed` emit doesn't duplicate.
        let outcome = self.inner.compare_exchange(
            ReadinessPhase::Starting as u8,
            ReadinessPhase::Ready as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if outcome.is_ok() {
            tracing::info!(
                event = event::READINESS_CHANGED,
                ready = "true",
                reason = "listeners_bound",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_is_starting() {
        let state = ReadinessState::new();
        assert_eq!(state.current(), ReadinessPhase::Starting);
    }

    #[test]
    fn marking_grpc_alone_does_not_promote_to_ready() {
        let state = ReadinessState::new();
        state.mark_grpc_bound();
        assert_eq!(state.current(), ReadinessPhase::Starting);
    }

    #[test]
    fn marking_http_alone_does_not_promote_to_ready() {
        let state = ReadinessState::new();
        state.mark_http_bound();
        assert_eq!(state.current(), ReadinessPhase::Starting);
    }

    #[test]
    fn marking_both_listeners_promotes_to_ready() {
        let state = ReadinessState::new();
        state.mark_grpc_bound();
        state.mark_http_bound();
        assert_eq!(state.current(), ReadinessPhase::Ready);
    }

    #[test]
    fn marking_both_in_reverse_order_promotes_to_ready() {
        let state = ReadinessState::new();
        state.mark_http_bound();
        state.mark_grpc_bound();
        assert_eq!(state.current(), ReadinessPhase::Ready);
    }
}
