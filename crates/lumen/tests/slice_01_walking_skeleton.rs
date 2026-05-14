// Kaleidoscope Lumen — slice 01 walking skeleton acceptance test
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

//! Slice 01 — `LogStore::ingest` + `LogStore::query` walking skeleton
//!
//! Maps to `docs/feature/lumen-v0/slices/slice-01-walking-skeleton.md`.
//! Companion story: US-LU-01.

use std::collections::BTreeMap;

use aegis::TenantId;
use lumen::{
    InMemoryLogStore, LogBatch, LogRecord, LogStore, NoopRecorder, SeverityNumber, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed_time_unix_nano: u64, service: &str, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    LogRecord {
        observed_time_unix_nano,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 / AC-1.3 — ingest + time-range query in order
// --------------------------------------------------------------------

#[test]
fn ingest_then_query_returns_records_in_observed_time_order() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let batch = LogBatch::with_records(vec![
        record(300, "checkout", "third"),
        record(100, "checkout", "first"),
        record(200, "checkout", "second"),
    ]);

    let receipt = store.ingest(&t, batch).expect("ingest");
    assert_eq!(receipt.count, 3);

    let out = store.query(&t, TimeRange::all()).expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].observed_time_unix_nano, 100);
    assert_eq!(out[1].observed_time_unix_nano, 200);
    assert_eq!(out[2].observed_time_unix_nano, 300);
    assert_eq!(out[0].body, "first");
    assert_eq!(out[1].body, "second");
    assert_eq!(out[2].body, "third");
}

#[test]
fn query_with_time_range_returns_only_matching_records() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let batch = LogBatch::with_records(vec![
        record(100, "checkout", "one"),
        record(200, "checkout", "two"),
        record(300, "checkout", "three"),
        record(400, "checkout", "four"),
    ]);
    store.ingest(&t, batch).expect("ingest");

    // [200, 400) → matches 200 and 300, not 400 (half-open).
    let out = store.query(&t, TimeRange::new(200, 400)).expect("query");
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].body, "two");
    assert_eq!(out[1].body, "three");
}

#[test]
fn multiple_ingests_compose_and_remain_sorted() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");

    store
        .ingest(&t, LogBatch::with_records(vec![record(200, "a", "b2")]))
        .expect("ingest 1");
    store
        .ingest(
            &t,
            LogBatch::with_records(vec![record(100, "a", "b1"), record(300, "a", "b3")]),
        )
        .expect("ingest 2");

    let out = store.query(&t, TimeRange::all()).expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].body, "b1");
    assert_eq!(out[1].body, "b2");
    assert_eq!(out[2].body, "b3");
}

// --------------------------------------------------------------------
// AC-1.4 — tenant isolation
// --------------------------------------------------------------------

#[test]
fn two_tenants_records_are_isolated() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let acme = tenant("acme");
    let globex = tenant("globex");

    store
        .ingest(
            &acme,
            LogBatch::with_records(vec![record(100, "a", "acme-only")]),
        )
        .expect("ingest acme");
    store
        .ingest(
            &globex,
            LogBatch::with_records(vec![record(200, "b", "globex-only")]),
        )
        .expect("ingest globex");

    let acme_out = store.query(&acme, TimeRange::all()).expect("acme query");
    let globex_out = store
        .query(&globex, TimeRange::all())
        .expect("globex query");
    assert_eq!(acme_out.len(), 1);
    assert_eq!(acme_out[0].body, "acme-only");
    assert_eq!(globex_out.len(), 1);
    assert_eq!(globex_out[0].body, "globex-only");
}

#[test]
fn query_on_unknown_tenant_returns_empty_vec() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let out = store
        .query(&tenant("ghost"), TimeRange::all())
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// AC-1.5 — byte-stable field preservation
// --------------------------------------------------------------------

#[test]
fn every_field_round_trips_byte_stable() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");

    let mut attributes = BTreeMap::new();
    attributes.insert("http.status_code".to_string(), "503".to_string());
    attributes.insert("http.method".to_string(), "POST".to_string());

    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    resource.insert("service.version".to_string(), "2.4.1".to_string());

    let original = LogRecord {
        observed_time_unix_nano: 1_700_000_000_000_000_000,
        severity_number: SeverityNumber::ERROR,
        severity_text: "ERROR".to_string(),
        body: "payment authorisation declined".to_string(),
        attributes: attributes.clone(),
        resource_attributes: resource.clone(),
        trace_id: Some([0x11; 16]),
        span_id: Some([0x22; 8]),
    };
    store
        .ingest(&t, LogBatch::with_records(vec![original.clone()]))
        .expect("ingest");

    let out = store.query(&t, TimeRange::all()).expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], original);
}

// --------------------------------------------------------------------
// AC-1.6 — empty result is Ok(Vec::new())
// --------------------------------------------------------------------

#[test]
fn empty_range_returns_ok_empty_not_error() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(&t, LogBatch::with_records(vec![record(100, "a", "b")]))
        .expect("ingest");

    // Range with no matches.
    let out = store.query(&t, TimeRange::new(500, 1000)).expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// KPI 1 — ingest latency p95 ≤ 1 ms per 100-record batch
// --------------------------------------------------------------------

#[test]
fn ingest_p95_latency_under_one_millisecond() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    fn make_batch(seed: u64) -> LogBatch {
        let records: Vec<LogRecord> = (0..100)
            .map(|i| record(seed * 1000 + i, "perf-svc", "perf body"))
            .collect();
        LogBatch::with_records(records)
    }

    // Warm up.
    for i in 0..50 {
        store.ingest(&t, make_batch(i)).expect("warmup");
    }

    // Measure 1000 ingests of 100-record batches.
    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let batch = make_batch(50 + i);
        let t0 = std::time::Instant::now();
        store.ingest(&t, batch).expect("ingest");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[950];
    // KPI 1 ceiling: 1 ms (1000 µs).
    assert!(
        p95 <= 1_000,
        "KPI 1: ingest p95 must be ≤ 1 ms (1000 µs); got {p95} µs (samples [..10]: {:?})",
        &samples[..10]
    );
}
