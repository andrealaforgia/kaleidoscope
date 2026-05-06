//! Slice 01 — Walking Skeleton.
//!
//! Maps to US-SI-01 ("Walking skeleton: error trace kept, non-error
//! trace dropped"). Two thin assertions that prove Sieve exists as
//! a code artefact and the keep / drop decision is observable as a
//! typed Rust enum:
//!
//! - An error-bearing fixture trace yields `Decision::Keep` at rate
//!   0.0 (the error-bias rule kicks in).
//! - An all-OK fixture trace yields `Decision::Drop` at rate 0.0
//!   (the rate-based rule rejects every non-error trace).
//!
//! ## DISTILL state
//!
//! `HeadSampler::sample` panics with `unimplemented!()`. Both tests
//! panic. DELIVER slice 01 turns them GREEN by landing the
//! `is_error_bearing` check and the `xxh3_64`-keyed rate decision.
//!
//! Per `tests/common/mod.rs` Strategy C: the sampler is exercised
//! through Sieve's public surface only (`HeadSampler::new`,
//! `HeadSampler::sample`, `Decision::Keep` / `Decision::Drop`).
//! The fixture views are built via the `__test_trace_view` doc-hidden
//! seam — explicitly part of the public surface contract per
//! ADR-0018 §"Test seams".

mod common;

use common::{fixture_error_trace, fixture_ok_trace, fixture_trace_id};
use sieve::{Decision, HeadSampler, Sampler};

// =========================================================================
// Walking-skeleton scenarios — one keep, one drop.
//
// Per `journey-sieve.yaml > scenarios > walking_skeleton`:
//   given: HeadSampler at rate 0.0
//   when: an error trace and a non-error trace are submitted
//   then: the error trace is kept; the non-error trace is dropped.
// =========================================================================

/// US-SI-01 Scenario: An error-bearing trace is always kept.
///
/// Given: a HeadSampler constructed with non-error rate 0.0
/// And:   a fixture trace whose root span carries status.code = ERROR
/// When:  the sampler is asked for a decision on that trace
/// Then:  the decision is Decision::Keep
#[test]
fn an_error_bearing_trace_is_kept_at_rate_zero() {
    let sampler =
        HeadSampler::new(0.0).expect("rate 0.0 is in [0.0, 1.0] and constructs successfully");

    let trace_id = fixture_trace_id(1);
    let spans = fixture_error_trace(trace_id);
    let view = sieve::__test_trace_view(trace_id, &spans);

    let decision = sampler.sample(&view);

    assert_eq!(
        decision,
        Decision::Keep,
        "an error-bearing trace must be kept at rate 0.0 (error-bias retention)"
    );
}

/// US-SI-01 Scenario: A non-error trace is dropped at rate 0.0.
///
/// Given: a HeadSampler constructed with non-error rate 0.0
/// And:   a fixture trace whose spans all carry status.code = OK
/// When:  the sampler is asked for a decision on that trace
/// Then:  the decision is Decision::Drop
#[test]
fn an_all_ok_trace_is_dropped_at_rate_zero() {
    let sampler =
        HeadSampler::new(0.0).expect("rate 0.0 is in [0.0, 1.0] and constructs successfully");

    let trace_id = fixture_trace_id(2);
    let spans = fixture_ok_trace(trace_id);
    let view = sieve::__test_trace_view(trace_id, &spans);

    let decision = sampler.sample(&view);

    assert_eq!(
        decision,
        Decision::Drop,
        "a non-error trace must be dropped at rate 0.0 (no error-bias, rate-based rejection)"
    );
}

/// Reflective sanity check: the configured rate is observable on
/// the constructed sampler. Used by the slice-06 INFO summary
/// which carries the rate so operators can confirm the configured
/// value without reading config (per ADR-0020 §5).
#[test]
fn head_sampler_exposes_its_configured_rate() {
    let sampler = HeadSampler::new(0.25).expect("rate 0.25 in range");
    assert!(
        (sampler.rate() - 0.25).abs() < f64::EPSILON,
        "HeadSampler must expose the rate it was constructed with"
    );
}
