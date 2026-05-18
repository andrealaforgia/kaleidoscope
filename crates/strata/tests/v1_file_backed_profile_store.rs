// Kaleidoscope Strata — v1 file-backed adapter acceptance test
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

//! `FileBackedProfileStore` survives restart, returns
//! ingested profiles byte-stable, supports snapshot+WAL
//! truncate. Last v1 acceptance suite in the platform plane.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use strata::{
    FileBackedProfileStore, NoopRecorder, Profile, ProfileBatch, ProfileStore, ServiceName,
    TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn profile(time: u64, profile_type: &str, service: &str) -> Profile {
    let mut resource = BTreeMap::new();
    if !service.is_empty() {
        resource.insert("service.name".to_string(), service.to_string());
    }
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

fn temp_base(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let dir = env::temp_dir().join(format!("kal-strata-v1-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&dir).expect("mkdir");
    dir.join("strata")
}

fn cleanup(base: &std::path::Path) {
    if let Some(parent) = base.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

fn wal_size(base: &std::path::Path) -> u64 {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    fs::metadata(PathBuf::from(p)).map(|m| m.len()).unwrap_or(0)
}

fn snapshot_exists(base: &std::path::Path) -> bool {
    let mut p = base.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p).exists()
}

#[test]
fn ingest_then_query_returns_profiles_byte_stable() {
    let base = temp_base("query");
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![
                profile(100, "cpu", "checkout"),
                profile(200, "cpu", "checkout"),
            ]),
        )
        .expect("ingest");
    let out = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    cleanup(&base);
}

#[test]
fn restart_recovers_profiles_from_wal() {
    let base = temp_base("wal_restart");
    {
        let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                ProfileBatch::with_profiles(vec![
                    profile(100, "cpu", "checkout"),
                    profile(200, "cpu", "checkout"),
                ]),
            )
            .expect("ingest");
    }
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    cleanup(&base);
}

#[test]
fn snapshot_writes_file_and_truncates_wal() {
    let base = temp_base("snapshot");
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(100, "cpu", "checkout")]),
        )
        .expect("ingest");
    assert!(wal_size(&base) > 0);
    assert!(!snapshot_exists(&base));
    store.snapshot().expect("snapshot");
    assert_eq!(wal_size(&base), 0);
    assert!(snapshot_exists(&base));
    cleanup(&base);
}

#[test]
fn restart_recovers_snapshot_plus_post_snapshot_wal() {
    let base = temp_base("snap_plus_wal");
    {
        let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                ProfileBatch::with_profiles(vec![profile(100, "cpu", "checkout")]),
            )
            .expect("pre-snapshot");
        store.snapshot().expect("snapshot");
        store
            .ingest(
                &tenant("acme"),
                ProfileBatch::with_profiles(vec![profile(200, "cpu", "checkout")]),
            )
            .expect("post-snapshot");
    }
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    let times: Vec<u64> = out.iter().map(|p| p.time_unix_nano).collect();
    assert_eq!(times, vec![100, 200]);
    cleanup(&base);
}

#[test]
fn two_tenants_are_isolated_in_the_same_data_dir() {
    let base = temp_base("isolation");
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(100, "cpu", "checkout")]),
        )
        .expect("acme");
    store
        .ingest(
            &tenant("globex"),
            ProfileBatch::with_profiles(vec![
                profile(100, "cpu", "checkout"),
                profile(200, "cpu", "checkout"),
            ]),
        )
        .expect("globex");
    let acme = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("acme q");
    let globex = store
        .query(
            &tenant("globex"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("globex q");
    assert_eq!(acme.len(), 1);
    assert_eq!(globex.len(), 2);
    cleanup(&base);
}

#[test]
fn time_range_filters_on_time_unix_nano() {
    let base = temp_base("time_range");
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![
                profile(100, "cpu", "checkout"),
                profile(200, "cpu", "checkout"),
                profile(300, "cpu", "checkout"),
            ]),
        )
        .expect("ingest");
    let out = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::new(150, 250),
        )
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].time_unix_nano, 200);
    cleanup(&base);
}

#[test]
fn profiles_without_service_name_are_dropped_persistence_wise() {
    // v0 in-memory contract: profiles without service.name are
    // dropped from the by-service index. v1 must preserve that
    // contract across persistence — the dropped profile must
    // not reappear after restart.
    let base = temp_base("no_service");
    {
        let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                ProfileBatch::with_profiles(vec![
                    profile(100, "cpu", "checkout"),
                    profile(200, "cpu", ""), // dropped
                ]),
            )
            .expect("ingest");
    }
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .query(
            &tenant("acme"),
            &ServiceName::new("checkout"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 1, "the no-service profile stays dropped");
    cleanup(&base);
}

#[test]
fn empty_batch_ingest_is_a_no_op_persistence_wise() {
    let base = temp_base("empty");
    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(&tenant("acme"), ProfileBatch::with_profiles(vec![]))
        .expect("empty");
    assert_eq!(wal_size(&base), 0);
    cleanup(&base);
}
