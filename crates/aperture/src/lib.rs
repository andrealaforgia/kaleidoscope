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
/// Holds the bound gRPC address (and, from Slice 02, the bound HTTP
/// address) plus the shutdown signal that the integration tests
/// trigger via `Handle::shutdown` (or, in Slice 01's tests, via the
/// implicit `Drop`).
#[derive(Debug)]
pub struct Handle {
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) http_addr: Option<SocketAddr>,
    pub(crate) shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    pub(crate) grpc_join: Option<tokio::task::JoinHandle<()>>,
}

impl Handle {
    /// The address the gRPC listener bound to.
    pub fn grpc_addr(&self) -> SocketAddr {
        self.grpc_addr
    }

    /// The address the HTTP/protobuf listener bound to. Slice 02 lands
    /// the HTTP listener; in Slice 01 this returns the placeholder
    /// `0.0.0.0:0` (so the test harness's `http_addr()` accessor does
    /// not panic — Slice 01 tests do not exercise it).
    pub fn http_addr(&self) -> SocketAddr {
        self.http_addr
            .unwrap_or_else(|| "0.0.0.0:0".parse().expect("placeholder addr parses"))
    }

    /// Block until the gRPC listener is bound and the application is
    /// ready to accept requests. Slice 01 returns immediately (the
    /// listener is bound by the time `spawn` returns).
    pub async fn wait_until_ready(&self) -> Result<()> {
        Ok(())
    }

    /// Initiate graceful shutdown. Slice 01's implementation triggers
    /// the gRPC server's shutdown signal and awaits the serving task.
    /// Slice 08 will land the full drain orchestrator.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.grpc_join.take() {
            let _ = join.await;
        }
        Ok(())
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            // Best-effort: signal shutdown so the serving task can wind
            // down. The join handle is abandoned because Drop is sync.
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
