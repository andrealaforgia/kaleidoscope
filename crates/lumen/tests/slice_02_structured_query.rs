// Kaleidoscope Lumen — slice 02 structured query acceptance test
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

//! Slice 02 — `Predicate` + service / severity filters
//!
//! Maps to `docs/feature/lumen-v0/slices/slice-02-structured-query.md`.
//! Companion story: US-LU-02. KPI 2: query p95 ≤ 10 ms over 10k records.

use std::collections::BTreeMap;

use aegis::TenantId;
use lumen::{
    CapturingRecorder, InMemoryLogStore, LogBatch, LogRecord, LogStore, NoopRecorder, Predicate,
    RecordedEvent, SeverityNumber, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(
    observed_time_unix_nano: u64,
    service: &str,
    severity: SeverityNumber,
    severity_text: &str,
    body: &str,
) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    LogRecord {
        observed_time_unix_nano,
        severity_number: severity,
        severity_text: severity_text.to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

// --------------------------------------------------------------------
// AC-2.1 — service filter
// --------------------------------------------------------------------

#[test]
fn service_predicate_filters_by_service_name_resource_attribute() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            LogBatch::with_records(vec![
                record(100, "checkout", SeverityNumber::INFO, "INFO", "c1"),
                record(200, "billing", SeverityNumber::INFO, "INFO", "b1"),
                record(300, "checkout", SeverityNumber::INFO, "INFO", "c2"),
                record(400, "billing", SeverityNumber::INFO, "INFO", "b2"),
            ]),
        )
        .expect("ingest");

    let out = store
        .query_with(&t, TimeRange::all(), &Predicate::new().service("checkout"))
        .expect("query");
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].body, "c1");
    assert_eq!(out[1].body, "c2");
}

#[test]
fn service_predicate_excludes_records_without_service_name_resource_attribute() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");

    let no_service = LogRecord {
        observed_time_unix_nano: 100,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: "anonymous".to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: BTreeMap::new(),
        trace_id: None,
        span_id: None,
    };
    store
        .ingest(
            &t,
            LogBatch::with_records(vec![
                no_service,
                record(200, "checkout", SeverityNumber::INFO, "INFO", "named"),
            ]),
        )
        .expect("ingest");

    let out = store
        .query_with(&t, TimeRange::all(), &Predicate::new().service("checkout"))
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].body, "named");
}

// --------------------------------------------------------------------
// AC-2.2 — severity floor
// --------------------------------------------------------------------

#[test]
fn min_severity_predicate_filters_below_floor() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            LogBatch::with_records(vec![
                record(100, "checkout", SeverityNumber::DEBUG, "DEBUG", "d"),
                record(200, "checkout", SeverityNumber::INFO, "INFO", "i"),
                record(300, "checkout", SeverityNumber::WARN, "WARN", "w"),
                record(400, "checkout", SeverityNumber::ERROR, "ERROR", "e"),
            ]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            TimeRange::all(),
            &Predicate::new().min_severity(SeverityNumber::WARN),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].body, "w");
    assert_eq!(out[1].body, "e");
}

// --------------------------------------------------------------------
// AC-2.3 — composition (intersection)
// --------------------------------------------------------------------

#[test]
fn service_and_min_severity_compose_as_intersection() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            LogBatch::with_records(vec![
                record(100, "checkout", SeverityNumber::INFO, "INFO", "ck-info"),
                record(200, "checkout", SeverityNumber::ERROR, "ERROR", "ck-err"),
                record(300, "billing", SeverityNumber::INFO, "INFO", "bl-info"),
                record(400, "billing", SeverityNumber::ERROR, "ERROR", "bl-err"),
            ]),
        )
        .expect("ingest");

    let predicate = Predicate::new()
        .service("checkout")
        .min_severity(SeverityNumber::ERROR);
    let out = store
        .query_with(&t, TimeRange::all(), &predicate)
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].body, "ck-err");
}

// --------------------------------------------------------------------
// AC-2.4 — empty predicate equals range-only query
// --------------------------------------------------------------------

#[test]
fn empty_predicate_equals_range_only_query() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            LogBatch::with_records(vec![
                record(100, "a", SeverityNumber::INFO, "INFO", "x"),
                record(200, "b", SeverityNumber::ERROR, "ERROR", "y"),
            ]),
        )
        .expect("ingest");

    let with_empty = store
        .query_with(&t, TimeRange::all(), &Predicate::new())
        .expect("query_with");
    let without = store.query(&t, TimeRange::all()).expect("query");
    assert_eq!(with_empty, without);
    assert_eq!(with_empty.len(), 2);
    assert!(Predicate::new().is_empty());
}

// --------------------------------------------------------------------
// AC-2.5 — no matches is Ok(Vec::new())
// --------------------------------------------------------------------

#[test]
fn predicate_with_no_matches_returns_empty_not_error() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            LogBatch::with_records(vec![record(
                100,
                "checkout",
                SeverityNumber::INFO,
                "INFO",
                "x",
            )]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            TimeRange::all(),
            &Predicate::new()
                .service("billing")
                .min_severity(SeverityNumber::FATAL),
        )
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// MetricsRecorder seam — every query emits one event
// --------------------------------------------------------------------

#[test]
fn capturing_recorder_observes_every_query_with_matched_count() {
    let recorder = CapturingRecorder::new();
    let store = InMemoryLogStore::new(Box::new(recorder.clone()));
    let t = tenant("acme");

    store
        .ingest(
            &t,
            LogBatch::with_records(vec![
                record(100, "checkout", SeverityNumber::ERROR, "ERROR", "a"),
                record(200, "checkout", SeverityNumber::INFO, "INFO", "b"),
            ]),
        )
        .expect("ingest");

    let _ = store
        .query_with(
            &t,
            TimeRange::all(),
            &Predicate::new().min_severity(SeverityNumber::WARN),
        )
        .expect("query");

    let events = recorder.snapshot();
    assert_eq!(events.len(), 2, "ingest + query events expected");
    assert!(matches!(
        events[0],
        RecordedEvent::Ingest {
            record_count: 2,
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
// KPI 2 — query p95 ≤ 10 ms over 10 000 records
// --------------------------------------------------------------------

#[test]
fn query_p95_latency_under_ten_milliseconds() {
    let store = InMemoryLogStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    // Seed 10 000 records spanning 4 services and 4 severities.
    let services = ["checkout", "billing", "shipping", "auth"];
    let severities = [
        (SeverityNumber::DEBUG, "DEBUG"),
        (SeverityNumber::INFO, "INFO"),
        (SeverityNumber::WARN, "WARN"),
        (SeverityNumber::ERROR, "ERROR"),
    ];
    let mut batch = LogBatch::new();
    for i in 0..10_000u64 {
        let svc = services[(i as usize) % services.len()];
        let (sev, sev_text) = severities[(i as usize) % severities.len()];
        batch.push(record(i + 1, svc, sev, sev_text, "body"));
    }
    store.ingest(&t, batch).expect("ingest");

    let predicate = Predicate::new()
        .service("checkout")
        .min_severity(SeverityNumber::WARN);

    // Warm up.
    for _ in 0..20 {
        let _ = store.query_with(&t, TimeRange::all(), &predicate);
    }

    let mut samples: Vec<u128> = Vec::with_capacity(200);
    for _ in 0..200 {
        let t0 = std::time::Instant::now();
        let _ = store
            .query_with(&t, TimeRange::all(), &predicate)
            .expect("query");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[190]; // index 95% of 200 = 190
                            // KPI 2 ceiling: 10 ms (10 000 µs).
    assert!(
        p95 <= 10_000,
        "KPI 2: query p95 must be ≤ 10 ms (10 000 µs); got {p95} µs (samples [..10]: {:?})",
        &samples[..10]
    );
}
