// Kaleidoscope consolidated runtime — bin `kaleidoscope`
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

//! `kaleidoscope` — the consolidated runtime binary (ADR-0076).
//!
//! A THIN composition root: resolve the host-binary surface (pillar root,
//! the unified `KALEIDOSCOPE_TENANT` with per-role overrides, the fixed
//! default ports, the optional read-auth set) from the environment, then hand
//! it to [`kaleidoscope_runtime::spawn_consolidated`] which owns the
//! shared-`Arc` composition. The binary itself carries NO store, NO router,
//! NO domain logic — only env plumbing and the shutdown wait.
//!
//! ## DISTILL scaffold state (RED-not-BROKEN, Mandate 7)
//!
//! `spawn_consolidated` is a `__SCAFFOLD__` panic until DELIVER, so running
//! this binary panics. That is the binary's RED state; `cargo test --workspace
//! --all-targets` only COMPILES the bin (it never runs `main`), so the bin
//! contributes a clean compile, not a failing test. DELIVER fills the lib
//! composition and this binary becomes the one-command consolidated runtime.

use std::net::SocketAddr;
use std::path::PathBuf;

use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};

/// Default pillar root when neither CLI arg 1 nor `KALEIDOSCOPE_PILLAR_ROOT`
/// is set, mirroring the gateway.
const DEFAULT_PILLAR_ROOT: &str = "kaleidoscope-data";

// The entry point resolves the environment and then blocks on the
// shutdown-signal wait; it cannot be unit-tested and every composition
// decision is delegated to the mutation-killed `spawn_consolidated` seam,
// so the only mutation here is the unkillable "replace body with Ok(())".
// Skipped to keep the gate-5 kill-rate honest (mirrors the sibling binaries).
#[mutants::skip]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // One shared JSON-to-stderr tracing subscriber (idempotent try_init);
    // `spawn_consolidated` installs the same subscriber as its first
    // statement, but installing here too keeps the bin's early lifecycle
    // emissions (the fail-closed `health.startup.refused` arm) rendering
    // even if the composition contract changes. aperture's + the read
    // tier's later installs no-op (ADR-0015/0009).
    query_http_common::init_tracing();

    let config = config_from_env();

    let runtime: RunningRuntime = match spawn_consolidated(config).await {
        Ok(runtime) => runtime,
        Err(e) => {
            // Fail-closed startup: any bind/probe failure refuses to start
            // (event=health.startup.refused), non-zero exit, no half-up
            // process (ADR-0076 DD3).
            tracing::error!(event = "health.startup.refused", reason = %e);
            return Err(format!("consolidated runtime startup refused: {e}").into());
        }
    };

    wait_for_shutdown_signal().await;
    runtime.shutdown().await?;
    Ok(())
}

/// Resolve the host-binary surface from the environment into a
/// [`ConsolidatedConfig`]. The unified `KALEIDOSCOPE_TENANT` drives all four
/// roles; per-role vars override it (precedence: per-role > unified > unset).
/// The five listeners take the documented fixed default ports; DELIVER wires
/// the per-listener address env knobs (`KALEIDOSCOPE_QUERY_ADDR` etc.).
///
/// Mutation posture: this and the helpers below read process-global env /
/// CLI args or block on a signal, so they are bin-level env-plumbing that
/// cannot be exercised by the in-process acceptance suite (which calls
/// [`spawn_consolidated`] directly). They carry the same `#[mutants::skip]`
/// posture as the sibling binaries; the testable composition surface is
/// `spawn_consolidated` in the library, which the gate-5 run covers at 100%.
#[mutants::skip]
fn config_from_env() -> ConsolidatedConfig {
    let pillar_root = resolve_pillar_root();
    let unified = non_empty_env("KALEIDOSCOPE_TENANT");
    let fixed = |port: u16| SocketAddr::from(([0, 0, 0, 0], port));

    ConsolidatedConfig {
        pillar_root,
        ingest_grpc_addr: fixed(4317),
        ingest_http_addr: fixed(4318),
        metrics_query_addr: fixed(9090),
        logs_query_addr: fixed(9091),
        traces_query_addr: fixed(9092),
        default_ingest_tenant: non_empty_env("KALEIDOSCOPE_DEFAULT_TENANT")
            .or_else(|| unified.clone()),
        metrics_query_tenant: non_empty_env("KALEIDOSCOPE_QUERY_TENANT")
            .or_else(|| unified.clone()),
        logs_query_tenant: non_empty_env("KALEIDOSCOPE_LOG_QUERY_TENANT")
            .or_else(|| unified.clone()),
        traces_query_tenant: non_empty_env("KALEIDOSCOPE_TRACE_QUERY_TENANT")
            .or_else(|| unified.clone()),
        // Auth off by default, never removed; DELIVER builds the validator
        // from the `KALEIDOSCOPE_*_QUERY_AUTH_*` set when present (ADR-0074),
        // refusing to start on a partial config (exit 2).
        read_auth: None,
        static_dir: non_empty_env("KALEIDOSCOPE_QUERY_STATIC_DIR").map(PathBuf::from),
        // Production serves the always-current demo via the read-side overlay
        // (ADR-0079): ON for a non-empty, never-staling first look with no seed.
        // Operator off-switch (KALEIDOSCOPE_DEMO_OVERLAY=0/false) for a staged
        // cutover or a raw-only instance; defaults ON.
        demo_overlay_enabled: demo_overlay_from_env(),
    }
}

/// Read the demo-overlay off-switch (ADR-0079): ON unless
/// `KALEIDOSCOPE_DEMO_OVERLAY` is explicitly `0`/`false`. Pure env-plumbing,
/// skipped from mutation like the other env readers.
#[mutants::skip]
fn demo_overlay_from_env() -> bool {
    !matches!(
        std::env::var("KALEIDOSCOPE_DEMO_OVERLAY"),
        Ok(v) if v == "0" || v.eq_ignore_ascii_case("false")
    )
}

#[mutants::skip]
fn resolve_pillar_root() -> PathBuf {
    if let Some(arg) = std::env::args().nth(1) {
        return PathBuf::from(arg);
    }
    if let Some(env_path) = non_empty_env("KALEIDOSCOPE_PILLAR_ROOT") {
        return PathBuf::from(env_path);
    }
    PathBuf::from(DEFAULT_PILLAR_ROOT)
}

/// Read an env var, treating unset OR empty as `None`.
#[mutants::skip]
fn non_empty_env(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Block until the first SIGTERM or SIGINT, mirroring the gateway.
#[mutants::skip]
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
