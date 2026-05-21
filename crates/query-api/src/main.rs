// Kaleidoscope query-api — composition-root binary
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

//! `query-api` — the thin composition root (DD1 / DD8 / DD9).
//!
//! Reads the environment and delegates every decision to the testable
//! `query_api::composition` seam: it opens the durable
//! [`pulse::FileBackedMetricStore`] at `pillar_root/pulse` (the same
//! store the gateway writes through), resolves the tenant from
//! `KALEIDOSCOPE_QUERY_TENANT` (fail-closed if unset or empty), runs the
//! Earned-Trust probe (wire -> probe -> use), then binds the axum
//! listener.
//!
//! ## Configuration (host-binary surface)
//!
//! - `pillar_root`: CLI arg 1, else `KALEIDOSCOPE_PILLAR_ROOT`, else a
//!   sensible default under the current directory (mirrors the gateway).
//! - `tenant`: `KALEIDOSCOPE_QUERY_TENANT`, else fail-closed (the
//!   listener refuses every request with a `status:error` body).
//! - `addr`: `KALEIDOSCOPE_QUERY_ADDR`, else `0.0.0.0:9090` — the
//!   conventional Prometheus HTTP API port.

use std::sync::Arc;

use pulse::{FileBackedMetricStore, MetricStore, NoopRecorder};
use query_api::composition::{
    probe, resolve_addr, resolve_pillar_root, resolve_tenant, PULSE_SUBDIR,
};
use tokio::net::TcpListener;

// The entry point blocks on `axum::serve` and cannot be unit-tested;
// every decision it makes is delegated to the mutation-killed
// `query_api::composition` seam, so the only mutation here is the
// unkillable "replace body with Ok(())". Skipped to keep the gate-5
// kill-rate honest rather than chasing a wiring mutant.
#[mutants::skip]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pillar_root = resolve_pillar_root(
        std::env::args().nth(1),
        std::env::var("KALEIDOSCOPE_PILLAR_ROOT").ok(),
    );
    let pulse_path = pillar_root.join(PULSE_SUBDIR);

    // Ensure the pillar root exists; `FileBackedMetricStore::open` opens
    // its WAL inside this directory. The query binary opens the same
    // store the gateway writes through and uses only `query` (DD8).
    std::fs::create_dir_all(&pillar_root)?;

    let store: Arc<dyn MetricStore + Send + Sync> = Arc::new(FileBackedMetricStore::open(
        &pulse_path,
        Box::new(NoopRecorder),
    )?);

    let tenant = resolve_tenant(std::env::var("KALEIDOSCOPE_QUERY_TENANT").ok());

    tracing::info!(
        event = "query_api_starting",
        pillar_root = %pillar_root.display(),
        tenant_resolved = tenant.is_some(),
    );

    // Earned-Trust: wire -> probe -> use (DD9 / ADR-0042). Refuse a
    // half-up listener: a tenant must resolve AND the store must be
    // readable before any socket binds.
    if let Err(reason) = probe(store.as_ref(), tenant.as_ref()) {
        tracing::error!(event = "health.startup.refused", reason = %reason);
        return Err(reason.into());
    }

    let addr = resolve_addr(std::env::var("KALEIDOSCOPE_QUERY_ADDR").ok())?;
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tracing::info!(event = "listener_bound", transport = "http", addr = %bound);

    let app = query_api::router(store, tenant);
    axum::serve(listener, app).await?;
    Ok(())
}
