//! `SparkGuard` — the opaque RAII guard.
//!
//! Per ADR-0016: opaque struct with private fields, `#[must_use]`
//! with a directive message explaining the silent-discard hazard,
//! Drop-only contract (no `shutdown()`, no `flush_now()`, no field
//! accessors), minimum trait derives.
//!
//! Per ADR-0014: Drop runs sequential per-provider `force_flush` with
//! a shared remaining-time budget; idempotent via `Option::take`;
//! emits exactly one observability event after the
//! `shutdown initiated` event; never panics.
//!
//! ## Slice 01 — walking skeleton
//!
//! Drop force-flushes the configured tracer provider so spans emitted
//! before scope exit reach Aperture's listener. Slice 06 widens this
//! to the bounded sequential per-provider flush across tracer +
//! logger + meter (ADR-0014 §1) and lands the
//! `shutdown initiated` / `shutdown complete` / `flush deadline
//! exceeded` event vocabulary (ADR-0014 §2).

use std::time::Duration;

use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;

/// Opaque RAII guard returned from [`crate::init`].
///
/// The application binds the guard to a *named* local variable in
/// `main` so its `Drop` runs at scope exit, force-flushing pending
/// OTLP exports synchronously within the configured deadline.
///
/// The `#[must_use]` attribute makes the compiler emit a warning if
/// the return value of `spark::init` is discarded — because losing
/// the guard (binding to `_` instead of `_guard`) drops it
/// immediately, stopping the OTel pipeline before any telemetry is
/// emitted.
///
/// # Canonical pattern
///
/// ```ignore
/// let _guard = spark::init(config)?;  // _guard binds (does NOT discard)
/// // ... application code ...
/// // _guard drops at end of scope, force-flushing exports.
/// ```
#[must_use = "SparkGuard must be held for the lifetime of the application; binding to `_` drops it immediately and stops the OTel pipeline before any telemetry is emitted"]
pub struct SparkGuard {
    pub(crate) inner: Option<Inner>,
}

/// Internal carrier for the SparkGuard's resources. Holds the OTel
/// SDK provider(s) and the configured flush timeout for use at drop
/// time. Private to the crate; never appears on the public surface.
///
/// At Slice 01 the carrier holds the tracer provider only. Slice 05
/// widens this to the logger and meter providers. The
/// `flush_timeout` is wired through from `init` so Slice 06 can build
/// the bounded sequential flush against it.
pub(crate) struct Inner {
    /// The OTel tracer provider Spark constructed and registered as
    /// the global provider. Held here so Drop can run `force_flush`
    /// before the application exits.
    pub(crate) tracer_provider: SdkTracerProvider,
    /// Configured flush deadline for the per-provider sequential
    /// flush (ADR-0014 §1). Default 5 s when not set on `SparkConfig`.
    #[allow(dead_code)]
    pub(crate) flush_timeout: Duration,
}

impl std::fmt::Debug for SparkGuard {
    /// Minimal Debug output — no fields, no resolved-configuration
    /// leak (per ADR-0016 §4). The resolved configuration is
    /// observable via the `tracing` INFO event Spark emits at init
    /// time, not via this Debug surface.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SparkGuard").finish_non_exhaustive()
    }
}

impl Drop for SparkGuard {
    fn drop(&mut self) {
        // Per ADR-0014 §4: idempotent second drop via `Option::take`.
        // The first drop takes the `Some(inner)` and runs the flush;
        // subsequent drops see `None` and return immediately.
        let Some(inner) = self.inner.take() else {
            return; // second drop: no-op
        };
        // Slice 01 force-flushes the tracer provider synchronously so
        // spans emitted before scope exit reach the configured OTLP
        // endpoint. Per ADR-0014 §3 (panic safety): every result is
        // matched on `Result`; nothing in this Drop unwraps. The full
        // bounded sequential flush across tracer + logger + meter
        // with shared remaining-time budget (ADR-0014 §1) and the
        // `shutdown initiated` / `shutdown complete` / `flush deadline
        // exceeded` event vocabulary (ADR-0014 §2) land in Slice 06.
        for result in inner.tracer_provider.force_flush() {
            // Errors are absorbed: the `tracing::warn!` event vocabulary
            // for the deadline-exceeded path is Slice 06's contract.
            // For Slice 01, a force-flush failure is ignored so Drop
            // remains panic-safe (the application's exit is bounded
            // by the upstream batch processor's timeout, which is
            // shorter than the v0 default 5 s flush deadline).
            let _ = result;
        }
    }
}
