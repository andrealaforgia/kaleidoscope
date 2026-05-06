//! `Counters` and `SummaryTask` — the summary aggregator and its
//! timer task.
//!
//! Per ADR-0020: three `AtomicU64`s on the hot path (no lock
//! contention), `swap` for snapshot-and-reset, a Tokio timer task
//! owned by [`crate::SamplingSink`] that fires every
//! `SIEVE_SUMMARY_TICK_MS` (default `60_000`).
//!
//! ## DELIVER state — slice 06
//!
//! - [`Counters`] exposes `record_kept_error_bearing`,
//!   `record_kept_sampled`, `record_dropped`, and
//!   [`Counters::snapshot_and_reset`]. The hot path uses
//!   `fetch_add(1, Relaxed)` (single CPU instruction on x86_64 and
//!   aarch64); the snapshot uses `swap(0, Relaxed)` per counter.
//! - [`SummaryTask`] spawns a Tokio task that ticks every
//!   `interval_ms` and emits the INFO summary on each tick. Owned by
//!   [`crate::SamplingSink`]; cancelled via `tokio_util`'s
//!   `CancellationToken` on drop.

#![allow(dead_code)]

use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::error::SieveConfigError;
use crate::observability::emit_summary;

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

    /// Record a kept trace whose `KeepReason` is `ErrorBearing`.
    ///
    /// Increments both `kept_total` and `kept_error_bearing`. The
    /// summary's `kept_sampled` is derived as `kept_total -
    /// kept_error_bearing`.
    pub(crate) fn record_kept_error_bearing(&self) {
        self.kept_total.fetch_add(1, Ordering::Relaxed);
        self.kept_error_bearing.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a kept trace whose `KeepReason` is `Sampled` (the
    /// rate-based rule kept it).
    ///
    /// Increments only `kept_total`; the `kept_sampled` figure in the
    /// summary is derived. This shape (independent counters,
    /// derivation at render time) matches ADR-0020 §1's "the
    /// relationships are derivable" rationale.
    pub(crate) fn record_kept_sampled(&self) {
        self.kept_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a dropped trace.
    pub(crate) fn record_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot and reset. Returns `(kept_total, kept_error_bearing,
    /// dropped)` at the time of the call. Safe to call concurrently
    /// with the recorder methods; the only ordering guarantee is that
    /// each counter's swap is atomic. A concurrent record between the
    /// three swaps lands in the next window — acceptable for a 60 s
    /// window because the operator's ask is "approximate aggregate",
    /// not "exact partition between windows".
    pub(crate) fn snapshot_and_reset(&self) -> (u64, u64, u64) {
        let kept = self.kept_total.swap(0, Ordering::Relaxed);
        let kept_err = self.kept_error_bearing.swap(0, Ordering::Relaxed);
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        (kept, kept_err, dropped)
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
/// Called by [`crate::SamplingSink::new`] at construction time. On
/// parse error the constructor falls back to the default rather than
/// surfacing a `Result` (per ADR-0018 §"Public surface": `new`
/// returns `Self`, not `Result`); the parser variant is reserved for
/// a future minor release that exposes a `try_new` constructor.
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

/// Tokio timer task that ticks every `interval_ms` and emits the
/// periodic INFO summary.
///
/// Per ADR-0020 §2-3: owned by [`crate::SamplingSink`], spawned at
/// `SamplingSink::new`, cancelled via `CancellationToken` on Drop.
/// The Drop path is sync-signal + abandon-join, matching Aperture's
/// `Handle::Drop` precedent (no `.await` from sync context).
pub(crate) struct SummaryTask {
    cancel: CancellationToken,
    join: Option<JoinHandle<()>>,
}

impl SummaryTask {
    /// Spawn the timer task on the ambient Tokio runtime.
    ///
    /// The task runs `tokio::time::interval(Duration::from_millis(
    /// interval_ms))` with `MissedTickBehavior::Delay` (so a paused
    /// runtime does not produce a burst of catch-up ticks). On every
    /// tick: snapshot the counters and call
    /// [`crate::observability::emit_summary`].
    ///
    /// On `cancel`: break out of the loop. The final flush is NOT
    /// inside the loop's exit branch because tests serialise via
    /// `#[serial_test::serial]` and a final-flush emission landing in
    /// a subsequent test's capture buffer is a cross-test pollution
    /// hazard. The integration-test path drives the summary through
    /// the [`crate::__test_summary_tick_now`] seam; the timer task's
    /// only emission point is the `interval` tick.
    pub(crate) fn spawn(counters: Arc<Counters>, rate: f64, interval_ms: u64) -> Self {
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        let join = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            // The first `tick()` fires immediately by Tokio's default;
            // consume it so the first emission lands at `interval_ms`,
            // not at construction time. A construction-time emission
            // would be empty (counters all zero) and noisy.
            let _ = ticker.tick().await;
            loop {
                tokio::select! {
                    _ = cancel_for_task.cancelled() => break,
                    _ = ticker.tick() => {
                        let (kept, kept_err, dropped) = counters.snapshot_and_reset();
                        emit_summary(kept, kept_err, dropped, rate);
                    }
                }
            }
        });
        Self {
            cancel,
            join: Some(join),
        }
    }
}

impl Drop for SummaryTask {
    fn drop(&mut self) {
        // Sync cancel + abandon join, matching Aperture's `Handle::Drop`
        // precedent (per ADR-0020 §3 Option A). The token signal is a
        // single atomic store; the JoinHandle is dropped here, which
        // detaches the future. The task observes the cancel at its
        // next `select!` await point and exits.
        self.cancel.cancel();
        // Drop the JoinHandle explicitly — `Option::take` makes the
        // intent visible (we do not await; we abandon).
        let _ = self.join.take();
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for `Counters` and `parse_summary_tick_ms_from_env`.
    //!
    //! Port-to-port at domain scope per Mandate 2: every test exercises
    //! the public method of `Counters` (the recorder methods and
    //! `snapshot_and_reset`) or the free function
    //! `parse_summary_tick_ms_from_env` (whose signature IS its
    //! driving port).
    //!
    //! The `SummaryTask` is exercised through the
    //! `slice_06_observability` integration test via the
    //! `__test_summary_tick_now` seam — the seam fires the same
    //! snapshot-and-emit path the timer would, synchronously. The
    //! timer-task spawn lifecycle itself (cancel on drop) is exercised
    //! at integration scope: every `SamplingSink` constructed in any
    //! slice test exercises spawn and drop.
    //!
    //! Test budget:
    //! - `Counters`: 3 distinct behaviours (record_kept_error_bearing,
    //!   record_kept_sampled, record_dropped) plus snapshot_and_reset
    //!   = 4 behaviours × 2 = 8 unit tests max. We use 4 (one per
    //!   behaviour) — every behaviour is exercised through the public
    //!   recorder + snapshot pair.
    //! - `parse_summary_tick_ms_from_env`: 1 distinct behaviour (parse
    //!   the env var), exercised via parametrised test across the
    //!   four input shapes (unset / parseable / non-numeric / zero).

    use super::*;
    use serial_test::serial;

    #[test]
    fn counters_record_kept_error_bearing_increments_kept_total_and_kept_error_bearing() {
        let counters = Counters::new();
        counters.record_kept_error_bearing();
        let (kept, kept_err, dropped) = counters.snapshot_and_reset();
        assert_eq!(kept, 1, "record_kept_error_bearing must bump kept_total");
        assert_eq!(
            kept_err, 1,
            "record_kept_error_bearing must bump kept_error_bearing"
        );
        assert_eq!(
            dropped, 0,
            "record_kept_error_bearing must NOT bump dropped"
        );
    }

    #[test]
    fn counters_record_kept_sampled_increments_kept_total_only() {
        let counters = Counters::new();
        counters.record_kept_sampled();
        let (kept, kept_err, dropped) = counters.snapshot_and_reset();
        assert_eq!(kept, 1, "record_kept_sampled must bump kept_total");
        assert_eq!(
            kept_err, 0,
            "record_kept_sampled must NOT bump kept_error_bearing"
        );
        assert_eq!(dropped, 0, "record_kept_sampled must NOT bump dropped");
    }

    #[test]
    fn counters_record_dropped_increments_dropped_only() {
        let counters = Counters::new();
        counters.record_dropped();
        let (kept, kept_err, dropped) = counters.snapshot_and_reset();
        assert_eq!(kept, 0, "record_dropped must NOT bump kept_total");
        assert_eq!(
            kept_err, 0,
            "record_dropped must NOT bump kept_error_bearing"
        );
        assert_eq!(dropped, 1, "record_dropped must bump dropped");
    }

    #[test]
    fn counters_snapshot_and_reset_returns_zeros_after_a_prior_snapshot() {
        // The contract is: after `snapshot_and_reset`, the next
        // `snapshot_and_reset` returns zeros for every counter that
        // has not been recorded against in the meantime. This pins
        // the `swap(0, Relaxed)` semantics: a mutation to `load` (no
        // reset) would leak the previous window's totals into the
        // next window's snapshot.
        let counters = Counters::new();
        counters.record_kept_error_bearing();
        counters.record_kept_sampled();
        counters.record_dropped();
        let _first = counters.snapshot_and_reset();
        let (kept, kept_err, dropped) = counters.snapshot_and_reset();
        assert_eq!(kept, 0, "second snapshot's kept_total must be zero");
        assert_eq!(
            kept_err, 0,
            "second snapshot's kept_error_bearing must be zero"
        );
        assert_eq!(dropped, 0, "second snapshot's dropped must be zero");
    }

    #[tokio::test]
    async fn summary_task_drop_cancels_the_cancellation_token() {
        // Pin the Drop impl: dropping a `SummaryTask` must cancel the
        // shared `CancellationToken` so the spawned task's
        // `select!` arm exits at its next await point. A mutation
        // that replaces `Drop::drop` with `()` would leave the token
        // un-cancelled and the task would run forever (or until the
        // runtime shuts down).
        //
        // We clone the cancellation token before drop so the test
        // retains a handle to it; after drop we assert the token is
        // cancelled.
        let counters = Arc::new(Counters::new());
        let task = SummaryTask::spawn(counters, 0.5, 60_000);
        let token_handle = task.cancel.clone();
        assert!(
            !token_handle.is_cancelled(),
            "freshly-spawned task's token must not be cancelled"
        );
        drop(task);
        assert!(
            token_handle.is_cancelled(),
            "after Drop, the cancellation token must be cancelled"
        );
    }

    #[test]
    fn counters_snapshot_and_reset_aggregates_multiple_records() {
        // Pins the aggregation across a mix of recorder calls. Three
        // error-bearing keeps + two sampled keeps + four drops = the
        // canonical "mixed window" shape the periodic summary emits.
        let counters = Counters::new();
        for _ in 0..3 {
            counters.record_kept_error_bearing();
        }
        for _ in 0..2 {
            counters.record_kept_sampled();
        }
        for _ in 0..4 {
            counters.record_dropped();
        }
        let (kept, kept_err, dropped) = counters.snapshot_and_reset();
        assert_eq!(kept, 5, "kept_total = 3 error-bearing + 2 sampled = 5");
        assert_eq!(kept_err, 3, "kept_error_bearing = 3");
        assert_eq!(dropped, 4, "dropped = 4");
    }

    #[test]
    #[serial]
    fn parse_summary_tick_ms_classifies_each_env_var_shape() {
        // Parametrised across the four shapes: unset, parseable,
        // non-numeric, and zero. Each shape pins one behavioural
        // branch of the parser. The `#[serial]` attribute serialises
        // against other tests in this module (and across the crate)
        // that touch `SIEVE_SUMMARY_TICK_MS`.
        //
        // The unset case is exercised by clearing the var; the
        // parseable case by setting "100"; the non-numeric case by
        // setting "abc"; the zero case by setting "0".

        // Snapshot the previous value so we can restore it at the end
        // of the test (in case a CI runner sets a non-default value).
        let previous = std::env::var(SIEVE_SUMMARY_TICK_MS_ENV).ok();

        // Case 1: unset → default.
        std::env::remove_var(SIEVE_SUMMARY_TICK_MS_ENV);
        let parsed = parse_summary_tick_ms_from_env().expect("unset env var must default");
        assert_eq!(
            parsed, DEFAULT_SUMMARY_TICK_MS,
            "unset env var must yield the default 60_000 ms"
        );

        // Case 2: parseable positive integer → that value.
        std::env::set_var(SIEVE_SUMMARY_TICK_MS_ENV, "100");
        let parsed = parse_summary_tick_ms_from_env().expect("100 must parse");
        assert_eq!(parsed, 100, "set to 100 must parse to 100");

        // Case 3: non-numeric → SummaryTickUnparseable.
        std::env::set_var(SIEVE_SUMMARY_TICK_MS_ENV, "abc");
        let err = parse_summary_tick_ms_from_env().expect_err("abc must fail to parse");
        assert!(
            matches!(err, SieveConfigError::SummaryTickUnparseable { .. }),
            "non-numeric env value must yield SummaryTickUnparseable; got {err:?}"
        );

        // Case 4: zero → SummaryTickUnparseable (per ADR-0020 §4:
        // `tokio::time::interval(Duration::ZERO)` panics; the parser
        // rejects zero before the timer constructor sees it).
        std::env::set_var(SIEVE_SUMMARY_TICK_MS_ENV, "0");
        let err = parse_summary_tick_ms_from_env().expect_err("0 must be rejected");
        assert!(
            matches!(err, SieveConfigError::SummaryTickUnparseable { .. }),
            "zero env value must yield SummaryTickUnparseable; got {err:?}"
        );

        // Restore the previous value so the parent test process is
        // unaffected after this binary exits.
        match previous {
            Some(v) => std::env::set_var(SIEVE_SUMMARY_TICK_MS_ENV, v),
            None => std::env::remove_var(SIEVE_SUMMARY_TICK_MS_ENV),
        }
    }
}
