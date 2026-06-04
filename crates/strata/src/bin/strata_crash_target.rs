// Kaleidoscope Strata — out-of-process crash target (kill-target helper)
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
//! proving (ADR-0060 §1, C5). The strata crash-durability acceptance suite
//! (`tests/v1_slice_03_crash_durability.rs`) spawns THIS binary as a real
//! child PROCESS (`std::process::Command`), lets it ack a profile, then
//! `SIGKILL`s it WHILE it is writing a snapshot — the out-of-process true
//! crash. The parent then reopens the store and asserts the crash-at-ANY-
//! point invariant (canonical path holds the OLD or NEW whole snapshot,
//! never a torn one) and that `open()` succeeds.
//!
//! Contract (the parent test drives these argv/env):
//!   - reads pillar root from `$KALEIDOSCOPE_CRASH_PILLAR_ROOT`; the store
//!     lives at `<root>/store` (the parent's `temp_base` convention).
//!   - mode `--seed-then-loop-snapshot`: open the store, ingest the acked
//!     profile (tenant `acme`, service `payment-svc`), print the readiness
//!     sentinel line `CRASH_TARGET_READY` to stdout (so the parent kills at
//!     a controlled moment), then loop calling `snapshot()` forever so a
//!     kill lands mid-snapshot.
//!   - mode `--open-then-idle`: open an empty store, print readiness, then
//!     idle (the empty-store boundary: a pre-write crash must leave a store
//!     that reopens cleanly with no spurious parse error).
//!   - mode `--probe-lying`: drive the composition root with a
//!     `LyingFsyncBackend`; emit `event=health.startup.refused
//!     substrate=<descriptor>` to stderr and exit non-zero WITHOUT opening
//!     the store for writes (AC-substrate-refusal, mechanism (b) variant).
//!
//! The binary writes ONLY under the tmp pillar root the parent hands it,
//! never a fixed path, so concurrent runs and the clean+ci environments do
//! not collide.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use aegis::TenantId;
use strata::{
    fsync_probe, FileBackedProfileStore, Function, Location, LyingFsyncBackend, Mapping,
    NoopRecorder, Profile, ProfileBatch, ProfileStore, Sample, SampleType, ValueType,
};

/// The tenant the parent later queries for. The acked profile carries
/// service `payment-svc`, the fixed convention the acceptance suite
/// asserts against.
const TENANT: &str = "acme";

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

fn seed_profile() -> Profile {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "payment-svc".to_string());
    Profile {
        time_unix_nano: 100,
        duration_nanos: 10_000_000,
        profile_type: "cpu".to_string(),
        sample_type: vec![SampleType {
            value_type: ValueType {
                type_index: 1,
                unit_index: 2,
            },
            aggregation_temporality: 1,
        }],
        samples: vec![Sample {
            location_ids: vec![1, 2],
            values: vec![100],
            attributes: BTreeMap::new(),
        }],
        locations: vec![Location {
            id: 1,
            mapping_id: 1,
            address: 0x1000,
            function_ids: vec![1],
        }],
        functions: vec![Function {
            id: 1,
            name_index: 3,
            system_name_index: 3,
            filename_index: 4,
            start_line: 10,
        }],
        mappings: vec![Mapping {
            id: 1,
            memory_start: 0x1000,
            memory_limit: 0x2000,
            file_offset: 0,
            filename_index: 4,
            build_id_index: 5,
        }],
        string_table: vec![
            "".to_string(),
            "samples".to_string(),
            "count".to_string(),
            "main".to_string(),
            "main.go".to_string(),
            "build-abc123".to_string(),
        ],
        resource_attributes: resource,
        attributes: BTreeMap::new(),
    }
}

fn signal_ready() {
    let mut stdout = std::io::stdout();
    writeln!(stdout, "CRASH_TARGET_READY").expect("emit readiness sentinel");
    stdout.flush().expect("flush readiness sentinel");
}

fn seed_then_loop_snapshot() -> ExitCode {
    let base = store_base();
    if let Some(parent) = base.parent() {
        std::fs::create_dir_all(parent).expect("create pillar root");
    }

    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder))
        .expect("open the store for seeding");
    store
        .ingest(
            &TenantId(TENANT.to_string()),
            ProfileBatch::with_profiles(vec![seed_profile()]),
        )
        .expect("ingest acks the profile");

    // Signal readiness AFTER the acked write is durable, so the parent's
    // SIGKILL lands while the loop below is writing snapshots — never
    // before the profile is on stable storage.
    signal_ready();

    // Loop writing snapshots forever so the kill lands mid-snapshot. Each
    // snapshot is atomic (tmp+fsync+rename+fsync-dir), so a kill at ANY
    // instant leaves the canonical path whole-or-absent, never torn.
    loop {
        store.snapshot().expect("snapshot");
    }
}

fn open_then_idle() -> ExitCode {
    let base = store_base();
    if let Some(parent) = base.parent() {
        std::fs::create_dir_all(parent).expect("create pillar root");
    }

    // Open an EMPTY store (no ingest). The empty-store boundary: a crash
    // before any write must still leave a store that reopens cleanly.
    let _store =
        FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open an empty store");

    // Signal readiness, then idle so the parent's SIGKILL lands on an
    // empty, never-written store.
    signal_ready();

    loop {
        std::thread::sleep(Duration::from_secs(3600));
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
        "--open-then-idle" => open_then_idle(),
        "--probe-lying" => probe_lying(),
        other => {
            eprintln!("unknown crash-target mode: {other:?}");
            ExitCode::FAILURE
        }
    }
}
