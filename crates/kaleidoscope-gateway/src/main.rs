// Kaleidoscope gateway — host composition binary
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! `kaleidoscope-gateway` — the host composition binary (DD2).
//!
//! Opens a durable [`lumen::FileBackedLogStore`] under `pillar_root`,
//! builds an [`aperture_storage_sink::StorageSink`], and starts the
//! aperture OTLP gateway with that sink injected through aperture's
//! `spawn(config, Arc<dyn OtlpSink>)` seam. The config forces
//! `sink.kind = stub` internally so aperture's composition root
//! forwards the injected sink unchanged (the only `SinkKind` it
//! forwards as-is); aperture gains no pillar dependency.
//!
//! ## Configuration (host-binary surface, DD9)
//!
//! - `pillar_root`: CLI arg 1, else `KALEIDOSCOPE_PILLAR_ROOT`, else a
//!   sensible default under the current directory.
//! - `default_tenant`: `KALEIDOSCOPE_DEFAULT_TENANT`, else fail-closed
//!   (records without a `tenant.id` resource attribute are refused).

use std::path::PathBuf;
use std::sync::Arc;

use aperture::config::Config;
use aperture::ports::{OtlpSink, Probe};
use aperture_storage_sink::{StorageSink, StorageSinkConfig};
use lumen::{FileBackedLogStore, NoopRecorder};
use pulse::{FileBackedMetricStore, NoopRecorder as PulseNoopRecorder};
use ray::{FileBackedTraceStore, NoopRecorder as RayNoopRecorder};

/// Default `pillar_root` when neither the CLI arg nor the env var is
/// set. Relative to the process working directory so a bare
/// `kaleidoscope-gateway` run in a writable cwd works out of the box.
const DEFAULT_PILLAR_ROOT: &str = "kaleidoscope-data";
/// Sub-path under `pillar_root` for the lumen log store.
const LUMEN_SUBDIR: &str = "lumen";
/// Sub-path under `pillar_root` for the ray trace store.
const RAY_SUBDIR: &str = "ray";
/// Sub-path under `pillar_root` for the pulse metric store.
const PULSE_SUBDIR: &str = "pulse";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pillar_root = resolve_pillar_root();
    let lumen_path = pillar_root.join(LUMEN_SUBDIR);
    let ray_path = pillar_root.join(RAY_SUBDIR);
    let pulse_path = pillar_root.join(PULSE_SUBDIR);

    // Ensure the pillar root exists; the `FileBacked*Store::open` calls
    // open their WAL inside this directory.
    std::fs::create_dir_all(&pillar_root)?;

    let log_store = Arc::new(FileBackedLogStore::open(
        &lumen_path,
        Box::new(NoopRecorder),
    )?);
    let trace_store = Arc::new(FileBackedTraceStore::open(
        &ray_path,
        Box::new(RayNoopRecorder),
    )?);
    let metric_store = Arc::new(FileBackedMetricStore::open(
        &pulse_path,
        Box::new(PulseNoopRecorder),
    )?);

    // Wire ALL THREE pillars (logs to lumen, traces to ray, metrics to
    // pulse) — the complete OTLP-to-durable pipeline (slice 03).
    let sink = StorageSink::with_all_stores(
        Arc::clone(&log_store),
        Arc::clone(&trace_store),
        Arc::clone(&metric_store),
        storage_sink_config(),
    );

    tracing::info!(
        event = "gateway_starting",
        pillar_root = %pillar_root.display(),
    );

    // Earned-Trust: wire → probe → use (DD5 / ADR-0041). The sink's
    // active write check runs against the real pillar_root before any
    // listener binds; a pillar_root that opens but is not writable
    // refuses startup with `event=health.startup.refused`, mirroring
    // aperture's `probe_or_refuse`.
    if let Err(e) = sink.probe().await {
        tracing::error!(event = "health.startup.refused", reason = %e);
        return Err(format!("storage sink probe failed: {e}").into());
    }

    // Force `sink.kind = stub` so aperture's composition root forwards
    // the injected StorageSink unchanged (it rebuilds the sink for
    // `SinkKind::Forwarding`, but passes Stub-kind sinks through).
    let config = Config::builder().build()?;

    let sink: Arc<dyn OtlpSink> = sink_as_dyn(sink);
    let handle = aperture::spawn(config, sink).await?;

    // Block until the operator signals shutdown, then drain. Mirrors
    // `aperture::run`'s SIGTERM/SIGINT path; the gateway delegates the
    // graceful-drain bookkeeping to the handle.
    wait_for_shutdown_signal().await;
    handle.shutdown().await?;
    Ok(())
}

/// Resolve `pillar_root` from CLI arg 1, else the
/// `KALEIDOSCOPE_PILLAR_ROOT` env var, else the default.
fn resolve_pillar_root() -> PathBuf {
    if let Some(arg) = std::env::args().nth(1) {
        return PathBuf::from(arg);
    }
    if let Ok(env_path) = std::env::var("KALEIDOSCOPE_PILLAR_ROOT") {
        return PathBuf::from(env_path);
    }
    PathBuf::from(DEFAULT_PILLAR_ROOT)
}

/// Build the sink config: a default tenant from
/// `KALEIDOSCOPE_DEFAULT_TENANT` when set, else fail-closed.
fn storage_sink_config() -> StorageSinkConfig {
    match std::env::var("KALEIDOSCOPE_DEFAULT_TENANT") {
        Ok(tenant) if !tenant.is_empty() => StorageSinkConfig::with_default_tenant(tenant),
        _ => StorageSinkConfig::no_default_tenant(),
    }
}

/// Erase the concrete sink to the `Arc<dyn OtlpSink>` aperture's spawn
/// seam takes. Extracted so the type erasure is named at one site.
fn sink_as_dyn(sink: StorageSink) -> Arc<dyn OtlpSink> {
    Arc::new(sink)
}

/// Block until the first SIGTERM or SIGINT. On non-unix targets only
/// SIGINT is observable.
async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
