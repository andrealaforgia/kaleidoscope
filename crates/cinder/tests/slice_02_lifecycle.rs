// Kaleidoscope Cinder — slice 02 lifecycle policy acceptance test
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

//! Slice 02 — age-based lifecycle policy
//!
//! Maps to `docs/feature/cinder-v0/slices/slice-02-lifecycle.md`.

use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{
    CapturingRecorder, InMemoryTieringStore, ItemId, NoopRecorder, RecordedEvent, Tier, TierPolicy,
    TieringStore,
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

// --------------------------------------------------------------------
// AC-2.1 / AC-2.2 — TierPolicy::age_based + evaluate_at returns count
// --------------------------------------------------------------------

#[test]
fn evaluate_at_returns_count_of_migrated_items() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    // Place 3 items in Hot at t=0.
    for id in ["a", "b", "c"] {
        store.place(&tn, &item(id), Tier::Hot, t(0));
    }
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));
    // Evaluate at t=3600 (= hot_to_warm threshold).
    let migrated = store.evaluate_at(t(3600), &policy);
    assert_eq!(migrated, 3);
}

// --------------------------------------------------------------------
// AC-2.3 — Hot → Warm at hot_to_warm threshold
// --------------------------------------------------------------------

#[test]
fn hot_items_move_to_warm_when_age_exceeds_threshold() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    store.place(&tn, &item("young"), Tier::Hot, t(0));
    store.place(&tn, &item("old"), Tier::Hot, t(0));
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));
    // At t=1800 nothing migrates (under threshold).
    let migrated = store.evaluate_at(t(1800), &policy);
    assert_eq!(migrated, 0);
    assert_eq!(store.get_tier(&tn, &item("young")), Some(Tier::Hot));

    // At t=3600 both items migrate.
    let migrated = store.evaluate_at(t(3600), &policy);
    assert_eq!(migrated, 2);
    assert_eq!(store.get_tier(&tn, &item("young")), Some(Tier::Warm));
    assert_eq!(store.get_tier(&tn, &item("old")), Some(Tier::Warm));
}

// --------------------------------------------------------------------
// AC-2.4 — Warm → Cold at warm_to_cold threshold
// --------------------------------------------------------------------

#[test]
fn warm_items_move_to_cold_when_age_exceeds_threshold() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    store.place(&tn, &item("a"), Tier::Warm, t(0));
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));
    let migrated = store.evaluate_at(t(86_400), &policy);
    assert_eq!(migrated, 1);
    assert_eq!(store.get_tier(&tn, &item("a")), Some(Tier::Cold));
}

// --------------------------------------------------------------------
// AC-2.5 — Cold items do not move automatically
// --------------------------------------------------------------------

#[test]
fn cold_items_do_not_move_automatically() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    store.place(&tn, &item("ancient"), Tier::Cold, t(0));
    let policy = TierPolicy::age_based(Duration::from_secs(60), Duration::from_secs(120));
    // Even with very large `now`, cold stays cold.
    let migrated = store.evaluate_at(t(10_000_000), &policy);
    assert_eq!(migrated, 0);
    assert_eq!(store.get_tier(&tn, &item("ancient")), Some(Tier::Cold));
}

// --------------------------------------------------------------------
// AC-2.6 — Idempotence under repeated evaluate_at(now, ...)
// --------------------------------------------------------------------

#[test]
fn evaluate_at_is_idempotent_under_repeated_invocation() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("acme");
    store.place(&tn, &item("a"), Tier::Hot, t(0));
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));
    let first = store.evaluate_at(t(3600), &policy);
    assert_eq!(first, 1);

    // Same now ⇒ no migration (item just moved at t=3600,
    // so migrated_at = 3600, age relative to new tier is 0).
    let second = store.evaluate_at(t(3600), &policy);
    assert_eq!(second, 0);

    // At t=89_999 (just before warm_to_cold from now's
    // perspective): age since migrated_at=3600 is 86_399 <
    // 86_400 threshold ⇒ no migration.
    let third = store.evaluate_at(t(89_999), &policy);
    assert_eq!(third, 0);

    // At t=90_000 it migrates to Cold.
    let fourth = store.evaluate_at(t(90_000), &policy);
    assert_eq!(fourth, 1);
    assert_eq!(store.get_tier(&tn, &item("a")), Some(Tier::Cold));
}

// --------------------------------------------------------------------
// AC-2.7 — per-tenant evaluation
// --------------------------------------------------------------------

#[test]
fn evaluate_at_evaluates_each_tenant_independently() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let acme = tenant("acme");
    let globex = tenant("globex");
    store.place(&acme, &item("a"), Tier::Hot, t(0));
    store.place(&globex, &item("a"), Tier::Hot, t(0));
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));
    let migrated = store.evaluate_at(t(3600), &policy);
    assert_eq!(migrated, 2);
    assert_eq!(store.get_tier(&acme, &item("a")), Some(Tier::Warm));
    assert_eq!(store.get_tier(&globex, &item("a")), Some(Tier::Warm));
}

// --------------------------------------------------------------------
// MetricsRecorder seam
// --------------------------------------------------------------------

#[test]
fn capturing_recorder_observes_place_migrate_evaluate() {
    let recorder = CapturingRecorder::new();
    let store = InMemoryTieringStore::new(Box::new(recorder.clone()));
    let tn = tenant("acme");
    store.place(&tn, &item("a"), Tier::Hot, t(0));
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));
    let _ = store.evaluate_at(t(3600), &policy);

    let events = recorder.snapshot();
    // Expected: 1 place, 1 migrate (Hot → Warm), 1
    // evaluate (per-tenant count).
    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[0],
        RecordedEvent::Place {
            tier: Tier::Hot,
            ..
        }
    ));
    assert!(matches!(
        events[1],
        RecordedEvent::Migrate {
            from: Tier::Hot,
            to: Tier::Warm,
            ..
        }
    ));
    assert!(matches!(
        events[2],
        RecordedEvent::Evaluate { migrated: 1, .. }
    ));
}

// --------------------------------------------------------------------
// KPI 2 — evaluate_at p95 ≤ 5 ms over 10 000 placed items
// --------------------------------------------------------------------

#[test]
fn evaluate_p95_latency_under_five_milliseconds() {
    let store = InMemoryTieringStore::new(Box::new(NoopRecorder));
    let tn = tenant("perf");
    // Place 10 000 items with mixed initial tiers and ages.
    for i in 0..10_000u64 {
        let tier = match i % 3 {
            0 => Tier::Hot,
            1 => Tier::Warm,
            _ => Tier::Cold,
        };
        store.place(&tn, &item(&format!("i-{i}")), tier, t(i));
    }
    let policy = TierPolicy::age_based(Duration::from_secs(3600), Duration::from_secs(86_400));

    // First call migrates many items; warm up by calling
    // it once so the subsequent measurements are steady-state.
    let _ = store.evaluate_at(t(1_000_000), &policy);

    let mut samples: Vec<u128> = Vec::with_capacity(200);
    for i in 0..200u64 {
        let now = t(1_000_000 + i);
        let t0 = std::time::Instant::now();
        let _ = store.evaluate_at(now, &policy);
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[190];
    assert!(
        p95 <= 5_000,
        "KPI 2: evaluate_at p95 must be ≤ 5 ms (5000 µs); got {p95} µs (first 10 {:?})",
        &samples[..10]
    );
}
