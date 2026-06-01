// Kaleidoscope Strata — slice 01 walking skeleton acceptance test
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

//! Slice 01 — `ProfileStore::ingest` + `query` walking skeleton
//!
//! Maps to `docs/feature/strata-v0/slices/slice-01-walking-skeleton.md`.

use std::collections::BTreeMap;

use aegis::TenantId;
use strata::{
    Function, InMemoryProfileStore, Location, Mapping, NoopRecorder, Profile, ProfileBatch,
    ProfileStore, Sample, SampleType, ServiceName, TimeRange, ValueType,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn simple_profile(time: u64, service: &str, profile_type: &str) -> Profile {
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

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 / AC-1.3 — ingest + query + ordering
// --------------------------------------------------------------------

#[test]
fn ingest_then_query_returns_profiles_in_time_order() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let batch = ProfileBatch::with_profiles(vec![
        simple_profile(300, "checkout", "cpu"),
        simple_profile(100, "checkout", "cpu"),
        simple_profile(200, "checkout", "cpu"),
    ]);
    let receipt = store.ingest(&t, batch).expect("ingest");
    assert_eq!(receipt.count, 3);

    let out = store
        .query(&t, &ServiceName::new("checkout"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].time_unix_nano, 100);
    assert_eq!(out[1].time_unix_nano, 200);
    assert_eq!(out[2].time_unix_nano, 300);
}

#[test]
fn query_with_time_range_returns_only_matching_profiles() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let batch = ProfileBatch::with_profiles(
        (1..=4)
            .map(|i| simple_profile(i * 100, "checkout", "cpu"))
            .collect(),
    );
    store.ingest(&t, batch).expect("ingest");

    let out = store
        .query(&t, &ServiceName::new("checkout"), TimeRange::new(200, 400))
        .expect("query");
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].time_unix_nano, 200);
    assert_eq!(out[1].time_unix_nano, 300);
}

// --------------------------------------------------------------------
// AC-1.4 — tenant isolation
// --------------------------------------------------------------------

#[test]
fn two_tenants_profiles_are_isolated() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let acme = tenant("acme");
    let globex = tenant("globex");

    store
        .ingest(
            &acme,
            ProfileBatch::with_profiles(vec![simple_profile(100, "checkout", "cpu")]),
        )
        .expect("acme");
    store
        .ingest(
            &globex,
            ProfileBatch::with_profiles(vec![simple_profile(200, "checkout", "cpu")]),
        )
        .expect("globex");

    let a = store
        .query(&acme, &ServiceName::new("checkout"), TimeRange::all())
        .expect("a");
    let g = store
        .query(&globex, &ServiceName::new("checkout"), TimeRange::all())
        .expect("g");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].time_unix_nano, 100);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].time_unix_nano, 200);
}

// --------------------------------------------------------------------
// AC-1.5 — byte-stable field preservation (full pprof shape)
// --------------------------------------------------------------------

#[test]
fn every_field_round_trips_byte_stable_including_string_table_and_locations() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");

    let mut profile_attrs = BTreeMap::new();
    profile_attrs.insert(
        "host.name".to_string(),
        "ip-10-0-0-7.eu-west-1.compute.internal".to_string(),
    );

    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    resource.insert("service.version".to_string(), "2.4.1".to_string());
    resource.insert(
        "telemetry.sdk.name".to_string(),
        "opentelemetry".to_string(),
    );

    let mut sample_attrs = BTreeMap::new();
    sample_attrs.insert("thread.id".to_string(), "12345".to_string());
    sample_attrs.insert("process.id".to_string(), "1".to_string());

    let original = Profile {
        time_unix_nano: 1_700_000_000_000_000_000,
        duration_nanos: 30_000_000_000, // 30s profile
        profile_type: "cpu".to_string(),
        sample_type: vec![
            SampleType {
                value_type: ValueType {
                    type_index: 1,
                    unit_index: 2,
                },
                aggregation_temporality: 1,
            },
            SampleType {
                value_type: ValueType {
                    type_index: 3,
                    unit_index: 4,
                },
                aggregation_temporality: 2,
            },
        ],
        samples: vec![
            Sample {
                location_ids: vec![1, 2, 3],
                values: vec![42, 4_200_000],
                attributes: sample_attrs.clone(),
            },
            Sample {
                location_ids: vec![1, 4],
                values: vec![17, 1_700_000],
                attributes: sample_attrs,
            },
        ],
        locations: vec![
            Location {
                id: 1,
                mapping_id: 1,
                address: 0x1000,
                function_ids: vec![1, 2], // inlined
            },
            Location {
                id: 2,
                mapping_id: 1,
                address: 0x1100,
                function_ids: vec![3],
            },
            Location {
                id: 3,
                mapping_id: 1,
                address: 0x1200,
                function_ids: vec![4],
            },
            Location {
                id: 4,
                mapping_id: 2,
                address: 0x2000,
                function_ids: vec![5],
            },
        ],
        functions: vec![
            Function {
                id: 1,
                name_index: 5,
                system_name_index: 5,
                filename_index: 6,
                start_line: 10,
            },
            Function {
                id: 2,
                name_index: 7,
                system_name_index: 7,
                filename_index: 6,
                start_line: 30,
            },
            Function {
                id: 3,
                name_index: 8,
                system_name_index: 8,
                filename_index: 6,
                start_line: 50,
            },
            Function {
                id: 4,
                name_index: 9,
                system_name_index: 9,
                filename_index: 6,
                start_line: 70,
            },
            Function {
                id: 5,
                name_index: 10,
                system_name_index: 10,
                filename_index: 11,
                start_line: 100,
            },
        ],
        mappings: vec![
            Mapping {
                id: 1,
                memory_start: 0x1000,
                memory_limit: 0x2000,
                file_offset: 0,
                filename_index: 6,
                build_id_index: 12,
            },
            Mapping {
                id: 2,
                memory_start: 0x2000,
                memory_limit: 0x3000,
                file_offset: 0,
                filename_index: 11,
                build_id_index: 13,
            },
        ],
        string_table: vec![
            "".to_string(),                     // 0 — pprof convention
            "samples".to_string(),              // 1
            "count".to_string(),                // 2
            "cpu".to_string(),                  // 3
            "nanoseconds".to_string(),          // 4
            "main.handleRequest".to_string(),   // 5
            "checkout.go".to_string(),          // 6
            "db.query".to_string(),             // 7
            "cache.read".to_string(),           // 8
            "net.write".to_string(),            // 9
            "runtime.epollwait".to_string(),    // 10
            "runtime/sys_linux.go".to_string(), // 11
            "build-abc123".to_string(),         // 12
            "build-def456".to_string(),         // 13
        ],
        resource_attributes: resource,
        attributes: profile_attrs,
    };
    store
        .ingest(&t, ProfileBatch::with_profiles(vec![original.clone()]))
        .expect("ingest");

    let out = store
        .query(&t, &ServiceName::new("checkout"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], original);
}

// --------------------------------------------------------------------
// AC-1.6 / AC-1.7 — empty results
// --------------------------------------------------------------------

#[test]
fn query_on_unknown_service_returns_empty() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let out = store
        .query(
            &tenant("acme"),
            &ServiceName::new("ghost"),
            TimeRange::all(),
        )
        .expect("query");
    assert!(out.is_empty());
}

#[test]
fn empty_range_returns_ok_empty() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            ProfileBatch::with_profiles(vec![simple_profile(100, "checkout", "cpu")]),
        )
        .expect("ingest");
    let out = store
        .query(&t, &ServiceName::new("checkout"), TimeRange::new(500, 1000))
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// KPI 1 — ingest p95 ≤ 5 ms per 10-profile batch
//
// Profiles are bigger than the per-100 batches used elsewhere — KB to
// MB each. The realistic OTLP-Profiles batch shape is ~10 profiles per
// flush. The 5 ms ceiling reflects that profile cloning is the
// dominant cost; v1's columnar substrate dedupes string tables across
// profiles and pays this only at compaction.
// --------------------------------------------------------------------

#[test]
fn ingest_p95_latency_under_five_milliseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    let services = ["checkout", "billing", "shipping", "auth"];

    fn make_batch(seed: u64, services: &[&str]) -> ProfileBatch {
        let profiles: Vec<Profile> = (0..10)
            .map(|i| {
                let service = services[(i as usize) % services.len()];
                simple_profile(seed * 1000 + i, service, "cpu")
            })
            .collect();
        ProfileBatch::with_profiles(profiles)
    }

    for i in 0..20 {
        store.ingest(&t, make_batch(i, &services)).expect("warmup");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(200);
    for i in 0..200 {
        let batch = make_batch(20 + i, &services);
        let t0 = std::time::Instant::now();
        store.ingest(&t, batch).expect("ingest");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[190]; // index 95% of 200 = 190
    assert!(
        p95 <= 5_000,
        "KPI 1: ingest p95 must be ≤ 5 ms (5000 µs); got {p95} µs (first 10 {:?})",
        &samples[..10]
    );
}

// --------------------------------------------------------------------
// Profiles without service.name are dropped from the by-service index
// --------------------------------------------------------------------

#[test]
fn profile_without_service_name_is_dropped_at_v0() {
    let store = InMemoryProfileStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let mut prof = simple_profile(100, "checkout", "cpu");
    prof.resource_attributes = BTreeMap::new();
    store
        .ingest(&t, ProfileBatch::with_profiles(vec![prof]))
        .expect("ingest");
    // Cannot be reached by service query.
    let out = store
        .query(&t, &ServiceName::new("checkout"), TimeRange::all())
        .expect("query");
    assert!(out.is_empty());
}
