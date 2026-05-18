// Kaleidoscope integration-suite — self-observability survives restart
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

//! Composed property: self-observability survives a process
//! restart end-to-end.
//!
//! Scenario:
//!
//! 1. Construct Pulse v1 (`FileBackedMetricStore`) with its
//!    own data directory.
//! 2. Construct Lumen v1 (`FileBackedLogStore`) wired to a
//!    `LumenToPulseRecorder` that feeds the Pulse store.
//! 3. Ingest a batch into Lumen — the bridge writes one
//!    `lumen.ingest.count` point into Pulse.
//! 4. Drop everything (simulated process exit).
//! 5. Reopen Pulse alone. Query for `lumen.ingest.count` and
//!    assert the observability point from phase 3 is still
//!    there.
//!
//! This proves a property that no single-crate test can
//! prove: the bridges feed the storage, the storage persists
//! across restart, and the observability state of the
//! platform itself survives a stop/restart cycle. Without
//! this, an operator restarting the binary would lose
//! visibility into what the platform did before the restart.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use lumen::{FileBackedLogStore, LogBatch, LogRecord, LogStore, SeverityNumber};
use pulse::{
    FileBackedMetricStore, MetricName, MetricStore, NoopRecorder as PulseRecorder,
    TimeRange as PulseTimeRange,
};
use self_observe::LumenToPulseRecorder;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn log_record(observed: u64) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: "hello".to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let root = env::temp_dir().join(format!("kal-selfobs-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

fn cleanup(root: &std::path::Path) {
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lumen_ingest_observability_survives_restart_via_pulse_v1() {
    let root = temp_root("lumen_ingest");
    let tn = tenant("acme");

    // --- Phase 1: run-time. Build Pulse v1 + Lumen v1 with the
    //              Lumen->Pulse bridge wired. Ingest 5 records
    //              into Lumen; the bridge writes one point to
    //              Pulse (value=5).
    {
        let pulse_store = Arc::new(
            FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
                .expect("pulse open"),
        );
        let lumen_recorder =
            LumenToPulseRecorder::new(pulse_store.clone() as Arc<dyn MetricStore + Send + Sync>);
        let lumen = FileBackedLogStore::open(root.join("lumen"), Box::new(lumen_recorder))
            .expect("lumen open");

        let batch = LogBatch::with_records((100..105u64).map(log_record).collect());
        lumen.ingest(&tn, batch).expect("lumen ingest");

        // Sanity: the point is visible in Pulse RIGHT NOW.
        let live_points = pulse_store
            .query(
                &tn,
                &MetricName::new("lumen.ingest.count"),
                PulseTimeRange::all(),
            )
            .expect("pulse query in-process");
        assert_eq!(live_points.len(), 1);
        assert_eq!(live_points[0].1.value, 5.0);

        // Drop both stores at end of scope. The Pulse WAL has
        // the ingest event appended; Lumen's data directory
        // also persisted but we don't care for the assertion.
    }

    // --- Phase 2: restart. Reopen Pulse alone. Query for the
    //              lumen.ingest.count metric. The point that
    //              the LumenToPulseRecorder fed during phase 1
    //              must still be there.
    let pulse_store = FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
        .expect("pulse reopen");
    let points = pulse_store
        .query(
            &tn,
            &MetricName::new("lumen.ingest.count"),
            PulseTimeRange::all(),
        )
        .expect("pulse query after restart");
    assert_eq!(
        points.len(),
        1,
        "the bridge's observability point must survive restart"
    );
    assert_eq!(
        points[0].1.value, 5.0,
        "the point value must match what the bridge emitted in phase 1"
    );

    cleanup(&root);
}

#[test]
fn multiple_lumen_ingests_aggregate_into_pulse_points_that_all_survive() {
    // Three Lumen ingest calls in phase 1 produce three Pulse
    // points; all three must be readable from a restarted
    // Pulse instance in phase 2.
    let root = temp_root("multi_ingest");
    let tn = tenant("acme");

    {
        let pulse_store = Arc::new(
            FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
                .expect("pulse open"),
        );
        let lumen_recorder =
            LumenToPulseRecorder::new(pulse_store.clone() as Arc<dyn MetricStore + Send + Sync>);
        let lumen = FileBackedLogStore::open(root.join("lumen"), Box::new(lumen_recorder))
            .expect("lumen open");

        for batch_idx in 0..3 {
            let base = 100 + batch_idx * 10;
            let batch = LogBatch::with_records((base..(base + 2)).map(log_record).collect());
            lumen.ingest(&tn, batch).expect("lumen ingest");
        }
    }

    let pulse_store = FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
        .expect("pulse reopen");
    let points = pulse_store
        .query(
            &tn,
            &MetricName::new("lumen.ingest.count"),
            PulseTimeRange::all(),
        )
        .expect("pulse query");
    assert_eq!(
        points.len(),
        3,
        "three bridge emissions must all survive restart"
    );
    for (_, p) in &points {
        assert_eq!(p.value, 2.0, "each batch carried 2 records");
    }
    cleanup(&root);
}

#[test]
fn observability_state_for_one_tenant_does_not_leak_to_another_after_restart() {
    // Phase 1: feed Pulse via the bridge for two tenants. Both
    // get their own buckets of observability points.
    // Phase 2: restart. Each tenant's points are still
    // partitioned correctly.
    let root = temp_root("tenant_iso");
    let acme = tenant("acme");
    let globex = tenant("globex");

    {
        let pulse_store = Arc::new(
            FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
                .expect("pulse open"),
        );
        let lumen_recorder =
            LumenToPulseRecorder::new(pulse_store.clone() as Arc<dyn MetricStore + Send + Sync>);
        let lumen = FileBackedLogStore::open(root.join("lumen"), Box::new(lumen_recorder))
            .expect("lumen open");

        lumen
            .ingest(&acme, LogBatch::with_records(vec![log_record(100)]))
            .expect("acme ingest");
        lumen
            .ingest(
                &globex,
                LogBatch::with_records(vec![log_record(200), log_record(210)]),
            )
            .expect("globex ingest");
    }

    let pulse_store = FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
        .expect("pulse reopen");
    let acme_points = pulse_store
        .query(
            &acme,
            &MetricName::new("lumen.ingest.count"),
            PulseTimeRange::all(),
        )
        .expect("acme query");
    let globex_points = pulse_store
        .query(
            &globex,
            &MetricName::new("lumen.ingest.count"),
            PulseTimeRange::all(),
        )
        .expect("globex query");
    assert_eq!(acme_points.len(), 1);
    assert_eq!(acme_points[0].1.value, 1.0);
    assert_eq!(globex_points.len(), 1);
    assert_eq!(globex_points[0].1.value, 2.0);
    cleanup(&root);
}
