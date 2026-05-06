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
//! ## DISTILL state
//!
//! The struct exists; the Drop impl exists but is a no-op (the
//! panic lives in `init.rs::init` which never returns at the day-one
//! stub, so no `SparkGuard` is ever constructed at DISTILL). The
//! `Inner` carrier is shaped for the DELIVER wave's bounded-flush
//! mechanism (per ADR-0014 §1).

use std::time::Duration;

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

/// Internal carrier for the SparkGuard's resources. Holds the three
/// OTel SDK providers and the configured flush timeout for use at
/// drop time. Private to the crate; never appears on the public
/// surface.
///
/// At DISTILL the field set is intentionally minimal — DELIVER fills
/// in the providers when Slice 01 lands. The `flush_timeout` field is
/// shaped here so Slice 06's tests can assert against the bound.
pub(crate) struct Inner {
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
        let Some(_inner) = self.inner.take() else {
            return; // second drop: no-op
        };
        // DISTILL state: no providers wired up yet (see Inner's
        // `#[allow(dead_code)]` placeholder). The full bounded-flush
        // logic — ADR-0014 §1 sequential per-provider flush with
        // shared budget; ADR-0014 §2 INFO/WARN event with
        // `drained=unknown`/`dropped=unknown`; ADR-0014 §3 panic-
        // safety posture — lands when DELIVER implements Slice 06.
    }
}
