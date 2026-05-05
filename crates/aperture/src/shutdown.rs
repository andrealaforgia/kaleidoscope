//! Shutdown orchestrator — flip `/readyz` to `Draining`, close listeners,
//! drain in-flight requests bounded by the configured deadline, name
//! the verdict on stderr.
//!
//! See `docs/feature/aperture/design/component-design.md > Module
//! structure :: shutdown/mod.rs`, ADR-0010 (drain interaction with the
//! per-transport semaphores), and `docs/feature/aperture/slices/
//! slice-08-graceful-shutdown.md` for the contract.
//!
//! The orchestrator is the only call site that emits the closed-vocabulary
//! events `shutdown_initiated`, `in_flight_drained`,
//! `drain_deadline_exceeded`, and `shutdown_complete`. The events fire
//! in this order:
//!
//! 1. `shutdown_initiated` (info) — first, so an operator sees the
//!    trigger before any other shutdown side effect.
//! 2. `readiness_changed ready=false reason=shutdown_drain` (info) —
//!    emitted by `flip_to_draining`; the `/readyz` flip is what an
//!    orchestrator's readiness probe acts on.
//! 3. `in_flight_drained drained_count=N` (info) OR
//!    `drain_deadline_exceeded dropped_count=N` (warn) — exactly one
//!    of these names the verdict.
//! 4. `shutdown_complete exit_code=N` (info) — last, naming the exit
//!    code the binary should return.
//!
//! `orchestrate_shutdown` runs in-process under a Tokio runtime; the
//! binary's `main.rs` calls it after `tokio::signal::unix` resolves on
//! SIGTERM/SIGINT, integration tests call it through
//! `Handle::shutdown`. Both paths emit the same event sequence — the
//! `signal` field on `shutdown_initiated` distinguishes them
//! (`SIGTERM`, `SIGINT`, or `handle_shutdown` for the in-process path).

use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::backpressure::ConcurrencyLimiter;
use crate::observability::event;
use crate::readiness::SharedReadinessState;

/// Grace period between flipping `/readyz` to `Draining` and signalling
/// the listeners to stop accepting. The DISCUSS Q1.2 "flip, wait, close,
/// drain" variant — short enough that even a fast drain stays within
/// the deadline budget, long enough that an orchestrator with a
/// readiness probe period of 100 ms (the slice 08 acceptance test's
/// budget) can observe the `503 "draining"` response before the
/// listener stops accepting.
///
/// 250 ms is the safe k8s value: kubelet's default
/// `readinessProbe.periodSeconds` is 10 s but tests poll every 5 ms,
/// and the slice 08 test polls for up to 100 ms. The grace period must
/// exceed the test's polling window so the probe sees the flipped
/// state before the listener closes.
const READYZ_DRAIN_GRACE: Duration = Duration::from_millis(250);

/// The trigger that initiated a shutdown. The `signal` field on the
/// `shutdown_initiated` event renders this as a stable string so
/// operators can grep stderr by trigger.
///
/// `Sigterm` and `Sigint` are wired by the binary's `aperture::run`;
/// the integration tests reach the orchestrator through
/// `HandleShutdown`. All three variants share the orchestrator entry
/// point so the event sequence is identical across triggers.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ShutdownTrigger {
    /// In-process shutdown via `Handle::shutdown`. The integration
    /// tests use this path; the production binary uses it after a real
    /// OS signal so the orchestrator sees a single shape.
    HandleShutdown,
    /// SIGTERM from the orchestrator (k8s `terminationGracePeriodSeconds`).
    Sigterm,
    /// SIGINT from a developer's Ctrl-C.
    Sigint,
}

impl ShutdownTrigger {
    /// Stable string used as the `signal` field on the
    /// `shutdown_initiated` event.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ShutdownTrigger::HandleShutdown => "handle_shutdown",
            ShutdownTrigger::Sigterm => "SIGTERM",
            ShutdownTrigger::Sigint => "SIGINT",
        }
    }
}

/// Outcome of a drain. `exit_code` is the integer the binary's `main`
/// should return to the process supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrainOutcome {
    /// All in-flight requests completed before the deadline.
    Clean { drained_count: u32 },
    /// The deadline expired with `dropped_count` requests still in-flight.
    DeadlineExceeded { dropped_count: u32 },
}

impl DrainOutcome {
    pub(crate) fn exit_code(self) -> u8 {
        match self {
            DrainOutcome::Clean { .. } => 0,
            DrainOutcome::DeadlineExceeded { .. } => 1,
        }
    }
}

/// Sum the per-transport in-flight counts. Saturating: if the sum
/// overflows `u32`, the count is clamped to `u32::MAX` rather than
/// wrapping. The orchestrator uses this for both the `drained_count`
/// (snapshot at signal time) and the `dropped_count` (snapshot at
/// deadline time), so the same arithmetic shape lives in one place.
///
/// Pinned by unit tests to kill the `+ -> -` and `+ -> *` mutations:
/// `(0, 1)` and `(1, 0)` test addition against subtraction; `(2, 3)`
/// tests against multiplication.
fn sum_in_flight(grpc: u32, http: u32) -> u32 {
    grpc.saturating_add(http)
}

/// Bundle of resources the orchestrator owns for the duration of a drain.
/// Constructed by `compose::spawn` and stored in the `Handle`; consumed
/// by `orchestrate_shutdown` (the bundle's join handles are awaited and
/// dropped, the listener-shutdown senders are signalled and dropped).
pub(crate) struct ShutdownBundle {
    pub(crate) readiness: SharedReadinessState,
    pub(crate) grpc_limiter: ConcurrencyLimiter,
    pub(crate) http_limiter: ConcurrencyLimiter,
    pub(crate) grpc_shutdown: oneshot::Sender<()>,
    pub(crate) http_shutdown: oneshot::Sender<()>,
    pub(crate) grpc_join: JoinHandle<()>,
    pub(crate) http_join: JoinHandle<()>,
    pub(crate) drain_deadline: Duration,
}

/// Orchestrate the full shutdown sequence. Returns the drain outcome so
/// the caller can map it to a process exit code.
pub(crate) async fn orchestrate_shutdown(
    trigger: ShutdownTrigger,
    bundle: ShutdownBundle,
) -> DrainOutcome {
    // 1. shutdown_initiated — first event, names the trigger and the
    //    deadline.
    let drain_deadline_ms = bundle.drain_deadline.as_millis() as u64;
    tracing::info!(
        event = event::SHUTDOWN_INITIATED,
        signal = trigger.as_str(),
        drain_deadline_ms = drain_deadline_ms,
    );

    // 2. Flip readiness immediately so /readyz returns 503 "draining\n"
    //    on the very next probe. `flip_to_draining` emits its own
    //    `event=readiness_changed` line.
    bundle.readiness.flip_to_draining();

    // 3. Snapshot in-flight before we ask listeners to close. Both
    //    transports' graceful-shutdown futures wait for in-flight
    //    requests to finish before resolving, so this is the count we
    //    expect to drain.
    let initial_in_flight = sum_in_flight(
        bundle.grpc_limiter.in_flight(),
        bundle.http_limiter.in_flight(),
    );

    // 4. "flip, wait, close, drain" — DISCUSS Q1.2's safer variant.
    //    The grace period exists so an orchestrator's readiness probe
    //    sees the `503 "draining"` response (and stops sending new
    //    traffic) before the TCP listener closes. Without this, the
    //    listener stops accepting before the probe lands and the
    //    orchestrator may keep routing traffic to a closing instance.
    //    The grace period is bounded; it never extends the total
    //    shutdown beyond `drain_deadline + READYZ_DRAIN_GRACE`.
    tokio::time::sleep(READYZ_DRAIN_GRACE).await;

    // 5. Trigger the listener shutdown signals. Tonic's
    //    `serve_with_incoming_shutdown` and axum's
    //    `with_graceful_shutdown` resolve their futures here —
    //    they stop accepting new connections AND wait for in-flight
    //    requests to complete. Their join handles are what we race
    //    against the deadline.
    let _ = bundle.grpc_shutdown.send(());
    let _ = bundle.http_shutdown.send(());

    // 6. Race the joins against the deadline.
    let join_grpc = bundle.grpc_join;
    let join_http = bundle.http_join;
    let drain_future = async move {
        let _ = join_grpc.await;
        let _ = join_http.await;
    };
    let outcome = match tokio::time::timeout(bundle.drain_deadline, drain_future).await {
        Ok(()) => {
            // Clean drain: every in-flight request completed before the
            // deadline. Name the count we observed at signal time.
            let drained_count = initial_in_flight;
            tracing::info!(
                event = event::IN_FLIGHT_DRAINED,
                drained_count = drained_count as u64,
            );
            DrainOutcome::Clean { drained_count }
        }
        Err(_elapsed) => {
            // Deadline expired with the joins still pending. Read
            // in-flight again to name the dropped count — it is the
            // count of permits still outstanding when the deadline
            // fired. This is loud, never silent: warn level, dropped
            // count named.
            let dropped_count = sum_in_flight(
                bundle.grpc_limiter.in_flight(),
                bundle.http_limiter.in_flight(),
            );
            tracing::warn!(
                event = event::DRAIN_DEADLINE_EXCEEDED,
                dropped_count = dropped_count as u64,
            );
            DrainOutcome::DeadlineExceeded { dropped_count }
        }
    };

    // 7. shutdown_complete — last event, names the exit code so an
    //    operator parsing the stderr stream sees a single closing line
    //    per process lifetime.
    tracing::info!(
        event = event::SHUTDOWN_COMPLETE,
        exit_code = outcome.exit_code() as u64,
    );

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_trigger_renders_handle_shutdown_string() {
        assert_eq!(ShutdownTrigger::HandleShutdown.as_str(), "handle_shutdown");
    }

    #[test]
    fn shutdown_trigger_renders_sigterm_string() {
        assert_eq!(ShutdownTrigger::Sigterm.as_str(), "SIGTERM");
    }

    #[test]
    fn shutdown_trigger_renders_sigint_string() {
        assert_eq!(ShutdownTrigger::Sigint.as_str(), "SIGINT");
    }

    #[test]
    fn clean_drain_exit_code_is_zero() {
        assert_eq!(DrainOutcome::Clean { drained_count: 3 }.exit_code(), 0);
    }

    #[test]
    fn deadline_exceeded_exit_code_is_one() {
        assert_eq!(
            DrainOutcome::DeadlineExceeded { dropped_count: 2 }.exit_code(),
            1
        );
    }

    #[test]
    fn sum_in_flight_returns_grpc_when_http_is_zero() {
        // (1, 0) — pins the addition shape against `+ -> -`: a
        // subtraction would yield 1 here too, so we need an asymmetric
        // companion case below to break the tie.
        assert_eq!(sum_in_flight(1, 0), 1);
    }

    #[test]
    fn sum_in_flight_returns_http_when_grpc_is_zero() {
        // (0, 1) — pins addition against `+ -> -`: subtraction would
        // yield 0u32 (saturating) instead of 1, so this asserts the
        // result equals the http arm exactly. Together with the test
        // above the `+ -> -` mutation cannot survive.
        assert_eq!(sum_in_flight(0, 1), 1);
    }

    #[test]
    fn sum_in_flight_adds_both_arms_when_both_non_zero() {
        // (2, 3) — pins addition against `+ -> *`: multiplication would
        // yield 6, addition yields 5. Tests both `+ -> -` (yields -1
        // saturating to 0) and `+ -> *` (yields 6) are killed by this
        // single assertion.
        assert_eq!(sum_in_flight(2, 3), 5);
    }

    #[test]
    fn sum_in_flight_saturates_on_overflow() {
        // Pin the saturating-add contract: u32::MAX + 1 stays at
        // u32::MAX rather than wrapping to 0. The drain orchestrator
        // never reaches this in practice (the cap is bounded), but the
        // saturating shape is what keeps the dropped_count event
        // monotonic with the underlying request flow.
        assert_eq!(sum_in_flight(u32::MAX, 1), u32::MAX);
    }
}
