// Kaleidoscope Strata — slice 02 structured query acceptance test
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

//! Slice 02 — Predicate + profile_type filter
//!
//! Maps to `docs/feature/strata-v0/slices/slice-02-structured-query.md`.

use std::collections::BTreeMap;

use aegis::TenantId;
use strata::{
    CapturingRecorder, InMemoryProfileStore, NoopRecorder, Predicate, Profile, ProfileBatch,
    ProfileStore, RecordedEvent, Sample, SampleType, ServiceName, TimeRange, ValueType,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn profile(time: u64, service: &str, profile_type: &str) -> Profile {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Profile {
        time_unix_nano: time,
        duration_nanos: 10_000_000,
        profile_type: profile_type.to_string(),
        sample_type: vec![SampleType {
            value_type: ValueType {
                type_index: 1,
                unit_index: 2,
            },
            aggregation_temporality: 1,
        }],
        samples: vec![Sample {
            location_ids: vec![1],
            values: vec![10],
            attributes: BTreeMap::new(),
        }],
        locations: Vec::new(),
        functions: Vec::new(),
        mappings: Vec::new(),
        string_table: vec!["".to_string(), "samples".to_string(), "count".to_string()],
        resource_attributes: resource,
        attributes: BTreeMap::new(),
    }
}

// --------------------------------------------------------------------
// AC-2.1 — profile_type filter
// --------------------------------------------------------------------

#[test]
fn profile_type_predicate_filters_by_kind() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            ProfileBatch::with_profiles(vec![
                profile(100, "checkout", "cpu"),
                profile(200, "checkout", "heap"),
                profile(300, "checkout", "cpu"),
                profile(400, "checkout", "goroutine"),
            ]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new().profile_type("cpu"),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|p| p.profile_type == "cpu"));
}

// --------------------------------------------------------------------
// AC-2.2 — empty predicate ≡ range-only query
// --------------------------------------------------------------------

#[test]
fn empty_predicate_equals_range_only_query() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            ProfileBatch::with_profiles(vec![
                profile(100, "checkout", "cpu"),
                profile(200, "checkout", "heap"),
            ]),
        )
        .expect("ingest");

    let with_empty = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new(),
        )
        .expect("query_with");
    let without = store
        .query(&t, &ServiceName::new("checkout"), TimeRange::all())
        .expect("query");
    assert_eq!(with_empty, without);
    assert!(Predicate::new().is_empty());
}

// --------------------------------------------------------------------
// AC-2.3 — no matches returns empty
// --------------------------------------------------------------------

#[test]
fn predicate_with_no_matches_returns_empty_not_error() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            ProfileBatch::with_profiles(vec![profile(100, "checkout", "cpu")]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new().profile_type("flamegraph-the-impossible"),
        )
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// MetricsRecorder seam
// --------------------------------------------------------------------

#[test]
fn capturing_recorder_observes_every_operation() {
    let recorder = CapturingRecorder::new();
    let store = InMemoryProfileStore::new(Box::new(recorder.clone()));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            ProfileBatch::with_profiles(vec![
                profile(100, "checkout", "cpu"),
                profile(200, "checkout", "heap"),
            ]),
        )
        .expect("ingest");
    let _ = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new().profile_type("cpu"),
        )
        .expect("query");

    let events = recorder.snapshot();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0],
        RecordedEvent::Ingest {
            profile_count: 2,
            ..
        }
    ));
    assert!(matches!(
        events[1],
        RecordedEvent::Query {
            matched_count: 1,
            ..
        }
    ));
}

// --------------------------------------------------------------------
// KPI 2 — query p95 ≤ 10 ms over 1000 profiles
// --------------------------------------------------------------------

#[test]
fn query_p95_latency_under_ten_milliseconds() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    let kinds = ["cpu", "heap", "goroutine", "block"];
    let mut batch = ProfileBatch::new();
    for i in 0..1000u64 {
        let kind = kinds[(i as usize) % kinds.len()];
        batch.push(profile(i + 1, "checkout", kind));
    }
    store.ingest(&t, batch).expect("ingest");

    let predicate = Predicate::new().profile_type("cpu");

    for _ in 0..20 {
        let _ = store.query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &predicate,
        );
    }

    let mut samples: Vec<u128> = Vec::with_capacity(200);
    for _ in 0..200 {
        let t0 = std::time::Instant::now();
        let _ = store
            .query_with(
                &t,
                &ServiceName::new("checkout"),
                TimeRange::all(),
                &predicate,
            )
            .expect("query");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[190];
    assert!(
        p95 <= 10_000,
        "KPI 2: query p95 must be ≤ 10 ms (10 000 µs); got {p95} µs (first 10 {:?})",
        &samples[..10]
    );
}
