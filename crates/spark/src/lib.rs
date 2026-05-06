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
//! ## DISTILL state
//!
//! The public surface declared below is the v0 contract locked by
//! ADR-0011. Every function, every type, every method panics with
//! `unimplemented!()` at the DISTILL boundary — this is the canonical
//! RED state. The integration tests under `tests/` are written against
//! this stable surface and panic at the first call. DELIVER (Crafty)
//! drives one panic away per slice, in order, until every test in
//! `slice_01..slice_06` and the two `invariant_*` binaries flips to
//! GREEN.
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
