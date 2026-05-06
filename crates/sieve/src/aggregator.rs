//! `Counters` and `SummaryTask` — the summary aggregator and its
//! timer task.
//!
//! Per ADR-0020: three `AtomicU64`s on the hot path (no lock
//! contention), `swap` for snapshot-and-reset, a Tokio timer task
//! owned by [`crate::SamplingSink`] that fires every
//! `SIEVE_SUMMARY_TICK_MS` (default `60_000`).
//!
//! ## DISTILL state
//!
//! Every method panics with `unimplemented!()` until DELIVER lands
//! the production implementation. The `Counters` struct itself is
//! real (three `AtomicU64` fields) so the type compiles and slice
//! tests can construct a `SamplingSink` whose internal state is
//! type-correct.

// DISTILL state — DELIVER slices wire each item up. Suppressed at
// the module level because `cargo clippy --all-targets -- -D
// warnings` runs against this crate and the dead-code warnings would
// otherwise fail the pre-commit hook.
#![allow(dead_code)]

use std::str::FromStr;
use std::sync::atomic::AtomicU64;

use crate::error::SieveConfigError;

/// The environment variable name carrying the summary tick interval
/// in milliseconds.
///
/// Locked at ADR-0020 §4. Default is `60_000` (60 seconds);
/// integration tests override to a smaller value (or use
/// `tokio::time::pause()` + `advance()` for deterministic-time tests).
///
/// `SIEVE_SUMMARY_TICK_MS` is NOT part of the consumer-facing
/// contract — it exists for test infrastructure and operational
/// override.
pub(crate) const SIEVE_SUMMARY_TICK_MS_ENV: &str = "SIEVE_SUMMARY_TICK_MS";

/// The default summary tick interval. Per ADR-0020 §4 + DISCUSS Q8.
pub(crate) const DEFAULT_SUMMARY_TICK_MS: u64 = 60_000;

/// The three counters Sieve aggregates over a summary window.
///
/// Per ADR-0020 §1: independent `AtomicU64`s, `Relaxed` ordering on
/// the hot path, `swap` for snapshot-and-reset. The cross-counter
/// race in `snapshot_and_reset` is semantically benign for the
/// "approximate aggregate over the window" contract.
pub(crate) struct Counters {
    pub(crate) kept_total: AtomicU64,
    pub(crate) kept_error_bearing: AtomicU64,
    pub(crate) dropped: AtomicU64,
}

impl Counters {
    /// Construct a fresh counter set with all three counters at zero.
    pub(crate) fn new() -> Self {
        Self {
            kept_total: AtomicU64::new(0),
            kept_error_bearing: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse the summary tick interval from the environment.
///
/// - Unset: returns [`DEFAULT_SUMMARY_TICK_MS`].
/// - Set to a positive integer: returns that value.
/// - Set to non-numeric or zero: returns
///   [`SieveConfigError::SummaryTickUnparseable`].
///
/// Real at DISTILL because [`crate::HeadSampler::from_env`] does NOT
/// call this (the rate parse is independent), but
/// [`crate::SamplingSink::new`] WILL call this at DELIVER. The
/// parser shape is locked here; the slice-06 RED test asserts on
/// the variant.
pub(crate) fn parse_summary_tick_ms_from_env() -> Result<u64, SieveConfigError> {
    let raw = std::env::var(SIEVE_SUMMARY_TICK_MS_ENV).ok();
    match raw {
        None => Ok(DEFAULT_SUMMARY_TICK_MS),
        Some(value) => {
            let parsed = u64::from_str(value.trim())
                .map_err(|_| SieveConfigError::SummaryTickUnparseable { raw: value.clone() })?;
            if parsed == 0 {
                Err(SieveConfigError::SummaryTickUnparseable { raw: value })
            } else {
                Ok(parsed)
            }
        }
    }
}
