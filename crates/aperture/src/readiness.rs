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
//! `Ready` (or `Starting`) flips to `Draining` exactly once on shutdown
//! initiation, via [`ReadinessState::flip_to_draining`].
//!
//! Slice 02 lights up `mark_grpc_bound`, `mark_http_bound`, and the
//! `/readyz` read path. Slice 08 lands `flip_to_draining` and the
//! `event=readiness_changed ready=false reason=shutdown_drain` emit.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use crate::observability::event;

/// Readiness phase a `/readyz` probe can observe.
///
/// Slice 02 lit up the `Starting → Ready` half of the state machine.
/// Slice 08 lands the `Draining` variant — the third state, sticky on
/// entry, producing 503 `"draining"` on `/readyz` so an orchestrator's
/// readiness probe stops sending traffic while in-flight requests
/// complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ReadinessPhase {
    Starting = 0,
    Ready = 1,
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
    ///
    /// The byte is private; only `Self` writes it. An out-of-range byte
    /// (impossible without an internal-invariant violation) is treated
    /// as `Starting`, the most conservative answer for a probe.
    pub(crate) fn current(&self) -> ReadinessPhase {
        match self.inner.load(Ordering::Acquire) {
            v if v == ReadinessPhase::Draining as u8 => ReadinessPhase::Draining,
            v if v == ReadinessPhase::Ready as u8 => ReadinessPhase::Ready,
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
    ///
    /// The CAS only fires when the current state is `Starting` —
    /// `Draining` is sticky and never demotes back. Slice 08's drain
    /// orchestrator may call `flip_to_draining` after both listeners
    /// have bound but before `recompute_ready` runs (the binding
    /// notifies are racy with the shutdown signal in pathological
    /// startup-then-immediate-SIGTERM windows); the CAS guard is what
    /// keeps `Draining` sticky.
    fn recompute_ready(&self) {
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

    /// Flip the readiness phase to `Draining`. Idempotent — the second
    /// and later calls return without re-emitting the
    /// `event=readiness_changed` line. The transition is sticky: once
    /// `Draining`, `recompute_ready` cannot demote the phase back to
    /// `Starting` or `Ready`.
    ///
    /// The CAS targets either `Starting` or `Ready` because Slice 08's
    /// shutdown orchestrator may fire before both listeners have
    /// bound (a SIGTERM during startup) — the contract is "flip to
    /// `Draining` from any other state, exactly once".
    #[allow(dead_code)] // Wired by `Handle::shutdown` in the next commit.
    pub(crate) fn flip_to_draining(&self) {
        // Try Ready -> Draining first (the common case).
        let from_ready = self.inner.compare_exchange(
            ReadinessPhase::Ready as u8,
            ReadinessPhase::Draining as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if from_ready.is_ok() {
            tracing::info!(
                event = event::READINESS_CHANGED,
                ready = "false",
                reason = "shutdown_drain",
            );
            return;
        }
        // Fall back to Starting -> Draining (SIGTERM before bind).
        let from_starting = self.inner.compare_exchange(
            ReadinessPhase::Starting as u8,
            ReadinessPhase::Draining as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if from_starting.is_ok() {
            tracing::info!(
                event = event::READINESS_CHANGED,
                ready = "false",
                reason = "shutdown_drain",
            );
        }
        // Already Draining (or any other byte): no-op, no emit.
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

    #[test]
    fn flip_to_draining_from_ready_lands_in_draining() {
        let state = ReadinessState::new();
        state.mark_grpc_bound();
        state.mark_http_bound();
        assert_eq!(state.current(), ReadinessPhase::Ready);
        state.flip_to_draining();
        assert_eq!(state.current(), ReadinessPhase::Draining);
    }

    #[test]
    fn flip_to_draining_from_starting_lands_in_draining() {
        // SIGTERM before either listener binds: the drain still flips
        // the readiness probe so any orchestrator's readiness check
        // returns 503 immediately.
        let state = ReadinessState::new();
        assert_eq!(state.current(), ReadinessPhase::Starting);
        state.flip_to_draining();
        assert_eq!(state.current(), ReadinessPhase::Draining);
    }

    #[test]
    fn draining_is_sticky_against_late_listener_bound_signals() {
        // Drain fires before listeners bind; subsequent `mark_*_bound`
        // calls must not promote the phase back to `Ready`. This pins
        // the sticky-Draining contract — without it a startup-then-SIGTERM
        // race could leave `/readyz` reporting 200 after a drain.
        let state = ReadinessState::new();
        state.flip_to_draining();
        state.mark_grpc_bound();
        state.mark_http_bound();
        assert_eq!(state.current(), ReadinessPhase::Draining);
    }

    #[test]
    fn flip_to_draining_is_idempotent() {
        let state = ReadinessState::new();
        state.mark_grpc_bound();
        state.mark_http_bound();
        state.flip_to_draining();
        state.flip_to_draining();
        state.flip_to_draining();
        assert_eq!(state.current(), ReadinessPhase::Draining);
    }
}
