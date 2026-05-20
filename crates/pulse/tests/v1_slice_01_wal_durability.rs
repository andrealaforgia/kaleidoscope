// Kaleidoscope Pulse v1 — slice 01 WAL durability acceptance test
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

//! Slice 01 — `FileBackedMetricStore::open` + ingest + query survive
//! a restart.
//!
//! Maps to `docs/feature/pulse-v1/slices/slice-01-wal-durability.md`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
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

fn gauge(metric_name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(metric_name),
        description: "test gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("pulse-v1-{test_name}-{pid}-{nanos}"));
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
// AC-1.1 / AC-1.2 — restart recovers points in ascending time order
// --------------------------------------------------------------------

#[test]
fn restart_recovers_points_in_time_order() {
    let base = temp_base("recover_order");
    {
        let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge(
                "cpu.utilization",
                "checkout",
                vec![point(300, 0.30), point(100, 0.10)],
            )]),
        )
        .expect("ingest 1");
        s.ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge(
                "cpu.utilization",
                "checkout",
                vec![point(200, 0.20)],
            )]),
        )
        .expect("ingest 2");
    }
    let s2 = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = s2
        .query(&tenant("acme"), &name("cpu.utilization"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].1.time_unix_nano, 100);
    assert_eq!(out[1].1.time_unix_nano, 200);
    assert_eq!(out[2].1.time_unix_nano, 300);
    assert_eq!(out[0].1.value, 0.10);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.3 — multiple ingests across reopen compose and stay sorted
// --------------------------------------------------------------------

#[test]
fn multiple_ingests_across_reopen_compose_and_remain_sorted() {
    let base = temp_base("compose_sorted");
    {
        let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge("m", "s", vec![point(200, 2.0)])]),
        )
        .expect("ingest 1");
    }
    {
        let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
        s.ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge(
                "m",
                "s",
                vec![point(100, 1.0), point(300, 3.0)],
            )]),
        )
        .expect("ingest 2");
    }
    let s3 = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 3");
    let out = s3
        .query(&tenant("acme"), &name("m"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].1.value, 1.0);
    assert_eq!(out[1].1.value, 2.0);
    assert_eq!(out[2].1.value, 3.0);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.4 — byte-stable round-trip of every field across restart
// --------------------------------------------------------------------

#[test]
fn every_field_round_trips_byte_stable_across_restart() {
    let base = temp_base("byte_stable");

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

    {
        let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        s.ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![original_metric.clone()]),
        )
        .expect("ingest");
    }
    let s2 = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = s2
        .query(
            &tenant("acme"),
            &name("http.server.duration"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 1);
    let (returned_metric, returned_point) = &out[0];
    assert_eq!(returned_point, &original_point);
    assert_eq!(returned_metric.name, original_metric.name);
    assert_eq!(returned_metric.description, original_metric.description);
    assert_eq!(returned_metric.unit, original_metric.unit);
    assert_eq!(returned_metric.kind, original_metric.kind);
    assert_eq!(
        returned_metric.resource_attributes,
        original_metric.resource_attributes
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.5 — empty batch ingest is a no-op (writes nothing to the WAL)
// --------------------------------------------------------------------

#[test]
fn empty_batch_ingest_writes_nothing_to_wal() {
    let base = temp_base("empty_batch");
    {
        let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        let r = s
            .ingest(&tenant("acme"), MetricBatch::with_metrics(vec![]))
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
// KPI 1 — ingest p95 ≤ 2 ms per 100-point batch
//
// 2 ms not 500 µs: v1 pays three costs v0 doesn't — batch clone for
// WAL serialisation, JSON encoding of 100 points, BufWriter flush.
// Plus the same sort-after-extend cost as v0. Local-workstation
// baseline is ~50-90 µs; GitHub Actions ubuntu-latest sits around
// 1100-1400 µs under contention. The 2 ms ceiling preserves a
// comfortable CI-realism margin (same honesty-move batch as Pulse v0
// KPI 1, Lumen v0 KPI 1 and Cinder KPI 2, 2026-05-19).
// --------------------------------------------------------------------

#[test]
fn ingest_p95_latency_under_two_milliseconds() {
    let base = temp_base("kpi1");
    let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
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
        p95 <= 2_000,
        "KPI 1: ingest p95 must be ≤ 2 ms (2000 µs); got {p95} µs (first samples {:?})",
        &samples[..10]
    );
    cleanup(&base);
}
