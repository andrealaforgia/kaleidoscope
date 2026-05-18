// Kaleidoscope self-observe — Strata → Pulse acceptance test
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

//! Kaleidoscope observes Strata (its continuous profiling
//! storage engine) through its own Pulse metric store. Same
//! template as `lumen_to_pulse` and `ray_to_pulse`.

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder,
    TimeRange as PulseTimeRange,
};
use self_observe::StrataToPulseRecorder;
use strata::{
    InMemoryProfileStore, Profile, ProfileBatch, ProfileStore, ServiceName,
    TimeRange as StrataTimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn profile(time: u64, profile_type: &str) -> Profile {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Profile {
        time_unix_nano: time,
        duration_nanos: 1_000_000_000,
        profile_type: profile_type.to_string(),
        sample_type: Vec::new(),
        samples: Vec::new(),
        locations: Vec::new(),
        functions: Vec::new(),
        mappings: Vec::new(),
        string_table: vec![String::new()],
        resource_attributes: resource,
        attributes: BTreeMap::new(),
    }
}

fn bridge_pulse() -> (Arc<InMemoryMetricStore>, InMemoryProfileStore) {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = StrataToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let strata = InMemoryProfileStore::new(Box::new(bridge));
    (pulse, strata)
}

#[test]
fn strata_ingest_produces_a_pulse_metric_point_under_same_tenant() {
    let (pulse, strata) = bridge_pulse();
    let tn = tenant("acme");
    strata
        .ingest(
            &tn,
            ProfileBatch::with_profiles(vec![
                profile(100, "cpu"),
                profile(200, "cpu"),
                profile(300, "heap"),
            ]),
        )
        .expect("ingest");

    let metric_name = MetricName::new("strata.ingest.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].1.value, 3.0);
}

#[test]
fn strata_query_produces_a_pulse_metric_point_with_matched_count() {
    let (pulse, strata) = bridge_pulse();
    let tn = tenant("acme");
    strata
        .ingest(
            &tn,
            ProfileBatch::with_profiles(vec![
                profile(100, "cpu"),
                profile(200, "cpu"),
                profile(300, "cpu"),
            ]),
        )
        .expect("ingest");
    let svc = ServiceName::new("checkout");
    let out = strata
        .query(&tn, &svc, StrataTimeRange::new(150, 250))
        .expect("query");
    assert_eq!(out.len(), 1);

    let q_metric = MetricName::new("strata.query.count");
    let q_points = pulse
        .query(&tn, &q_metric, PulseTimeRange::all())
        .expect("q");
    assert_eq!(q_points.len(), 1);
    assert_eq!(q_points[0].1.value, 1.0);
}

#[test]
fn two_tenants_strata_events_land_in_isolated_pulse_buckets() {
    let (pulse, strata) = bridge_pulse();
    let acme = tenant("acme");
    let globex = tenant("globex");
    strata
        .ingest(
            &acme,
            ProfileBatch::with_profiles(vec![profile(100, "cpu")]),
        )
        .expect("acme");
    strata
        .ingest(
            &globex,
            ProfileBatch::with_profiles(vec![profile(100, "cpu"), profile(200, "cpu")]),
        )
        .expect("globex");

    let metric = MetricName::new("strata.ingest.count");
    let a = pulse
        .query(&acme, &metric, PulseTimeRange::all())
        .expect("a");
    let g = pulse
        .query(&globex, &metric, PulseTimeRange::all())
        .expect("g");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].1.value, 1.0);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].1.value, 2.0);
}

#[test]
fn no_strata_event_means_no_pulse_metric_point() {
    let (pulse, _strata) = bridge_pulse();
    let metric = MetricName::new("strata.ingest.count");
    let out = pulse
        .query(&tenant("acme"), &metric, PulseTimeRange::all())
        .expect("pulse");
    assert!(out.is_empty());
}

#[test]
fn the_bridge_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<StrataToPulseRecorder>();
}
