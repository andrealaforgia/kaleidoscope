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

//! Kaleidoscope observes Cinder (its own tier policy engine)
//! through its own Pulse metric store. The bridge wires Cinder's
//! `MetricsRecorder` events into Pulse points keyed by tenant.
//! Tier names appear as point attributes so a dashboard can
//! break the rate down per tier or per transition.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use aegis::TenantId;
use cinder::{InMemoryTieringStore, ItemId, Tier, TierPolicy, TieringStore};
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder,
    TimeRange as PulseTimeRange,
};
use self_observe::CinderToPulseRecorder;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn bridge_pulse() -> (Arc<InMemoryMetricStore>, InMemoryTieringStore) {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = CinderToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let cinder = InMemoryTieringStore::new(Box::new(bridge));
    (pulse, cinder)
}

#[test]
fn cinder_place_produces_a_pulse_point_with_tier_attribute() {
    let (pulse, cinder) = bridge_pulse();
    let tn = tenant("acme");
    let item = ItemId::new("acme/widget-1");
    cinder.place(&tn, &item, Tier::Hot, SystemTime::UNIX_EPOCH);

    let metric_name = MetricName::new("cinder.place.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].1.value, 1.0);
    assert_eq!(
        points[0].1.attributes.get("tier").map(String::as_str),
        Some("hot")
    );
}

#[test]
fn cinder_three_places_each_get_their_own_point_with_correct_tier() {
    let (pulse, cinder) = bridge_pulse();
    let tn = tenant("acme");
    cinder.place(&tn, &ItemId::new("w-1"), Tier::Hot, SystemTime::UNIX_EPOCH);
    cinder.place(&tn, &ItemId::new("w-2"), Tier::Warm, SystemTime::UNIX_EPOCH);
    cinder.place(&tn, &ItemId::new("w-3"), Tier::Cold, SystemTime::UNIX_EPOCH);

    let metric_name = MetricName::new("cinder.place.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 3);
    let tiers: Vec<&str> = points
        .iter()
        .map(|p| p.1.attributes.get("tier").map(String::as_str).unwrap_or(""))
        .collect();
    assert!(tiers.contains(&"hot"));
    assert!(tiers.contains(&"warm"));
    assert!(tiers.contains(&"cold"));
}

#[test]
fn cinder_migrate_produces_a_pulse_point_with_from_and_to_attributes() {
    let (pulse, cinder) = bridge_pulse();
    let tn = tenant("acme");
    let item = ItemId::new("acme/widget-1");
    cinder.place(&tn, &item, Tier::Hot, SystemTime::UNIX_EPOCH);
    cinder
        .migrate(
            &tn,
            &item,
            Tier::Warm,
            SystemTime::UNIX_EPOCH + Duration::from_secs(60),
        )
        .expect("migrate");

    let metric_name = MetricName::new("cinder.migrate.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1);
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
fn cinder_evaluate_produces_a_pulse_point_with_migrated_count_value() {
    // Policy: Hot -> Warm after 1 minute, Warm -> Cold after 5 minutes.
    // Place 2 items in Hot at t=0, evaluate at t=2min: both migrate.
    let (pulse, cinder) = bridge_pulse();
    let tn = tenant("acme");
    cinder.place(&tn, &ItemId::new("w-1"), Tier::Hot, SystemTime::UNIX_EPOCH);
    cinder.place(&tn, &ItemId::new("w-2"), Tier::Hot, SystemTime::UNIX_EPOCH);

    let policy = TierPolicy::age_based(Duration::from_secs(60), Duration::from_secs(300));
    let migrated = cinder.evaluate_at(SystemTime::UNIX_EPOCH + Duration::from_secs(120), &policy);
    assert_eq!(migrated, 2, "both items should age out of Hot");

    let metric_name = MetricName::new("cinder.evaluate.migrated.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].1.value, 2.0);
}

#[test]
fn cinder_evaluate_with_zero_migrations_emits_no_point_because_cinder_does_not() {
    // Documentary test: pinning down what `evaluate_at` actually
    // does at Cinder v0. The InMemoryTieringStore only calls
    // record_evaluate once per tenant whose items actually
    // migrated — a tenant whose items are all still in their
    // grace period gets no event at all.
    //
    // The bridge faithfully forwards what Cinder tells it, so a
    // zero-migration evaluate pass produces zero Pulse points.
    // If Cinder ever changes its contract to emit
    // record_evaluate(tenant, 0) for every known tenant, this
    // test will start failing and the bridge will need no code
    // change to match — the new emission shape is already
    // covered by the previous test.
    let (pulse, cinder) = bridge_pulse();
    let tn = tenant("acme");
    cinder.place(&tn, &ItemId::new("w-1"), Tier::Hot, SystemTime::UNIX_EPOCH);

    let policy = TierPolicy::age_based(Duration::from_secs(60), Duration::from_secs(300));
    // Evaluate at t=10s: still in Hot's grace period, nothing migrates.
    let migrated = cinder.evaluate_at(SystemTime::UNIX_EPOCH + Duration::from_secs(10), &policy);
    assert_eq!(migrated, 0);

    let metric_name = MetricName::new("cinder.evaluate.migrated.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert!(
        points.is_empty(),
        "Cinder v0 emits record_evaluate only for tenants that migrated; bridge mirrors that"
    );
}

#[test]
fn two_tenants_cinder_events_land_in_isolated_pulse_buckets() {
    let (pulse, cinder) = bridge_pulse();
    let acme = tenant("acme");
    let globex = tenant("globex");
    cinder.place(
        &acme,
        &ItemId::new("a-1"),
        Tier::Hot,
        SystemTime::UNIX_EPOCH,
    );
    cinder.place(
        &acme,
        &ItemId::new("a-2"),
        Tier::Hot,
        SystemTime::UNIX_EPOCH,
    );
    cinder.place(
        &globex,
        &ItemId::new("g-1"),
        Tier::Cold,
        SystemTime::UNIX_EPOCH,
    );

    let metric_name = MetricName::new("cinder.place.count");
    let acme_points = pulse
        .query(&acme, &metric_name, PulseTimeRange::all())
        .expect("acme");
    let globex_points = pulse
        .query(&globex, &metric_name, PulseTimeRange::all())
        .expect("globex");
    assert_eq!(acme_points.len(), 2);
    assert_eq!(globex_points.len(), 1);
    assert_eq!(
        globex_points[0]
            .1
            .attributes
            .get("tier")
            .map(String::as_str),
        Some("cold")
    );
}

#[test]
fn the_bridge_is_send_and_sync() {
    // Cinder's MetricsRecorder requires Send + Sync; this is a
    // compile-time assertion that the bridge satisfies that
    // bound. Failure here is a build error, not a runtime one.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToPulseRecorder>();
}
