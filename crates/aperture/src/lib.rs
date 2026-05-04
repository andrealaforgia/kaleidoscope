//! # `aperture`
//!
//! OTLP gateway. Listens on gRPC `:4317` and HTTP/protobuf `:4318`,
//! validates every payload through the
//! [`otlp-conformance-harness`](https://crates.io/crates/otlp-conformance-harness),
//! and hands accepted records to an [`OtlpSink`](ports::OtlpSink).
//!
//! ## DISTILL state
//!
//! The implementation is intentionally absent at DISTILL. Every public
//! function in this crate returns `unimplemented!()`. The acceptance
//! tests in `tests/slice_*.rs` panic on these calls — that is the RED
//! state of the outermost loop of double-loop TDD. The DELIVER wave's
//! `nw-software-crafter` agent drives the panics away one at a time as
//! it lands each slice from `docs/feature/aperture/slices/`.
//!
//! ## Public surface
//!
//! Per `docs/feature/aperture/design/component-design.md`, the library
//! surface is small and stable:
//!
//! - [`config`] — the [`Config`](config::Config) type and its loader.
//! - [`ports`] — the [`OtlpSink`](ports::OtlpSink) trait, the
//!   [`SinkRecord`](ports::SinkRecord) enum, [`SinkError`](ports::SinkError),
//!   and the [`Probe`](ports::Probe) trait.
//! - [`testing`] — test doubles for integration tests
//!   ([`RecordingSink`](testing::RecordingSink)).
//! - Top-level [`run`], [`spawn`], and [`Handle`] — the seam an
//!   integration test uses to start an Aperture instance with custom
//!   ports and a custom sink.

// SCAFFOLD: true
// Status: DISTILL RED scaffold. Every public function in this crate
// returns `unimplemented!()`. The integration tests under `tests/`
// link against this scaffold, compile clean, and panic at runtime on
// the first call into the production surface — that is the canonical
// RED state of the outermost loop of double-loop TDD.

#![forbid(unsafe_code)]

pub mod config;
pub mod ports;
pub mod testing;

// Private placeholder modules matching `design/component-design.md`'s
// module tree. DELIVER replaces each `mod.rs` with the real
// implementation, growing module-by-module per the slice plan. These
// are private at DISTILL because the tests do not import them; the
// public surface is `config`, `ports`, `testing`, plus the top-level
// `run`/`spawn`/`Handle`/`ApertureError` items below.
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

/// Top-level error type. DELIVER replaces this with the rich
/// `ApertureError` enum specified in
/// `docs/feature/aperture/design/component-design.md > error::ApertureError`.
/// At DISTILL it is an opaque `String` so the stub compiles without
/// pulling in `thiserror`.
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

/// Handle to a running Aperture instance. Returned by [`spawn`]. The
/// integration tests use this to discover the bound listener addresses
/// (which are ephemeral when tests bind to `127.0.0.1:0`) and to
/// trigger graceful shutdown without sending an OS signal.
///
/// DELIVER fills in the actual structure; at DISTILL every method
/// panics with `unimplemented!()`.
#[derive(Debug)]
pub struct Handle {
    _private: (),
}

impl Handle {
    /// The address the gRPC listener bound to. Available only once the
    /// listener has bound; tests should call this only after
    /// [`Handle::wait_until_ready`] returns.
    pub fn grpc_addr(&self) -> SocketAddr {
        unimplemented!("aperture::Handle::grpc_addr — DELIVER lands this with Slice 01")
    }

    /// The address the HTTP/protobuf listener bound to.
    pub fn http_addr(&self) -> SocketAddr {
        unimplemented!("aperture::Handle::http_addr — DELIVER lands this with Slice 02")
    }

    /// Block until both listeners are bound and `/readyz` would return
    /// 200 `"ready"`. Returns an error if startup fails (port in use,
    /// probe failure, etc.).
    pub async fn wait_until_ready(&self) -> Result<()> {
        unimplemented!("aperture::Handle::wait_until_ready — DELIVER lands this with Slices 01-02")
    }

    /// Initiate graceful shutdown (equivalent to SIGTERM). Returns once
    /// the drain has completed (clean drain or deadline-exceeded).
    pub async fn shutdown(self) -> Result<()> {
        unimplemented!("aperture::Handle::shutdown — DELIVER lands this with Slice 08")
    }
}

/// Run an Aperture instance with the given configuration and sink,
/// blocking the caller until shutdown. This is the entry point
/// `main.rs` uses; tests prefer [`spawn`] so they can drive the
/// listener over the wire while still owning the instance.
pub async fn run(_config: Config, _sink: Arc<dyn OtlpSink>) -> Result<()> {
    unimplemented!("aperture::run — DELIVER lands this in compose.rs per the design contract")
}

/// Spawn an Aperture instance on the current Tokio runtime and return a
/// [`Handle`]. The integration tests use this entry point to bind to
/// ephemeral ports (`127.0.0.1:0`) and assert observable behaviour
/// against the real listeners.
pub async fn spawn(_config: Config, _sink: Arc<dyn OtlpSink>) -> Result<Handle> {
    unimplemented!("aperture::spawn — DELIVER lands this in compose.rs per the design contract")
}
