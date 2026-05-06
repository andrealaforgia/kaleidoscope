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
        // remaining-time arithmetic deterministic for tests). One
        // closure per provider feeds [`flush_with_budget`]; the budget
        // arithmetic and the post-flush outcome combination live in
        // exactly one place so mutation testing has one boundary to
        // exercise. The per-provider outcomes are accumulated into a
        // small array and reduced through `.iter().all(...)` so the
        // WARN-vs-INFO decision has a single boolean expression rather
        // than a chained `||` whose mutants survive when only one
        // provider fails.
        let outcomes = [
            flush_with_budget(deadline, || results_ok(inner.tracer_provider.force_flush())),
            flush_with_budget(deadline, || results_ok(inner.logger_provider.force_flush())),
            flush_with_budget(deadline, || inner.meter_provider.force_flush().is_ok()),
        ];
        if !outcomes.iter().all(|ok| *ok) {
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

/// Run a per-provider flush within the shared remaining-time budget.
///
/// Returns `true` on a clean flush, `false` if the deadline already
/// expired before the closure runs OR if the closure itself reported a
/// non-Ok flush outcome.
///
/// Per ADR-0014 §1: when the deadline has already expired before this
/// provider's flush is attempted, the closure is NOT called (no work
/// done, `false` returned so the caller knows to record the deadline-
/// exceeded outcome). The intra-flush deadline is enforced by the
/// SDK's own `max_export_timeout` (bound to `flush_timeout` at init
/// time, see `crate::init::build_pipeline`); a force_flush that
/// exceeds its export budget returns Err, which the closure translates
/// to `false`.
///
/// Pulling the budget arithmetic out into one helper keeps the three
/// per-provider sites uniform and reduces the mutation surface to a
/// single boundary the integration tests already cover (the
/// unreachable-endpoint scenarios exercise the `false`-returning path
/// via the tracer; the healthy-aperture scenarios exercise the
/// `true`-returning path via all three).
fn flush_with_budget<F>(deadline: Instant, flush: F) -> bool
where
    F: FnOnce() -> bool,
{
    if Instant::now() >= deadline {
        return false;
    }
    flush()
}

/// Combine a per-processor flush results vector (TraceResult / LogResult)
/// into a single outcome boolean. Returns `true` only when every result
/// is Ok.
fn results_ok<E>(results: Vec<Result<(), E>>) -> bool {
    results.iter().all(|r| r.is_ok())
}
