// Kaleidoscope Ray v1 — age-based retention purge acceptance test
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

//! Age-based retention purge — `FileBackedTraceStore::purge_older_than`.
//!
//! Physically removes every span strictly older than a cutoff from the
//! `by_trace` index, rebuilds the derived `by_service` index from the
//! survivors (so an in-process reader sees the purge on BOTH paths),
//! compacts to disk via the existing snapshot+WAL path, and is durable
//! across a reopen. The strongest falsifiability hook: `get_trace`
//! (which applies NO age filter) returns empty for the purged trace.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use ray::{
    FileBackedTraceStore, NoopRecorder, ServiceName, Span, SpanBatch, SpanId, SpanKind, SpanStatus,
    TimeRange, TraceId, TraceStore,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn trace_id(byte: u8) -> TraceId {
    TraceId([byte; 16])
}

fn span_id(byte: u8) -> SpanId {
    SpanId([byte; 8])
}

fn span(trace_byte: u8, span_byte: u8, service: &str, name: &str, start: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Span {
        trace_id: trace_id(trace_byte),
        span_id: span_id(span_byte),
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
    path.push(format!("ray-v1-purge-{test_name}-{pid}-{nanos}"));
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
// Real removal AND footprint shrink across reopen — the falsifiable
// core, proven via BOTH the by-id (get_trace, no age filter) and the
// rebuilt by-service paths.
// --------------------------------------------------------------------

#[test]
fn purge_removes_old_spans_durably_on_both_indices_and_shrinks_footprint() {
    let base = temp_base("removal_shrink");
    let cutoff = 1_000u64;
    {
        let s = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
        // Trace 0xDD: 50 OLD spans (start 1..=50), all in service "gateway".
        // Trace 0xEE: 1 IN-WINDOW span at the cutoff, same service.
        let mut spans: Vec<Span> = (1..=50u64)
            .map(|i| span(0xDD, (i % 256) as u8, "gateway", "old", i))
            .collect();
        spans.push(span(0xEE, 0x01, "gateway", "live", cutoff));
        s.ingest(&tenant("acme"), SpanBatch::with_spans(spans))
            .expect("ingest");

        let before = on_disk_bytes(&base);

        let removed = s.purge_older_than(cutoff).expect("purge");
        assert_eq!(removed, 50, "exactly the 50 sub-cutoff spans are removed");

        // STRONGEST hook: get_trace applies NO age filter. The purged
        // trace bucket must be empty (in-process, no reopen).
        let purged = s
            .get_trace(&tenant("acme"), &trace_id(0xDD))
            .expect("get_trace");
        assert!(
            purged.is_empty(),
            "purged trace bucket is empty on the by-id path"
        );

        // The in-window trace survives intact.
        let live_trace = s
            .get_trace(&tenant("acme"), &trace_id(0xEE))
            .expect("get_trace");
        assert_eq!(live_trace.len(), 1);
        assert_eq!(live_trace[0].start_time_unix_nano, cutoff);

        // by_service must be rebuilt: only the in-window span remains.
        let by_service = s
            .query(
                &tenant("acme"),
                &ServiceName::new("gateway"),
                TimeRange::all(),
            )
            .expect("query");
        assert_eq!(
            by_service.len(),
            1,
            "rebuilt by_service drops the old spans"
        );
        assert_eq!(by_service[0].start_time_unix_nano, cutoff);

        // On-disk footprint strictly decreased.
        let after = on_disk_bytes(&base);
        assert!(
            after < before,
            "on-disk footprint must strictly shrink: before={before} after={after}"
        );
    }

    // Durable across reopen: the purged trace is STILL gone on the by-id
    // path, and the in-window span survives on both paths.
    let s2 = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    assert!(
        s2.get_trace(&tenant("acme"), &trace_id(0xDD))
            .expect("get_trace")
            .is_empty(),
        "purge is durable across reopen on the by-id path"
    );
    let live = s2
        .get_trace(&tenant("acme"), &trace_id(0xEE))
        .expect("get_trace");
    assert_eq!(live.len(), 1);
    let svc = s2
        .query(
            &tenant("acme"),
            &ServiceName::new("gateway"),
            TimeRange::all(),
        )
        .expect("query");
    assert_eq!(
        svc.len(),
        1,
        "by_service rebuilt from purged survivors after reopen"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// In-window span untouched — content unchanged, field-for-field.
// --------------------------------------------------------------------

#[test]
fn in_window_span_content_is_unchanged_after_purge() {
    let base = temp_base("content_unchanged");
    let cutoff = 500u64;
    let s = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");

    let survivor = span(0xEE, 0x09, "checkout", "place-order", cutoff + 100);
    s.ingest(
        &tenant("acme"),
        SpanBatch::with_spans(vec![
            span(0xDD, 0x01, "checkout", "old", 10),
            survivor.clone(),
        ]),
    )
    .expect("ingest");

    s.purge_older_than(cutoff).expect("purge");

    let rows = s
        .get_trace(&tenant("acme"), &trace_id(0xEE))
        .expect("get_trace");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0], survivor,
        "surviving span is byte-for-byte the ingested one"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// Boundary — at-cutoff KEPT, cutoff-1 REMOVED (half-open).
// --------------------------------------------------------------------

#[test]
fn purge_is_half_open_at_the_cutoff_boundary() {
    let base = temp_base("boundary");
    let cutoff = 1_000u64;
    let s = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        SpanBatch::with_spans(vec![
            span(0xAA, 0x01, "svc", "just-too-old", cutoff - 1),
            span(0xAA, 0x02, "svc", "at-cutoff", cutoff),
        ]),
    )
    .expect("ingest");

    let removed = s.purge_older_than(cutoff).expect("purge");
    assert_eq!(removed, 1, "only the cutoff-1 span is removed");

    let rows = s
        .get_trace(&tenant("acme"), &trace_id(0xAA))
        .expect("get_trace");
    assert_eq!(rows.len(), 1, "the at-cutoff span is kept");
    assert_eq!(rows[0].start_time_unix_nano, cutoff);
    cleanup(&base);
}

// --------------------------------------------------------------------
// Monotonic bound — repeated old-ingest + purge does not grow unbounded.
// --------------------------------------------------------------------

#[test]
fn repeated_old_ingest_and_purge_stays_bounded() {
    let base = temp_base("monotonic");
    let cutoff = 1_000u64;
    let s = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        SpanBatch::with_spans(vec![span(0xEE, 0x01, "svc", "in-window", cutoff + 1)]),
    )
    .expect("seed in-window");

    let mut footprints: Vec<u64> = Vec::new();
    for iter in 0..5u64 {
        let old: Vec<Span> = (1..=20u64)
            .map(|i| span(0xDD, (i % 256) as u8, "svc", "old", iter * 100 + i))
            .collect();
        s.ingest(&tenant("acme"), SpanBatch::with_spans(old))
            .expect("ingest old");
        s.purge_older_than(cutoff).expect("purge");
        footprints.push(on_disk_bytes(&base));
    }

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
// No-op — cutoff below every span removes nothing, returns 0.
// --------------------------------------------------------------------

#[test]
fn purge_is_a_no_op_when_nothing_is_older_than_cutoff() {
    let base = temp_base("noop");
    let s = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        SpanBatch::with_spans(vec![
            span(0xAA, 0x01, "svc", "a", 100),
            span(0xAA, 0x02, "svc", "b", 200),
        ]),
    )
    .expect("ingest");

    let removed = s.purge_older_than(50).expect("purge");
    assert_eq!(removed, 0, "nothing older than the cutoff");

    let rows = s
        .get_trace(&tenant("acme"), &trace_id(0xAA))
        .expect("get_trace");
    assert_eq!(rows.len(), 2, "both spans remain");
    cleanup(&base);
}
