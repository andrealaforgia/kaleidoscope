// Kaleidoscope integration-suite — six v1 adapters compose under restart
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

//! All six v1 adapters compose under a shared `aegis::TenantId`
//! and survive a single process restart together with consistent
//! state. Extends the original `v1_three_adapters_compose_under_restart`
//! from three engines (Cinder + Sluice + Lumen) to all six.
//!
//! The platform claim: "every named storage engine in the
//! architecture document survives a process restart". This test
//! makes that claim executable.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, NoopRecorder as LumenRecorder,
    SeverityNumber, TimeRange as LumenTimeRange,
};
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder as PulseRecorder, TimeRange as PulseTimeRange,
};
use ray::{
    FileBackedTraceStore, NoopRecorder as RayRecorder, ServiceName as RayServiceName, Span,
    SpanBatch, SpanId, SpanKind, SpanStatus, StatusCode, TimeRange as RayTimeRange, TraceId,
    TraceStore,
};
use sluice::{FileBackedQueue, NoopRecorder as SluiceRecorder, Queue};
use strata::{
    FileBackedProfileStore, NoopRecorder as StrataRecorder, Profile, ProfileBatch, ProfileStore,
    ServiceName as StrataServiceName, TimeRange as StrataTimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let root = env::temp_dir().join(format!("kal-v1-six-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

fn cleanup(root: &std::path::Path) {
    let _ = fs::remove_dir_all(root);
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

fn span(trace: [u8; 16], span_id: [u8; 8], start: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Span {
        trace_id: TraceId(trace),
        span_id: SpanId(span_id),
        parent_span_id: None,
        name: "GET /checkout".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 100,
        status: SpanStatus {
            code: StatusCode::Ok,
            message: String::new(),
        },
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

fn profile(time: u64) -> Profile {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Profile {
        time_unix_nano: time,
        duration_nanos: 1_000_000_000,
        profile_type: "cpu".to_string(),
        sample_type: Vec::new(),
        samples: Vec::new(),
        locations: Vec::new(),
        functions: Vec::new(),
        mappings: Vec::new(),
        string_table: vec![String::new()],
        resource_attributes: resource,
        attributes: BTreeMap::new(),
    }
}

fn metric(name: &str, time: u64, value: f64) -> Metric {
    Metric {
        name: MetricName::new(name),
        description: String::new(),
        unit: "1".to_string(),
        kind: MetricKind::Sum,
        points: vec![MetricPoint {
            time_unix_nano: time,
            start_time_unix_nano: 0,
            attributes: BTreeMap::new(),
            value,
        }],
        resource_attributes: BTreeMap::new(),
    }
}

#[test]
fn all_six_v1_adapters_survive_one_process_restart_together() {
    let root = temp_root("all_six");
    let tn = tenant("acme");
    let trace = [42u8; 16];

    // --- Phase 1: ingest into all six adapters, drop the
    //              stores (simulated process exit).
    {
        let lumen = FileBackedLogStore::open(root.join("lumen"), Box::new(LumenRecorder))
            .expect("lumen open");
        let cinder = FileBackedTieringStore::open(root.join("cinder"), Box::new(CinderRecorder))
            .expect("cinder open");
        let sluice = FileBackedQueue::open(root.join("sluice"), 100, Box::new(SluiceRecorder))
            .expect("sluice open");
        let pulse = FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
            .expect("pulse open");
        let ray =
            FileBackedTraceStore::open(root.join("ray"), Box::new(RayRecorder)).expect("ray open");
        let strata = FileBackedProfileStore::open(root.join("strata"), Box::new(StrataRecorder))
            .expect("strata open");

        lumen
            .ingest(
                &tn,
                LogBatch::with_records(vec![log_record(100), log_record(200)]),
            )
            .expect("lumen ingest");
        cinder.place(
            &tn,
            &ItemId::new("batch-1"),
            Tier::Hot,
            SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        );
        sluice
            .enqueue(&tn, b"batch-1 processed".to_vec())
            .expect("sluice enqueue");
        pulse
            .ingest(
                &tn,
                MetricBatch::with_metrics(vec![metric("http.requests.count", 100, 42.0)]),
            )
            .expect("pulse ingest");
        ray.ingest(
            &tn,
            SpanBatch::with_spans(vec![span(trace, [1; 8], 100), span(trace, [2; 8], 110)]),
        )
        .expect("ray ingest");
        strata
            .ingest(&tn, ProfileBatch::with_profiles(vec![profile(100)]))
            .expect("strata ingest");
    }

    // --- Phase 2: reopen all six stores. Assert each one
    //              recovered tenant `acme`'s state.
    let lumen = FileBackedLogStore::open(root.join("lumen"), Box::new(LumenRecorder))
        .expect("lumen reopen");
    let cinder = FileBackedTieringStore::open(root.join("cinder"), Box::new(CinderRecorder))
        .expect("cinder reopen");
    let sluice = FileBackedQueue::open(root.join("sluice"), 100, Box::new(SluiceRecorder))
        .expect("sluice reopen");
    let pulse = FileBackedMetricStore::open(root.join("pulse"), Box::new(PulseRecorder))
        .expect("pulse reopen");
    let ray =
        FileBackedTraceStore::open(root.join("ray"), Box::new(RayRecorder)).expect("ray reopen");
    let strata = FileBackedProfileStore::open(root.join("strata"), Box::new(StrataRecorder))
        .expect("strata reopen");

    // Lumen — both log records recovered.
    let logs = lumen
        .query(&tn, LumenTimeRange::all())
        .expect("lumen query");
    assert_eq!(logs.len(), 2, "Lumen survived restart");

    // Cinder — tier metadata for batch-1 still Hot.
    assert_eq!(
        cinder.get_tier(&tn, &ItemId::new("batch-1")),
        Some(Tier::Hot),
        "Cinder tier metadata survived restart"
    );

    // Sluice — the queued notification is still pending.
    assert_eq!(sluice.depth(&tn), 1, "Sluice queue depth survived restart");
    let msg = sluice.dequeue(&tn).expect("sluice dequeue");
    assert_eq!(msg.payload, b"batch-1 processed");

    // Pulse — the metric point is still queryable.
    let points = pulse
        .query(
            &tn,
            &MetricName::new("http.requests.count"),
            PulseTimeRange::all(),
        )
        .expect("pulse query");
    assert_eq!(points.len(), 1, "Pulse survived restart");
    assert_eq!(points[0].1.value, 42.0);

    // Ray — both spans of the trace recovered, both indexes
    // (by trace + by service) work after restart.
    let by_trace = ray.get_trace(&tn, &TraceId(trace)).expect("ray get_trace");
    assert_eq!(by_trace.len(), 2);
    let by_service = ray
        .query(&tn, &RayServiceName::new("checkout"), RayTimeRange::all())
        .expect("ray query");
    assert_eq!(
        by_service.len(),
        2,
        "Ray service-index rebuild worked after restart"
    );

    // Strata — the profile is queryable by service name.
    let profiles = strata
        .query(
            &tn,
            &StrataServiceName::new("checkout"),
            StrataTimeRange::all(),
        )
        .expect("strata query");
    assert_eq!(profiles.len(), 1, "Strata survived restart");

    // Tenant isolation — nothing leaked to a different tenant.
    let globex = tenant("globex");
    assert!(lumen
        .query(&globex, LumenTimeRange::all())
        .expect("")
        .is_empty());
    assert_eq!(cinder.get_tier(&globex, &ItemId::new("batch-1")), None);
    assert_eq!(sluice.depth(&globex), 0);
    assert!(pulse
        .query(
            &globex,
            &MetricName::new("http.requests.count"),
            PulseTimeRange::all()
        )
        .expect("")
        .is_empty());
    assert!(ray
        .get_trace(&globex, &TraceId(trace))
        .expect("")
        .is_empty());
    assert!(strata
        .query(
            &globex,
            &StrataServiceName::new("checkout"),
            StrataTimeRange::all()
        )
        .expect("")
        .is_empty());

    cleanup(&root);
}
