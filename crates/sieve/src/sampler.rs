//! `Sampler` trait + `HeadSampler` concrete sampler.
//!
//! Per ADR-0018 §"Public surface (final list)": one trait, one
//! method, one return value. The error-bias rule and the rate-based
//! rule live inside `HeadSampler::sample` per ADR-0018
//! §"`HeadSampler::sample` mechanism".
//!
//! ## DISTILL state
//!
//! - `HeadSampler::new(rate)` and `HeadSampler::from_env()` are real
//!   (return `Result<HeadSampler, SieveConfigError>`). Slice tests
//!   need to call them to compile.
//! - `HeadSampler::rate()` is real (returns the stored rate). Slice
//!   tests need it for the slice-06 INFO summary assertion.
//! - `HeadSampler::sample(...)` panics with `unimplemented!()`. The
//!   slice 01 / 02 / 03 / 04 RED tests panic on this until DELIVER.

use std::str::FromStr;

use opentelemetry_proto::tonic::trace::v1::status::StatusCode;
use opentelemetry_proto::tonic::trace::v1::Span;

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
    ///
    /// This constructor is real at DISTILL because the slice tests
    /// call it directly to build fixtures. The actual sampling
    /// decision (`sample`) panics on `unimplemented!()` until
    /// DELIVER.
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

        // Slice 01 contract: at rate 0.0, the rate-based rule drops
        // every non-error trace. Per ADR-0018 §"Boundary semantics for
        // rate": `mapped < 0.0` is always false because `mapped >= 0.0`,
        // so no non-error trace is kept at rate 0.0.
        if self.rate == 0.0 {
            return Decision::Drop;
        }

        // Slice 03 lands the `xxh3_64`-keyed rate decision for
        // non-zero, non-error rates. Until then, calling `sample` with
        // a non-zero rate on a non-error trace is out of scope.
        unimplemented!(
            "HeadSampler::sample for non-zero non-error rates lands at DELIVER slice 03"
        );
    }
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
}
