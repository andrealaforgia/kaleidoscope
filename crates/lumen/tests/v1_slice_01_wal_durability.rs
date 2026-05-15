// Kaleidoscope Lumen v1 — slice 01 WAL durability acceptance test
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

//! Slice 01 — `FileBackedLogStore::open` + ingest + query survive
//! a restart.
//!
//! Maps to `docs/feature/lumen-v1/slices/slice-01-wal-durability.md`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, LogStoreError, NoopRecorder, SeverityNumber,
    TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, service: &str, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("lumen-v1-{test_name}-{pid}-{nanos}"));
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
// AC-1.1 / AC-1.2 — open + ingest
// --------------------------------------------------------------------

#[test]
fn open_creates_a_fresh_store_and_ingest_persists() {
    let base = temp_base("fresh");
    let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let r = s
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(100, "checkout", "first")]),
        )
        .expect("ingest");
    assert_eq!(r.count, 1);
    let out = s.query(&tenant("acme"), TimeRange::all()).expect("query");
    assert_eq!(out.len(), 1);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.3 / AC-1.4 — restart recovers records in observed-time order
// --------------------------------------------------------------------

#[test]
fn restart_recovers_records_in_observed_time_order() {
    let base = temp_base("recover_order");
    {
        let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(300, "a", "third"), record(100, "a", "first")]),
        )
        .expect("ingest 1");
        s.ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(200, "a", "second")]),
        )
        .expect("ingest 2");
    }
    let s2 = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = s2.query(&tenant("acme"), TimeRange::all()).expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].body, "first");
    assert_eq!(out[1].body, "second");
    assert_eq!(out[2].body, "third");
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.5 — byte-stable round-trip of every field including trace_id
// --------------------------------------------------------------------

#[test]
fn every_field_round_trips_byte_stable_across_restart() {
    let base = temp_base("byte_stable");

    let mut attrs = BTreeMap::new();
    attrs.insert("http.status_code".to_string(), "503".to_string());
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    resource.insert("service.version".to_string(), "2.4.1".to_string());

    let original = LogRecord {
        observed_time_unix_nano: 1_700_000_000_000_000_000,
        severity_number: SeverityNumber::ERROR,
        severity_text: "ERROR".to_string(),
        body: "payment declined".to_string(),
        attributes: attrs,
        resource_attributes: resource,
        trace_id: Some([0x11; 16]),
        span_id: Some([0x22; 8]),
    };

    {
        let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![original.clone()]),
        )
        .expect("ingest");
    }
    let s2 = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = s2.query(&tenant("acme"), TimeRange::all()).expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], original);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.6 — tenant isolation across restart
// --------------------------------------------------------------------

#[test]
fn two_tenants_records_isolated_across_restart() {
    let base = temp_base("tenant_iso");
    {
        let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(100, "a", "acme-only")]),
        )
        .expect("ingest a");
        s.ingest(
            &tenant("globex"),
            LogBatch::with_records(vec![record(200, "b", "globex-only")]),
        )
        .expect("ingest g");
    }
    let s2 = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let a = s2
        .query(&tenant("acme"), TimeRange::all())
        .expect("query a");
    let g = s2
        .query(&tenant("globex"), TimeRange::all())
        .expect("query g");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].body, "acme-only");
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].body, "globex-only");
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.7 — query_with works against recovered state
// --------------------------------------------------------------------

#[test]
fn query_with_predicate_works_against_recovered_state() {
    use lumen::Predicate;
    let base = temp_base("predicate");
    {
        let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![
                record(100, "checkout", "a"),
                record(200, "billing", "b"),
                record(300, "checkout", "c"),
            ]),
        )
        .expect("ingest");
    }
    let s2 = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = s2
        .query_with(
            &tenant("acme"),
            TimeRange::all(),
            &Predicate::new().service("checkout"),
        )
        .expect("query_with");
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|r| r
        .resource_attributes
        .get("service.name")
        .map(|s| s.as_str())
        == Some("checkout")));
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.8 — corrupted WAL surfaces as PersistenceFailed
// --------------------------------------------------------------------

#[test]
fn corrupted_wal_surfaces_typed_persistence_error() {
    let base = temp_base("corrupted");
    {
        let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(100, "a", "good")]),
        )
        .expect("ingest");
    }
    let wal_path = {
        let mut p = base.as_os_str().to_owned();
        p.push(".wal");
        PathBuf::from(p)
    };
    let existing = fs::read_to_string(&wal_path).expect("read");
    fs::write(&wal_path, format!("{existing}{{not valid json}}\n")).expect("write");

    let err = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect_err("should fail");
    assert!(matches!(err, LogStoreError::PersistenceFailed { .. }));
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.9 — empty batch ingest is a no-op
// --------------------------------------------------------------------

#[test]
fn empty_batch_ingest_writes_nothing_to_wal() {
    let base = temp_base("empty_batch");
    {
        let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        let r = s
            .ingest(&tenant("acme"), LogBatch::with_records(vec![]))
            .expect("ingest empty");
        assert_eq!(r.count, 0);
    }
    // WAL should be empty.
    let wal_path = {
        let mut p = base.as_os_str().to_owned();
        p.push(".wal");
        PathBuf::from(p)
    };
    let meta = fs::metadata(&wal_path).expect("meta");
    assert_eq!(meta.len(), 0, "empty batch must not write to WAL");
    cleanup(&base);
}

// --------------------------------------------------------------------
// KPI 1 — ingest p95 ≤ 1.5 ms per 100-record batch
//
// 1.5 ms not 500 µs: v1 pays three costs v0 doesn't — batch clone for
// WAL serialisation, JSON encoding of 100 records, BufWriter flush.
// Plus the same sort-after-extend cost as v0. See
// docs/feature/lumen-v1/discuss/outcome-kpis.md § KPI 1 for the
// honesty rationale (mirrors Ray, Aegis, Cinder v1, Sluice v1).
// --------------------------------------------------------------------

#[test]
fn ingest_p95_latency_under_one_point_five_milliseconds() {
    let base = temp_base("kpi1");
    let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let tn = tenant("perf");

    fn make_batch(seed: u64) -> LogBatch {
        LogBatch::with_records(
            (0..100)
                .map(|i| record(seed * 1000 + i, "perf-svc", "perf body"))
                .collect(),
        )
    }

    for i in 0..50 {
        s.ingest(&tn, make_batch(i)).expect("warm");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let batch = make_batch(50 + i);
        let t0 = std::time::Instant::now();
        s.ingest(&tn, batch).expect("ingest");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[950];
    assert!(
        p95 <= 1_500,
        "KPI 1: ingest p95 must be ≤ 1.5 ms (1500 µs); got {p95} µs (first 10 {:?})",
        &samples[..10]
    );
    cleanup(&base);
}
