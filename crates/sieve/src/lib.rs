//! Sieve — Kaleidoscope's head-based trace sampler with error-bias
//! retention.
//!
//! Sieve is the platform component that sits between Aperture (the
//! OTLP gateway) and the next pipeline stage. At v0 it performs
//! head-based probabilistic sampling on trace data with error-bias
//! retention (an error-bearing trace is always kept, regardless of
//! the configured rate); logs and metrics pass through unchanged.
//!
//! ## Public surface (locked at ADR-0018)
//!
//! - [`Sampler`] — the sampling-decision contract Sieve exposes.
//! - [`Decision`] — sealed two-variant `Keep` / `Drop` enum.
//! - [`KeepReason`] — observability metadata (carried by the DEBUG
//!   event, NOT by the return value).
//! - [`HeadSampler`] — concrete `Sampler` keyed by `xxh3_64(trace_id)`.
//! - [`SamplingSink`] — generic `OtlpSink + Probe` decorator wrapping
//!   any inner sink (per ADR-0021).
//! - [`TraceView`] — borrowed view over a logical trace's spans.
//! - [`SieveConfigError`] — the configuration error surface.
//!
//! Plus two `#[doc(hidden)]` test seams (per ADR-0018 §"Test seams"):
//!
//! - [`__test_trace_view`] — construct a [`TraceView`] from fixture
//!   spans without running the decorator's grouping pass.
//! - [`__test_summary_tick_now`] — fire the periodic INFO summary
//!   synchronously without waiting for the tokio timer.
//!
//! ## DISTILL state
//!
//! At DISTILL completion every public method panics with
//! `unimplemented!()` (or returns the canonical RED placeholder for
//! constructors that the slice tests must call to compile). DELIVER
//! turns each panic into a real implementation, one slice at a time.
//!
//! The two constructors that tests must call to compile are
//! intentionally real enough to run:
//!
//! - [`HeadSampler::from_env`] returns `Result<HeadSampler,
//!   SieveConfigError>` so slice tests can assert against the error
//!   surface from day one.
//! - [`HeadSampler::new`] returns `Result<HeadSampler,
//!   SieveConfigError>` for the same reason.
//! - [`__test_trace_view`] returns a real `TraceView<'_>` so slice
//!   tests can build fixture views without a running decorator.
//!
//! Every method that performs an actual sampling decision (`sample`,
//! `accept`, `probe`) panics on `unimplemented!()` until DELIVER.

#![forbid(unsafe_code)]

mod aggregator;
mod decision;
mod decorator;
mod error;
mod observability;
mod sampler;
mod trace_view;

// =========================================================================
// Public surface — re-exports per ADR-0018 §"Public surface (final list)".
// =========================================================================

pub use decision::{Decision, KeepReason};
pub use decorator::SamplingSink;
pub use error::SieveConfigError;
pub use sampler::{HeadSampler, Sampler};
pub use trace_view::TraceView;

// =========================================================================
// Test seams — `#[doc(hidden)]` per ADR-0018 §"Test seams".
//
// The `__` prefix + `#[doc(hidden)]` combination follows Spark's
// ADR-0011 precedent (`__reset_for_testing`). `cargo public-api`
// records the seams on the manifest; the convention signals "stable
// across versions, but explicitly not part of the consumer-facing
// contract".
// =========================================================================

#[doc(hidden)]
pub use trace_view::__test_trace_view;

#[doc(hidden)]
pub use decorator::__test_summary_tick_now;

/// The env var name that carries the non-error trace rate. Exposed
/// for slice tests that drive `HeadSampler::from_env`. NOT a
/// consumer-facing public-surface item; the contract is that
/// operators set this in their deployment manifest. Surfacing the
/// name as a constant lets the slice-06 integration test set and
/// unset the env var without duplicating the literal.
#[doc(hidden)]
pub fn sampler_env_for_tests() -> &'static str {
    sampler::SIEVE_NON_ERROR_TRACE_RATE_ENV
}
