// Kaleidoscope Ray — slice 01 walking skeleton acceptance test
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

//! Slice 01 — `TraceStore::ingest` + `get_trace` + `query`
//!
//! Maps to `docs/feature/ray-v0/slices/slice-01-walking-skeleton.md`.

use std::collections::BTreeMap;

use aegis::TenantId;
use ray::{
    InMemoryTraceStore, NoopRecorder, ServiceName, Span, SpanBatch, SpanEvent, SpanId, SpanKind,
    SpanLink, SpanStatus, StatusCode, TimeRange, TraceId, TraceStore,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn trace_id(byte: u8) -> TraceId {
    TraceId([byte; 16])
}

fn span_id(byte: u8) -> SpanId {
    SpanId([byte; 8])
}

fn span(trace_byte: u8, span_byte: u8, service: &str, name: &str, start: u64, end: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Span {
        trace_id: trace_id(trace_byte),
        span_id: span_id(span_byte),
        parent_span_id: None,
        name: name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        status: SpanStatus::default(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 — ingest + get_trace returns spans in time order
// --------------------------------------------------------------------

#[test]
fn ingest_then_get_trace_returns_spans_in_start_time_order() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let batch = SpanBatch::with_spans(vec![
        span(0xAA, 0x03, "checkout", "third", 300, 350),
        span(0xAA, 0x01, "checkout", "first", 100, 150),
        span(0xAA, 0x02, "checkout", "second", 200, 250),
    ]);
    let receipt = store.ingest(&t, batch).expect("ingest");
    assert_eq!(receipt.count, 3);

    let spans = store.get_trace(&t, &trace_id(0xAA)).expect("get_trace");
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].name, "first");
    assert_eq!(spans[1].name, "second");
    assert_eq!(spans[2].name, "third");
}

// --------------------------------------------------------------------
// AC-1.3 — query by (service, range)
// --------------------------------------------------------------------

#[test]
fn query_by_service_and_range_returns_only_matching_spans() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let batch = SpanBatch::with_spans(vec![
        span(0xAA, 0x01, "checkout", "early", 100, 150),
        span(0xAA, 0x02, "checkout", "middle", 200, 250),
        span(0xBB, 0x03, "checkout", "late", 400, 450),
        span(0xCC, 0x04, "billing", "noise", 200, 250),
    ]);
    store.ingest(&t, batch).expect("ingest");

    // checkout, [150, 400) → middle only.
    let out = store
        .query(&t, &ServiceName::new("checkout"), TimeRange::new(150, 400))
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "middle");
}

// --------------------------------------------------------------------
// AC-1.4 — tenant isolation
// --------------------------------------------------------------------

#[test]
fn two_tenants_traces_are_isolated() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let acme = tenant("acme");
    let globex = tenant("globex");

    store
        .ingest(
            &acme,
            SpanBatch::with_spans(vec![span(0xAA, 0x01, "s", "a", 100, 200)]),
        )
        .expect("ingest acme");
    store
        .ingest(
            &globex,
            SpanBatch::with_spans(vec![span(0xAA, 0x02, "s", "g", 100, 200)]),
        )
        .expect("ingest globex");

    let a = store.get_trace(&acme, &trace_id(0xAA)).expect("a");
    let g = store.get_trace(&globex, &trace_id(0xAA)).expect("g");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].name, "a");
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].name, "g");
}

// --------------------------------------------------------------------
// AC-1.5 — byte-stable field preservation (full field set)
// --------------------------------------------------------------------

#[test]
fn every_field_round_trips_byte_stable_including_events_and_links() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");

    let mut span_attrs = BTreeMap::new();
    span_attrs.insert("http.route".to_string(), "/api/checkout".to_string());
    span_attrs.insert("http.status_code".to_string(), "503".to_string());

    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    resource.insert("service.version".to_string(), "2.4.1".to_string());

    let mut event_attrs = BTreeMap::new();
    event_attrs.insert("exception.type".to_string(), "PaymentDeclined".to_string());
    let event = SpanEvent {
        time_unix_nano: 1_700_000_001_000_000_000,
        name: "payment.declined".to_string(),
        attributes: event_attrs,
    };

    let mut link_attrs = BTreeMap::new();
    link_attrs.insert("relation".to_string(), "follows-from".to_string());
    let link = SpanLink {
        trace_id: trace_id(0xBB),
        span_id: span_id(0x99),
        attributes: link_attrs,
    };

    let original = Span {
        trace_id: trace_id(0xAA),
        span_id: span_id(0x01),
        parent_span_id: Some(span_id(0x00)),
        name: "POST /api/checkout".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: 1_700_000_000_000_000_000,
        end_time_unix_nano: 1_700_000_002_000_000_000,
        status: SpanStatus {
            code: StatusCode::Error,
            message: "payment authorisation declined".to_string(),
        },
        attributes: span_attrs,
        resource_attributes: resource,
        events: vec![event],
        links: vec![link],
    };
    store
        .ingest(&t, SpanBatch::with_spans(vec![original.clone()]))
        .expect("ingest");

    let spans = store.get_trace(&t, &trace_id(0xAA)).expect("get_trace");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], original);
}

// --------------------------------------------------------------------
// AC-1.6 / AC-1.7 — empty results for unknowns
// --------------------------------------------------------------------

#[test]
fn get_trace_on_unknown_trace_id_returns_empty() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let spans = store
        .get_trace(&tenant("acme"), &trace_id(0xFF))
        .expect("get_trace");
    assert!(spans.is_empty());
}

#[test]
fn query_on_unknown_service_returns_empty() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
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
fn spans_without_service_name_are_unreachable_via_query_but_via_get_trace() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    // A span without a service.name resource attribute.
    let span_no_service = Span {
        resource_attributes: BTreeMap::new(),
        ..span(0xAA, 0x01, "_unused", "anonymous", 100, 200)
    };
    store
        .ingest(&t, SpanBatch::with_spans(vec![span_no_service.clone()]))
        .expect("ingest");

    // Reachable by trace_id.
    let spans = store.get_trace(&t, &trace_id(0xAA)).expect("get_trace");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "anonymous");

    // Not reachable by service query.
    let out = store
        .query(&t, &ServiceName::new("_unused"), TimeRange::all())
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// KPI 1 — ingest p95 ≤ 2 ms per 100-span batch
//
// Realistic OTLP batches contain spans from many traces and a small
// number of services. We seed 100 spans per batch across 10 distinct
// trace_ids (10 spans/trace, matching the typical depth of an
// instrumented request) and 4 services.
//
// The 2 ms ceiling (vs Lumen's / Pulse's 1 ms) reflects the dual
// `by_trace` + `by_service` index: every ingested span lands in two
// buckets and both are sorted-after-extend. See
// docs/feature/ray-v0/discuss/outcome-kpis.md § KPI 1 for rationale.
// --------------------------------------------------------------------

#[test]
fn ingest_p95_latency_under_two_milliseconds() {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    let services = ["checkout", "billing", "shipping", "auth"];

    fn make_batch(seed: u64, services: &[&str]) -> SpanBatch {
        let spans: Vec<Span> = (0..100)
            .map(|i| {
                // 10 distinct traces per batch — trace_byte
                // distinguishes by `(seed, trace_idx_within_batch)`.
                let trace_byte = ((seed.wrapping_mul(10) + i / 10) % 256) as u8;
                let span_byte = (i % 256) as u8;
                let service = services[(i as usize) % services.len()];
                span(
                    trace_byte,
                    span_byte,
                    service,
                    "perf",
                    seed * 1000 + i,
                    seed * 1000 + i + 10,
                )
            })
            .collect();
        SpanBatch::with_spans(spans)
    }

    for i in 0..50 {
        store.ingest(&t, make_batch(i, &services)).expect("warmup");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let batch = make_batch(50 + i, &services);
        let t0 = std::time::Instant::now();
        store.ingest(&t, batch).expect("ingest");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[950];
    assert!(
        p95 <= 2_000,
        "KPI 1: ingest p95 must be ≤ 2 ms (2000 µs); got {p95} µs (first 10 {:?})",
        &samples[..10]
    );
}
