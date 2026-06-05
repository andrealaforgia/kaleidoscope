// Kaleidoscope Cinder v1 — slice 01 WAL durability acceptance test
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

//! Slice 01 — `FileBackedTieringStore::open` + `place` + `migrate`
//! survive a restart.
//!
//! Maps to `docs/feature/cinder-v1/slices/slice-01-wal-durability.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{
    FileBackedTieringStore, ItemId, MigrateError, NoopRecorder, Tier, TierPolicy, TieringStore,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn item(id: &str) -> ItemId {
    ItemId::new(id)
}

fn t(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// Build a unique temp directory path; the caller is
/// responsible for cleanup.
fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("cinder-v1-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 — open creates WAL; place appends a record
// --------------------------------------------------------------------

#[test]
fn open_creates_a_fresh_store_and_place_persists() {
    let base = temp_base("fresh_place");
    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let tn = tenant("acme");
    store
        .place(&tn, &item("a"), Tier::Hot, t(1_000))
        .expect("place");
    assert_eq!(store.get_tier(&tn, &item("a")), Some(Tier::Hot));
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.4 — restart recovers prior placements
// --------------------------------------------------------------------

#[test]
fn restart_recovers_prior_placements_and_migrations() {
    let base = temp_base("restart_recovers");
    {
        let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        let tn = tenant("acme");
        store
            .place(&tn, &item("a"), Tier::Hot, t(1_000))
            .expect("place");
        store
            .place(&tn, &item("b"), Tier::Hot, t(2_000))
            .expect("place");
        store
            .place(&tn, &item("c"), Tier::Warm, t(3_000))
            .expect("place");
        store
            .migrate(&tn, &item("a"), Tier::Warm, t(4_000))
            .expect("migrate");
        // store is dropped at end of scope, BufWriter flushes
    }

    // Reopen.
    let store2 = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let tn = tenant("acme");
    assert_eq!(store2.get_tier(&tn, &item("a")), Some(Tier::Warm));
    assert_eq!(store2.get_tier(&tn, &item("b")), Some(Tier::Hot));
    assert_eq!(store2.get_tier(&tn, &item("c")), Some(Tier::Warm));

    let entry_a = store2.get_entry(&tn, &item("a")).unwrap();
    assert_eq!(entry_a.placed_at, t(1_000));
    assert_eq!(entry_a.migrated_at, t(4_000));

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.3 — migrate on unknown item is typed error and DOES NOT
// touch the WAL (verified by reopening and observing no orphan)
// --------------------------------------------------------------------

#[test]
fn migrate_on_unknown_item_returns_typed_error_and_skips_wal() {
    let base = temp_base("migrate_unknown");
    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
    let err = store
        .migrate(&tenant("acme"), &item("ghost"), Tier::Warm, t(100))
        .unwrap_err();
    assert!(matches!(err, MigrateError::UnknownItem { .. }));
    drop(store);

    // Reopen — no orphan tier metadata.
    let store2 = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    assert!(store2.get_entry(&tenant("acme"), &item("ghost")).is_none());
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.5 — placed_at and migrated_at byte-stable across roundtrip
// --------------------------------------------------------------------

#[test]
fn timestamps_round_trip_byte_stable_across_restart() {
    let base = temp_base("timestamps");
    let placed = t(1_700_000_000);
    let migrated = t(1_700_003_600);
    {
        let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .place(&tenant("acme"), &item("x"), Tier::Hot, placed)
            .expect("place");
        store
            .migrate(&tenant("acme"), &item("x"), Tier::Warm, migrated)
            .expect("migrate");
    }
    let store2 = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let entry = store2.get_entry(&tenant("acme"), &item("x")).unwrap();
    assert_eq!(entry.placed_at, placed);
    assert_eq!(entry.migrated_at, migrated);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.6 — evaluate_at works against recovered state
// --------------------------------------------------------------------

#[test]
fn evaluate_at_works_against_recovered_state() {
    let base = temp_base("evaluate_recovered");
    {
        let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .place(&tenant("acme"), &item("a"), Tier::Hot, t(0))
            .expect("place");
        store
            .place(&tenant("acme"), &item("b"), Tier::Hot, t(0))
            .expect("place");
    }
    let store2 = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));
    let migrated = store2.evaluate_at(t(3_600), &policy).expect("evaluate");
    assert_eq!(migrated, 2);
    assert_eq!(
        store2.get_tier(&tenant("acme"), &item("a")),
        Some(Tier::Warm)
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.8 — tenant isolation across restart
// --------------------------------------------------------------------

#[test]
fn tenant_isolation_preserved_across_restart() {
    let base = temp_base("tenant_isolation");
    {
        let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .place(&tenant("acme"), &item("x"), Tier::Hot, t(100))
            .expect("place");
        store
            .place(&tenant("globex"), &item("x"), Tier::Cold, t(100))
            .expect("place");
    }
    let store2 = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    assert_eq!(
        store2.get_tier(&tenant("acme"), &item("x")),
        Some(Tier::Hot)
    );
    assert_eq!(
        store2.get_tier(&tenant("globex"), &item("x")),
        Some(Tier::Cold)
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.7 — corrupted WAL surfaces as PersistenceFailed
// --------------------------------------------------------------------

#[test]
fn corrupted_wal_surfaces_typed_persistence_error_on_open() {
    let base = temp_base("corrupted");
    // Create a valid WAL with one record, then append
    // garbage.
    {
        let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .place(&tenant("acme"), &item("x"), Tier::Hot, t(100))
            .expect("place");
    }
    // Append invalid JSON to the WAL.
    let wal_path = {
        let mut p = base.as_os_str().to_owned();
        p.push(".wal");
        PathBuf::from(p)
    };
    let existing = fs::read_to_string(&wal_path).expect("read");
    fs::write(&wal_path, format!("{existing}{{not valid json}}\n")).expect("write");

    let err = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect_err("should fail");
    assert!(matches!(err, MigrateError::PersistenceFailed { .. }));
    cleanup(&base);
}

// --------------------------------------------------------------------
// KPI 1 — place p95 ≤ 200 µs
// --------------------------------------------------------------------

#[test]
fn place_p95_latency_under_two_hundred_microseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let base = temp_base("kpi1_place");
    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let tn = tenant("perf");

    // Warm up.
    for i in 0..100u64 {
        store
            .place(&tn, &item(&format!("warm-{i}")), Tier::Hot, t(i))
            .expect("place");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1_000);
    for i in 0..1_000u64 {
        let t0 = std::time::Instant::now();
        store
            .place(&tn, &item(&format!("measure-{i}")), Tier::Hot, t(i))
            .expect("place");
        samples.push(t0.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let p95_ns = samples[950];
    let p95_us = p95_ns / 1_000;
    assert!(
        p95_us <= 200,
        "KPI 1: place p95 must be ≤ 200 µs; got {p95_us} µs ({p95_ns} ns) (first 10 ns {:?})",
        &samples[..10]
    );
    cleanup(&base);
}
