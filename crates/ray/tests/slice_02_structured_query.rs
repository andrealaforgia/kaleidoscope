// Kaleidoscope Ray — slice 02 structured query acceptance test
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

//! Slice 02 — Predicate + span_name / kind / status filters
//!
//! Maps to `docs/feature/ray-v0/slices/slice-02-structured-query.md`.

use std::collections::BTreeMap;

use aegis::TenantId;
use ray::{
    CapturingRecorder, InMemoryTraceStore, NoopRecorder, Predicate, RecordedEvent, ServiceName,
    Span, SpanBatch, SpanId, SpanKind, SpanStatus, StatusCode, TimeRange, TraceId, TraceStore,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn span_with(
    name: &str,
    kind: SpanKind,
    status: StatusCode,
    service: &str,
    start: u64,
    trace_byte: u8,
    span_byte: u8,
) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Span {
        trace_id: TraceId([trace_byte; 16]),
        span_id: SpanId([span_byte; 8]),
        parent_span_id: None,
        name: name.to_string(),
        kind,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 10,
        status: SpanStatus {
            code: status,
            message: String::new(),
        },
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

// --------------------------------------------------------------------
// AC-2.1 — span_name filter
// --------------------------------------------------------------------

#[test]
fn span_name_predicate_filters_by_name() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![
                span_with(
                    "db.query",
                    SpanKind::Client,
                    StatusCode::Ok,
                    "checkout",
                    100,
                    1,
                    1,
                ),
                span_with(
                    "http.handle",
                    SpanKind::Server,
                    StatusCode::Ok,
                    "checkout",
                    200,
                    1,
                    2,
                ),
                span_with(
                    "db.query",
                    SpanKind::Client,
                    StatusCode::Ok,
                    "checkout",
                    300,
                    1,
                    3,
                ),
            ]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new().span_name("db.query"),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|s| s.name == "db.query"));
}

// --------------------------------------------------------------------
// AC-2.2 — kind filter
// --------------------------------------------------------------------

#[test]
fn kind_predicate_filters_by_span_kind() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![
                span_with("a", SpanKind::Client, StatusCode::Ok, "checkout", 100, 1, 1),
                span_with("b", SpanKind::Server, StatusCode::Ok, "checkout", 200, 1, 2),
                span_with("c", SpanKind::Client, StatusCode::Ok, "checkout", 300, 1, 3),
            ]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new().kind(SpanKind::Client),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
}

// --------------------------------------------------------------------
// AC-2.3 — status filter
// --------------------------------------------------------------------

#[test]
fn status_predicate_filters_by_status_code() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![
                span_with("a", SpanKind::Server, StatusCode::Ok, "checkout", 100, 1, 1),
                span_with(
                    "b",
                    SpanKind::Server,
                    StatusCode::Error,
                    "checkout",
                    200,
                    1,
                    2,
                ),
                span_with(
                    "c",
                    SpanKind::Server,
                    StatusCode::Unset,
                    "checkout",
                    300,
                    1,
                    3,
                ),
            ]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new().status(StatusCode::Error),
        )
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "b");
}

// --------------------------------------------------------------------
// AC-2.4 — composition
// --------------------------------------------------------------------

#[test]
fn span_name_and_kind_and_status_compose_as_intersection() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![
                // Matches all three.
                span_with(
                    "db.query",
                    SpanKind::Client,
                    StatusCode::Error,
                    "checkout",
                    100,
                    1,
                    1,
                ),
                // Wrong status.
                span_with(
                    "db.query",
                    SpanKind::Client,
                    StatusCode::Ok,
                    "checkout",
                    200,
                    1,
                    2,
                ),
                // Wrong kind.
                span_with(
                    "db.query",
                    SpanKind::Server,
                    StatusCode::Error,
                    "checkout",
                    300,
                    1,
                    3,
                ),
                // Wrong name.
                span_with(
                    "http.handle",
                    SpanKind::Client,
                    StatusCode::Error,
                    "checkout",
                    400,
                    1,
                    4,
                ),
            ]),
        )
        .expect("ingest");

    let predicate = Predicate::new()
        .span_name("db.query")
        .kind(SpanKind::Client)
        .status(StatusCode::Error);
    let out = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &predicate,
        )
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].start_time_unix_nano, 100);
}

// --------------------------------------------------------------------
// AC-2.5 — empty predicate ≡ range-only
// --------------------------------------------------------------------

#[test]
fn empty_predicate_equals_range_only_query() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![
                span_with("a", SpanKind::Server, StatusCode::Ok, "checkout", 100, 1, 1),
                span_with(
                    "b",
                    SpanKind::Client,
                    StatusCode::Error,
                    "checkout",
                    200,
                    1,
                    2,
                ),
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
// AC-2.6 — no matches → Ok(Vec::new())
// --------------------------------------------------------------------

#[test]
fn predicate_with_no_matches_returns_empty_not_error() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![span_with(
                "a",
                SpanKind::Server,
                StatusCode::Ok,
                "checkout",
                100,
                1,
                1,
            )]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new()
                .span_name("never")
                .status(StatusCode::Error),
        )
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// MetricsRecorder seam — every operation emits one event
// --------------------------------------------------------------------

#[test]
fn capturing_recorder_observes_every_operation() {
    let recorder = CapturingRecorder::new();
    let store = InMemoryTraceStore::new(Box::new(recorder.clone()));
    let t = tenant("acme");

    store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![
                span_with(
                    "a",
                    SpanKind::Server,
                    StatusCode::Error,
                    "checkout",
                    100,
                    1,
                    1,
                ),
                span_with("b", SpanKind::Server, StatusCode::Ok, "checkout", 200, 1, 2),
            ]),
        )
        .expect("ingest");

    let _ = store
        .query_with(
            &t,
            &ServiceName::new("checkout"),
            TimeRange::all(),
            &Predicate::new().status(StatusCode::Error),
        )
        .expect("query");

    let events = recorder.snapshot();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0],
        RecordedEvent::Ingest { span_count: 2, .. }
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
// KPI 2 — query p95 ≤ 10 ms over 10 000 spans
// --------------------------------------------------------------------

#[test]
fn query_p95_latency_under_ten_milliseconds() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    let names = ["db.query", "http.handle", "cache.read", "queue.publish"];
    let kinds = [
        SpanKind::Client,
        SpanKind::Server,
        SpanKind::Internal,
        SpanKind::Producer,
    ];
    let statuses = [StatusCode::Ok, StatusCode::Error, StatusCode::Unset];

    let mut batch = SpanBatch::new();
    for i in 0..10_000u64 {
        let name = names[(i as usize) % names.len()];
        let kind = kinds[(i as usize) % kinds.len()];
        let status = statuses[(i as usize) % statuses.len()];
        batch.push(span_with(
            name,
            kind,
            status,
            "checkout",
            i + 1,
            (i % 256) as u8,
            (i % 256) as u8,
        ));
    }
    store.ingest(&t, batch).expect("ingest");

    let predicate = Predicate::new()
        .span_name("db.query")
        .status(StatusCode::Error);

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
