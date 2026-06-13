// Kaleidoscope log-query-api — composition-root binary
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

//! `log-query-api` — the thin composition root (ADR-0047 Decision 6).
//!
//! Reads the environment and delegates every decision to the testable
//! `log_query_api::composition` seam: it opens the durable
//! [`lumen::FileBackedLogStore`] at `pillar_root/lumen` (the same store
//! the gateway writes through), resolves the tenant from
//! `KALEIDOSCOPE_LOG_QUERY_TENANT` (fail-closed if unset or empty), runs
//! the Earned-Trust probe (wire -> probe -> use), then binds the axum
//! listener.
//!
//! ## Configuration (host-binary surface)
//!
//! - `pillar_root`: CLI arg 1, else `KALEIDOSCOPE_PILLAR_ROOT`, else a
//!   sensible default under the current directory (mirrors the gateway).
//! - `tenant`: `KALEIDOSCOPE_LOG_QUERY_TENANT`, else fail-closed (the
//!   listener refuses every request with a `status:error` body at 401).
//! - `addr`: `KALEIDOSCOPE_LOG_QUERY_ADDR`, else `0.0.0.0:9091`.

use std::process::ExitCode;
use std::sync::Arc;

use log_query_api::composition::{
    probe, resolve_addr, resolve_pillar_root, resolve_read_auth, resolve_tenant,
    startup_probe_tenant, LUMEN_SUBDIR,
};
use lumen::{FileBackedLogStore, LogStore, NoopRecorder};
use tokio::net::TcpListener;

/// Exit code for a refuse-to-start config error (ADR-0074 DD1; mirrors
/// aperture's `tls-config-reject` / ingest-auth refusal). A partial
/// read-auth config or an unreadable secret_file refuses to start with
/// this code and NO listener bound.
const EXIT_CONFIG_ERROR: u8 = 2;
/// Exit code for a non-config startup refusal (probe failure, bind
/// failure, serve-loop death). Distinct from the config-error code so an
/// operator and a black-box harness can tell a half-configured auth set
/// apart from a sick dependency.
const EXIT_STARTUP_ERROR: u8 = 1;

// The entry point blocks on `axum::serve` and cannot be unit-tested;
// every decision it makes is delegated to the mutation-killed
// `log_query_api::composition` seam, so the only mutation here is the
// unkillable "replace body with Ok(())". Skipped to keep the gate-5
// kill-rate honest rather than chasing a wiring mutant.
#[mutants::skip]
#[tokio::main]
async fn main() -> ExitCode {
    // Install the read-tier tracing subscriber FIRST, before any
    // `tracing::` call and before the earliest fallible startup steps, so
    // every lifecycle event from `log_query_api_starting` onward (and any
    // pre-bind refusal) reaches stderr (read-api-tracing-subscriber-v0,
    // DD2).
    query_http_common::init_tracing();

    let pillar_root = resolve_pillar_root(
        std::env::args().nth(1),
        std::env::var("KALEIDOSCOPE_PILLAR_ROOT").ok(),
    );
    let lumen_path = pillar_root.join(LUMEN_SUBDIR);

    // Resolve the OPTIONAL read-auth config BEFORE any filesystem or
    // socket side effect (ADR-0074 DD1). A partial config or an
    // unreadable secret_file is a refuse-to-start config error: exit 2
    // with `event=config_validation_failed` naming the missing key / the
    // offending PATH (never a secret byte), and NO listener bound. A
    // wholly absent config is the additive opt-out (env-tenant mode).
    let auth = match resolve_read_auth(
        std::env::var("KALEIDOSCOPE_LOG_QUERY_AUTH_ISSUER").ok(),
        std::env::var("KALEIDOSCOPE_LOG_QUERY_AUTH_AUDIENCE").ok(),
        std::env::var("KALEIDOSCOPE_LOG_QUERY_AUTH_SECRET_FILE").ok(),
        std::env::var("KALEIDOSCOPE_LOG_QUERY_AUTH_CATALOGUE").ok(),
    ) {
        Ok(auth) => auth,
        Err(reason) => {
            tracing::error!(event = "config_validation_failed", reason = %reason);
            return ExitCode::from(EXIT_CONFIG_ERROR);
        }
    };

    // Ensure the pillar root exists; `FileBackedLogStore::open` opens its
    // WAL inside this directory. The query binary opens the same store
    // the gateway writes through and uses only `query`.
    if let Err(reason) = std::fs::create_dir_all(&pillar_root) {
        tracing::error!(event = "health.startup.refused", reason = %reason);
        return ExitCode::from(EXIT_STARTUP_ERROR);
    }

    let store: Arc<dyn LogStore + Send + Sync> =
        match FileBackedLogStore::open(&lumen_path, Box::new(NoopRecorder)) {
            Ok(store) => Arc::new(store),
            Err(reason) => {
                tracing::error!(event = "health.startup.refused", reason = %reason);
                return ExitCode::from(EXIT_STARTUP_ERROR);
            }
        };

    let tenant = resolve_tenant(std::env::var("KALEIDOSCOPE_LOG_QUERY_TENANT").ok());

    tracing::info!(
        event = "log_query_api_starting",
        pillar_root = %pillar_root.display(),
        tenant_resolved = tenant.is_some(),
        auth_enabled = auth.is_some(),
    );

    // Earned-Trust: wire -> probe -> use (ADR-0047 Decision 6). Refuse a
    // half-up listener: the store must be readable before any socket
    // binds. In auth mode the per-request tenant comes from the bearer
    // (ADR-0074 DD3 arm 1), so the probe runs under a synthetic sentinel
    // tenant and binds even with the env tenant unset; in env-tenant mode
    // the existing fail-closed behaviour is preserved (an unset env tenant
    // refuses).
    let probe_tenant = startup_probe_tenant(auth.is_some(), tenant.clone());
    if let Err(reason) = probe(store.as_ref(), probe_tenant.as_ref()) {
        tracing::error!(event = "health.startup.refused", reason = %reason);
        return ExitCode::from(EXIT_STARTUP_ERROR);
    }

    let addr = match resolve_addr(std::env::var("KALEIDOSCOPE_LOG_QUERY_ADDR").ok()) {
        Ok(addr) => addr,
        Err(reason) => {
            tracing::error!(event = "health.startup.refused", reason = %reason);
            return ExitCode::from(EXIT_STARTUP_ERROR);
        }
    };
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(reason) => {
            tracing::error!(event = "health.startup.refused", reason = %reason);
            return ExitCode::from(EXIT_STARTUP_ERROR);
        }
    };
    let bound = match listener.local_addr() {
        Ok(bound) => bound,
        Err(reason) => {
            tracing::error!(event = "health.startup.refused", reason = %reason);
            return ExitCode::from(EXIT_STARTUP_ERROR);
        }
    };
    tracing::info!(event = "listener_bound", transport = "http", addr = %bound);

    let app = log_query_api::router_with_auth(store, tenant, auth);
    if let Err(reason) = axum::serve(listener, app).await {
        tracing::error!(event = "serve_loop_failed", reason = %reason);
        return ExitCode::from(EXIT_STARTUP_ERROR);
    }
    ExitCode::SUCCESS
}
