//! `SieveConfigError` — the configuration error surface.
//!
//! Locked at ADR-0018 §"Public surface (final list)". The variant
//! set is `#[non_exhaustive]` so future configuration knobs can add
//! variants without breaking downstream consumers.
//!
//! Slice-test relevance: the slice-06 observability test asserts
//! `HeadSampler::from_env` rejects an out-of-range
//! `SIEVE_NON_ERROR_TRACE_RATE` and a non-numeric
//! `SIEVE_SUMMARY_TICK_MS` with the right variant. This file is
//! therefore real, not stubbed: the variant tags are part of the
//! contract slice tests rely on.

use thiserror::Error;

/// Sieve's configuration error surface.
///
/// Returned by [`crate::HeadSampler::new`] and
/// [`crate::HeadSampler::from_env`] when the configured rate or the
/// summary tick interval is unparseable or out-of-range.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SieveConfigError {
    /// The non-error trace rate is NaN, infinite, or outside
    /// `[0.0, 1.0]`.
    #[error("rate must be a finite value in [0.0, 1.0]; got {got}")]
    RateOutOfRange {
        /// The offending value as parsed (or `f64::NAN` /
        /// `f64::INFINITY` for the boundary cases).
        got: f64,
    },

    /// The `SIEVE_NON_ERROR_TRACE_RATE` env var contained a value
    /// that does not parse as a finite `f64`.
    #[error("rate value '{raw}' is not parseable as a float")]
    RateUnparseable {
        /// The verbatim env-var value that failed to parse.
        raw: String,
    },

    /// The `SIEVE_SUMMARY_TICK_MS` env var contained a value that
    /// does not parse as a positive integer (zero is also rejected
    /// per ADR-0020 §4 — `tokio::time::interval(Duration::ZERO)`
    /// panics).
    #[error("summary tick value '{raw}' is not parseable as a positive integer (milliseconds)")]
    SummaryTickUnparseable {
        /// The verbatim env-var value that failed to parse.
        raw: String,
    },
}
