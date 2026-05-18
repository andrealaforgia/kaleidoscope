// Kaleidoscope integration-suite — six bridges compose
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

//! Executable proof of the platform claim: "Kaleidoscope
//! observes itself end-to-end across every storage engine."
//!
//! Each of the six metric-bearing crates is constructed with
//! its OTLP-JSON writer pointed at the same in-memory buffer.
//! Each one is then driven with a handful of realistic
//! operations. The buffer is parsed as NDJSON; we assert that
//! all six `kaleidoscope.<crate>` scope names appear, with the
//! metric names the bridges promise, in a single coherent
//! stream. A sidecar process receiving this NDJSON could
//! forward it to a real OTLP/HTTP collector without any change
//! per-crate.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use aegis::TenantId;
use augur::{AnomalyObserver, MetricsRecorder as AugurRec, ZScoreObserver};
use cinder::{InMemoryTieringStore, ItemId, Tier, TierPolicy, TieringStore};
use lumen::{InMemoryLogStore, LogBatch, LogRecord, LogStore, SeverityNumber};
use ray::{
    InMemoryTraceStore, Span, SpanBatch, SpanId, SpanKind, SpanStatus, StatusCode, TraceId,
    TraceStore,
};
use self_observe::{
    AugurToOtlpJsonWriter, CinderToOtlpJsonWriter, LumenToOtlpJsonWriter, RayToOtlpJsonWriter,
    SluiceToOtlpJsonWriter, StrataToOtlpJsonWriter,
};
use serde_json::Value;
use sluice::{InMemoryQueue, Queue};
use strata::{InMemoryProfileStore, Profile, ProfileBatch, ProfileStore};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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

fn span(trace: [u8; 16]) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Span {
        trace_id: TraceId(trace),
        span_id: SpanId([1; 8]),
        parent_span_id: None,
        name: "GET /checkout".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: 100,
        end_time_unix_nano: 200,
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

#[test]
fn all_six_bridges_emit_into_the_same_otlp_ndjson_stream() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let tn = tenant("acme");

    // --- Lumen ---
    let lumen_writer = LumenToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let lumen = InMemoryLogStore::new(Box::new(lumen_writer));
    lumen
        .ingest(&tn, LogBatch::with_records(vec![log_record(100)]))
        .expect("lumen ingest");

    // --- Cinder ---
    // Sequence chosen so all three Cinder events fire:
    //   - place(widget-1, Hot) at t=0
    //     -> cinder.place.count
    //   - evaluate_at(t=120s) with Hot->Warm threshold = 30s
    //     -> widget-1 has aged 120s, exceeds 30s, migrates
    //     -> cinder.migrate.count + cinder.evaluate.migrated.count
    let cinder_writer = CinderToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let cinder = InMemoryTieringStore::new(Box::new(cinder_writer));
    cinder.place(
        &tn,
        &ItemId::new("widget-1"),
        Tier::Hot,
        SystemTime::UNIX_EPOCH,
    );
    let migrated = cinder.evaluate_at(
        SystemTime::UNIX_EPOCH + Duration::from_secs(120),
        &TierPolicy::age_based(Duration::from_secs(30), Duration::from_secs(300)),
    );
    assert_eq!(migrated, 1, "widget-1 must migrate Hot->Warm at t=120s");

    // --- Sluice ---
    let sluice_writer = SluiceToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let queue = InMemoryQueue::new(10, Box::new(sluice_writer));
    let msg = queue.enqueue(&tn, b"payload".to_vec()).expect("enqueue");
    let _ = queue.dequeue(&tn).expect("dequeue");
    queue.ack(msg);

    // --- Ray ---
    let ray_writer = RayToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let ray = InMemoryTraceStore::new(Box::new(ray_writer));
    ray.ingest(&tn, SpanBatch::with_spans(vec![span([1; 16])]))
        .expect("ray ingest");

    // --- Augur (with the observer self-instrumentation wired) ---
    let augur_writer = AugurToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let augur_rec: Arc<dyn AugurRec + Send + Sync> = Arc::new(augur_writer);
    let mut zscore = ZScoreObserver::new(3.0, 3).with_recorder(augur_rec);
    for v in [10.0, 10.1, 9.9, 10.05, 9.95, 10.02, 9.98, 10.01] {
        zscore.observe(&tn, v, SystemTime::UNIX_EPOCH);
    }
    let anomaly = zscore.observe(&tn, 50.0, SystemTime::UNIX_EPOCH);
    assert!(anomaly.is_some(), "50.0 must cross 3-sigma");

    // --- Strata ---
    let strata_writer = StrataToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let strata = InMemoryProfileStore::new(Box::new(strata_writer));
    strata
        .ingest(&tn, ProfileBatch::with_profiles(vec![profile(100)]))
        .expect("strata ingest");

    // --- Verify: parse the unified stream ---
    let bytes = buf.lock().unwrap().clone();
    let stream = String::from_utf8(bytes).expect("utf8");
    let lines: Vec<Value> = stream
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse otlp-json"))
        .collect();

    assert!(
        !lines.is_empty(),
        "the stream must carry at least one event per crate"
    );

    let scope = |v: &Value| -> String {
        v["scopeMetrics"][0]["scope"]["name"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };
    let metric = |v: &Value| -> String {
        v["scopeMetrics"][0]["metrics"][0]["name"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };

    let scopes: std::collections::HashSet<String> = lines.iter().map(scope).collect();
    for required in &[
        "kaleidoscope.lumen",
        "kaleidoscope.cinder",
        "kaleidoscope.sluice",
        "kaleidoscope.ray",
        "kaleidoscope.augur",
        "kaleidoscope.strata",
    ] {
        assert!(
            scopes.contains(*required),
            "scope {required} must appear in the unified OTLP-JSON stream — found {scopes:?}"
        );
    }

    let metrics: std::collections::HashSet<String> = lines.iter().map(metric).collect();
    for required in &[
        "lumen.ingest.count",
        "cinder.place.count",
        "cinder.migrate.count",
        "cinder.evaluate.migrated.count",
        "sluice.enqueue.count",
        "sluice.dequeue.count",
        "sluice.ack.count",
        "ray.ingest.count",
        "augur.observation.count",
        "augur.anomaly.count",
        "augur.anomaly.score",
        "strata.ingest.count",
    ] {
        assert!(
            metrics.contains(*required),
            "metric {required} must appear in the unified OTLP-JSON stream — found {metrics:?}"
        );
    }

    // Every line carries the same tenant resource attribute,
    // because every event in this test was driven on behalf of
    // the single tenant `acme`. The platform claim is "events
    // are keyed by tenant identity"; the proof is in the
    // unified stream.
    for line in &lines {
        let tenant_attr = line["resource"]["attributes"][0]["value"]["stringValue"]
            .as_str()
            .unwrap_or("");
        assert_eq!(tenant_attr, "acme", "every line carries the same tenant");
    }
}
