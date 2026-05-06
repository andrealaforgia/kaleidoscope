//! `SamplingSink<S, N>` — the `OtlpSink + Probe` decorator that adds
//! head-based sampling on the `Traces` variant and forwards `Logs` /
//! `Metrics` unchanged.
//!
//! Per ADR-0021: generic over the inner sink type `S` and the
//! sampler type `N`; consumes Aperture's existing `OtlpSink +
//! Probe` traits; no Aperture-side trait amendment.
//!
//! ## DISTILL state
//!
//! - [`SamplingSink::new`] is `unimplemented!()` (DELIVER slice 01
//!   lands the timer-task spawn).
//! - The `OtlpSink::accept` impl panics on `unimplemented!()`
//!   (DELIVER slices 01–05 land the routing logic).
//! - The `Probe::probe` impl panics on `unimplemented!()` (DELIVER
//!   slice 01 lands the delegation).
//! - The [`__test_summary_tick_now`] test seam is real-but-stub: it
//!   accepts a `&SamplingSink<S, N>` and panics; DELIVER slice 06
//!   replaces the body with the synchronous summary emission.

// DISTILL state — fields are populated at DELIVER slice 01.
#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};

use crate::aggregator::Counters;
use crate::sampler::Sampler;

/// `OtlpSink + Probe` decorator adding head-based sampling on the
/// `Traces` variant; forwards `Logs` / `Metrics` unchanged per
/// DISCUSS Q6.
///
/// Generic over the inner sink type `S` (so the test path uses
/// concrete types and the production path uses `Arc<dyn OtlpSink>`
/// via Aperture's existing pattern) and the sampler type `N` (so
/// `HeadSampler` is the v0 concrete and a future tail sampler can
/// slot in without reshape).
///
/// Constructed via [`SamplingSink::new`]; the timer task is spawned
/// at construction on the ambient Tokio runtime per ADR-0020 §2.
pub struct SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    /// The inner sink the decorator wraps. Held in an `Arc` so the
    /// timer task can hold a clone without bounding the decorator's
    /// lifetime to the runtime's tick cadence.
    inner: Arc<S>,

    /// The sampler the decorator consults for `Traces` records.
    sampler: Arc<N>,

    /// The aggregator's counters. Held in an `Arc` so the timer task
    /// can read them concurrently with the hot path.
    counters: Arc<Counters>,
}

impl<S, N> SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    /// Wrap the inner sink with the given sampler.
    ///
    /// Spawns the periodic summary timer task on the ambient Tokio
    /// runtime per ADR-0020 §2. The task runs until the
    /// `SamplingSink` is dropped (cooperative cancellation via
    /// `tokio_util::sync::CancellationToken`).
    ///
    /// At DISTILL this constructor panics with `unimplemented!()`.
    /// DELIVER slice 01 lands the real spawn-and-store body.
    pub fn new(_inner: S, _sampler: N) -> Self {
        // The construction shape is locked here: pull the tick
        // interval from `SIEVE_SUMMARY_TICK_MS` (or default to
        // 60_000), spawn the timer task on the ambient runtime,
        // store the join handle and the cancellation token. DELIVER
        // slice 01 replaces this body.
        unimplemented!("SamplingSink::new lands at DELIVER slice 01");
    }
}

// =========================================================================
// `OtlpSink` and `Probe` impls — the integration point with Aperture.
//
// At DISTILL the bodies are `unimplemented!()`. The trait impls
// themselves exist so the `invariant_sampling_sink_is_otlp_sink_and_probe`
// test compiles: that test asserts at the type level that
// `SamplingSink<S, N>: OtlpSink + Probe` whenever `S: OtlpSink +
// Probe + Send + Sync + 'static` and `N: Sampler`.
// =========================================================================

impl<S, N> OtlpSink for SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    fn accept<'a>(
        &'a self,
        _record: SinkRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            // DELIVER slices 01-05 land the routing body:
            //   - SinkRecord::Logs / SinkRecord::Metrics →
            //     forward to inner unchanged (slice 05).
            //   - SinkRecord::Traces → group by trace_id, ask
            //     sampler per trace, emit DEBUG event per decision,
            //     forward kept-traces-only envelope to inner
            //     (slices 01-04).
            unimplemented!("SamplingSink::accept lands at DELIVER slices 01-05");
        })
    }
}

impl<S, N> Probe for SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    fn probe<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        Box::pin(async move {
            // DELIVER slice 01 lands the delegation body:
            //   `self.inner.probe().await`.
            // Per ADR-0021 §6: Sieve has no external dependency to
            // probe; the only external dependency is the inner sink.
            unimplemented!("SamplingSink::probe lands at DELIVER slice 01");
        })
    }
}

// =========================================================================
// Test seam — `__test_summary_tick_now` (per ADR-0018 §"Test seams"
// + ADR-0020 §6).
//
// Fires the snapshot-and-emit-INFO path synchronously, bypassing the
// Tokio timer entirely. Slice-06 uses this so the assertion does not
// depend on wall-clock time.
//
// At DISTILL the body is `unimplemented!()`; DELIVER slice 06
// replaces it with a snapshot of the counters and a call into
// `observability::emit_summary`.
// =========================================================================

/// Fire the periodic summary synchronously, without waiting for the
/// timer.
///
/// `#[doc(hidden)]` and the `__` prefix mark this as a test seam. The
/// slice-06 integration test calls this, then asserts the captured
/// `target = "sieve"` INFO event carries the expected field set.
#[doc(hidden)]
pub fn __test_summary_tick_now<S, N>(_sink: &SamplingSink<S, N>)
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    unimplemented!("__test_summary_tick_now lands at DELIVER slice 06");
}
