// Kaleidoscope Pulse — slice 02 structured query acceptance test
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

//! Slice 02 — `Predicate` + service / label_eq filters
//!
//! Maps to `docs/feature/pulse-v0/slices/slice-02-structured-query.md`.
//! Companion story: US-PU-02. KPI 2: query p95 ≤ 10 ms over 10k points.

use std::collections::BTreeMap;

use aegis::TenantId;
use pulse::{
    CapturingRecorder, InMemoryMetricStore, Metric, MetricBatch, MetricKind, MetricName,
    MetricPoint, MetricStore, NoopRecorder, Predicate, RecordedEvent, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn name(s: &str) -> MetricName {
    MetricName::new(s)
}

fn point_with(time_unix_nano: u64, value: f64, attrs: &[(&str, &str)]) -> MetricPoint {
    let mut attributes = BTreeMap::new();
    for (k, v) in attrs {
        attributes.insert((*k).to_string(), (*v).to_string());
    }
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes,
        value,
    }
}

fn gauge_with_service(name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(name),
        description: String::new(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

// --------------------------------------------------------------------
// AC-2.1 — service filter
// --------------------------------------------------------------------

#[test]
fn service_predicate_filters_by_resource_attribute() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    // Same metric name, different services → different
    // `(tenant, metric_name)` keys would conflate them, so we
    // query by metric_name only; the predicate narrows by
    // resource attribute.
    //
    // Two different metrics with the same name should not exist
    // in OTel; the resource is part of the identity. To test
    // service filtering we use one shared series and a second
    // distinct one to demonstrate the v0 semantics: the
    // predicate either matches the series' resource or not.
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge_with_service(
                "rps",
                "checkout",
                vec![point_with(100, 100.0, &[])],
            )]),
        )
        .expect("ingest");
    store
        .ingest(
            &tenant("other"),
            MetricBatch::with_metrics(vec![gauge_with_service(
                "rps",
                "billing",
                vec![point_with(100, 200.0, &[])],
            )]),
        )
        .expect("ingest");

    // Predicate matches.
    let out_match = store
        .query_with(
            &t,
            &name("rps"),
            TimeRange::all(),
            &Predicate::new().service("checkout"),
        )
        .expect("query");
    assert_eq!(out_match.len(), 1);
    assert_eq!(out_match[0].1.value, 100.0);

    // Predicate excludes.
    let out_exclude = store
        .query_with(
            &t,
            &name("rps"),
            TimeRange::all(),
            &Predicate::new().service("billing"),
        )
        .expect("query");
    assert!(out_exclude.is_empty());
}

// --------------------------------------------------------------------
// AC-2.2 — label_eq filter
// --------------------------------------------------------------------

#[test]
fn label_eq_predicate_filters_by_point_attribute() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge_with_service(
                "http.server.duration",
                "checkout",
                vec![
                    point_with(100, 0.10, &[("http.route", "/a")]),
                    point_with(200, 0.20, &[("http.route", "/b")]),
                    point_with(300, 0.30, &[("http.route", "/a")]),
                ],
            )]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &name("http.server.duration"),
            TimeRange::all(),
            &Predicate::new().label_eq("http.route", "/a"),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].1.value, 0.10);
    assert_eq!(out[1].1.value, 0.30);
}

// --------------------------------------------------------------------
// AC-2.3 — composition (intersection)
// --------------------------------------------------------------------

#[test]
fn service_and_multiple_label_eq_compose_as_intersection() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge_with_service(
                "http.server.duration",
                "checkout",
                vec![
                    point_with(
                        100,
                        0.10,
                        &[("http.route", "/a"), ("http.status_code", "200")],
                    ),
                    point_with(
                        200,
                        0.20,
                        &[("http.route", "/a"), ("http.status_code", "500")],
                    ),
                    point_with(
                        300,
                        0.30,
                        &[("http.route", "/b"), ("http.status_code", "200")],
                    ),
                ],
            )]),
        )
        .expect("ingest");

    let predicate = Predicate::new()
        .service("checkout")
        .label_eq("http.route", "/a")
        .label_eq("http.status_code", "500");
    let out = store
        .query_with(
            &t,
            &name("http.server.duration"),
            TimeRange::all(),
            &predicate,
        )
        .expect("query");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].1.value, 0.20);
}

// --------------------------------------------------------------------
// AC-2.4 — empty predicate ≡ range-only query
// --------------------------------------------------------------------

#[test]
fn empty_predicate_equals_range_only_query() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge_with_service(
                "m",
                "s",
                vec![point_with(100, 1.0, &[]), point_with(200, 2.0, &[])],
            )]),
        )
        .expect("ingest");

    let with_empty = store
        .query_with(&t, &name("m"), TimeRange::all(), &Predicate::new())
        .expect("query_with");
    let without = store
        .query(&t, &name("m"), TimeRange::all())
        .expect("query");
    assert_eq!(with_empty.len(), without.len());
    for (a, b) in with_empty.iter().zip(without.iter()) {
        assert_eq!(a.1, b.1);
    }
    assert!(Predicate::new().is_empty());
}

// --------------------------------------------------------------------
// AC-2.5 — no matches is Ok(Vec::new())
// --------------------------------------------------------------------

#[test]
fn predicate_with_no_matches_returns_empty_not_error() {
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge_with_service(
                "m",
                "checkout",
                vec![point_with(100, 1.0, &[("http.route", "/a")])],
            )]),
        )
        .expect("ingest");

    let out = store
        .query_with(
            &t,
            &name("m"),
            TimeRange::all(),
            &Predicate::new()
                .service("billing")
                .label_eq("http.route", "/z"),
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
    let store = InMemoryMetricStore::new(Box::new(recorder.clone()));
    let t = tenant("acme");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge_with_service(
                "m",
                "s",
                vec![
                    point_with(100, 1.0, &[("k", "v")]),
                    point_with(200, 2.0, &[("k", "v")]),
                ],
            )]),
        )
        .expect("ingest");

    let _ = store
        .query_with(
            &t,
            &name("m"),
            TimeRange::all(),
            &Predicate::new().label_eq("k", "v"),
        )
        .expect("query");

    let events = recorder.snapshot();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0],
        RecordedEvent::Ingest { point_count: 2, .. }
    ));
    assert!(matches!(
        events[1],
        RecordedEvent::Query {
            matched_count: 2,
            ..
        }
    ));
}

// --------------------------------------------------------------------
// KPI 2 — query p95 ≤ 10 ms over 10 000 points
// --------------------------------------------------------------------

#[test]
fn query_p95_latency_under_ten_milliseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let store = InMemoryMetricStore::new(Box::new(NoopRecorder));
    let t = tenant("perf");

    // 10 000 points under one metric, alternating across 4 routes.
    let routes = ["/a", "/b", "/c", "/d"];
    let points: Vec<MetricPoint> = (0..10_000u64)
        .map(|i| {
            let route = routes[(i as usize) % routes.len()];
            point_with(i + 1, i as f64, &[("http.route", route)])
        })
        .collect();
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge_with_service(
                "http.server.duration",
                "checkout",
                points,
            )]),
        )
        .expect("ingest");

    let predicate = Predicate::new()
        .service("checkout")
        .label_eq("http.route", "/a");

    for _ in 0..20 {
        let _ = store.query_with(
            &t,
            &name("http.server.duration"),
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
                &name("http.server.duration"),
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
        "KPI 2: query p95 must be ≤ 10 ms (10 000 µs); got {p95} µs (first samples {:?})",
        &samples[..10]
    );
}
