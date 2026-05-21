// Kaleidoscope Strata v1 — slice 01 WAL durability acceptance test
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

//! Slice 01 — `FileBackedProfileStore::open` + ingest + query survive
//! a restart.
//!
//! Maps to `docs/feature/strata-v1/design/wave-decisions.md`
//! (US-SV1-01, AC-1.x) and KPI 1 in
//! `docs/feature/strata-v1/discuss/outcome-kpis.md`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use strata::{
    FileBackedProfileStore, Function, Location, Mapping, NoopRecorder, Profile, ProfileBatch,
    ProfileStore, Sample, SampleType, ServiceName, TimeRange, ValueType,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

/// A representative single profile carrying the full pprof table set
/// so the durability path exercises the heavy payload honestly, not a
/// stripped stub.
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

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("strata-v1-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn wal_size_bytes(base: &std::path::Path) -> u64 {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    fs::metadata(PathBuf::from(p)).map(|m| m.len()).unwrap_or(0)
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 — restart recovers profiles in ascending time order
// --------------------------------------------------------------------

#[test]
fn restart_recovers_profiles_in_time_order() {
    let base = temp_base("recover_order");
    {
        let s = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        // First ingest out of order.
        s.ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![
                profile(300, "checkout", "cpu"),
                profile(100, "checkout", "cpu"),
            ]),
        )
        .expect("ingest 1");
        // Second ingest interleaves a middle timestamp.
        s.ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(200, "checkout", "cpu")]),
        )
        .expect("ingest 2");
    }
    let s2 = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = s2
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].time_unix_nano, 100);
    assert_eq!(out[1].time_unix_nano, 200);
    assert_eq!(out[2].time_unix_nano, 300);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.3 — multiple ingests across reopen compose and stay sorted
// --------------------------------------------------------------------

#[test]
fn multiple_ingests_across_reopen_compose_and_remain_sorted() {
    let base = temp_base("compose_sorted");
    {
        let s = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(200, "svc", "cpu")]),
        )
        .expect("ingest 1");
    }
    {
        let s = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
        s.ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![
                profile(100, "svc", "cpu"),
                profile(300, "svc", "cpu"),
            ]),
        )
        .expect("ingest 2");
    }
    let s3 = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 3");
    let out = s3
        .query(&tenant("acme"), &ServiceName::new("svc"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].time_unix_nano, 100);
    assert_eq!(out[1].time_unix_nano, 200);
    assert_eq!(out[2].time_unix_nano, 300);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.4 — byte-stable round-trip of the full structured profile
//
// DD5: there is no byte field on any profile type, so no hex
// assertion is needed. The round-trip must instead preserve the full
// pprof payload — numeric vectors, the string table and all three
// attribute maps — verbatim across a restart.
// --------------------------------------------------------------------

#[test]
fn full_profile_payload_round_trips_across_restart() {
    let base = temp_base("payload_round_trip");

    let mut sample_attrs = BTreeMap::new();
    sample_attrs.insert("thread.id".to_string(), "7".to_string());
    sample_attrs.insert("process.id".to_string(), "1234".to_string());

    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    resource.insert("service.version".to_string(), "2.4.1".to_string());

    let mut profile_attrs = BTreeMap::new();
    profile_attrs.insert("profile.id".to_string(), "abc-123".to_string());

    let original = Profile {
        time_unix_nano: 1_700_000_000_000_000_000,
        duration_nanos: 30_000_000_000,
        profile_type: "heap".to_string(),
        sample_type: vec![SampleType {
            value_type: ValueType {
                type_index: 1,
                unit_index: 2,
            },
            aggregation_temporality: 2,
        }],
        samples: vec![Sample {
            location_ids: vec![10, 20, 30],
            values: vec![4096, -1, 2048],
            attributes: sample_attrs,
        }],
        locations: vec![Location {
            id: 10,
            mapping_id: 1,
            address: 0xdead_beef,
            function_ids: vec![100, 101],
        }],
        functions: vec![Function {
            id: 100,
            name_index: 3,
            system_name_index: 4,
            filename_index: 5,
            start_line: 42,
        }],
        mappings: vec![Mapping {
            id: 1,
            memory_start: 0x4000,
            memory_limit: 0x8000,
            file_offset: 0,
            filename_index: 5,
            build_id_index: 6,
        }],
        string_table: vec![
            "".to_string(),
            "space".to_string(),
            "bytes".to_string(),
            "allocate".to_string(),
            "runtime.malloc".to_string(),
            "malloc.go".to_string(),
            "build-deadbeef".to_string(),
        ],
        resource_attributes: resource,
        attributes: profile_attrs,
    };

    {
        let s = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![original.clone()]),
        )
        .expect("ingest");
    }
    let s2 = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = s2
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 1);
    // Full structural equality — every numeric vector, the string
    // table and all three attribute maps survive verbatim.
    assert_eq!(out[0], original);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC — empty batch ingest is a no-op (writes nothing to the WAL)
// --------------------------------------------------------------------

#[test]
fn empty_batch_ingest_writes_nothing_to_wal() {
    let base = temp_base("empty_batch");
    {
        let s = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        let r = s
            .ingest(&tenant("acme"), ProfileBatch::with_profiles(vec![]))
            .expect("ingest empty");
        assert_eq!(r.count, 0);
    }
    assert_eq!(
        wal_size_bytes(&base),
        0,
        "empty batch must not write to WAL"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// KPI 1 — ingest p95 <= 8 ms per 100-profile batch (debug build)
//
// 8 ms not Ray's 5 ms or Pulse's 2 ms: the payload weight is the
// whole story. A Profile is the heaviest payload of the six pillars.
// Every profile carries samples (each a stack of location ids, a
// values vector and an attribute map) plus the supporting pprof
// tables (locations, functions, mappings) plus a string table holding
// every name, unit, filename and build id, plus two resource and
// profile attribute maps. Serialising 100 such profiles into one
// NDJSON line is materially more JSON-encoding work per batch than
// 100 spans, let alone 100 metric points. The 8 ms ceiling is set
// against GitHub Actions ubuntu-latest from the first commit, with
// the CI-realism margin already baked in. This is exactly the
// discipline the 2026-05-19 timing-bump batch taught: Lumen v1 and
// Cinder v1 were calibrated against a fast workstation and failed on
// CI for roughly two weeks before being raised. Better to set it
// right from DISCUSS than bump it at DELIVER. v2's columnar adapter
// changes the serialisation cost profile entirely and this ceiling is
// expected to drop.
// --------------------------------------------------------------------

#[test]
fn ingest_p95_latency_under_eight_milliseconds() {
    let base = temp_base("kpi1");
    let s = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let t = tenant("perf");

    fn make_batch(seed: u64) -> ProfileBatch {
        let profiles: Vec<Profile> = (0..100)
            .map(|i| profile(seed * 1000 + i, "perf-svc", "cpu"))
            .collect();
        ProfileBatch::with_profiles(profiles)
    }

    for i in 0..50 {
        s.ingest(&t, make_batch(i)).expect("warmup");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let batch = make_batch(50 + i);
        let t0 = std::time::Instant::now();
        s.ingest(&t, batch).expect("ingest");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[950];
    assert!(
        p95 <= 8_000,
        "KPI 1: ingest p95 must be <= 8 ms (8000 us); got {p95} us (first samples {:?})",
        &samples[..10]
    );
    cleanup(&base);
}
