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
/// Holds the bound gRPC and HTTP addresses plus the shutdown signals
/// that the integration tests trigger via `Handle::shutdown` (or via
/// the implicit `Drop`). Each transport owns a dedicated oneshot
/// because a single oneshot can only deliver to one receiver; Slice
/// 08 will replace the per-transport oneshots with a `broadcast`
/// sender wired through the shutdown orchestrator.
#[derive(Debug)]
pub struct Handle {
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) http_addr: SocketAddr,
    pub(crate) grpc_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    pub(crate) http_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    pub(crate) grpc_join: Option<tokio::task::JoinHandle<()>>,
    pub(crate) http_join: Option<tokio::task::JoinHandle<()>>,
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

    /// Initiate graceful shutdown. Triggers the shutdown signal on
    /// both transports and awaits both serving tasks. Slice 08 will
    /// land the full drain orchestrator with deadline semantics.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(tx) = self.grpc_shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = self.http_shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.grpc_join.take() {
            let _ = join.await;
        }
        if let Some(join) = self.http_join.take() {
            let _ = join.await;
        }
        Ok(())
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Best-effort: signal shutdown to both transports so the
        // serving tasks can wind down. The join handles are abandoned
        // because Drop is sync.
        if let Some(tx) = self.grpc_shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = self.http_shutdown.take() {
            let _ = tx.send(());
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
