//! Slice 02 — Error-bias retention: errors always survive sampling.
//!
//! Maps to US-SI-02. Three scenarios:
//!
//! - Parameterised test: an error-bearing trace is kept at rates
//!   0.0, 0.1, 0.5, 1.0 (the error-bias rule overrides the
//!   rate-based decision).
//! - A multi-span trace with one error span is kept (the
//!   `is_error_bearing` check reaches every span, not just the
//!   root).
//! - An all-OK trace is NOT forced to Keep by the error-bias rule
//!   — it falls through to the rate-based decision (and at rate 0.0
//!   is dropped).
//!
//! ## DISTILL state
//!
//! `HeadSampler::sample` panics with `unimplemented!()`. Every
//! test below panics until DELIVER slice 02 lands the
//! `is_error_bearing` check.

mod common;

use common::{
    fixture_error_trace, fixture_multi_span_one_error, fixture_ok_trace, fixture_trace_id,
};
use sieve::{Decision, HeadSampler, Sampler};

// =========================================================================
// Parameterised: error-bearing trace kept at every rate.
//
// US-SI-02 Scenario: An error trace is kept at every non-zero rate.
//
// Given: a HeadSampler constructed with non-error rate <r>,
//        where <r> ∈ {0.0, 0.1, 0.5, 1.0}
// And:   a fixture trace containing at least one span with
//        status.code = ERROR
// When:  the sampler is asked for a decision
// Then:  the decision is Decision::Keep
//
// Implemented as four tests rather than a parameterised body so
// each rate's failure is reported independently and the runner shows
// which rate broke if a regression lands.
// =========================================================================

#[test]
fn error_bearing_trace_kept_at_rate_0_0() {
    assert_error_kept_at(0.0);
}

#[test]
fn error_bearing_trace_kept_at_rate_0_1() {
    assert_error_kept_at(0.1);
}

#[test]
fn error_bearing_trace_kept_at_rate_0_5() {
    assert_error_kept_at(0.5);
}

#[test]
fn error_bearing_trace_kept_at_rate_1_0() {
    assert_error_kept_at(1.0);
}

fn assert_error_kept_at(rate: f64) {
    let sampler = HeadSampler::new(rate).expect("rate in [0.0, 1.0]");
    let trace_id = fixture_trace_id(100);
    let spans = fixture_error_trace(trace_id);
    let view = sieve::__test_trace_view(trace_id, &spans);

    let decision = sampler.sample(&view);

    assert_eq!(
        decision,
        Decision::Keep,
        "error-bearing trace must be kept at rate {rate}"
    );
}

// =========================================================================
// Multi-span trace, one error span.
//
// US-SI-02 Scenario: A multi-span trace with one error span is kept.
//
// Given: a HeadSampler with non-error rate 0.0
// And:   a fixture trace with 12 spans, one of which has
//        status.code = ERROR
// When:  the sampler is asked for a decision
// Then:  the decision is Decision::Keep
// =========================================================================

#[test]
fn multi_span_trace_with_single_error_span_is_kept() {
    let sampler = HeadSampler::new(0.0).expect("rate 0.0 in range");
    let trace_id = fixture_trace_id(200);
    let spans = fixture_multi_span_one_error(trace_id, 12);
    let view = sieve::__test_trace_view(trace_id, &spans);

    let decision = sampler.sample(&view);

    assert_eq!(
        decision,
        Decision::Keep,
        "a 12-span trace with one error span must be kept; the error-bias check must scan every span"
    );
}

// =========================================================================
// All-OK trace at rate 0.0 — error-bias rule does NOT fire; the
// rate-based rule rejects.
//
// US-SI-02 Scenario: A trace with no error spans is not retained by
// the error-bias rule.
//
// Given: a HeadSampler with non-error rate 0.0
// And:   a fixture trace with 5 spans, all of which have
//        status.code = OK
// When:  the sampler is asked for a decision
// Then:  the decision is NOT forced to Keep by the error-bias rule
// And:   the rate-based decision applies (and at rate 0.0 is Drop)
// =========================================================================

#[test]
fn all_ok_trace_at_rate_zero_is_dropped_by_rate_rule() {
    let sampler = HeadSampler::new(0.0).expect("rate 0.0 in range");
    let trace_id = fixture_trace_id(201);
    // Five OK spans (US-SI-02 example 3).
    let mut spans = Vec::new();
    for _ in 0..5 {
        spans.push(common::fixture_span_with_status(
            trace_id,
            opentelemetry_proto::tonic::trace::v1::status::StatusCode::Ok,
        ));
    }
    let _ = fixture_ok_trace(trace_id); // keep import alive
    let view = sieve::__test_trace_view(trace_id, &spans);

    let decision = sampler.sample(&view);

    assert_eq!(
        decision,
        Decision::Drop,
        "an all-OK trace at rate 0.0 must be dropped by the rate rule (no error-bias retention)"
    );
}

// =========================================================================
// Configuration error path: out-of-range rate is rejected.
//
// US-SI-06 brief mentions "out-of-range or unparseable value is
// rejected with a clear error". Slice 02 covers the rate-side; the
// summary tick parser is covered in slice 06.
// =========================================================================

#[test]
fn head_sampler_new_rejects_negative_rate() {
    let err = HeadSampler::new(-0.1).expect_err("negative rate must be rejected");
    match err {
        sieve::SieveConfigError::RateOutOfRange { got } => {
            assert!(
                (got - (-0.1)).abs() < f64::EPSILON,
                "RateOutOfRange must carry the offending value verbatim"
            );
        }
        other => panic!("expected RateOutOfRange, got {other:?}"),
    }
}

#[test]
fn head_sampler_new_rejects_rate_above_one() {
    let err = HeadSampler::new(1.5).expect_err("rate > 1.0 must be rejected");
    match err {
        sieve::SieveConfigError::RateOutOfRange { got } => {
            assert!(
                (got - 1.5).abs() < f64::EPSILON,
                "RateOutOfRange must carry the offending value verbatim"
            );
        }
        other => panic!("expected RateOutOfRange, got {other:?}"),
    }
}

#[test]
fn head_sampler_new_rejects_nan_rate() {
    let err = HeadSampler::new(f64::NAN).expect_err("NaN rate must be rejected");
    assert!(matches!(
        err,
        sieve::SieveConfigError::RateOutOfRange { .. }
    ));
}
