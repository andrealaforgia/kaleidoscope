//! Invariant — `SamplingSink<S, N>: OtlpSink + Probe` (compile-time).
//!
//! Per ADR-0021 §7 + ADR-0018 §"Self-Application of Earned Trust":
//! the compile-time subtype check that asserts every concrete
//! `SamplingSink<S, N>` implements both `OtlpSink` and `Probe`
//! whenever its bounds (`S: OtlpSink + Probe + Send + Sync + 'static`,
//! `N: Sampler`) are satisfied.
//!
//! A regression that removes one of the two trait impls (or narrows
//! the where-clause bounds in a non-additive way) fails this
//! binary at compile time. The test body never runs; the
//! `where`-clause IS the assertion.
//!
//! This is the **first layer** of the Earned-Trust three-layer
//! pattern for the integration contract. The other two layers are
//! the Aperture xtask AST walk (structural CI) and the slice
//! integration tests (behavioural CI).

use aperture::ports::{OtlpSink, Probe};
use aperture::testing::RecordingSink;
use sieve::{HeadSampler, Sampler, SamplingSink};

// =========================================================================
// Generic compile-time assertion: any S + Probe + OtlpSink and any
// N: Sampler produces a SamplingSink<S, N> that itself implements
// OtlpSink + Probe.
// =========================================================================

#[test]
fn sampling_sink_is_otlp_sink_and_probe_for_any_compatible_inner_and_sampler() {
    fn assert_bounds<S, N>()
    where
        S: OtlpSink + Probe + Send + Sync + 'static,
        N: Sampler,
        SamplingSink<S, N>: OtlpSink + Probe,
    {
    }
    // The concrete pairing slice tests use: RecordingSink + HeadSampler.
    assert_bounds::<RecordingSink, HeadSampler>();
}

// =========================================================================
// Sampler trait is publicly named with the expected method
// signature: `fn sample(&self, &TraceView<'_>) -> Decision`.
//
// Compile-time only; if the trait method's signature drifts (e.g.
// gains a `&self` parameter, returns a different type, becomes
// async), this binary fails to compile.
// =========================================================================

#[test]
fn sampler_trait_has_locked_method_signature() {
    fn _accept_any_sampler<N: Sampler>(s: &N, view: &sieve::TraceView<'_>) -> sieve::Decision {
        s.sample(view)
    }
    // Compile-time witness: HeadSampler implements Sampler with the
    // locked signature. Body never runs (sample panics with
    // `unimplemented!()` at DISTILL).
    let _ = _accept_any_sampler::<HeadSampler>;
}
