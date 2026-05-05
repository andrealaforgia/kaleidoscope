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
#[derive(Debug)]
pub struct Handle {
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) http_addr: SocketAddr,
    pub(crate) bundle: Option<ShutdownBundle>,
}

impl std::fmt::Debug for ShutdownBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShutdownBundle")
            .field("drain_deadline", &self.drain_deadline)
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

impl Drop for Handle {
    fn drop(&mut self) {
        // Best-effort fast path on drop: if the test-owner forgot to
        // call `shutdown()` explicitly, signal both listeners so the
        // serving tasks can wind down without leaking. The drain
        // orchestrator owns the structured-event path; `Drop` is sync
        // and cannot await joins, so we surrender the deadline
        // bookkeeping here. Tests that assert on shutdown events MUST
        // call `Handle::shutdown` explicitly.
        if let Some(bundle) = self.bundle.take() {
            let _ = bundle.grpc_shutdown.send(());
            let _ = bundle.http_shutdown.send(());
            // Join handles are abandoned because Drop is sync. Tokio
            // will drop the spawned tasks as the runtime tears down.
        }
    }
}

/// Run an Aperture instance, blocking the caller until shutdown. The
/// sink is chosen from `config.sink_kind`. This is the entry point
/// `main.rs` uses; tests prefer [`spawn`] (with a custom sink) so they
/// can drive the listener over the wire while still owning the
/// instance and observing its hand-off.
pub async fn run(config: Config) -> Result<()> {
    let sink: Arc<dyn OtlpSink> = crate::compose::wire_sink(&config).await?;
    let handle = spawn(config, sink).await?;
    // Block until SIGTERM/SIGINT (Slice 08 will land a drain
    // orchestrator). Slice 01 honours an interrupt by triggering
    // graceful shutdown of the gRPC listener.
    let interrupt = tokio::signal::ctrl_c();
    let _ = interrupt.await;
    handle.shutdown().await
}

/// Spawn an Aperture instance on the current Tokio runtime and return a
/// [`Handle`]. The integration tests use this entry point to bind to
/// ephemeral ports (`127.0.0.1:0`) and assert observable behaviour
/// against the real listeners.
pub async fn spawn(config: Config, sink: Arc<dyn OtlpSink>) -> Result<Handle> {
    crate::compose::spawn(config, sink).await
}
