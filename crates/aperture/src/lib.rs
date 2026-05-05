//! # `aperture`
//!
//! OTLP gateway. Listens on gRPC `:4317` and HTTP/protobuf `:4318`,
//! validates every payload through the
//! [`otlp-conformance-harness`](https://crates.io/crates/otlp-conformance-harness),
//! and hands accepted records to an [`OtlpSink`](ports::OtlpSink).
//!
//! ## DELIVER state — Slice 01 walking skeleton
//!
//! The gRPC arm is alive: a real `tonic` Server bound to the
//! configured address accepts `ExportLogsServiceRequest`, validates
//! through the real harness, and hands typed `SinkRecord::Logs` to the
//! configured sink. The HTTP arm and the Traces/Metrics services land
//! in subsequent slices (per `docs/feature/aperture/slices/`).
//!
//! ## Public surface
//!
//! - [`config`] — the [`Config`](config::Config) type and its loader.
//! - [`ports`] — the [`OtlpSink`](ports::OtlpSink) trait, the
//!   [`SinkRecord`](ports::SinkRecord) enum, [`SinkError`](ports::SinkError),
//!   and the [`Probe`](ports::Probe) trait.
//! - [`testing`] — test doubles for integration tests
//!   ([`RecordingSink`](testing::RecordingSink)) and the stderr capture
//!   seam.
//! - Top-level [`run`], [`spawn`], and [`Handle`] — the seam an
//!   integration test uses to start an Aperture instance with custom
//!   ports and a custom sink.

#![forbid(unsafe_code)]

pub mod config;
pub mod ports;
pub mod testing;

mod app;
mod backpressure;
mod compose;
mod error;
mod observability;
mod readiness;
mod shutdown;
mod sinks;
mod transport;

use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::Config;
use crate::ports::OtlpSink;
use crate::shutdown::{orchestrate_shutdown, DrainOutcome, ShutdownBundle, ShutdownTrigger};

/// Top-level error type. Slice 01 keeps the simpler `ApertureError(pub
/// String)` shape; subsequent slices replace it with the rich
/// `thiserror`-derived enum from `component-design.md > error`.
#[derive(Debug)]
pub struct ApertureError(pub String);

impl std::fmt::Display for ApertureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ApertureError {}

/// Convenience alias matching the design contract.
pub type Result<T> = std::result::Result<T, ApertureError>;

/// Handle to a running Aperture instance. Returned by [`spawn`].
///
/// Holds the bound gRPC and HTTP addresses plus the [`ShutdownBundle`]
/// the integration tests trigger via `Handle::shutdown` (or via the
/// implicit `Drop`). The bundle owns the per-transport listener
/// shutdown senders, the join handles, the per-transport concurrency
/// limiters (used to compute in-flight counts during the drain), the
/// shared readiness state (flipped to `Draining` when shutdown is
/// initiated), and the configured drain deadline.
pub struct Handle {
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) http_addr: SocketAddr,
    pub(crate) bundle: Option<ShutdownBundle>,
}

impl std::fmt::Debug for Handle {
    /// Render the handle as the bound listener addresses; the bundle's
    /// internals (join handles, oneshots, semaphore Arcs) have no
    /// stable debug representation and are intentionally elided.
    /// Slice 06's tests rely on `{result:?}` over `Result<Handle, _>`
    /// for failure paths — the addresses are what an operator would
    /// want to see in such a panic message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("grpc_addr", &self.grpc_addr)
            .field("http_addr", &self.http_addr)
            .field("shutdown_pending", &self.bundle.is_some())
            .finish()
    }
}

impl Handle {
    /// The address the gRPC listener bound to.
    pub fn grpc_addr(&self) -> SocketAddr {
        self.grpc_addr
    }

    /// The address the HTTP/protobuf listener bound to.
    pub fn http_addr(&self) -> SocketAddr {
        self.http_addr
    }

    /// Block until both listeners are bound and the application is
    /// ready to accept requests. Slice 02 returns immediately because
    /// `spawn` only completes after both `spawn_grpc` and `spawn_http`
    /// have bound their listeners and flipped the readiness flags.
    pub async fn wait_until_ready(&self) -> Result<()> {
        Ok(())
    }

    /// Initiate graceful shutdown. Equivalent to a SIGTERM in
    /// behaviour: emits `event=shutdown_initiated`, flips `/readyz` to
    /// `503 "draining"`, closes the listeners, drains in-flight
    /// requests bounded by the configured `drain_deadline`, and emits
    /// the verdict (`event=in_flight_drained` on a clean drain;
    /// `event=drain_deadline_exceeded` on timeout) followed by
    /// `event=shutdown_complete`. The integration tests use this
    /// surface as the deterministic seam; the production binary
    /// reaches the same orchestrator after a real OS signal.
    pub async fn shutdown(mut self) -> Result<()> {
        self.shutdown_with_trigger(ShutdownTrigger::HandleShutdown)
            .await
            .map(|_| ())
    }

    /// Internal entry point used by both `Handle::shutdown` and the
    /// binary's `main.rs` SIGTERM/SIGINT path. Returns the drain
    /// outcome so the binary can map it to a process exit code.
    pub(crate) async fn shutdown_with_trigger(
        &mut self,
        trigger: ShutdownTrigger,
    ) -> Result<DrainOutcome> {
        let Some(bundle) = self.bundle.take() else {
            // Already shut down (or `Drop` raced). Nothing to do; the
            // observable shape is "shutdown is idempotent".
            return Ok(DrainOutcome::Clean { drained_count: 0 });
        };
        Ok(orchestrate_shutdown(trigger, bundle).await)
    }
}

impl Handle {
    /// Best-effort listener wind-down used by `Drop`. Signals both
    /// transports' shutdown senders; the join handles are abandoned
    /// because `Drop` is sync and cannot await them. Returns the
    /// number of senders that successfully delivered (`0`, `1`, or
    /// `2`) so a unit test can pin the side effect and kill the
    /// `replace drop with ()` mutation.
    fn drop_signal_listeners(&mut self) -> u8 {
        let Some(bundle) = self.bundle.take() else {
            return 0;
        };
        let grpc_ok = bundle.grpc_shutdown.send(()).is_ok();
        let http_ok = bundle.http_shutdown.send(()).is_ok();
        u8::from(grpc_ok) + u8::from(http_ok)
        // bundle.grpc_join / http_join drop here, abandoning the
        // futures. Tokio will drop the spawned tasks as the runtime
        // tears down.
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Best-effort fast path on drop: if the test-owner forgot to
        // call `shutdown()` explicitly, signal both listeners so the
        // serving tasks can wind down without leaking. The drain
        // orchestrator owns the structured-event path; `Drop` is sync
        // and cannot await joins, so we surrender the deadline
        // bookkeeping here. Tests that assert on shutdown events MUST
        // call `Handle::shutdown` explicitly.
        let _ = self.drop_signal_listeners();
        #[cfg(test)]
        {
            // Test-only counter so a unit test can pin the Drop body
            // and kill the `replace drop with ()` mutation — without
            // this hook the Drop body's only side effect is consuming
            // the bundle's `Option<…>` content, which a mutation can
            // collapse to a no-op without any test noticing.
            tests::DROP_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// Run an Aperture instance, blocking the caller until shutdown. The
/// sink is chosen from `config.sink_kind`. This is the entry point
/// `main.rs` uses; tests prefer [`spawn`] (with a custom sink) so they
/// can drive the listener over the wire while still owning the
/// instance and observing its hand-off.
///
/// Returns the process exit code: 0 on a clean drain, 1 when the drain
/// deadline expired with in-flight requests outstanding. The binary's
/// `main` propagates this code to the supervisor.
pub async fn run(config: Config) -> Result<u8> {
    let sink: Arc<dyn OtlpSink> = crate::compose::wire_sink(&config).await?;
    let handle = spawn(config, sink).await?;

    // Block until SIGTERM (k8s `terminationGracePeriodSeconds`) or
    // SIGINT (developer Ctrl-C). On Unix, both are first-class signals;
    // `tokio::signal::ctrl_c` is portable but only covers SIGINT. The
    // unix-specific path below registers SIGTERM explicitly so an
    // operator-managed deployment gets the graceful drain path.
    //
    // SIGKILL is not handled: by definition the process cannot observe
    // SIGKILL; the operator runbook documents this trade-off.
    let trigger = wait_for_shutdown_signal().await;
    drain_to_exit_code(handle, trigger).await
}

/// Common drain-to-exit-code shape used by `run` and pinned by unit
/// tests. Extracted so the exit-code mapping (clean drain → 0, deadline
/// exceeded → 1) is testable without a process-spawning fixture.
async fn drain_to_exit_code(mut handle: Handle, trigger: ShutdownTrigger) -> Result<u8> {
    let outcome = handle.shutdown_with_trigger(trigger).await?;
    Ok(outcome.exit_code())
}

/// Wait for the first SIGTERM or SIGINT. Returns the matching
/// [`ShutdownTrigger`] so the orchestrator's `shutdown_initiated`
/// event names the trigger correctly.
///
/// On non-unix targets only SIGINT is observable; the function reduces
/// to `tokio::signal::ctrl_c` and returns `ShutdownTrigger::Sigint`.
async fn wait_for_shutdown_signal() -> ShutdownTrigger {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                // Unable to register SIGTERM; fall back to SIGINT only.
                let _ = tokio::signal::ctrl_c().await;
                return ShutdownTrigger::Sigint;
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return ShutdownTrigger::Sigint;
            }
        };
        tokio::select! {
            _ = sigterm.recv() => ShutdownTrigger::Sigterm,
            _ = sigint.recv() => ShutdownTrigger::Sigint,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        ShutdownTrigger::Sigint
    }
}

/// Spawn an Aperture instance on the current Tokio runtime and return a
/// [`Handle`]. The integration tests use this entry point to bind to
/// ephemeral ports (`127.0.0.1:0`) and assert observable behaviour
/// against the real listeners.
pub async fn spawn(config: Config, sink: Arc<dyn OtlpSink>) -> Result<Handle> {
    crate::compose::spawn(config, sink).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts every invocation of `Handle::drop`. A unit test reads this
    /// to pin the Drop body against the `replace drop with ()` mutation.
    pub(super) static DROP_INVOCATIONS: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn aperture_error_display_renders_the_inner_message() {
        // Pin the Display impl: an error rendered through `format!`
        // round-trips the inner string. Kills the
        // `<Display>::fmt -> Ok(default())` mutation that would emit
        // an empty string regardless of payload.
        let err = ApertureError("listener bind failed for grpc: address in use".to_string());
        assert_eq!(
            format!("{err}"),
            "listener bind failed for grpc: address in use"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dropping_a_handle_runs_the_drop_body() {
        // Pin the Drop body itself: a freshly-spawned handle, dropped
        // without explicit shutdown, increments the test-only counter
        // exactly once. Without the Drop body the counter would not
        // budge — the `replace drop with ()` mutation surfaces here.
        use crate::ports::OtlpSink;
        let sink: Arc<dyn OtlpSink> = Arc::new(crate::sinks::StubSink);
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build()
            .unwrap();
        let before = DROP_INVOCATIONS.load(Ordering::SeqCst);
        let handle = spawn(cfg, sink).await.expect("spawn");
        drop(handle);
        let after = DROP_INVOCATIONS.load(Ordering::SeqCst);
        assert_eq!(after - before, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drop_signal_listeners_returns_two_for_a_fresh_handle() {
        // Pin the Drop helper: a freshly-spawned handle has both
        // shutdown senders pending, so calling drop_signal_listeners
        // returns 2 (both deliveries successful). This kills the
        // `replace drop with ()` mutation — without the body the
        // helper would still need to compile, but a `Default::default`
        // mutation on the helper itself would yield 0, breaking the
        // assert.
        use crate::ports::OtlpSink;
        let sink: Arc<dyn OtlpSink> = Arc::new(crate::sinks::StubSink);
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build()
            .unwrap();
        let mut handle = spawn(cfg, sink).await.expect("spawn");
        assert_eq!(handle.drop_signal_listeners(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drop_signal_listeners_returns_zero_after_bundle_already_consumed() {
        // After Handle::shutdown has consumed the bundle, the helper's
        // None-branch is exercised: zero senders to deliver. Pinning
        // both branches kills the `match -> default` family of mutations
        // on `let Some(bundle) = self.bundle.take() else { return 0; };`.
        use crate::ports::OtlpSink;
        let sink: Arc<dyn OtlpSink> = Arc::new(crate::sinks::StubSink);
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build()
            .unwrap();
        let mut handle = spawn(cfg, sink).await.expect("spawn");
        // Drain the bundle through the public shutdown path.
        let bundle = handle.bundle.take().expect("bundle present");
        let _ = bundle.grpc_shutdown.send(());
        let _ = bundle.http_shutdown.send(());
        let _ = bundle.grpc_join.await;
        let _ = bundle.http_join.await;
        assert_eq!(handle.drop_signal_listeners(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_to_exit_code_returns_zero_for_a_clean_drain() {
        // Pin the run-path exit-code mapping: a handle with no
        // in-flight requests drains cleanly, exit code is 0. Together
        // with the deadline-exceeded test below this kills the
        // `drain_to_exit_code -> Ok(N)` mutation family.
        use crate::ports::OtlpSink;
        let sink: Arc<dyn OtlpSink> = Arc::new(crate::sinks::StubSink);
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build()
            .unwrap();
        let handle = spawn(cfg, sink).await.expect("spawn");
        let exit_code = drain_to_exit_code(handle, ShutdownTrigger::HandleShutdown)
            .await
            .expect("drain");
        assert_eq!(exit_code, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_to_exit_code_returns_one_when_deadline_exceeded() {
        // Pin the deadline-exceeded leg of the mapping: a Handle whose
        // listener join handles never resolve (synthetic infinite
        // tasks) reaches the orchestrator's deadline branch and exits
        // 1. This kills the `drain_to_exit_code -> Ok(0)` mutation —
        // without the real exit_code propagation the test sees 0
        // instead of 1.
        use crate::backpressure::{CapTransport, ConcurrencyLimiter};
        use crate::readiness::ReadinessState;
        use crate::shutdown::ShutdownBundle;
        use std::time::Duration;
        let readiness = ReadinessState::new();
        let grpc_limiter = ConcurrencyLimiter::new(1, CapTransport::Grpc);
        let http_limiter = ConcurrencyLimiter::new(1, CapTransport::HttpProtobuf);
        let (grpc_tx, mut grpc_rx) = tokio::sync::oneshot::channel::<()>();
        let (http_tx, mut http_rx) = tokio::sync::oneshot::channel::<()>();
        // Spawn never-completing listener tasks: they read the
        // shutdown signal but then loop forever, simulating an
        // unresponsive in-flight that survives the deadline.
        let grpc_join = tokio::spawn(async move {
            let _ = (&mut grpc_rx).await;
            // After receiving the shutdown signal, keep "draining"
            // forever. The orchestrator's timeout fires before we
            // resolve.
            std::future::pending::<()>().await;
        });
        let http_join = tokio::spawn(async move {
            let _ = (&mut http_rx).await;
            std::future::pending::<()>().await;
        });
        let bundle = ShutdownBundle {
            readiness,
            grpc_limiter,
            http_limiter,
            grpc_shutdown: grpc_tx,
            http_shutdown: http_tx,
            grpc_join,
            http_join,
            // 200 ms deadline so the test runs fast.
            drain_deadline: Duration::from_millis(200),
        };
        let handle = Handle {
            grpc_addr: "127.0.0.1:0".parse().unwrap(),
            http_addr: "127.0.0.1:0".parse().unwrap(),
            bundle: Some(bundle),
        };
        let exit_code = drain_to_exit_code(handle, ShutdownTrigger::HandleShutdown)
            .await
            .expect("drain");
        assert_eq!(exit_code, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn handle_debug_reports_bound_addresses() {
        // Pin the Handle Debug impl: a real spawn yields a handle whose
        // debug formatting names both ephemeral addresses. This kills
        // the `Debug::fmt -> Ok(default)` mutation — without the field
        // calls the formatter would render `Handle { .. }` (or any
        // empty shape) and the assertions below fail.
        use crate::ports::OtlpSink;
        let sink: Arc<dyn OtlpSink> = Arc::new(crate::sinks::StubSink);
        let cfg = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .build()
            .unwrap();
        let handle = spawn(cfg, sink).await.expect("spawn");
        let rendered = format!("{:?}", handle);
        assert!(rendered.contains("Handle"), "got: {rendered}");
        assert!(rendered.contains("grpc_addr"), "got: {rendered}");
        assert!(rendered.contains("http_addr"), "got: {rendered}");
        assert!(rendered.contains("shutdown_pending"), "got: {rendered}");
        let _ = handle.shutdown().await;
    }
}
