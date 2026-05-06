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

use std::time::{Duration, Instant};

use opentelemetry_sdk::logs::LoggerProvider as SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;

use crate::observability;

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
/// At Slice 01 the carrier held the tracer provider only. Slice 05
/// widens it to the logger and meter providers — Drop now flushes
/// all three in sequence so a counter increment immediately before
/// guard drop reaches Aperture, and so a `tracing::info!` event
/// routed through the appender bridge does the same. The full
/// bounded-deadline-budget mechanics (ADR-0014 §1, shared
/// remaining-time budget across the three providers) land in Slice
/// 06. The `flush_timeout` field carries the configured deadline so
/// Slice 06 can wire it without restructuring this carrier.
pub(crate) struct Inner {
    /// The OTel tracer provider Spark constructed and registered as
    /// the global provider. Held here so Drop can run `force_flush`
    /// before the application exits.
    pub(crate) tracer_provider: SdkTracerProvider,
    /// The OTel logger provider Spark constructed and configured the
    /// `opentelemetry-appender-tracing` bridge against. Held here so
    /// Drop runs `force_flush` to push any pending log records the
    /// bridge enqueued before scope exit (per ADR-0017 the bridge
    /// converts every non-`spark`-target `tracing::*!` event into an
    /// OTel `LogRecord` flowing through this provider's batch
    /// processor).
    pub(crate) logger_provider: SdkLoggerProvider,
    /// The OTel meter provider Spark constructed and set as the global
    /// meter provider. Held here so Drop runs `force_flush` to push
    /// any pending metric data points (counter increments are batched
    /// in the periodic reader; without an explicit flush at drop, a
    /// short-running application's last increment never reaches the
    /// wire — see Slice 05 acceptance test
    /// `developer_increments_one_counter_and_metrics_export_carries_*`).
    pub(crate) meter_provider: SdkMeterProvider,
    /// Configured flush deadline for the per-provider sequential
    /// flush (ADR-0014 §1). Default 5 s when not set on `SparkConfig`.
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

        // Per ADR-0014 §1: sequential per-provider flush with a shared
        // remaining-time budget. The total drop time is bounded by
        // `flush_timeout`; each provider sees as much of the budget as
        // the preceding ones did not consume.
        //
        // Per ADR-0014 §3 (panic safety): every fallible call is
        // matched on `Result`; no `unwrap()`, no `expect()`, no
        // `catch_unwind`. `tracing::info!` and `tracing::warn!` are
        // infallible by design (they fail silently when no subscriber
        // is attached, the correct library posture).
        observability::emit_shutdown_initiated(inner.flush_timeout);

        let deadline = Instant::now() + inner.flush_timeout;
        let mut deadline_exceeded = false;

        // Order: traces, then logs, then metrics (ADR-0014 §1 is
        // agnostic about the order; this fixed sequence makes the
        // remaining-time arithmetic deterministic for tests).
        if !flush_tracer(&inner.tracer_provider, deadline) {
            deadline_exceeded = true;
        }
        if !flush_logger(&inner.logger_provider, deadline) {
            deadline_exceeded = true;
        }
        if !flush_meter(&inner.meter_provider, deadline) {
            deadline_exceeded = true;
        }

        // Per ADR-0014 §2: choose WARN-vs-INFO based on whether any
        // provider hit the deadline OR returned a non-Ok flush outcome.
        if deadline_exceeded {
            observability::emit_flush_deadline_exceeded(inner.flush_timeout);
        } else {
            observability::emit_shutdown_complete();
        }

        // Per ADR-0014 §"Idempotent Drop" (extension to ADR-0015): the
        // single-init AtomicBool reservation is released on guard drop
        // so a process that legitimately re-initialises Spark after a
        // dropped guard (hot-reload of OTel config, integration-test
        // binaries that run multiple init→drop cycles) gets a clean
        // slate. The single-init invariant continues to defend the
        // "two `init` calls while a guard is alive" case
        // (`invariant_single_init.rs` exercises this contract).
        crate::init::reset_after_drop();
    }
}

/// Sequentially flush the tracer provider with the shared remaining-
/// time budget. Returns `true` on a clean flush, `false` if the
/// deadline expired before this call ran OR if any per-processor
/// `force_flush` returned a non-Ok result.
///
/// Per ADR-0014 §1: when the deadline has already expired before this
/// provider's flush is attempted, the call is skipped (no work done,
/// `false` returned so the caller knows to record the deadline-
/// exceeded outcome).
fn flush_tracer(provider: &SdkTracerProvider, deadline: Instant) -> bool {
    if Instant::now() >= deadline {
        return false;
    }
    let mut all_ok = true;
    for result in provider.force_flush() {
        if result.is_err() {
            all_ok = false;
        }
    }
    if Instant::now() > deadline {
        all_ok = false;
    }
    all_ok
}

/// Sequentially flush the logger provider with the shared remaining-
/// time budget. See [`flush_tracer`] for the contract.
fn flush_logger(provider: &SdkLoggerProvider, deadline: Instant) -> bool {
    if Instant::now() >= deadline {
        return false;
    }
    let mut all_ok = true;
    for result in provider.force_flush() {
        if result.is_err() {
            all_ok = false;
        }
    }
    if Instant::now() > deadline {
        all_ok = false;
    }
    all_ok
}

/// Sequentially flush the meter provider with the shared remaining-
/// time budget. The meter provider's `force_flush` returns a single
/// `MetricResult<()>` (rather than a Vec like the tracer/logger
/// providers); the contract is otherwise identical to
/// [`flush_tracer`].
fn flush_meter(provider: &SdkMeterProvider, deadline: Instant) -> bool {
    if Instant::now() >= deadline {
        return false;
    }
    let result = provider.force_flush();
    let mut all_ok = result.is_ok();
    if Instant::now() > deadline {
        all_ok = false;
    }
    all_ok
}
