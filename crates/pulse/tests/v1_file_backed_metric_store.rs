// Kaleidoscope Pulse — v1 file-backed adapter acceptance test
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

//! `FileBackedMetricStore` survives process restart, returns
//! ingested points byte-stable after re-open, and supports the
//! snapshot+WAL-truncate cycle. Same shape as the Lumen v1 +
//! Cinder v1 + Sluice v1 acceptance tests.

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

fn metric(name: &str, points: Vec<MetricPoint>, kind: MetricKind) -> Metric {
    Metric {
        name: MetricName::new(name),
        description: String::new(),
        unit: "1".to_string(),
        kind,
        points,
        resource_attributes: BTreeMap::new(),
    }
}

fn point(time_unix_nano: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value,
    }
}

fn temp_base(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let dir = p.join(format!("kal-pulse-v1-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&dir).expect("mkdir");
    p = dir.join("pulse");
    p
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
fn ingest_then_query_returns_points_byte_stable() {
    let base = temp_base("query");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");

    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![metric(
                "http.requests.count",
                vec![point(100, 10.0), point(200, 20.0), point(300, 30.0)],
                MetricKind::Sum,
            )]),
        )
        .expect("ingest");

    let out = store
        .query(
            &tenant("acme"),
            &MetricName::new("http.requests.count"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].1.value, 10.0);
    assert_eq!(out[1].1.value, 20.0);
    assert_eq!(out[2].1.value, 30.0);
    cleanup(&base);
}

#[test]
fn restart_recovers_ingested_points_from_wal() {
    let base = temp_base("wal_restart");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![metric(
                    "http.requests.count",
                    vec![point(100, 10.0), point(200, 20.0)],
                    MetricKind::Sum,
                )]),
            )
            .expect("ingest");
    }
    // Process boundary: drop + reopen.
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .query(
            &tenant("acme"),
            &MetricName::new("http.requests.count"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 2, "WAL replay recovers both points");
    let values: Vec<f64> = out.iter().map(|(_, p)| p.value).collect();
    assert_eq!(values, vec![10.0, 20.0]);
    cleanup(&base);
}

#[test]
fn snapshot_writes_file_and_truncates_wal() {
    let base = temp_base("snapshot");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![metric(
                "http.requests.count",
                vec![point(100, 10.0), point(200, 20.0)],
                MetricKind::Sum,
            )]),
        )
        .expect("ingest");

    assert!(wal_size(&base) > 0, "WAL has data before snapshot");
    assert!(!snapshot_exists(&base), "no snapshot before call");

    store.snapshot().expect("snapshot");

    assert_eq!(wal_size(&base), 0, "WAL truncated after snapshot");
    assert!(snapshot_exists(&base), "snapshot file written");
    cleanup(&base);
}

#[test]
fn restart_recovers_from_snapshot_alone() {
    let base = temp_base("snap_restart");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![metric(
                    "http.requests.count",
                    vec![point(100, 10.0), point(200, 20.0), point(300, 30.0)],
                    MetricKind::Sum,
                )]),
            )
            .expect("ingest");
        store.snapshot().expect("snapshot");
    }
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .query(
            &tenant("acme"),
            &MetricName::new("http.requests.count"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 3);
    cleanup(&base);
}

#[test]
fn restart_recovers_snapshot_plus_wal_added_after_snapshot() {
    // The composition test: snapshot pins everything up to T;
    // post-snapshot ingests land in a fresh WAL; restart sees
    // both.
    let base = temp_base("snap_plus_wal");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![metric(
                    "http.requests.count",
                    vec![point(100, 10.0)],
                    MetricKind::Sum,
                )]),
            )
            .expect("pre-snapshot ingest");
        store.snapshot().expect("snapshot");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![metric(
                    "http.requests.count",
                    vec![point(200, 20.0)],
                    MetricKind::Sum,
                )]),
            )
            .expect("post-snapshot ingest");
    }
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let out = store
        .query(
            &tenant("acme"),
            &MetricName::new("http.requests.count"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(out.len(), 2);
    let values: Vec<f64> = out.iter().map(|(_, p)| p.value).collect();
    assert_eq!(values, vec![10.0, 20.0]);
    cleanup(&base);
}

#[test]
fn two_tenants_are_isolated_in_the_same_data_dir() {
    let base = temp_base("isolation");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![metric(
                "http.requests.count",
                vec![point(100, 1.0)],
                MetricKind::Sum,
            )]),
        )
        .expect("acme");
    store
        .ingest(
            &tenant("globex"),
            MetricBatch::with_metrics(vec![metric(
                "http.requests.count",
                vec![point(100, 99.0), point(200, 100.0)],
                MetricKind::Sum,
            )]),
        )
        .expect("globex");

    let acme_out = store
        .query(
            &tenant("acme"),
            &MetricName::new("http.requests.count"),
            TimeRange::all(),
        )
        .expect("acme q");
    let globex_out = store
        .query(
            &tenant("globex"),
            &MetricName::new("http.requests.count"),
            TimeRange::all(),
        )
        .expect("globex q");
    assert_eq!(acme_out.len(), 1);
    assert_eq!(acme_out[0].1.value, 1.0);
    assert_eq!(globex_out.len(), 2);
    cleanup(&base);
}

#[test]
fn gauge_and_sum_metrics_both_round_trip_through_persistence() {
    // Pulse v0 supports two metric kinds. v1 must preserve the
    // kind across restart, since downstream queries depend on
    // the metric metadata.
    let base = temp_base("gauge_and_sum");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![
                    metric("cpu.utilisation", vec![point(100, 0.42)], MetricKind::Gauge),
                    metric(
                        "http.requests.count",
                        vec![point(100, 1.0)],
                        MetricKind::Sum,
                    ),
                ]),
            )
            .expect("ingest");
    }
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 2");
    let cpu = store
        .query(
            &tenant("acme"),
            &MetricName::new("cpu.utilisation"),
            TimeRange::all(),
        )
        .expect("cpu");
    let http = store
        .query(
            &tenant("acme"),
            &MetricName::new("http.requests.count"),
            TimeRange::all(),
        )
        .expect("http");
    assert_eq!(cpu[0].0.kind, MetricKind::Gauge);
    assert_eq!(http[0].0.kind, MetricKind::Sum);
    cleanup(&base);
}

#[test]
fn empty_batch_ingest_is_a_no_op_persistence_wise() {
    let base = temp_base("empty_batch");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(&tenant("acme"), MetricBatch::with_metrics(vec![]))
        .expect("empty");
    // Empty batch must not append to the WAL — that would
    // bloat the file with no information content. The
    // recorder still sees a record_ingest(tenant, 0) event
    // (matches the v0 InMemory contract).
    assert_eq!(wal_size(&base), 0, "empty batch does not append to WAL");
    cleanup(&base);
}
