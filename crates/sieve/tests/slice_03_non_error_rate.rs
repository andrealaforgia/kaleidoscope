//! Slice 03 — Non-error rate honoured statistically.
//!
//! Maps to US-SI-03. Three scenarios over a deterministic 10 000
//! distinct-trace_id fixture:
//!
//! - At rate 0.0, at most 2 non-error traces are kept.
//! - At rate 1.0, at least 9998 non-error traces are kept.
//! - At rate 0.5, between 4700 and 5300 non-error traces are kept
//!   (±3% band; locked at DISCUSS time per US-SI-03).
//!
//! ## Determinism
//!
//! Per ADR-0018 §"`HeadSampler::sample` mechanism": the rate-based
//! decision is `xxh3_64(trace_id) / u64::MAX < rate`. The fixture
//! uses sequential 64-bit seeds (0..10_000) wrapped into 16-byte
//! trace_ids by [`fixture_trace_id`]. The sequential keys are
//! distributed uniformly by `xxh3_64`, so the kept count at rate
//! 0.5 lands in the ±3% band on every run.
//!
//! The test is therefore **not flaky**: same fixture, same hash,
//! same kept count. The bands account for the discrete
//! distribution's natural variance, not for runtime randomness.
//!
//! ## DISTILL state
//!
//! `HeadSampler::sample` panics with `unimplemented!()`. Every test
//! below panics until DELIVER slice 03 lands the rate-based
//! decision body.

mod common;

use common::{fixture_ok_trace, fixture_trace_id};
use sieve::{Decision, HeadSampler, Sampler};

const FIXTURE_SIZE: u64 = 10_000;

// =========================================================================
// Helper: count kept decisions across the deterministic fixture.
// =========================================================================

fn count_kept_at_rate(rate: f64) -> u64 {
    let sampler = HeadSampler::new(rate).expect("rate in [0.0, 1.0]");
    let mut kept = 0u64;
    for seed in 0..FIXTURE_SIZE {
        let trace_id = fixture_trace_id(seed);
        let spans = fixture_ok_trace(trace_id);
        let view = sieve::__test_trace_view(trace_id, &spans);
        if sampler.sample(&view) == Decision::Keep {
            kept += 1;
        }
    }
    kept
}

// =========================================================================
// US-SI-03 Scenario: At rate 0.0 essentially no non-error traces are kept.
//
// Given: a HeadSampler at rate 0.0
// And:   10000 distinct fixture trace IDs, all non-error
// When:  each is asked for a decision
// Then:  at most 2 are kept
// =========================================================================

#[test]
fn at_rate_zero_at_most_two_non_error_traces_are_kept() {
    let kept = count_kept_at_rate(0.0);
    assert!(
        kept <= 2,
        "at rate 0.0, at most 2 non-error traces may be kept; got {kept}"
    );
}

// =========================================================================
// US-SI-03 Scenario: At rate 1.0 essentially all non-error traces are kept.
//
// Given: a HeadSampler at rate 1.0
// And:   10000 distinct fixture trace IDs, all non-error
// When:  each is asked for a decision
// Then:  at least 9998 are kept
// =========================================================================

#[test]
fn at_rate_one_at_least_nine_thousand_nine_hundred_ninety_eight_are_kept() {
    let kept = count_kept_at_rate(1.0);
    assert!(
        kept >= 9998,
        "at rate 1.0, at least 9998 non-error traces must be kept; got {kept}"
    );
}

// =========================================================================
// US-SI-03 Scenario: At rate 0.5 about half the non-error traces
// are kept.
//
// Given: a HeadSampler at rate 0.5
// And:   10000 distinct fixture trace IDs, all non-error
// When:  each is asked for a decision
// Then:  between 4700 and 5300 are kept
// =========================================================================

#[test]
fn at_rate_half_kept_count_lies_in_three_percent_band_around_half() {
    let kept = count_kept_at_rate(0.5);
    assert!(
        (4700..=5300).contains(&kept),
        "at rate 0.5, kept count must lie in [4700, 5300] (±3% of 5000); got {kept}"
    );
}
