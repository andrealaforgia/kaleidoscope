//! `SparkError` — the closed v0 error enum.
//!
//! Per ADR-0012: four variants locked at DISCUSS, `#[non_exhaustive]`
//! posture, explicit `Display` and `Error` impls (not `thiserror`
//! derive at v0), `Debug`-only trait derives, `source()` chain via
//! `Box<dyn Error + Send + Sync>` for `ExporterInitFailed`.

use std::fmt;

/// The error type returned from [`crate::init`].
///
/// Closed at v0 (four variants). `#[non_exhaustive]` so additions in
/// future Spark versions are non-breaking; pattern-match consumers
/// must use a wildcard arm. Renames or removals are version-bump.
#[non_exhaustive]
#[derive(Debug)]
pub enum SparkError {
    /// A required attribute was absent or empty when `spark::init`
    /// validated the configuration.
    ///
    /// `name` is the OpenTelemetry semantic-conventions key (e.g.
    /// `"service.name"` or `"tenant.id"`) so the application's error
    /// handler can map it to a configuration field.
    MissingRequiredAttribute {
        /// The OTel attribute key that was absent or empty.
        name: String,
    },

    /// The resolved endpoint URI failed to parse, or its scheme was
    /// neither `http` nor `https`.
    ///
    /// `endpoint` is the literal value Spark attempted to use;
    /// `reason` is a human-readable parse-failure message ultimately
    /// sourced from `url::ParseError` or from Spark's own scheme
    /// check.
    InvalidEndpoint {
        /// The literal endpoint string Spark attempted to parse.
        endpoint: String,
        /// Human-readable parse-failure message.
        reason: String,
    },

    /// The upstream `opentelemetry-otlp` exporter constructor
    /// returned an error (TLS configuration, transport setup, runtime
    /// not available, etc.).
    ///
    /// `reason` carries the upstream error's `Display` form. The
    /// causal chain is exposed via `std::error::Error::source()`.
    ExporterInitFailed {
        /// Human-readable description of the failure.
        reason: String,
        /// Optional source error for chained-error inspection.
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// `spark::init` was called twice in the same process, OR
    /// `opentelemetry::global::set_tracer_provider` had already been
    /// called by some other code in this process before `spark::init`
    /// ran.
    ///
    /// Carries no payload — the diagnostic is the variant name.
    GlobalAlreadyInitialised,
}

impl fmt::Display for SparkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequiredAttribute { name } => {
                write!(f, "spark: missing required attribute: {name}")
            }
            Self::InvalidEndpoint { endpoint, reason } => {
                write!(f, "spark: invalid endpoint {endpoint:?}: {reason}")
            }
            Self::ExporterInitFailed { reason, .. } => {
                write!(f, "spark: exporter initialisation failed: {reason}")
            }
            Self::GlobalAlreadyInitialised => {
                f.write_str("spark: opentelemetry global tracer provider already initialised")
            }
        }
    }
}

impl std::error::Error for SparkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ExporterInitFailed {
                source: Some(s), ..
            } => Some(s.as_ref()),
            _ => None,
        }
    }
}
