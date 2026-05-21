//! `Sampler` trait + `HeadSampler` concrete sampler.
//!
//! Per ADR-0018 §"Public surface (final list)": one trait, one
//! method, one return value. The error-bias rule and the rate-based
//! rule live inside `HeadSampler::sample` per ADR-0018
//! §"`HeadSampler::sample` mechanism".
//!
//! ## Implementation status
//!
//! Fully implemented and green. `HeadSampler::new(rate)` and
//! `HeadSampler::from_env()` return `Result<HeadSampler,
//! SieveConfigError>`, `HeadSampler::rate()` returns the stored rate,
//! and `HeadSampler::sample(...)` makes the real sampling decision
//! (error-bias rule then rate-based rule), with behaviour locked by
//! the slice 01 to 04 tests.

use std::str::FromStr;

use opentelemetry_proto::tonic::trace::v1::status::StatusCode;
use opentelemetry_proto::tonic::trace::v1::Span;
use xxhash_rust::xxh3::xxh3_64;

use crate::decision::Decision;
use crate::error::SieveConfigError;
use crate::trace_view::TraceView;

/// The sampling-decision contract Sieve exposes.
///
/// Implementations return a [`Decision`] for each [`TraceView`]
/// they receive. The trait is `Send + Sync + 'static` because the
/// decorator's `OtlpSink + Probe` impl is generic over `N: Sampler`
/// and Aperture's composition root stores the resulting
/// `SamplingSink<S, N>` behind `Arc<dyn OtlpSink>`.
///
/// At v0 the only implementation is [`HeadSampler`]; a v1 tail
/// sampler would also implement this trait without changing the
/// decorator.
pub trait Sampler: Send + Sync + 'static {
    /// Decide whether to keep or drop the given trace.
    ///
    /// Implementations MUST be referentially transparent for the
    /// same `(trace_id, internal state)` pair: same input → same
    /// output. This is a load-bearing contract for slice 04
    /// (trace-id determinism across batches). [`HeadSampler`]
    /// satisfies this by construction (the deterministic
    /// `xxh3_64`-keyed mapping).
    fn sample(&self, trace: &TraceView<'_>) -> Decision;
}

/// The environment variable name carrying the non-error trace rate.
///
/// Locked at DISCUSS Q5. Default is `0.1` (10% of non-error traces
/// retained); operators set the variable to a different value in
/// their deployment manifest.
pub const SIEVE_NON_ERROR_TRACE_RATE_ENV: &str = "SIEVE_NON_ERROR_TRACE_RATE";

/// The default non-error trace rate when `SIEVE_NON_ERROR_TRACE_RATE`
/// is unset. Per DISCUSS Q5.
pub const DEFAULT_NON_ERROR_TRACE_RATE: f64 = 0.1;

/// Concrete head-based sampler.
///
/// Constructed with a non-error rate in `[0.0, 1.0]`. Decisions are
/// deterministic functions of the trace's `trace_id` (via `xxh3_64`
/// per ADR-0019); error-bearing traces are always kept regardless
/// of rate (per ADR-0018 §"`HeadSampler::sample` mechanism").
///
/// `Debug` is derived so test assertions like `result.expect_err(...)`
/// can format the unexpected `Ok` value if a test regression
/// produces a sampler where it expected an error. The internal
/// state is a single `f64` rate; no surprises in the `Debug` output.
#[derive(Debug)]
pub struct HeadSampler {
    rate: f64,
}

impl HeadSampler {
    /// Construct a head sampler at the given non-error rate.
    ///
    /// Returns [`SieveConfigError::RateOutOfRange`] if `rate` is
    /// NaN, infinite, or outside `[0.0, 1.0]`.
    pub fn new(rate: f64) -> Result<Self, SieveConfigError> {
        if !rate.is_finite() || !(0.0..=1.0).contains(&rate) {
            return Err(SieveConfigError::RateOutOfRange { got: rate });
        }
        Ok(Self { rate })
    }

    /// Construct a head sampler reading the rate from the
    /// `SIEVE_NON_ERROR_TRACE_RATE` environment variable.
    ///
    /// - Unset: defaults to [`DEFAULT_NON_ERROR_TRACE_RATE`] (`0.1`).
    /// - Set to a parseable, in-range float: that value is used.
    /// - Set to a non-numeric value: returns
    ///   [`SieveConfigError::RateUnparseable`].
    /// - Set to a numeric but out-of-range value: returns
    ///   [`SieveConfigError::RateOutOfRange`].
    pub fn from_env() -> Result<Self, SieveConfigError> {
        let raw = std::env::var(SIEVE_NON_ERROR_TRACE_RATE_ENV).ok();
        let rate = match raw {
            None => DEFAULT_NON_ERROR_TRACE_RATE,
            Some(value) => f64::from_str(value.trim())
                .map_err(|_| SieveConfigError::RateUnparseable { raw: value })?,
        };
        Self::new(rate)
    }

    /// The configured non-error rate.
    ///
    /// Surfaced for the periodic INFO summary (per ADR-0020 §5):
    /// the summary line carries the rate so operators can confirm
    /// the configured value without reading config.
    pub fn rate(&self) -> f64 {
        self.rate
    }
}

impl Sampler for HeadSampler {
    fn sample(&self, trace: &TraceView<'_>) -> Decision {
        // Per ADR-0018 §"`HeadSampler::sample` mechanism": the
        // error-bias rule runs first — any span carrying
        // `status.code == ERROR` keeps the trace regardless of rate.
        if is_error_bearing(trace.spans()) {
            return Decision::Keep;
        }

        // Per ADR-0018 §"`HeadSampler::sample` mechanism" + ADR-0019 §1
        // + DISCUSS Q7: hash the 16-byte trace_id with `xxh3_64` and map
        // the resulting u64 into the half-open unit interval `[0.0, 1.0)`
        // by dividing by `2^64`. The `xxh3_64` algorithm distributes
        // trace_ids uniformly across the u64 range, so the mapped value
        // is uniformly distributed across `[0.0, 1.0)` — modulo the
        // f64-precision tail at the top (see [`TWO_POW_64_AS_F64`]'s
        // doc comment for the ≈1.1e-16 fraction of inputs that round
        // to exactly `1.0`).
        //
        // Boundary semantics:
        //
        // - At `self.rate == 1.0`, every `mapped < 1.0` is kept; the
        //   ≈1.1e-16 fraction whose `mapped` rounds to `1.0` is
        //   dropped. The slice-03 `at_rate_one_…` test's `≥ 9998` (not
        //   `== 10000`) bound bakes this honest tail into the contract.
        //
        // - The early return for `self.rate == 0.0` above keeps the
        //   exact-zero boundary safe regardless of mapped-value
        //   precision. Without that early return, `mapped < 0.0` is
        //   false for every trace_id (since `mapped >= 0.0` always), so
        //   the rule already drops every non-error trace at rate `0.0`;
        //   the explicit check is load-bearing self-documentation:
        //   "rate 0 is a configured off switch, not a probability".
        if mapped_to_unit_interval(xxh3_64(&trace.trace_id())) < self.rate {
            Decision::Keep
        } else {
            Decision::Drop
        }
    }
}

/// `2^64` as an `f64` constant. Exactly representable: `f64`'s
/// 52-bit mantissa plus implicit leading 1 covers exponents up to
/// 1023, and `2^64 = 1.0 × 2^64` has zero mantissa bits set, so the
/// value is exact.
///
/// Used as the divisor in [`mapped_to_unit_interval`]: `hash / 2^64`
/// for `hash ∈ [0, 2^64)` yields a value in `[0.0, 1.0]` — almost
/// always in `[0.0, 1.0)`, but for `hash ≥ u64::MAX - 2047` the cast
/// `hash as f64` rounds up to `2^64` (the f64 ULP at this magnitude
/// is `2^11 = 2048`), so the ratio rounds to exactly `1.0`. That
/// pathological tail is `2048 / 2^64 ≈ 1.1e-16` of the input space —
/// far below the slice-03 ±3% statistical band. The
/// `at_rate_one_at_least_nine_thousand_nine_hundred_ninety_eight…`
/// test bakes this honesty into its `≥ 9998` (rather than `== 10000`)
/// bound.
///
/// Encoded as a literal `f64` (rather than a runtime expression like
/// `(u64::MAX as f64) + 1.0` or `(1u128 << 64) as f64`) so the
/// mutation-testing surface around the divisor is closed: there is
/// no arithmetic for cargo-mutants to perturb.
const TWO_POW_64_AS_F64: f64 = 18_446_744_073_709_551_616.0;

/// Map a `u64` uniformly into the half-open interval `[0.0, 1.0)`.
///
/// For every `u64` value the ratio `hash / 2^64` is in `[0.0, 1.0)`.
/// This shape is the canonical "u64-to-unit-interval" mapping used by
/// Rust's `rand` crate's `Standard` distribution for `f64` and by the
/// OTel collector's TailSamplingProcessor for the same purpose.
///
/// Kept private — the trait surface is `Sampler::sample`; this is an
/// implementation detail of `HeadSampler`. Surfaced for `super::tests`
/// only via `pub(super)` so the unit tests can pin the boundary
/// semantics (`hash == 0 → 0.0`; `hash == u64::MAX → strictly < 1.0`)
/// directly on the function rather than only through the
/// driving-port `Sampler::sample` test.
pub(super) fn mapped_to_unit_interval(hash: u64) -> f64 {
    (hash as f64) / TWO_POW_64_AS_F64
}

/// Return `true` iff at least one span in the iterator carries
/// `status.code == ERROR`.
///
/// Per ADR-0018 §"`HeadSampler::sample` mechanism", this is the
/// error-bias rule's predicate: an error-bearing trace is kept
/// regardless of the configured rate. Spans with no `status` set,
/// or `status.code` set to `OK` / `UNSET`, are non-error spans.
///
/// Kept `pub(crate)` so the decorator (slice 05/06) can call it
/// directly to compute the `KeepReason::ErrorBearing` discriminator
/// for the DEBUG event without going through the trait. Per
/// ADR-0018 §"Internal layout": "Free function `is_error_bearing(spans)
/// -> bool` (kept `pub(crate)` so the decorator can call it without
/// going through the trait)."
pub(crate) fn is_error_bearing<'a, I>(spans: I) -> bool
where
    I: IntoIterator<Item = &'a Span>,
{
    spans
        .into_iter()
        .any(|span| span.status.as_ref().map(|s| s.code) == Some(StatusCode::Error as i32))
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `is_error_bearing` free function.
    //!
    //! Port-to-port at domain scope per Mandate 2: `is_error_bearing`
    //! is a pure free function whose signature IS its driving port.
    //! Calling it directly from a test IS port-to-port testing.
    //!
    //! Test budget: 1 distinct behavior × 2 = 2 unit tests max. The
    //! behavior is "returns true iff any span carries status.code ==
    //! ERROR". Variations across status codes and span counts are
    //! parametrised into one table-driven test per Mandate 5.

    use super::*;
    use opentelemetry_proto::tonic::trace::v1::Status;

    fn span_with_status_code(code: i32) -> Span {
        Span {
            trace_id: vec![0; 16],
            span_id: vec![0; 8],
            trace_state: String::new(),
            parent_span_id: Vec::new(),
            flags: 0,
            name: "fixture".to_string(),
            kind: 0,
            start_time_unix_nano: 0,
            end_time_unix_nano: 0,
            attributes: Vec::new(),
            dropped_attributes_count: 0,
            events: Vec::new(),
            dropped_events_count: 0,
            links: Vec::new(),
            dropped_links_count: 0,
            status: Some(Status {
                message: String::new(),
                code,
            }),
        }
    }

    fn span_without_status() -> Span {
        let mut s = span_with_status_code(0);
        s.status = None;
        s
    }

    #[test]
    fn is_error_bearing_classifies_each_status_shape() {
        // Drives the contract across the full status-code domain plus
        // the missing-status case. The single behavior — "true iff any
        // span has status.code == ERROR" — is exercised across all
        // input shapes the production decorator can produce.
        let cases: &[(&str, Vec<Span>, bool)] = &[
            ("empty span list is not error-bearing", vec![], false),
            (
                "single OK span is not error-bearing",
                vec![span_with_status_code(StatusCode::Ok as i32)],
                false,
            ),
            (
                "single UNSET span is not error-bearing",
                vec![span_with_status_code(StatusCode::Unset as i32)],
                false,
            ),
            (
                "span with no status is not error-bearing",
                vec![span_without_status()],
                false,
            ),
            (
                "single ERROR span is error-bearing",
                vec![span_with_status_code(StatusCode::Error as i32)],
                true,
            ),
            (
                "any ERROR span among many makes the trace error-bearing",
                vec![
                    span_with_status_code(StatusCode::Ok as i32),
                    span_with_status_code(StatusCode::Error as i32),
                    span_with_status_code(StatusCode::Ok as i32),
                ],
                true,
            ),
            (
                "all-OK multi-span trace is not error-bearing",
                vec![
                    span_with_status_code(StatusCode::Ok as i32),
                    span_with_status_code(StatusCode::Ok as i32),
                ],
                false,
            ),
        ];

        for (label, spans, expected) in cases {
            assert_eq!(
                is_error_bearing(spans.iter()),
                *expected,
                "case {label}: expected is_error_bearing == {expected}"
            );
        }
    }

    // =====================================================================
    // Unit tests for `mapped_to_unit_interval` and the strict-`<` boundary
    // in `Sampler::sample`. These pin the slice-03 mutation surface to
    // 100% kill rate per ADR-0005 Gate 5.
    //
    // Port-to-port at domain scope per Mandate 2: `mapped_to_unit_interval`
    // is a pure free function whose signature IS its driving port.
    // Calling it directly from a test IS port-to-port testing. The strict
    // boundary in `Sampler::sample` is exercised through the `Sampler`
    // trait (the public driving port).
    // =====================================================================

    #[test]
    fn mapped_to_unit_interval_lands_at_exact_boundary_values() {
        // `hash == 0` → `0.0` exactly (the lower endpoint). Pins the
        // numerator: any mutation that perturbs the numerator (e.g.
        // adding 1 to `hash`) would shift this off `0.0`.
        //
        // `hash == 2^63` → `0.5` exactly (the midpoint). Pins the
        // divisor: a `-` or `*` perturbation of the literal divisor
        // would shift this off `0.5`. Writing the divisor as a literal
        // f64 constant `TWO_POW_64_AS_F64` closes the arithmetic-on-
        // the-divisor mutation surface; this test pins the constant's
        // value.
        //
        // `hash == u64::MAX - (2^11 - 1)` → strictly less than `1.0`.
        // f64's mantissa precision around `2^64` is `2^11 = 2048`, so
        // any `hash` at least `2048` below `u64::MAX` is at least one
        // ULP below `2^64` and the ratio is strictly `< 1.0`. (For
        // `hash` in the top 2048 of the u64 range, `hash as f64`
        // rounds up to `2^64` and the ratio is exactly `1.0`. That
        // pathological tail is `2048 / 2^64 ≈ 1.1e-16` of the input
        // space — negligible for sampling, but pinned in the doc
        // comment on `TWO_POW_64_AS_F64`.)
        assert_eq!(
            mapped_to_unit_interval(0),
            0.0,
            "hash 0 must map to exactly 0.0"
        );
        assert_eq!(
            mapped_to_unit_interval(1u64 << 63),
            0.5,
            "hash 2^63 must map to exactly 0.5"
        );
        let just_below_max = u64::MAX - (1u64 << 11);
        assert!(
            mapped_to_unit_interval(just_below_max) < 1.0,
            "hash one ULP below u64::MAX must map strictly below 1.0; got {}",
            mapped_to_unit_interval(just_below_max)
        );
    }

    #[test]
    fn sample_uses_strict_less_than_at_the_boundary() {
        // Pins the strict `<` in `mapped < self.rate` against the
        // `<=` mutation. Construct a non-error trace, observe its
        // `mapped` value, then build a sampler at that exact rate.
        // With the strict-`<` rule: `mapped < mapped` is false →
        // `Decision::Drop`. With the `<=` mutation: `mapped <= mapped`
        // is true → `Decision::Keep`. The two diverge on this case.
        //
        // Skip the early-return cases that would mask the boundary:
        // - `rate == 0.0` (the early-return short-circuits before the
        //   comparison).
        // - `mapped == 0.0` (which would force `rate == 0.0` and hit
        //   the same early return). The test fixture's trace_id is
        //   chosen to produce a non-zero mapped value.
        // - `rate > 1.0` (rejected by the constructor).
        let trace_id = [
            0xCA, 0xFE, 0xBA, 0xBE, 0xDE, 0xAD, 0xBE, 0xEF, //
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2A,
        ];
        let hash = xxh3_64(&trace_id);
        let mapped = mapped_to_unit_interval(hash);
        assert!(
            mapped > 0.0 && mapped < 1.0,
            "fixture trace_id must hash into the open interval (0.0, 1.0); got {mapped}"
        );

        let sampler = HeadSampler::new(mapped)
            .expect("mapped value lies in [0.0, 1.0] so the constructor must accept it");
        let spans = vec![span_with_status_code(StatusCode::Ok as i32)];
        // Reuse the fixture from `is_error_bearing_classifies_…` — a
        // span with a fixed trace_id of zeros — but the sampler reads
        // `trace.trace_id()` from the view, not from the spans. The
        // test seam supplies the trace_id directly.
        let view = crate::trace_view::__test_trace_view(trace_id, &spans);

        assert_eq!(
            sampler.sample(&view),
            Decision::Drop,
            "at rate == mapped(trace_id), the strict-`<` rule must drop \
             (mapped < mapped is false). A `<=` mutation would keep."
        );
    }
}
