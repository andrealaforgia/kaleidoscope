// Kaleidoscope self-observe — Cinder → Pulse acceptance test
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

//! Kaleidoscope observes itself: Cinder tier events as Pulse points.
//!
//! The bridge wires Cinder's `MetricsRecorder` events into a
//! Pulse `MetricStore`. The acceptance tests assert that Cinder's
//! `place`, `migrate` and `evaluate_at` calls land as queryable
//! `cinder.place.count` / `cinder.migrate.count` /
//! `cinder.evaluate.migrated.count` metric points in Pulse, with
//! tenant identity, tier topology, migration direction and
//! migrated-count preserved per the ADR-0038 §2 per-event
//! emission contract.
//!
//! Test seam (locked by ADR-0038 §3, DESIGN DD1): drive the
//! bridge through `cinder::InMemoryTieringStore`; assert against
//! `pulse::InMemoryMetricStore`. The dual-emission contract from
//! DISCUSS D3 is naturally expressible in one Slice 03 test by
//! letting `evaluate_at` cascade through the recorder.
//!
//! Every test is tagged `@in-memory` in spirit (Walking Skeleton
//! Strategy A, declared in DISTILL wave-decisions): both the
//! driver (`InMemoryTieringStore`) and the assertion target
//! (`InMemoryMetricStore`) are real in-process adapters. No
//! filesystem, no subprocess, no network.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use aegis::TenantId;
use cinder::{InMemoryTieringStore, ItemId, MigrateError, Tier, TierPolicy, TieringStore};
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder, TimeRange,
};
use self_observe::CinderToPulseRecorder;

// ---------- helpers (mirror lumen_to_pulse.rs naming) -------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn item(id: &str) -> ItemId {
    ItemId::new(id)
}

/// Construct the standard test wiring: a `Pulse` in-memory store
/// shared between the bridge (write side) and the assertion
/// (read side), and a `Cinder` in-memory tiering store whose
/// recorder is the bridge.
fn wire() -> (Arc<InMemoryMetricStore>, InMemoryTieringStore) {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = CinderToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let cinder = InMemoryTieringStore::new(Box::new(bridge));
    (pulse, cinder)
}

fn place_count() -> MetricName {
    MetricName::new("cinder.place.count")
}

fn migrate_count() -> MetricName {
    MetricName::new("cinder.migrate.count")
}

fn evaluate_count() -> MetricName {
    MetricName::new("cinder.evaluate.migrated.count")
}

// ----- Slice 01: place events -----------------------------------------
// Story: US-01 — Cinder `place` events land as queryable Pulse points
// KPI:   OK1
// Tags:  @in-memory @US-01

#[test]
fn cinder_place_produces_a_pulse_metric_point_under_same_tenant() {
    // Scenario: Single place event lands as one cinder.place.count
    // point under same tenant.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");

    cinder
        .place(
            &acme,
            &item("trade-2026-05-18-001"),
            Tier::Hot,
            SystemTime::now(),
        )
        .expect("place");

    let points = pulse
        .query(&acme, &place_count(), TimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1, "exactly one place event recorded");
    assert_eq!(points[0].1.value, 1.0);
    assert_eq!(
        points[0].1.attributes.get("tier").map(String::as_str),
        Some("hot")
    );
}

#[test]
fn cinder_place_serialises_each_tier_as_lowercase_string() {
    // Scenario: Place events for different tiers land with correct
    // tier attribute (set equals {"hot","warm","cold"}).
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let now = SystemTime::now();

    cinder
        .place(&acme, &item("trade-001"), Tier::Hot, now)
        .expect("place");
    cinder
        .place(&acme, &item("trade-002"), Tier::Warm, now)
        .expect("place");
    cinder
        .place(&acme, &item("trade-003"), Tier::Cold, now)
        .expect("place");

    let points = pulse
        .query(&acme, &place_count(), TimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 3, "three place events recorded");

    let observed_tiers: std::collections::BTreeSet<String> = points
        .iter()
        .map(|(_, p)| {
            p.attributes
                .get("tier")
                .cloned()
                .expect("tier attribute present")
        })
        .collect();
    let expected_tiers: std::collections::BTreeSet<String> = ["hot", "warm", "cold"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(observed_tiers, expected_tiers);
}

#[test]
fn two_tenants_cinder_place_events_land_in_isolated_pulse_buckets() {
    // Scenario: Place events isolate per tenant.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let globex = tenant("globex");
    let now = SystemTime::now();

    cinder
        .place(&acme, &item("a1"), Tier::Hot, now)
        .expect("place");
    cinder
        .place(&globex, &item("g1"), Tier::Hot, now)
        .expect("place");
    cinder
        .place(&globex, &item("g2"), Tier::Hot, now)
        .expect("place");

    let acme_points = pulse
        .query(&acme, &place_count(), TimeRange::all())
        .expect("acme pulse query");
    let globex_points = pulse
        .query(&globex, &place_count(), TimeRange::all())
        .expect("globex pulse query");

    assert_eq!(acme_points.len(), 1, "acme has one placement");
    assert_eq!(globex_points.len(), 2, "globex has two placements");
}

#[test]
fn no_cinder_event_means_no_pulse_metric_point() {
    // Scenario: No Cinder event means no Pulse metric point.
    // (Cross-cutting quiescence assertion; lives in Slice 01 by
    // convention, covers all three metric names.)
    let (pulse, _cinder) = wire();
    let any_tenant = tenant("acme");

    for name in [place_count(), migrate_count(), evaluate_count()] {
        let out = pulse
            .query(&any_tenant, &name, TimeRange::all())
            .expect("pulse query");
        assert!(
            out.is_empty(),
            "no cinder call should yield no {} points",
            name.as_str()
        );
    }
}

// ----- Slice 02: migrate events ---------------------------------------
// Story: US-02 — Cinder `migrate` events land with from/to attributes
// KPI:   OK2
// Tags:  @in-memory @US-02

#[test]
fn cinder_migrate_produces_a_pulse_point_with_from_and_to_attributes() {
    // Scenario: Migrate event preserves source and destination tier
    // as attributes.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let id = item("trade-2026-05-18-001");
    let t0 = SystemTime::now();
    let t1 = t0 + Duration::from_secs(60);

    cinder.place(&acme, &id, Tier::Hot, t0).expect("place");
    cinder
        .migrate(&acme, &id, Tier::Warm, t1)
        .expect("migrate ok");

    let points = pulse
        .query(&acme, &migrate_count(), TimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1, "exactly one migrate event recorded");
    assert_eq!(points[0].1.value, 1.0);
    assert_eq!(
        points[0].1.attributes.get("from").map(String::as_str),
        Some("hot")
    );
    assert_eq!(
        points[0].1.attributes.get("to").map(String::as_str),
        Some("warm")
    );
}

#[test]
fn cinder_migrate_failure_with_unknown_item_emits_no_pulse_point() {
    // Scenario: Failed migrate (unknown item) emits no metric point.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let t1 = SystemTime::now();

    let result = cinder.migrate(&acme, &item("ghost"), Tier::Warm, t1);
    assert!(
        matches!(result, Err(MigrateError::UnknownItem { .. })),
        "migrate on never-placed item must return UnknownItem"
    );

    let points = pulse
        .query(&acme, &migrate_count(), TimeRange::all())
        .expect("pulse query");
    assert!(
        points.is_empty(),
        "failed migrate must not emit a metric point"
    );
}

#[test]
fn two_tenants_cinder_migrate_events_land_in_isolated_pulse_buckets() {
    // Scenario: Migrate events isolate per tenant.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let globex = tenant("globex");
    let t0 = SystemTime::now();
    let t1 = t0 + Duration::from_secs(60);

    cinder
        .place(&acme, &item("a1"), Tier::Hot, t0)
        .expect("place");
    cinder
        .place(&globex, &item("g1"), Tier::Hot, t0)
        .expect("place");

    cinder
        .migrate(&acme, &item("a1"), Tier::Warm, t1)
        .expect("acme");
    cinder
        .migrate(&globex, &item("g1"), Tier::Cold, t1)
        .expect("globex");

    let acme_points = pulse
        .query(&acme, &migrate_count(), TimeRange::all())
        .expect("acme pulse query");
    let globex_points = pulse
        .query(&globex, &migrate_count(), TimeRange::all())
        .expect("globex pulse query");

    assert_eq!(acme_points.len(), 1);
    assert_eq!(
        acme_points[0].1.attributes.get("from").map(String::as_str),
        Some("hot")
    );
    assert_eq!(
        acme_points[0].1.attributes.get("to").map(String::as_str),
        Some("warm")
    );

    assert_eq!(globex_points.len(), 1);
    assert_eq!(
        globex_points[0]
            .1
            .attributes
            .get("from")
            .map(String::as_str),
        Some("hot")
    );
    assert_eq!(
        globex_points[0].1.attributes.get("to").map(String::as_str),
        Some("cold")
    );
}

// ----- Slice 03: evaluate events --------------------------------------
// Story: US-03 — Cinder `evaluate` events land with per-tenant counts
// KPI:   OK3
// Tags:  @in-memory @US-03

#[test]
fn cinder_evaluate_emits_per_item_migrate_points_and_one_evaluate_point() {
    // Scenario: Evaluate that migrates N items for one tenant emits
    // N migrate points AND 1 evaluate point.
    //
    // Highest-information-density assertion in the suite: the
    // dual-emission contract from DISCUSS D3 is verified by
    // cross-asserting both `cinder.migrate.count` (per item) AND
    // `cinder.evaluate.migrated.count` (per tenant) after a single
    // `evaluate_at` call.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let t0 = SystemTime::now();
    let policy = TierPolicy::age_based(
        Duration::from_secs(24 * 3600), // hot -> warm at 24h
        Duration::from_secs(72 * 3600), // warm -> cold at 72h
    );

    for n in 0..5 {
        cinder
            .place(&acme, &item(&format!("trade-{n}")), Tier::Hot, t0)
            .expect("place");
    }

    let migrated = cinder
        .evaluate_at(t0 + Duration::from_secs(25 * 3600), &policy)
        .expect("evaluate");
    assert_eq!(migrated, 5, "evaluate_at returns total migration count");

    let migrate_points = pulse
        .query(&acme, &migrate_count(), TimeRange::all())
        .expect("migrate query");
    assert_eq!(migrate_points.len(), 5, "five per-item migrate points");
    for (_, p) in &migrate_points {
        assert_eq!(p.attributes.get("from").map(String::as_str), Some("hot"));
        assert_eq!(p.attributes.get("to").map(String::as_str), Some("warm"));
    }

    let evaluate_points = pulse
        .query(&acme, &evaluate_count(), TimeRange::all())
        .expect("evaluate query");
    assert_eq!(evaluate_points.len(), 1, "one per-tenant evaluate point");
    assert_eq!(
        evaluate_points[0].1.value, 5.0,
        "evaluate point value equals migrated count"
    );
}

#[test]
fn cinder_evaluate_with_no_eligible_items_emits_no_evaluate_point() {
    // Scenario: Evaluate with zero eligible items emits no
    // evaluate point for that tenant.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let t0 = SystemTime::now();
    let policy = TierPolicy::age_based(
        Duration::from_secs(24 * 3600),
        Duration::from_secs(72 * 3600),
    );

    for n in 0..3 {
        cinder
            .place(&acme, &item(&format!("trade-{n}")), Tier::Hot, t0)
            .expect("place");
    }

    let migrated = cinder
        .evaluate_at(t0 + Duration::from_secs(3600), &policy)
        .expect("evaluate");
    assert_eq!(migrated, 0, "nothing eligible for migration at +1h");

    let evaluate_points = pulse
        .query(&acme, &evaluate_count(), TimeRange::all())
        .expect("evaluate query");
    assert!(
        evaluate_points.is_empty(),
        "zero-migration evaluate must not emit a point"
    );

    let migrate_points = pulse
        .query(&acme, &migrate_count(), TimeRange::all())
        .expect("migrate query");
    assert!(migrate_points.is_empty(), "no per-item migrations either");
}

#[test]
fn cinder_evaluate_across_two_tenants_emits_per_tenant_counts() {
    // Scenario: Evaluate across two tenants emits per-tenant
    // evaluate points.
    let (pulse, cinder) = wire();
    let acme = tenant("acme");
    let globex = tenant("globex");
    let t0 = SystemTime::now();
    let policy = TierPolicy::age_based(
        Duration::from_secs(24 * 3600),
        Duration::from_secs(72 * 3600),
    );

    for n in 0..5 {
        cinder
            .place(&acme, &item(&format!("a-{n}")), Tier::Hot, t0)
            .expect("place");
    }
    for n in 0..2 {
        cinder
            .place(&globex, &item(&format!("g-{n}")), Tier::Hot, t0)
            .expect("place");
    }

    let migrated = cinder
        .evaluate_at(t0 + Duration::from_secs(25 * 3600), &policy)
        .expect("evaluate");
    assert_eq!(migrated, 7, "5 acme + 2 globex");

    let acme_eval = pulse
        .query(&acme, &evaluate_count(), TimeRange::all())
        .expect("acme evaluate query");
    let globex_eval = pulse
        .query(&globex, &evaluate_count(), TimeRange::all())
        .expect("globex evaluate query");
    assert_eq!(acme_eval.len(), 1);
    assert_eq!(acme_eval[0].1.value, 5.0);
    assert_eq!(globex_eval.len(), 1);
    assert_eq!(globex_eval[0].1.value, 2.0);

    let acme_migrate = pulse
        .query(&acme, &migrate_count(), TimeRange::all())
        .expect("acme migrate query");
    let globex_migrate = pulse
        .query(&globex, &migrate_count(), TimeRange::all())
        .expect("globex migrate query");
    assert_eq!(acme_migrate.len(), 5);
    assert_eq!(globex_migrate.len(), 2);
}

// ----- Cross-cutting properties ---------------------------------------

#[test]
fn the_bridge_is_send_and_sync() {
    // @property — structural Earned-Trust probe (ADR-0038
    // Principle 12 layer 1). The compile-time bound makes
    // `Box<dyn cinder::MetricsRecorder + Send + Sync>` accept
    // the bridge; losing either bound breaks compilation, not
    // runtime, of this single line.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToPulseRecorder>();
}
