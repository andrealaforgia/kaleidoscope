// Kaleidoscope Ray v1 — slice 03 torn-tail recovery acceptance suite
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

//! Slice 03 — torn-tail recovery, ray store-reopen path
//! (wal-torn-tail-recovery-v0, US-01; AC-4 is the ray headline).
//!
//! Feature: a crashed-then-restarted trace store recovers its intact acked
//! prefix and drops the torn tail. Ray carries AC-4 (snapshot present, WAL
//! is a single torn line on top of it: the store opens, recovers exactly
//! the snapshot state, and the never-acked torn span is absent). It also
//! carries the per-pillar AC-9 scope coverage: the core torn-tail-tolerated
//! case and the mid-file-fail-closed negative.
//!
//! Driving port: `FileBackedTraceStore::open` reopened on a crashed tmp
//! `pillar_root`, then queried through the `TraceStore` trait.
//!
//! ## I-O strategy: C (real local I/O). See
//! `docs/feature/wal-torn-tail-recovery-v0/distill/wave-decisions.md` DWD-1.
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! Every scenario is `#[ignore]`d until its DELIVER slice removes the
//! marker (Outside-In). The tests drive ONLY existing public APIs
//! (`FileBackedTraceStore::open` / `snapshot` / `get_trace`, on-disk WAL
//! bytes), so they COMPILE against today's code with no scaffold. They are
//! RED because today's `open` refuses a torn tail with `PersistenceFailed`;
//! never BROKEN.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use ray::{
    FileBackedTraceStore, NoopRecorder, ServiceName, Span, SpanBatch, SpanId, SpanKind, SpanStatus,
    TimeRange, TraceId, TraceStore, TraceStoreError,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn span(trace: u8, span_byte: u8, service: &str, name: &str, start: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Span {
        trace_id: TraceId([trace; 16]),
        span_id: SpanId([span_byte; 8]),
        parent_span_id: None,
        name: name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 10,
        status: SpanStatus::default(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("ray-torn-tail-{test_name}-{pid}-{nanos}"));
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
// AC-4 (ray headline): snapshot present, WAL is a SINGLE torn line on top
// of it. The store opens, recovers exactly the snapshot state, and the
// never-acked torn span is absent.
// --------------------------------------------------------------------

#[test]
fn snapshot_plus_single_torn_tail_recovers_exactly_the_snapshot_state() {
    // @real-io @adapter-integration @US-01 @AC-4
    let base = temp_base("snapshot_plus_torn");
    {
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("seed open");
        store
            .ingest(
                &tenant("globex"),
                SpanBatch::with_spans(vec![
                    span(0xA1, 0x01, "checkout", "alpha", 100),
                    span(0xA1, 0x02, "checkout", "beta", 200),
                ]),
            )
            .expect("seed ingest");
        // Persist all globex spans into the snapshot and truncate the WAL.
        store.snapshot().expect("snapshot");
    }
    // One trace-ingest record was being appended when an OOM kill hit,
    // leaving a WAL of exactly ONE torn line with no trailing newline.
    append_torn_tail(
        &base,
        "{\"op\":\"ingest\",\"tenant\":\"globex\",\"spans\":[{\"trace_id\":\"a1b2",
    );

    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the snapshot state past the single torn tail");
    // Every span present in the snapshot is queryable.
    let recovered = store
        .get_trace(&tenant("globex"), &TraceId([0xA1; 16]))
        .expect("get_trace");
    assert_eq!(
        recovered.len(),
        2,
        "exactly the two snapshot spans recover; the torn span is absent"
    );
    let names: Vec<&str> = recovered.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-9 scope (ray): the core torn-tail-tolerated case, WAL-only (no
// snapshot). The intact acked prefix recovers; the torn tail is dropped.
// --------------------------------------------------------------------

#[test]
fn reopen_recovers_the_intact_prefix_after_a_torn_tail() {
    // @real-io @adapter-integration @US-01 @AC-1 @AC-9
    let base = temp_base("ray_prefix");
    {
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("seed open");
        store
            .ingest(
                &tenant("globex"),
                SpanBatch::with_spans(vec![
                    span(0xB2, 0x01, "gateway", "first", 100),
                    span(0xB2, 0x02, "gateway", "second", 200),
                ]),
            )
            .expect("seed ingest");
    }
    append_torn_tail(
        &base,
        "{\"op\":\"ingest\",\"tenant\":\"globex\",\"spans\":[{\"trace_id\":\"to",
    );

    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the intact prefix");
    let out = store
        .get_trace(&tenant("globex"), &TraceId([0xB2; 16]))
        .expect("get_trace");
    assert_eq!(out.len(), 2, "both acked spans recover; torn tail dropped");
    // The by-service index is rebuilt on recovery and is queryable too.
    let by_service = store
        .query(
            &tenant("globex"),
            &ServiceName::new("gateway"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(by_service.len(), 2);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-5 (NEGATIVE, ray): mid-file corruption stays fail-closed.
// --------------------------------------------------------------------

#[test]
fn mid_file_corruption_stays_fail_closed() {
    // @real-io @adapter-integration @US-01 @AC-5 @AC-9
    let base = temp_base("ray_midfile");
    {
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("seed open");
        store
            .ingest(
                &tenant("globex"),
                SpanBatch::with_spans(vec![
                    span(0xC3, 0x01, "svc", "first", 100),
                    span(0xC3, 0x02, "svc", "second", 200),
                ]),
            )
            .expect("seed ingest");
    }
    let wal = wal_path_of(&base);
    let mut lines: Vec<String> = fs::read_to_string(&wal)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    lines.insert(1, "{not valid json".to_string());
    fs::write(&wal, format!("{}\n", lines.join("\n"))).expect("rewrite wal");

    let err = FileBackedTraceStore::open(&base, Box::new(NoopRecorder))
        .expect_err("mid-file corruption must refuse");
    assert!(matches!(err, TraceStoreError::PersistenceFailed { .. }));
    cleanup(&base);
}
