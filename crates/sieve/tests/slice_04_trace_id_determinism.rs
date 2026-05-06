//! Slice 04 — Trace coherence: same trace_id always yields the
//! same decision.
//!
//! Maps to US-SI-04. Two scenarios:
//!
//! - Same trace_id queried twice yields identical decisions.
//! - Same trace_id queried 100 times never flips the decision —
//!   100 calls produce exactly one outcome (variance zero).
//!
//! ## Why this is a separate slice
//!
//! Slice 03's hash-keyed mechanism is deterministic by
//! construction. This slice promotes the property to an explicit
//! CI invariant: cross-batch trace coherence is the load-bearing
//! property that keeps downstream UIs (Prism / Tempo / Jaeger)
//! showing whole traces. A regression that introduces randomness
//! anywhere in the sampler's decision path would land here first.
//!
//! ## DISTILL state
//!
//! `HeadSampler::sample` panics with `unimplemented!()`. Both tests
//! below panic until DELIVER slice 04 lands (the slice 03 body
//! delivers the property; slice 04 promotes it to invariant).

mod common;

use common::{fixture_ok_trace, fixture_trace_id};
use sieve::{Decision, HeadSampler, Sampler};

// =========================================================================
// US-SI-04 Scenario: Same trace_id queried twice yields identical
// decisions.
//
// Given: a HeadSampler at rate 0.5
// And:   a non-error fixture trace with a specific trace_id
// When:  the sampler is asked for a decision twice
// Then:  both decisions are equal
// =========================================================================

#[test]
fn same_trace_id_queried_twice_yields_equal_decisions() {
    let sampler = HeadSampler::new(0.5).expect("rate 0.5 in range");
    let trace_id = fixture_trace_id(42);
    let spans = fixture_ok_trace(trace_id);
    let view_a = sieve::__test_trace_view(trace_id, &spans);
    let view_b = sieve::__test_trace_view(trace_id, &spans);

    let decision_a = sampler.sample(&view_a);
    let decision_b = sampler.sample(&view_b);

    assert_eq!(
        decision_a, decision_b,
        "same trace_id must yield the same decision under the same sampler"
    );
}

// =========================================================================
// US-SI-04 Scenario: Same trace_id across 100 queries never flips
// decision.
//
// Given: a HeadSampler at rate 0.5
// And:   a non-error fixture trace with a specific trace_id
// When:  the sampler is asked for a decision 100 times
// Then:  the decision count for one outcome is exactly 100 and
//        for the other is exactly 0
// =========================================================================

#[test]
fn same_trace_id_across_one_hundred_queries_never_flips_decision() {
    let sampler = HeadSampler::new(0.5).expect("rate 0.5 in range");
    let trace_id = fixture_trace_id(42);
    let spans = fixture_ok_trace(trace_id);

    let mut kept = 0u32;
    let mut dropped = 0u32;
    for _ in 0..100 {
        let view = sieve::__test_trace_view(trace_id, &spans);
        match sampler.sample(&view) {
            Decision::Keep => kept += 1,
            Decision::Drop => dropped += 1,
        }
    }

    assert!(
        (kept == 100 && dropped == 0) || (kept == 0 && dropped == 100),
        "the same trace_id must produce the same decision on every call; got {kept} kept and {dropped} dropped"
    );
}

// =========================================================================
// Cross-rate: the same trace_id may flip across DIFFERENT samplers
// (different rates). This is NOT a determinism violation — it is the
// expected behaviour when the rate is the variable. Asserts the
// determinism is per-(trace_id, rate) pair, not per-trace_id alone.
//
// At rate 0.0 the sampler always returns Drop for non-error traces
// (per slice 01); at rate 1.0 it always returns Keep. So the same
// trace_id legitimately produces different decisions across these
// two rates. This test pins the contract that determinism is
// referential transparency, not a per-trace_id sticky outcome.
// =========================================================================

#[test]
fn same_trace_id_under_different_rates_may_yield_different_decisions() {
    let trace_id = fixture_trace_id(99);
    let spans = fixture_ok_trace(trace_id);

    let s_zero = HeadSampler::new(0.0).expect("rate 0.0 in range");
    let s_one = HeadSampler::new(1.0).expect("rate 1.0 in range");
    let view_zero = sieve::__test_trace_view(trace_id, &spans);
    let view_one = sieve::__test_trace_view(trace_id, &spans);

    let d_zero = s_zero.sample(&view_zero);
    let d_one = s_one.sample(&view_one);

    assert_eq!(
        d_zero,
        Decision::Drop,
        "non-error trace at rate 0.0 must be dropped"
    );
    assert_eq!(
        d_one,
        Decision::Keep,
        "non-error trace at rate 1.0 must be kept"
    );
}
