//! Invariant — public API smoke test.
//!
//! Per ADR-0018 §"Internal layout": one binary that asserts the
//! public-surface items compile and the expected constructors
//! return `Ok` for nominal inputs. A regression that removes a
//! public item or changes a signature in a non-additive way fails
//! this binary.
//!
//! This is the **subtype-check layer** of the Earned-Trust
//! three-layer pattern (per ADR-0018 §"Self-Application of
//! Earned Trust"): the compiler enforces that the public surface
//! has the expected items and the expected shapes.
//!
//! ## DISTILL state
//!
//! Constructors are real (`HeadSampler::new`, `HeadSampler::from_env`,
//! `__test_trace_view`); `sample`, `accept`, `probe`,
//! `__test_summary_tick_now` panic with `unimplemented!()`. The
//! tests below avoid the panicking methods — they assert the public
//! surface is type-correct, not that it produces the right value.

use sieve::{
    Decision, HeadSampler, KeepReason, Sampler, SamplingSink, SieveConfigError, TraceView,
    __test_trace_view,
};

// =========================================================================
// Decision and KeepReason variants compile.
// =========================================================================

#[test]
fn decision_keep_and_drop_are_distinct_values() {
    let keep = Decision::Keep;
    let drop = Decision::Drop;
    assert_ne!(keep, drop, "Keep and Drop must be distinct");
}

#[test]
fn keep_reason_error_bearing_and_sampled_are_distinct_values() {
    let eb = KeepReason::ErrorBearing;
    let sampled = KeepReason::Sampled;
    assert_ne!(eb, sampled, "ErrorBearing and Sampled must be distinct");
}

// =========================================================================
// HeadSampler constructors compile and behave for nominal inputs.
// =========================================================================

#[test]
fn head_sampler_new_accepts_zero_one_and_half() {
    let _ = HeadSampler::new(0.0).expect("rate 0.0 in range");
    let _ = HeadSampler::new(1.0).expect("rate 1.0 in range");
    let _ = HeadSampler::new(0.5).expect("rate 0.5 in range");
}

#[test]
fn head_sampler_new_returns_rate_out_of_range_for_two_point_zero() {
    let err = HeadSampler::new(2.0).expect_err("rate > 1.0 must be rejected");
    assert!(matches!(err, SieveConfigError::RateOutOfRange { .. }));
}

// =========================================================================
// TraceView and the test seam compile.
// =========================================================================

#[test]
fn trace_view_exposes_trace_id_and_spans() {
    let trace_id = [7u8; 16];
    let spans: Vec<opentelemetry_proto::tonic::trace::v1::Span> = Vec::new();
    let view: TraceView<'_> = __test_trace_view(trace_id, &spans);
    assert_eq!(view.trace_id(), trace_id, "trace_id must round-trip");
    assert_eq!(
        view.spans().count(),
        0,
        "empty fixture spans iterate as zero"
    );
}

// =========================================================================
// Public-surface shape: SamplingSink<S, N> is a public type with
// generic parameters bound by `OtlpSink + Probe` and `Sampler`.
// Compile-time only — the test body is empty.
// =========================================================================

#[test]
fn sampling_sink_is_publicly_named_with_otlp_sink_probe_and_sampler_bounds() {
    fn _assert<S, N>()
    where
        S: aperture::ports::OtlpSink + aperture::ports::Probe,
        N: Sampler,
        SamplingSink<S, N>: Sized,
    {
    }
    _assert::<aperture::testing::RecordingSink, HeadSampler>();
}
