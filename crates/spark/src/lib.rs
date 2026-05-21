//! # `spark`
//!
//! Kaleidoscope's Apache-2.0 Rust SDK. A thin wrapper around the
//! upstream `opentelemetry`, `opentelemetry_sdk`, and
//! `opentelemetry-otlp` crates that injects Kaleidoscope's house
//! resource attributes (`service.name`, optional `tenant.id`, optional
//! `feature_flag.*`, optional `experiment.id`) on every emitted signal,
//! lints required attributes at startup, honours the OTel-canonical
//! environment-variable contract, and flushes pending exports
//! synchronously when the returned guard drops.
//!
//! ## Implementation status
//!
//! Fully implemented and green. The public surface declared below is
//! the v0 contract locked by ADR-0011; the integration tests under
//! `tests/` (`slice_01..slice_06` and the two `invariant_*` binaries)
//! exercise that surface and lock its behaviour.
//!
//! ## Public surface (locked by ADR-0011 §"Public surface")
//!
//! Exactly four items, alphabetised below. No re-exports of the
//! upstream OTel SDK types — consumers depend on `opentelemetry`,
//! `opentelemetry_sdk`, and `opentelemetry-otlp` directly. The
//! no-re-exports rule keeps the dependency edge visible (harness
//! ADR-0001 precedent).
//!
//! - [`init`] — the one entry point; takes a [`SparkConfig`] and
//!   returns a [`SparkGuard`] on success or a [`SparkError`] on failure.
//! - [`SparkConfig`] — the value-consuming builder (see [`config`]).
//! - [`SparkError`] — the closed error enum (see [`error`]).
//! - [`SparkGuard`] — the opaque RAII guard (see [`guard`]).
//!
//! ## Canonical pattern
//!
//! ```ignore
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Bind the guard to a *named* local so its Drop runs at end of
//!     // scope — not `let _ = ...` which would discard it immediately.
//!     let _guard = spark::init(
//!         spark::SparkConfig::for_service("payments-api")
//!             .require_tenant_id()
//!             .with_tenant_id("acme-prod"),
//!     )?;
//!
//!     // ... business logic emitting telemetry via the standard OTel
//!     //     API: opentelemetry::global::tracer(...), logger_provider(),
//!     //     meter() ...
//!
//!     Ok(())
//!     // _guard drops here, force-flushing pending exports synchronously
//!     // within the configured deadline (default 5 s).
//! }
//! ```

#![forbid(unsafe_code)]

mod config;
mod error;
mod guard;
mod init;
mod observability;

pub use crate::config::SparkConfig;
pub use crate::error::SparkError;
pub use crate::guard::SparkGuard;

/// Initialise Spark.
///
/// On `Ok`, returns a [`SparkGuard`] whose `Drop` runs at scope exit
/// to flush pending OTLP exports synchronously with the configured
/// deadline (default 5 s, see [`SparkConfig::with_flush_timeout`]).
/// On `Err`, no global state has changed: no OTel SDK provider has
/// been set, no exporter has been constructed, no telemetry has
/// reached any backend.
///
/// # Errors
///
/// Returns one of the four [`SparkError`] variants:
/// - [`SparkError::MissingRequiredAttribute`] if `service.name` is
///   empty or `tenant.id` is empty after [`SparkConfig::require_tenant_id`].
/// - [`SparkError::InvalidEndpoint`] if the resolved endpoint cannot
///   be parsed or its scheme is neither `http` nor `https`.
/// - [`SparkError::ExporterInitFailed`] if the upstream OTLP exporter
///   constructor returns an error.
/// - [`SparkError::GlobalAlreadyInitialised`] if `spark::init` has
///   already returned `Ok` in this process or if some other code has
///   set the OTel global tracer provider before this call.
///
/// # Panics
///
/// Never panics on user input — every failure path is a
/// [`SparkError`] variant. Panics only on true Spark-internal
/// invariant violations, which constitute a defect.
///
/// # Example
///
/// ```ignore
/// let _guard = spark::init(
///     spark::SparkConfig::for_service("payments-api")
///         .require_tenant_id()
///         .with_tenant_id("acme-prod"),
/// )?;
/// ```
pub fn init(config: SparkConfig) -> Result<SparkGuard, SparkError> {
    crate::init::init(config)
}

/// Doc-hidden test seam. Resets Spark's per-process single-init flag
/// so a test binary holding several `[[test]]` functions can call
/// [`init`] more than once. Production code never invokes this — the
/// flag's set-once-per-process semantic is the load-bearing contract
/// of ADR-0015 §1.
///
/// The accompanying test must already serialise (e.g. via
/// `serial_test::serial`) so two parallel test threads do not race on
/// the OTel global tracer provider — which has no public reset API at
/// `=0.27`. This function does NOT reset OTel global state; only
/// Spark's own flag is reset.
#[doc(hidden)]
pub fn __reset_for_testing() {
    crate::init::reset_for_testing();
}

/// Doc-hidden test seam: retrieve the `LoggerProvider` Spark's most
/// recent successful [`init`] call constructed.
///
/// Per ADR-0017 §"Verification": the Slice 05 logs-emission
/// integration tests need to build an
/// `OpenTelemetryTracingBridge::new(&logger_provider)` against the
/// provider Spark configured, so they can install the bridge into
/// the test subscriber's reload slot. The OTel SDK at `=0.27` has no
/// global logger-provider getter (this is the upstream limitation
/// ADR-0017 routes around); the test seam provides the getter Spark's
/// integration tests need without expanding the consumer-facing
/// public surface beyond the four-item lock (ADR-0011).
///
/// Production code never invokes this — Spark's `init` installs the
/// global tracing subscriber containing the bridge layer when no
/// subscriber is pre-installed. The seam is a test-only
/// implementation detail; the `__` prefix and `#[doc(hidden)]`
/// attribute mark it as the well-known Rust idiom for "stable across
/// versions but explicitly not part of the consumer-facing contract"
/// (ADR-0011's "Test seam" subsection).
#[doc(hidden)]
pub fn __test_logger_provider() -> Option<opentelemetry_sdk::logs::LoggerProvider> {
    crate::init::test_logger_provider()
}
