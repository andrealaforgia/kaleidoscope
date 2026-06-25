// Kaleidoscope Pulse v1 — age-based retention purge acceptance test
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

//! Age-based retention purge — `FileBackedMetricStore::purge_older_than`.
//!
//! Physically removes every metric point strictly older than a cutoff,
//! compacts the surviving state to disk via the existing snapshot+WAL
//! path, and is durable across a reopen. The falsifiable core: data gone
//! even after reopen, on-disk files smaller.

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
    let mut attributes = BTreeMap::new();
    attributes.insert("route".to_string(), "/checkout".to_string());
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: time_unix_nano.saturating_sub(5),
        attributes,
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
    path.push(format!("pulse-v1-purge-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn size_of(base: &std::path::Path, suffix: &str) -> u64 {
    let mut p = base.as_os_str().to_owned();
    p.push(suffix);
    fs::metadata(PathBuf::from(p)).map(|m| m.len()).unwrap_or(0)
}

fn on_disk_bytes(base: &std::path::Path) -> u64 {
    size_of(base, ".wal") + size_of(base, ".snapshot")
}

// --------------------------------------------------------------------
// Real removal AND footprint shrink across reopen — the falsifiable core.
// --------------------------------------------------------------------

#[test]
fn purge_removes_old_points_durably_and_shrinks_on_disk_footprint() {
    let base = temp_base("removal_shrink");
    let cutoff = 1_000u64;
    {
        let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
        // Many OLD points (times 1..=50, all < cutoff) plus one IN-WINDOW
        // point (time == cutoff). The old points bloat the WAL; the purge
        // must shrink the on-disk footprint by reclaiming them.
        let mut points: Vec<MetricPoint> = (1..=50u64).map(|i| point(i, i as f64)).collect();
        points.push(point(cutoff, 99.0));
        s.ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge("rps", "svc", points)]),
        )
        .expect("ingest");

        let before = on_disk_bytes(&base);

        let removed = s.purge_older_than(cutoff).expect("purge");
        assert_eq!(removed, 50, "exactly the 50 sub-cutoff points are removed");

        // (a) Querying returns only the in-window point.
        let after_purge = s
            .query(&tenant("acme"), &name("rps"), TimeRange::all())
            .expect("query");
        assert_eq!(after_purge.len(), 1, "only the in-window point survives");
        assert_eq!(after_purge[0].1.time_unix_nano, cutoff);

        // (c) On-disk footprint strictly decreased.
        let after = on_disk_bytes(&base);
        assert!(
            after < before,
            "on-disk footprint must strictly shrink: before={before} after={after}"
        );
    }

    // (b) Reopen from the same path: old points STILL gone, in-window intact.
    let s2 = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let reopened = s2
        .query(&tenant("acme"), &name("rps"), TimeRange::all())
        .expect("query reopen");
    assert_eq!(reopened.len(), 1, "purge is durable across reopen");
    assert_eq!(reopened[0].1.time_unix_nano, cutoff);
    cleanup(&base);
}

// --------------------------------------------------------------------
// In-window record untouched — content unchanged, field-for-field.
// --------------------------------------------------------------------

#[test]
fn in_window_point_content_is_unchanged_after_purge() {
    let base = temp_base("content_unchanged");
    let cutoff = 500u64;
    let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");

    let survivor = point(cutoff + 100, 42.5);
    let metric = gauge("rps", "checkout", vec![point(10, 1.0), survivor.clone()]);
    s.ingest(
        &tenant("acme"),
        MetricBatch::with_metrics(vec![metric.clone()]),
    )
    .expect("ingest");

    s.purge_older_than(cutoff).expect("purge");

    let rows = s
        .query(&tenant("acme"), &name("rps"), TimeRange::all())
        .expect("query");
    assert_eq!(rows.len(), 1);
    let (got_metric, got_point) = &rows[0];
    // Point: every field equals what was ingested.
    assert_eq!(
        got_point, &survivor,
        "surviving point is byte-for-byte the ingested one"
    );
    // Metric metadata: name/unit/kind/description/resource attributes intact.
    assert_eq!(got_metric.name, metric.name);
    assert_eq!(got_metric.unit, metric.unit);
    assert_eq!(got_metric.kind, metric.kind);
    assert_eq!(got_metric.description, metric.description);
    assert_eq!(got_metric.resource_attributes, metric.resource_attributes);
    cleanup(&base);
}

// --------------------------------------------------------------------
// Boundary — at-cutoff KEPT, cutoff-1 REMOVED (half-open).
// --------------------------------------------------------------------

#[test]
fn purge_is_half_open_at_the_cutoff_boundary() {
    let base = temp_base("boundary");
    let cutoff = 1_000u64;
    let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        MetricBatch::with_metrics(vec![gauge(
            "rps",
            "svc",
            vec![point(cutoff - 1, 1.0), point(cutoff, 2.0)],
        )]),
    )
    .expect("ingest");

    let removed = s.purge_older_than(cutoff).expect("purge");
    assert_eq!(removed, 1, "only the cutoff-1 point is removed");

    let rows = s
        .query(&tenant("acme"), &name("rps"), TimeRange::all())
        .expect("query");
    assert_eq!(rows.len(), 1, "the at-cutoff point is kept");
    assert_eq!(rows[0].1.time_unix_nano, cutoff);
    cleanup(&base);
}

// --------------------------------------------------------------------
// Monotonic bound — repeated old-ingest + purge does not grow unbounded.
// --------------------------------------------------------------------

#[test]
fn repeated_old_ingest_and_purge_stays_bounded() {
    let base = temp_base("monotonic");
    let cutoff = 1_000u64;
    let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    // One in-window point that survives every purge, so the snapshot is
    // non-trivial but constant across iterations.
    s.ingest(
        &tenant("acme"),
        MetricBatch::with_metrics(vec![gauge("rps", "svc", vec![point(cutoff + 1, 7.0)])]),
    )
    .expect("seed in-window");

    let mut footprints: Vec<u64> = Vec::new();
    for iter in 0..5u64 {
        // A fresh batch of OLD points (all < cutoff).
        let old: Vec<MetricPoint> = (1..=20u64)
            .map(|i| point(i, (iter * 100 + i) as f64))
            .collect();
        s.ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge("rps", "svc", old)]),
        )
        .expect("ingest old");
        s.purge_older_than(cutoff).expect("purge");
        footprints.push(on_disk_bytes(&base));
    }

    // After each purge the WAL is truncated and the snapshot holds only
    // the single in-window point — so the footprint must not grow across
    // iterations beyond a small constant slack.
    let baseline = footprints[0];
    for (iter, &f) in footprints.iter().enumerate() {
        assert!(
            f <= baseline + 64,
            "footprint must stay bounded; iter {iter} = {f}, baseline = {baseline}"
        );
    }
    cleanup(&base);
}

// --------------------------------------------------------------------
// No-op — cutoff below every record removes nothing, returns 0.
// --------------------------------------------------------------------

#[test]
fn purge_is_a_no_op_when_nothing_is_older_than_cutoff() {
    let base = temp_base("noop");
    let s = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        MetricBatch::with_metrics(vec![gauge(
            "rps",
            "svc",
            vec![point(100, 1.0), point(200, 2.0)],
        )]),
    )
    .expect("ingest");

    let removed = s.purge_older_than(50).expect("purge");
    assert_eq!(removed, 0, "nothing older than the cutoff");

    let rows = s
        .query(&tenant("acme"), &name("rps"), TimeRange::all())
        .expect("query");
    assert_eq!(rows.len(), 2, "both points remain");
    cleanup(&base);
}
