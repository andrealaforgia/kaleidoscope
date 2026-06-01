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

use std::sync::Arc;

use log_query_api::composition::{
    probe, resolve_addr, resolve_pillar_root, resolve_tenant, LUMEN_SUBDIR,
};
use lumen::{FileBackedLogStore, LogStore, NoopRecorder};
use tokio::net::TcpListener;

// The entry point blocks on `axum::serve` and cannot be unit-tested;
// every decision it makes is delegated to the mutation-killed
// `log_query_api::composition` seam, so the only mutation here is the
// unkillable "replace body with Ok(())". Skipped to keep the gate-5
// kill-rate honest rather than chasing a wiring mutant.
#[mutants::skip]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install the read-tier tracing subscriber FIRST, before any
    // `tracing::` call and before the earliest fallible startup steps, so
    // every lifecycle event from `log_query_api_starting` onward reaches
    // stderr (read-api-tracing-subscriber-v0, DD2). NO-OP at DISTILL
    // close (the helper body is a scaffold); DELIVER fills the body.
    query_http_common::init_tracing();

    let pillar_root = resolve_pillar_root(
        std::env::args().nth(1),
        std::env::var("KALEIDOSCOPE_PILLAR_ROOT").ok(),
    );
    let lumen_path = pillar_root.join(LUMEN_SUBDIR);

    // Ensure the pillar root exists; `FileBackedLogStore::open` opens its
    // WAL inside this directory. The query binary opens the same store
    // the gateway writes through and uses only `query`.
    std::fs::create_dir_all(&pillar_root)?;

    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(FileBackedLogStore::open(
        &lumen_path,
        Box::new(NoopRecorder),
    )?);

    let tenant = resolve_tenant(std::env::var("KALEIDOSCOPE_LOG_QUERY_TENANT").ok());

    tracing::info!(
        event = "log_query_api_starting",
        pillar_root = %pillar_root.display(),
        tenant_resolved = tenant.is_some(),
    );

    // Earned-Trust: wire -> probe -> use (ADR-0047 Decision 6). Refuse a
    // half-up listener: a tenant must resolve AND the store must be
    // readable before any socket binds.
    if let Err(reason) = probe(store.as_ref(), tenant.as_ref()) {
        tracing::error!(event = "health.startup.refused", reason = %reason);
        return Err(reason.into());
    }

    let addr = resolve_addr(std::env::var("KALEIDOSCOPE_LOG_QUERY_ADDR").ok())?;
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tracing::info!(event = "listener_bound", transport = "http", addr = %bound);

    let app = log_query_api::router(store, tenant);
    axum::serve(listener, app).await?;
    Ok(())
}
