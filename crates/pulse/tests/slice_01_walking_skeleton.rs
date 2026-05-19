// Kaleidoscope Pulse — slice 01 walking skeleton acceptance test
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

//! Slice 01 — `MetricStore::ingest` + `MetricStore::query`
//!
//! Maps to `docs/feature/pulse-v0/slices/slice-01-walking-skeleton.md`.
//! Companion story: US-PU-01.

use std::collections::BTreeMap;

use aegis::TenantId;
use pulse::{
    InMemoryMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn name(s: &str) -> MetricName {
    MetricName::new(s)
}

fn point(time_unix_nano: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value,
    }
}

fn gauge(name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(name),
        description: "test gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 / AC-1.3 — ingest + time-range query in order
// --------------------------------------------------------------------

#[test]
fn ingest_then_query_returns_points_in_time_order() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let metric = gauge(
        "cpu.utilization",
        "checkout",
        vec![point(300, 0.30), point(100, 0.10), point(200, 0.20)],
    );
    let receipt = store
        .ingest(&t, MetricBatch::with_metrics(vec![metric]))
        .expect("ingest");
    assert_eq!(receipt.count, 3);

    let out = store
        .query(&t, &name("cpu.utilization"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].1.time_unix_nano, 100);
    assert_eq!(out[1].1.time_unix_nano, 200);
    assert_eq!(out[2].1.time_unix_nano, 300);
    assert_eq!(out[0].1.value, 0.10);
}

#[test]
fn query_with_time_range_returns_only_matching_points() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    let metric = gauge(
        "cpu.utilization",
        "checkout",
        (1..=4).map(|i| point(i * 100, i as f64)).collect(),
    );
    store
        .ingest(&t, MetricBatch::with_metrics(vec![metric]))
        .expect("ingest");

    // [200, 400) → matches 200 and 300.
    let out = store
        .query(&t, &name("cpu.utilization"), TimeRange::new(200, 400))
        .expect("query");
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].1.time_unix_nano, 200);
    assert_eq!(out[1].1.time_unix_nano, 300);
}

#[test]
fn multiple_ingests_compose_and_remain_sorted() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");

    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge("m", "s", vec![point(200, 2.0)])]),
        )
        .expect("ingest 1");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "m",
                "s",
                vec![point(100, 1.0), point(300, 3.0)],
            )]),
        )
        .expect("ingest 2");

    let out = store
        .query(&t, &name("m"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].1.value, 1.0);
    assert_eq!(out[1].1.value, 2.0);
    assert_eq!(out[2].1.value, 3.0);
}

// --------------------------------------------------------------------
// AC-1.4 — tenant isolation
// --------------------------------------------------------------------

#[test]
fn two_tenants_points_are_isolated() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let acme = tenant("acme");
    let globex = tenant("globex");

    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge("m", "s", vec![point(100, 1.0)])]),
        )
        .expect("ingest acme");
    store
        .ingest(
            &globex,
            MetricBatch::with_metrics(vec![gauge("m", "s", vec![point(200, 2.0)])]),
        )
        .expect("ingest globex");

    let a = store
        .query(&acme, &name("m"), TimeRange::all())
        .expect("acme");
    let g = store
        .query(&globex, &name("m"), TimeRange::all())
        .expect("globex");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].1.value, 1.0);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].1.value, 2.0);
}

#[test]
fn query_on_unknown_metric_returns_empty_vec() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let out = store
        .query(&tenant("acme"), &name("unknown"), TimeRange::all())
        .expect("query");
    assert!(out.is_empty());
}

#[test]
fn query_on_unknown_tenant_returns_empty_vec() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let out = store
        .query(&tenant("ghost"), &name("m"), TimeRange::all())
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// AC-1.5 — byte-stable field preservation
// --------------------------------------------------------------------

#[test]
fn every_field_round_trips_byte_stable() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");

    let mut attributes = BTreeMap::new();
    attributes.insert("http.route".to_string(), "/api/checkout".to_string());
    attributes.insert("http.status_code".to_string(), "503".to_string());

    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    resource.insert("service.version".to_string(), "2.4.1".to_string());

    let original_point = MetricPoint {
        time_unix_nano: 1_700_000_000_000_000_000,
        start_time_unix_nano: 1_699_999_999_000_000_000,
        attributes: attributes.clone(),
        value: 0.756,
    };
    let original_metric = Metric {
        name: MetricName::new("http.server.duration"),
        description: "HTTP server request duration".to_string(),
        unit: "s".to_string(),
        kind: MetricKind::Sum,
        points: vec![original_point.clone()],
        resource_attributes: resource.clone(),
    };
    store
        .ingest(&t, MetricBatch::with_metrics(vec![original_metric.clone()]))
        .expect("ingest");

    let out = store
        .query(&t, &name("http.server.duration"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 1);
    let (returned_metric, returned_point) = &out[0];
    // Point field-by-field.
    assert_eq!(returned_point, &original_point);
    // Metric metadata field-by-field (points are returned
    // separately, so the metadata copy should hold).
    assert_eq!(returned_metric.name, original_metric.name);
    assert_eq!(returned_metric.description, original_metric.description);
    assert_eq!(returned_metric.unit, original_metric.unit);
    assert_eq!(returned_metric.kind, original_metric.kind);
    assert_eq!(
        returned_metric.resource_attributes,
        original_metric.resource_attributes
    );
}

// --------------------------------------------------------------------
// AC-1.6 — empty range returns Ok(Vec::new())
// --------------------------------------------------------------------

#[test]
fn empty_range_returns_ok_empty_not_error() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge("m", "s", vec![point(100, 1.0)])]),
        )
        .expect("ingest");

    let out = store
        .query(&t, &name("m"), TimeRange::new(500, 1000))
        .expect("query");
    assert!(out.is_empty());
}

// --------------------------------------------------------------------
// KPI 1 — ingest latency p95 ≤ 2 ms per 100-point batch
//
// 2 ms not 1 ms: local-workstation baseline is ~50-90 µs; GitHub
// Actions ubuntu-latest sits around 1100-1400 µs under
// contention. Same CI-realism bump batch as Lumen v0 KPI 1 and
// Cinder KPI 2 (2026-05-19).
// --------------------------------------------------------------------

#[test]
fn ingest_p95_latency_under_two_milliseconds() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    fn make_batch(seed: u64) -> MetricBatch {
        let points: Vec<MetricPoint> = (0..100).map(|i| point(seed * 1000 + i, i as f64)).collect();
        MetricBatch::with_metrics(vec![Metric {
            name: MetricName::new("perf.metric"),
            description: String::new(),
            unit: "1".to_string(),
            kind: MetricKind::Gauge,
            points,
            resource_attributes: BTreeMap::new(),
        }])
    }

    for i in 0..50 {
        store.ingest(&t, make_batch(i)).expect("warmup");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let batch = make_batch(50 + i);
        let t0 = std::time::Instant::now();
        store.ingest(&t, batch).expect("ingest");
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[950];
    assert!(
        p95 <= 2_000,
        "KPI 1: ingest p95 must be ≤ 2 ms (2000 µs); got {p95} µs (first samples {:?})",
        &samples[..10]
    );
}
