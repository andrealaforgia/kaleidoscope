// Kaleidoscope Lumen v1 — age-based retention purge acceptance test
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

//! Age-based retention purge — `FileBackedLogStore::purge_older_than`.
//!
//! Physically removes every log record strictly older than a cutoff,
//! compacts the surviving state to disk via the existing snapshot+WAL
//! path, and is durable across a reopen. The falsifiable core: data gone
//! even after reopen, on-disk files smaller.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, NoopRecorder, SeverityNumber, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, service: &str, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    let mut attributes = BTreeMap::new();
    attributes.insert("http.route".to_string(), "/checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes,
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("lumen-v1-purge-{test_name}-{pid}-{nanos}"));
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
fn purge_removes_old_records_durably_and_shrinks_on_disk_footprint() {
    let base = temp_base("removal_shrink");
    let cutoff = 1_000u64;
    {
        let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");
        // Many OLD records (times 1..=50) plus one IN-WINDOW record at the
        // cutoff. The old records bloat the WAL; the purge must reclaim
        // them and shrink the on-disk footprint.
        let mut records: Vec<LogRecord> = (1..=50u64)
            .map(|i| record(i, "svc", &format!("old-{i}")))
            .collect();
        records.push(record(cutoff, "svc", "in-window"));
        s.ingest(&tenant("acme"), LogBatch::with_records(records))
            .expect("ingest");

        let before = on_disk_bytes(&base);

        let removed = s.purge_older_than(cutoff).expect("purge");
        assert_eq!(removed, 50, "exactly the 50 sub-cutoff records are removed");

        // (a) Querying returns only the in-window record.
        let after_purge = s.query(&tenant("acme"), TimeRange::all()).expect("query");
        assert_eq!(after_purge.len(), 1, "only the in-window record survives");
        assert_eq!(after_purge[0].observed_time_unix_nano, cutoff);

        // (c) On-disk footprint strictly decreased.
        let after = on_disk_bytes(&base);
        assert!(
            after < before,
            "on-disk footprint must strictly shrink: before={before} after={after}"
        );
    }

    // (b) Reopen from the same path: old records STILL gone, in-window intact.
    let s2 = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let reopened = s2
        .query(&tenant("acme"), TimeRange::all())
        .expect("query reopen");
    assert_eq!(reopened.len(), 1, "purge is durable across reopen");
    assert_eq!(reopened[0].observed_time_unix_nano, cutoff);
    cleanup(&base);
}

// --------------------------------------------------------------------
// In-window record untouched — content unchanged, field-for-field.
// --------------------------------------------------------------------

#[test]
fn in_window_record_content_is_unchanged_after_purge() {
    let base = temp_base("content_unchanged");
    let cutoff = 500u64;
    let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");

    let survivor = record(cutoff + 100, "checkout", "payment accepted");
    s.ingest(
        &tenant("acme"),
        LogBatch::with_records(vec![record(10, "svc", "old"), survivor.clone()]),
    )
    .expect("ingest");

    s.purge_older_than(cutoff).expect("purge");

    let rows = s.query(&tenant("acme"), TimeRange::all()).expect("query");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0], survivor,
        "surviving record is byte-for-byte the ingested one"
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
    let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        LogBatch::with_records(vec![
            record(cutoff - 1, "svc", "just-too-old"),
            record(cutoff, "svc", "at-cutoff"),
        ]),
    )
    .expect("ingest");

    let removed = s.purge_older_than(cutoff).expect("purge");
    assert_eq!(removed, 1, "only the cutoff-1 record is removed");

    let rows = s.query(&tenant("acme"), TimeRange::all()).expect("query");
    assert_eq!(rows.len(), 1, "the at-cutoff record is kept");
    assert_eq!(rows[0].observed_time_unix_nano, cutoff);
    cleanup(&base);
}

// --------------------------------------------------------------------
// Monotonic bound — repeated old-ingest + purge does not grow unbounded.
// --------------------------------------------------------------------

#[test]
fn repeated_old_ingest_and_purge_stays_bounded() {
    let base = temp_base("monotonic");
    let cutoff = 1_000u64;
    let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        LogBatch::with_records(vec![record(cutoff + 1, "svc", "in-window")]),
    )
    .expect("seed in-window");

    let mut footprints: Vec<u64> = Vec::new();
    for iter in 0..5u64 {
        let old: Vec<LogRecord> = (1..=20u64)
            .map(|i| record(i, "svc", &format!("old-{iter}-{i}")))
            .collect();
        s.ingest(&tenant("acme"), LogBatch::with_records(old))
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
// No-op — cutoff below every record removes nothing, returns 0.
// --------------------------------------------------------------------

#[test]
fn purge_is_a_no_op_when_nothing_is_older_than_cutoff() {
    let base = temp_base("noop");
    let s = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");
    s.ingest(
        &tenant("acme"),
        LogBatch::with_records(vec![record(100, "svc", "a"), record(200, "svc", "b")]),
    )
    .expect("ingest");

    let removed = s.purge_older_than(50).expect("purge");
    assert_eq!(removed, 0, "nothing older than the cutoff");

    let rows = s.query(&tenant("acme"), TimeRange::all()).expect("query");
    assert_eq!(rows.len(), 2, "both records remain");
    cleanup(&base);
}
