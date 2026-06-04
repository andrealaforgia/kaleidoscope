// Kaleidoscope Pulse — out-of-process crash target (kill-target helper)
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

//! Kill-target helper binary for pulse's snapshot-atomicity proving
//! (mechanism (a), ADR-0060 §1, C5). pulse is SNAPSHOT-ONLY — its WAL is
//! already crash-durable under ADR-0049 (per-record `sync_all`), so it has
//! NO wal-fsync AC and the process-kill is its only proving mechanism. The
//! snapshot-atomicity acceptance suite
//! (`tests/v1_slice_06_snapshot_atomicity.rs`) spawns THIS binary as a real
//! child PROCESS (`std::process::Command`), lets it ack a metric ingest,
//! then `SIGKILL`s it WHILE it is writing a snapshot — the out-of-process
//! true crash ADR-0049 §3/alt-A RESERVED. The parent then reopens the store
//! and asserts the crash-at-ANY-point invariant (the canonical snapshot path
//! holds the OLD or NEW whole snapshot, never a torn one) and that `open()`
//! succeeds, serving the acked `acme`/`http_requests_total` series.
//!
//! Contract (the parent test drives these argv/env):
//!   - reads pillar root from `$KALEIDOSCOPE_CRASH_PILLAR_ROOT`; the store
//!     lives at `<root>/store` (the parent's `temp_base` convention).
//!   - mode `--seed-then-loop-snapshot`: open the store, ingest an acked
//!     `http_requests_total` gauge point for tenant `acme` (the series the
//!     parent later queries for), print the readiness sentinel line
//!     `CRASH_TARGET_READY` to stdout (so the parent kills at a controlled
//!     moment), then loop calling `snapshot()` forever so a kill lands
//!     mid-snapshot.
//!
//! The binary writes ONLY under the tmp pillar root the parent hands it,
//! never a fixed path, so concurrent runs and the clean+ci environments do
//! not collide.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use aegis::TenantId;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder,
};

/// The acked series the parent later queries for: tenant `acme`,
/// metric `http_requests_total`.
const TENANT: &str = "acme";
const METRIC: &str = "http_requests_total";

fn pillar_root() -> PathBuf {
    let root = std::env::var_os("KALEIDOSCOPE_CRASH_PILLAR_ROOT")
        .expect("KALEIDOSCOPE_CRASH_PILLAR_ROOT must be set by the parent test");
    PathBuf::from(root)
}

/// The store base path: `<pillar_root>/store`, matching the parent's
/// `temp_base` convention (`base = <root>/store`).
fn store_base() -> PathBuf {
    pillar_root().join("store")
}

fn acked_gauge() -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Metric {
        name: MetricName::new(METRIC),
        description: "seeded gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points: vec![MetricPoint {
            time_unix_nano: 100,
            start_time_unix_nano: 0,
            attributes: BTreeMap::new(),
            value: 1.0,
        }],
        resource_attributes: resource,
    }
}

fn seed_then_loop_snapshot() -> ExitCode {
    let base = store_base();
    if let Some(parent) = base.parent() {
        std::fs::create_dir_all(parent).expect("create pillar root");
    }

    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder))
        .expect("open the store for seeding");
    store
        .ingest(
            &TenantId(TENANT.to_string()),
            MetricBatch::with_metrics(vec![acked_gauge()]),
        )
        .expect("ingest acks the metric");

    // Signal readiness AFTER the acked write is durable, so the parent's
    // SIGKILL lands while the loop below is writing snapshots — never
    // before the ingest is on stable storage.
    let mut stdout = std::io::stdout();
    writeln!(stdout, "CRASH_TARGET_READY").expect("emit readiness sentinel");
    stdout.flush().expect("flush readiness sentinel");

    // Loop writing snapshots forever so the kill lands mid-snapshot. Each
    // snapshot is atomic (tmp+fsync+rename+fsync-dir), so a kill at ANY
    // instant leaves the canonical path whole-or-absent, never torn.
    loop {
        store.snapshot().expect("snapshot");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");

    match mode {
        "--seed-then-loop-snapshot" => seed_then_loop_snapshot(),
        other => {
            eprintln!("unknown crash-target mode: {other:?}");
            ExitCode::FAILURE
        }
    }
}
