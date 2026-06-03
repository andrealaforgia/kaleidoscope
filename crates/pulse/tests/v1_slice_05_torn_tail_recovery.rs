// Kaleidoscope Pulse v1 — slice 05 torn-tail recovery acceptance suite
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

//! Slice 05 — torn-tail recovery, pulse store-reopen path
//! (wal-torn-tail-recovery-v0, US-01; AC-9 pulse scope + FLAG-1 property).
//!
//! Feature: a crashed-then-restarted metric store recovers its intact
//! acked prefix and drops the torn tail. Pulse is IN SCOPE this slice
//! (DESIGN FLAG 1 / ADR-0059 Decision 5). Its `tenant_counts` cardinality
//! watermark (ADR-0051) is reseeded from the rebuilt series map after
//! replay, so dropping the torn tail is transparent: the post-recovery
//! cardinality reflects exactly the recovered prefix's distinct series,
//! never the torn (never-acked) one.
//!
//! Driving port: `FileBackedMetricStore::open` reopened on a crashed tmp
//! `pillar_root`, then read through the `MetricStore` trait (`query`).
//!
//! ## I-O strategy: C (real local I/O). See
//! `docs/feature/wal-torn-tail-recovery-v0/distill/wave-decisions.md` DWD-1.
//!
//! ## Cardinality property is asserted observably
//!
//! The shadow `tenant_counts` watermark is internal (it gates the
//! ADR-0051 cap on a future live ingest). The acceptance suite asserts it
//! OBSERVABLY through the driving port: after recovery, exactly the
//! prefix's series are queryable and the torn-tail series is absent, so
//! the recovered cardinality equals the prefix cardinality. A white-box
//! peek at the count (as the inline `open_seeds_tenant_counts_from_rebuilt_series`
//! unit test does) belongs to the crafter's DELIVER mutation-coverage work,
//! not to this black-box suite (Dim 7, observable behaviour).
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! Every scenario is `#[ignore]`d until its DELIVER slice removes the
//! marker (Outside-In). The tests drive ONLY existing public APIs, so they
//! COMPILE with no scaffold. They are RED because today's `open` refuses a
//! torn tail with `PersistenceFailed`; never BROKEN.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    MetricStoreError, NoopRecorder, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn point(time: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano: time,
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
        description: String::new(),
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
    path.push(format!("pulse-torn-tail-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn wal_path_of(base: &Path) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    PathBuf::from(p)
}

fn append_torn_tail(base: &Path, torn: &str) -> usize {
    let wal = wal_path_of(base);
    let existing = fs::read_to_string(&wal).unwrap_or_default();
    fs::write(&wal, format!("{existing}{torn}")).expect("append torn tail");
    torn.len()
}

// --------------------------------------------------------------------
// AC-9 scope (pulse) + FLAG-1 cardinality property: the core
// torn-tail-tolerated case. The intact acked prefix recovers; the torn
// tail is dropped; the recovered cardinality reflects exactly the
// recovered prefix's distinct series.
// --------------------------------------------------------------------

#[test]
fn reopen_recovers_the_intact_prefix_and_cardinality_stays_consistent() {
    // @real-io @adapter-integration @US-01 @AC-1 @AC-9
    let base = temp_base("pulse_prefix");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("seed open");
        // Two distinct series for tenant acme: rps@checkout and rps@billing.
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![
                    gauge("rps", "checkout", vec![point(100, 1.0)]),
                    gauge("rps", "billing", vec![point(200, 2.0)]),
                ]),
            )
            .expect("seed ingest");
        drop(store);
    }
    // A crash tore a third ingest batch mid-write: partial JSON, no newline.
    append_torn_tail(
        &base,
        "{\"op\":\"ingest\",\"tenant\":\"acme\",\"metrics\":[{\"name\":\"rps\",\"resource_attributes\":{\"service.name\":\"shi",
    );

    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the intact prefix");
    // Exactly the two acked series recover, both points present.
    let rows = store
        .query(&tenant("acme"), &MetricName::new("rps"), TimeRange::all())
        .expect("query");
    assert_eq!(
        rows.len(),
        2,
        "exactly the two acked series-points recover; the torn third is absent"
    );
    let mut services: Vec<String> = rows
        .iter()
        .filter_map(|(m, _)| m.resource_attributes.get("service.name").cloned())
        .collect();
    services.sort();
    assert_eq!(
        services,
        vec!["billing".to_string(), "checkout".to_string()],
        "the recovered series are exactly the acked prefix's two; no torn series leaked in"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-5 (NEGATIVE, pulse): mid-file corruption stays fail-closed.
// --------------------------------------------------------------------

#[test]
fn mid_file_corruption_stays_fail_closed() {
    // @real-io @adapter-integration @US-01 @AC-5 @AC-9
    let base = temp_base("pulse_midfile");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("seed open");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![gauge("rps", "checkout", vec![point(100, 1.0)])]),
            )
            .expect("seed ingest 1");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![gauge("rps", "billing", vec![point(200, 2.0)])]),
            )
            .expect("seed ingest 2");
        drop(store);
    }
    let wal = wal_path_of(&base);
    let mut lines: Vec<String> = fs::read_to_string(&wal)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    lines.insert(1, "{not valid json".to_string());
    fs::write(&wal, format!("{}\n", lines.join("\n"))).expect("rewrite wal");

    let err = FileBackedMetricStore::open(&base, Box::new(NoopRecorder))
        .expect_err("mid-file corruption must refuse");
    assert!(matches!(err, MetricStoreError::PersistenceFailed { .. }));
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-6 (NEGATIVE, pulse): a malformed FINAL line that DOES end in a
// trailing newline stays fail-closed.
// --------------------------------------------------------------------

#[test]
fn newline_terminated_malformed_final_line_stays_fail_closed() {
    // @real-io @adapter-integration @US-01 @AC-6
    let base = temp_base("pulse_newline_malformed");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("seed open");
        store
            .ingest(
                &tenant("acme"),
                MetricBatch::with_metrics(vec![gauge("rps", "checkout", vec![point(100, 1.0)])]),
            )
            .expect("seed ingest");
        drop(store);
    }
    let wal = wal_path_of(&base);
    let existing = fs::read_to_string(&wal).unwrap();
    fs::write(&wal, format!("{existing}{{not valid json}}\n")).expect("append malformed line");

    let err = FileBackedMetricStore::open(&base, Box::new(NoopRecorder))
        .expect_err("a complete-but-malformed final line must refuse");
    assert!(matches!(err, MetricStoreError::PersistenceFailed { .. }));
    cleanup(&base);
}
