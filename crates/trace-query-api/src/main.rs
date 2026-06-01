// Kaleidoscope trace-query-api — composition-root binary
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

//! `trace-query-api` — the thin composition root (ADR-0048 Decision 8).
//!
//! Reads the environment and delegates every decision to the testable
//! `trace_query_api::composition` seam: it opens the durable
//! [`ray::FileBackedTraceStore`] at `pillar_root/ray` (the same store
//! the aperture trace path writes through), resolves the tenant from
//! `KALEIDOSCOPE_TRACE_QUERY_TENANT` (fail-closed if unset or empty),
//! runs the Earned-Trust probe (wire -> probe -> use), then binds the
//! axum listener.
//!
//! ## Configuration (host-binary surface)
//!
//! - `pillar_root`: CLI arg 1, else `KALEIDOSCOPE_PILLAR_ROOT`, else a
//!   sensible default under the current directory (mirrors the
//!   gateway and the sibling read APIs).
//! - `tenant`: `KALEIDOSCOPE_TRACE_QUERY_TENANT`, else fail-closed (the
//!   listener refuses every request with a `status:error` body at
//!   401).
//! - `addr`: `KALEIDOSCOPE_TRACE_QUERY_ADDR`, else `0.0.0.0:9092`.

use std::sync::Arc;

use ray::{FileBackedTraceStore, NoopRecorder, TraceStore};
use tokio::net::TcpListener;
use trace_query_api::composition::{
    probe, resolve_addr, resolve_pillar_root, resolve_tenant, RAY_SUBDIR,
};

// The entry point blocks on `axum::serve` and cannot be unit-tested;
// every decision it makes is delegated to the mutation-killed
// `trace_query_api::composition` seam, so the only mutation here is
// the unkillable "replace body with Ok(())". Skipped to keep the
// gate-5 kill-rate honest rather than chasing a wiring mutant.
#[mutants::skip]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install the read-tier tracing subscriber FIRST, before any
    // `tracing::` call and before the earliest fallible startup steps, so
    // every lifecycle event from `trace_query_api_starting` onward
    // reaches stderr (read-api-tracing-subscriber-v0, DD2). NO-OP at
    // DISTILL close (the helper body is a scaffold); DELIVER fills it.
    query_http_common::init_tracing();

    let pillar_root = resolve_pillar_root(
        std::env::args().nth(1),
        std::env::var("KALEIDOSCOPE_PILLAR_ROOT").ok(),
    );
    let ray_path = pillar_root.join(RAY_SUBDIR);

    // Ensure the pillar root exists; `FileBackedTraceStore::open` opens
    // its WAL inside this directory. The query binary opens the same
    // store the aperture trace path writes through and uses only
    // `query`.
    std::fs::create_dir_all(&pillar_root)?;

    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FileBackedTraceStore::open(
        &ray_path,
        Box::new(NoopRecorder),
    )?);

    let tenant = resolve_tenant(std::env::var("KALEIDOSCOPE_TRACE_QUERY_TENANT").ok());

    tracing::info!(
        event = "trace_query_api_starting",
        pillar_root = %pillar_root.display(),
        tenant_resolved = tenant.is_some(),
    );

    // Earned-Trust: wire -> probe -> use (ADR-0048 Decision 8). Refuse
    // a half-up listener: a tenant must resolve AND the store must be
    // readable before any socket binds.
    if let Err(reason) = probe(store.as_ref(), tenant.as_ref()) {
        tracing::error!(event = "health.startup.refused", reason = %reason);
        return Err(reason.into());
    }

    let addr = resolve_addr(std::env::var("KALEIDOSCOPE_TRACE_QUERY_ADDR").ok())?;
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tracing::info!(event = "listener_bound", transport = "http", addr = %bound);

    let app = trace_query_api::router(store, tenant);
    axum::serve(listener, app).await?;
    Ok(())
}
