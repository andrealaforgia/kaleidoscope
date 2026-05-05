//! Per-transport concurrency cap — deterministic refusal under load.
//!
//! Slice 05 lights up the v0 backpressure policy locked in DISCUSS Q4
//! and ADR-0010: each transport carries an independent
//! `tokio::sync::Semaphore`, sized from
//! `Config::max_concurrent_requests`. New requests acquire a permit via
//! [`Semaphore::try_acquire_owned`] BEFORE the validator is invoked;
//! the permit is held for the lifetime of the request future and
//! dropped when the response is sent (so the sink hand-off-and-await
//! counts as in-flight, per ADR-0010). Saturation is the immediate
//! refusal path: gRPC `RESOURCE_EXHAUSTED`, HTTP 503 with
//! `Retry-After: 1`. There is no internal queue (Sluice's job, Phase
//! 7), no block (violates the OTel SDK contract), and no silent drop
//! (an explicit anti-pattern per DISCUSS D5).
//!
//! The same `Semaphore::available_permits()` primitive Slice 05 uses
//! here will drive the Slice 08 drain orchestrator's in-flight count.

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

use crate::observability::event;

/// Transport identifier used in the `concurrency_cap_hit` event's
/// `transport` field. The closed v0 vocabulary fixes the two strings
/// the structured-log line carries.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CapTransport {
    Grpc,
    HttpProtobuf,
}

impl CapTransport {
    /// Stable string used as the `transport` field on the
    /// `concurrency_cap_hit` event.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CapTransport::Grpc => "grpc",
            CapTransport::HttpProtobuf => "http_protobuf",
        }
    }
}

/// Per-transport concurrency limiter. Wraps the `Arc<Semaphore>` plus
/// the configured cap so call sites have a single value to clone into
/// the per-request handler.
#[derive(Clone)]
pub(crate) struct ConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
    cap: u32,
    transport: CapTransport,
}

impl ConcurrencyLimiter {
    /// Construct a new limiter with the given cap. Slice 05 wires one
    /// of these per transport in [`crate::compose::spawn`].
    pub(crate) fn new(cap: u32, transport: CapTransport) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(cap as usize)),
            cap,
            transport,
        }
    }

    /// The configured cap. Used by the refusal path to name the cap in
    /// the gRPC `grpc-message` and the HTTP body.
    pub(crate) fn cap(&self) -> u32 {
        self.cap
    }

    /// Try to acquire a permit. Returns `Ok(permit)` on success; on
    /// saturation, emits the `event=concurrency_cap_hit` warn line
    /// (with `transport`, `cap`, and `in_flight_at_refusal` fields)
    /// and returns `Err(())`. The permit is held for the lifetime of
    /// the caller's binding and dropped automatically at end of scope
    /// (which is the contract-specified "released on response sent").
    pub(crate) fn try_acquire(&self) -> Result<OwnedSemaphorePermit, ()> {
        match Arc::clone(&self.semaphore).try_acquire_owned() {
            Ok(permit) => Ok(permit),
            Err(TryAcquireError::NoPermits) => {
                emit_cap_hit_event(self.transport, self.cap, self.cap);
                Err(())
            }
            Err(TryAcquireError::Closed) => {
                // Closed semaphores arise only on shutdown; treat as
                // refusal so the caller surfaces the canonical refusal
                // shape rather than a panic. The `concurrency_cap_hit`
                // event with `in_flight_at_refusal == cap` is the
                // closest available shape; Slice 08's drain
                // orchestrator owns the explicit shutdown path.
                emit_cap_hit_event(self.transport, self.cap, self.cap);
                Err(())
            }
        }
    }
}

/// Emit the `event=concurrency_cap_hit` warn line. Closed v0
/// vocabulary; fields are `transport`, `cap`, and
/// `in_flight_at_refusal`. The latter is cheap to compute from
/// `Semaphore::available_permits()` at the moment of refusal — when
/// the cap is hit, every permit is outstanding, so `in_flight ==
/// cap`.
fn emit_cap_hit_event(transport: CapTransport, cap: u32, in_flight_at_refusal: u32) {
    tracing::warn!(
        event = event::CONCURRENCY_CAP_HIT,
        transport = transport.as_str(),
        cap = cap as u64,
        in_flight_at_refusal = in_flight_at_refusal as u64,
    );
}

/// Operator-facing diagnostic naming the cap. Used as the gRPC
/// `grpc-message` and the HTTP response body. The format is exactly
/// what Slice 05's acceptance tests assert: `cap of {N}` (the
/// alternate `cap={N}` form is also accepted by the test, kept here
/// for readability).
pub(crate) fn refusal_message(transport: CapTransport, cap: u32) -> String {
    format!(
        "aperture: {} concurrency cap of {} reached on transport={}",
        match transport {
            CapTransport::Grpc => "gRPC",
            CapTransport::HttpProtobuf => "HTTP",
        },
        cap,
        transport.as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_transport_grpc_renders_as_grpc() {
        assert_eq!(CapTransport::Grpc.as_str(), "grpc");
    }

    #[test]
    fn cap_transport_http_protobuf_renders_as_http_protobuf() {
        assert_eq!(CapTransport::HttpProtobuf.as_str(), "http_protobuf");
    }

    #[test]
    fn refusal_message_for_grpc_names_the_cap_and_transport() {
        let s = refusal_message(CapTransport::Grpc, 4);
        assert!(s.contains("cap of 4"), "got: {s}");
        assert!(s.contains("transport=grpc"), "got: {s}");
    }

    #[test]
    fn refusal_message_for_http_protobuf_names_the_cap_and_transport() {
        let s = refusal_message(CapTransport::HttpProtobuf, 4);
        assert!(s.contains("cap of 4"), "got: {s}");
        assert!(s.contains("transport=http_protobuf"), "got: {s}");
    }

    #[tokio::test]
    async fn limiter_with_cap_two_permits_first_two_acquires() {
        let limiter = ConcurrencyLimiter::new(2, CapTransport::Grpc);
        let _p1 = limiter.try_acquire().expect("first acquire");
        let _p2 = limiter.try_acquire().expect("second acquire");
    }

    #[tokio::test]
    async fn limiter_refuses_third_acquire_when_cap_is_two() {
        let limiter = ConcurrencyLimiter::new(2, CapTransport::Grpc);
        let _p1 = limiter.try_acquire().expect("first acquire");
        let _p2 = limiter.try_acquire().expect("second acquire");
        assert!(limiter.try_acquire().is_err());
    }

    #[tokio::test]
    async fn limiter_recovers_capacity_when_a_permit_is_dropped() {
        let limiter = ConcurrencyLimiter::new(1, CapTransport::Grpc);
        {
            let _p = limiter.try_acquire().expect("first acquire");
            // _p is in scope; cap is 1 and saturated.
            assert!(limiter.try_acquire().is_err());
        }
        // _p was dropped at end-of-block; capacity restored.
        let _p = limiter.try_acquire().expect("acquire after drop");
    }

    #[tokio::test]
    async fn limiter_cap_reports_the_configured_value() {
        let limiter = ConcurrencyLimiter::new(7, CapTransport::HttpProtobuf);
        assert_eq!(limiter.cap(), 7);
    }
}
