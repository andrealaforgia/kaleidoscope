// Kaleidoscope Cinder — out-of-process crash target (kill-target helper)
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

//! Kill-target helper binary for mechanism (a) — snapshot-atomicity
//! proving (ADR-0060 §1, C5). The cinder crash-durability acceptance suite
//! (`tests/v1_slice_04_crash_durability.rs`) spawns THIS binary as a real
//! child PROCESS (`std::process::Command`), lets it ack a migration, then
//! `SIGKILL`s it WHILE it is writing a snapshot — the out-of-process true
//! crash ADR-0049 §3/alt-A RESERVED. The parent then reopens the store and
//! asserts the crash-at-ANY-point invariant (canonical path holds the OLD
//! or NEW whole snapshot, never a torn one) and that `open()` succeeds with
//! the acked hot-to-warm migration of `blk-7781` in the recovered ledger.
//!
//! Contract (the parent test drives these argv/env):
//!   - reads pillar root from `$KALEIDOSCOPE_CRASH_PILLAR_ROOT`; the store
//!     lives at `<root>/store` (the parent's `temp_base` convention).
//!   - mode `--seed-then-loop-snapshot`: open the store, place item
//!     `blk-7781` (tenant `acme`) Hot then migrate it to Warm (the acked
//!     move the parent later queries for), print the readiness sentinel
//!     line `CRASH_TARGET_READY` to stdout (so the parent kills at a
//!     controlled moment), then loop calling `snapshot()` forever so a
//!     kill lands mid-snapshot.
//!   - mode `--probe-lying`: drive the composition root with a
//!     `LyingFsyncBackend`; emit `event=health.startup.refused
//!     substrate=<descriptor>` to stderr and exit non-zero WITHOUT opening
//!     the store for writes (AC-substrate-refusal, mechanism (b) variant).
//!
//! The binary writes ONLY under the tmp pillar root the parent hands it,
//! never a fixed path, so concurrent runs and the clean+ci environments do
//! not collide.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::SystemTime;

use aegis::TenantId;
use cinder::{
    fsync_probe, FileBackedTieringStore, ItemId, LyingFsyncBackend, NoopRecorder, Tier,
    TieringStore,
};

/// The acked migration the parent later queries for. Tenant `acme`, item
/// `blk-7781`, hot-to-warm — the fixed convention the acceptance suite
/// asserts against.
const TENANT: &str = "acme";
const ITEM: &str = "blk-7781";

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

fn seed_then_loop_snapshot() -> ExitCode {
    let base = store_base();
    if let Some(parent) = base.parent() {
        std::fs::create_dir_all(parent).expect("create pillar root");
    }

    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder))
        .expect("open the store for seeding");
    let tenant = TenantId(TENANT.to_string());
    let item = ItemId(ITEM.to_string());
    store
        .place(&tenant, &item, Tier::Hot, SystemTime::UNIX_EPOCH)
        .expect("place acks the hot seed");
    store
        .migrate(&tenant, &item, Tier::Warm, SystemTime::UNIX_EPOCH)
        .expect("migrate acks the hot-to-warm move");

    // Signal readiness AFTER the acked write is durable, so the parent's
    // SIGKILL lands while the loop below is writing snapshots — never
    // before the migration is on stable storage.
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

fn probe_lying() -> ExitCode {
    let root = pillar_root();
    std::fs::create_dir_all(&root).expect("create pillar root");

    // Drive the composition-root discipline: probe the substrate BEFORE
    // opening the store for writes. A lying substrate is refused here, so
    // no write is ever acked against a substrate proven to lie.
    let backend = LyingFsyncBackend::no_op();
    match fsync_probe(&root, &backend) {
        Ok(()) => {
            // Should never happen with a lying substrate; if it did, the
            // contract (refuse) would be violated, so exit non-zero too.
            eprintln!("event=health.startup.refused substrate=fsync-unexpected-pass");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!(
                "event=health.startup.refused substrate={}",
                error.substrate_descriptor()
            );
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");

    match mode {
        "--seed-then-loop-snapshot" => seed_then_loop_snapshot(),
        "--probe-lying" => probe_lying(),
        other => {
            eprintln!("unknown crash-target mode: {other:?}");
            ExitCode::FAILURE
        }
    }
}
