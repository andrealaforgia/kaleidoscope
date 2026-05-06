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
    fn sample(&self, _trace: &TraceView<'_>) -> Decision {
        // RED: DELIVER's slice 01 / 02 / 03 / 04 land the production
        // implementation. The shape is locked at ADR-0018
        // §"`HeadSampler::sample` mechanism": error-bias check first,
        // `xxh3_64(trace_id)` mapping into `[0.0, 1.0]`, compare
        // against `self.rate`.
        unimplemented!("HeadSampler::sample lands at DELIVER slices 01-04");
    }
}
