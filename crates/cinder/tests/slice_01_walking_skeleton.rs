// Kaleidoscope Cinder — slice 01 walking skeleton acceptance test
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

//! Slice 01 — `TieringStore::place` + `get_tier` + `migrate`
//!
//! Maps to `docs/feature/cinder-v0/slices/slice-01-walking-skeleton.md`.

use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{InMemoryTieringStore, ItemId, MigrateError, NoopRecorder, Tier, TieringStore};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn item(id: &str) -> ItemId {
    ItemId::new(id)
}

fn t(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 — place + get_tier
// --------------------------------------------------------------------

#[test]
fn place_then_get_tier_returns_placed_tier() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    let id = item("checkout/2026-05-15/15:00");
    store
        .place(&tn, &id, Tier::Hot, t(1_000_000))
        .expect("place");
    assert_eq!(store.get_tier(&tn, &id), Some(Tier::Hot));
}

#[test]
fn place_records_timestamps() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    let id = item("x");
    store
        .place(&tn, &id, Tier::Hot, t(1_000_000))
        .expect("place");
    let entry = store.get_entry(&tn, &id).expect("present");
    assert_eq!(entry.tier, Tier::Hot);
    assert_eq!(entry.placed_at, t(1_000_000));
    assert_eq!(entry.migrated_at, t(1_000_000));
}

// --------------------------------------------------------------------
// AC-1.3 — migrate updates tier + migrated_at, preserves placed_at
// --------------------------------------------------------------------

#[test]
fn migrate_updates_tier_and_migrated_at_preserves_placed_at() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    let id = item("x");
    store
        .place(&tn, &id, Tier::Hot, t(1_000_000))
        .expect("place");
    store
        .migrate(&tn, &id, Tier::Warm, t(1_003_600))
        .expect("migrate");

    let entry = store.get_entry(&tn, &id).expect("present");
    assert_eq!(entry.tier, Tier::Warm);
    assert_eq!(entry.placed_at, t(1_000_000));
    assert_eq!(entry.migrated_at, t(1_003_600));

    // Migrate to cold.
    store
        .migrate(&tn, &id, Tier::Cold, t(1_090_000))
        .expect("migrate");
    let entry = store.get_entry(&tn, &id).expect("present");
    assert_eq!(entry.tier, Tier::Cold);
    assert_eq!(entry.placed_at, t(1_000_000));
    assert_eq!(entry.migrated_at, t(1_090_000));
}

// --------------------------------------------------------------------
// AC-1.4 — tenant isolation
// --------------------------------------------------------------------

#[test]
fn two_tenants_tier_metadata_is_isolated() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let acme = tenant("acme");
    let globex = tenant("globex");
    let id = item("shared-id");
    store.place(&acme, &id, Tier::Hot, t(100)).expect("place");
    store
        .place(&globex, &id, Tier::Cold, t(100))
        .expect("place");
    assert_eq!(store.get_tier(&acme, &id), Some(Tier::Hot));
    assert_eq!(store.get_tier(&globex, &id), Some(Tier::Cold));
}

// --------------------------------------------------------------------
// AC-1.5 — list_by_tier
// --------------------------------------------------------------------

#[test]
fn list_by_tier_returns_every_item_in_tier_for_tenant() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    store
        .place(&tn, &item("a"), Tier::Hot, t(100))
        .expect("place");
    store
        .place(&tn, &item("b"), Tier::Warm, t(100))
        .expect("place");
    store
        .place(&tn, &item("c"), Tier::Hot, t(100))
        .expect("place");
    store
        .place(&tn, &item("d"), Tier::Cold, t(100))
        .expect("place");
    // Other tenant.
    store
        .place(&tenant("other"), &item("e"), Tier::Hot, t(100))
        .expect("place");

    let mut hot = store.list_by_tier(&tn, Tier::Hot);
    hot.sort();
    assert_eq!(hot, vec![item("a"), item("c")]);
    assert_eq!(store.list_by_tier(&tn, Tier::Warm), vec![item("b")]);
    assert_eq!(store.list_by_tier(&tn, Tier::Cold), vec![item("d")]);
}

// --------------------------------------------------------------------
// AC-1.6 — unknown items return None
// --------------------------------------------------------------------

#[test]
fn unknown_item_returns_none_not_error() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    assert_eq!(store.get_tier(&tenant("acme"), &item("ghost")), None);
    assert!(store.get_entry(&tenant("acme"), &item("ghost")).is_none());
}

// --------------------------------------------------------------------
// AC-1.7 — migrate on unknown item is typed error
// --------------------------------------------------------------------

#[test]
fn migrate_on_unknown_item_returns_typed_error() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let err = store
        .migrate(&tenant("acme"), &item("ghost"), Tier::Warm, t(100))
        .unwrap_err();
    assert!(matches!(err, MigrateError::UnknownItem { .. }));
}

// --------------------------------------------------------------------
// Place is overwrite-on-collision
// --------------------------------------------------------------------

#[test]
fn place_overwrites_prior_placement_for_same_key() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    let id = item("x");
    store.place(&tn, &id, Tier::Hot, t(100)).expect("place");
    store.place(&tn, &id, Tier::Warm, t(200)).expect("place");
    let entry = store.get_entry(&tn, &id).expect("present");
    assert_eq!(entry.tier, Tier::Warm);
    assert_eq!(entry.placed_at, t(200));
    assert_eq!(entry.migrated_at, t(200));
}

// --------------------------------------------------------------------
// KPI 1 — get_tier p95 ≤ 50 µs over 10 000 placed items
// --------------------------------------------------------------------

#[test]
fn get_tier_p95_latency_under_fifty_microseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("perf");
    // Place 10 000 items.
    for i in 0..10_000u64 {
        store
            .place(
                &tn,
                &item(&format!("item-{i}")),
                Tier::Hot,
                t(1_000_000 + i),
            )
            .expect("place");
    }

    // Warm up.
    for i in 0..50u64 {
        let _ = store.get_tier(&tn, &item(&format!("item-{}", i * 50)));
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000u64 {
        // Cycle through items so we don't hit the same hot
        // cache line repeatedly.
        let id = item(&format!("item-{}", i % 10_000));
        let t0 = std::time::Instant::now();
        let _ = store.get_tier(&tn, &id);
        samples.push(t0.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let p95_ns = samples[950];
    let p95_us = p95_ns / 1_000;
    assert!(
        p95_us <= 50,
        "KPI 1: get_tier p95 must be ≤ 50 µs; got {p95_us} µs ({p95_ns} ns) (first 10 {:?} ns)",
        &samples[..10]
    );
}
