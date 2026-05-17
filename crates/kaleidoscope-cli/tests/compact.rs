// Kaleidoscope CLI — compact subcommand acceptance test
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

//! `compact` triggers `snapshot()` on Lumen v1 and Cinder v1
//! file-backed stores. The test asserts the side effects an
//! operator cares about: snapshot files appear, the next
//! `open()` recovers state, and the WAL files shrink to zero
//! bytes (bounding recovery time).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{compact, ingest, read, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn temp_data_dir(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("kal-cli-compact-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &std::path::Path) {
    let _ = fs::remove_dir_all(p);
}

fn ndjson(records: &[LogRecord]) -> String {
    records
        .iter()
        .map(|r| serde_json::to_string(r).expect("serialise"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn wal_size_bytes(base: &std::path::Path) -> u64 {
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
fn compact_writes_snapshot_files_and_truncates_wals() {
    let dir = temp_data_dir("snapshot_truncate");
    let tn = tenant("acme");
    let records: Vec<LogRecord> = (0..50u64).map(|i| record(i, "body")).collect();
    ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("ingest");

    let lumen_base = dir.join("lumen");
    let cinder_base = dir.join("cinder");
    assert!(wal_size_bytes(&lumen_base) > 0, "lumen WAL has data");
    assert!(wal_size_bytes(&cinder_base) > 0, "cinder WAL has data");
    assert!(!snapshot_exists(&lumen_base), "no lumen snapshot yet");
    assert!(!snapshot_exists(&cinder_base), "no cinder snapshot yet");

    let stats = compact(&dir).expect("compact");
    assert!(stats.lumen_snapshotted);
    assert!(stats.cinder_snapshotted);

    assert_eq!(
        wal_size_bytes(&lumen_base),
        0,
        "lumen WAL truncated after compact"
    );
    assert_eq!(
        wal_size_bytes(&cinder_base),
        0,
        "cinder WAL truncated after compact"
    );
    assert!(snapshot_exists(&lumen_base), "lumen snapshot written");
    assert!(snapshot_exists(&cinder_base), "cinder snapshot written");
    cleanup(&dir);
}

#[test]
fn compact_preserves_query_results_across_subsequent_reopen() {
    let dir = temp_data_dir("preserves_query");
    let tn = tenant("acme");
    let records: Vec<LogRecord> = (0..30u64)
        .map(|i| record(i * 10, &format!("body-{i}")))
        .collect();
    ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("ingest");

    compact(&dir).expect("compact");

    // Read after compact should still return all 30 records in
    // observed-time order.
    let mut buf: Vec<u8> = Vec::new();
    let count = read(&tn, &dir, &mut buf).expect("read");
    assert_eq!(count, 30);
    let out = String::from_utf8(buf).expect("utf8");
    let parsed: Vec<LogRecord> = out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse"))
        .collect();
    assert_eq!(parsed, records);
    cleanup(&dir);
}

#[test]
fn compact_is_idempotent_under_no_intervening_writes() {
    let dir = temp_data_dir("idempotent");
    ingest(
        &tenant("acme"),
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(100, "a")]).into_bytes()),
        None,
    )
    .expect("ingest");

    compact(&dir).expect("compact 1");
    // A second compact with no intervening writes must succeed
    // and leave the on-disk state consistent.
    compact(&dir).expect("compact 2");

    let lumen_base = dir.join("lumen");
    let cinder_base = dir.join("cinder");
    assert!(snapshot_exists(&lumen_base));
    assert!(snapshot_exists(&cinder_base));
    assert_eq!(wal_size_bytes(&lumen_base), 0);
    assert_eq!(wal_size_bytes(&cinder_base), 0);
    cleanup(&dir);
}

#[test]
fn compact_on_empty_data_dir_does_not_error() {
    // An operator might run `compact` before any data has been
    // ingested. The stores open as empty, snapshot writes an
    // empty state, no error.
    let dir = temp_data_dir("empty_compact");
    let stats = compact(&dir).expect("compact empty");
    assert!(stats.lumen_snapshotted);
    assert!(stats.cinder_snapshotted);
    cleanup(&dir);
}

#[test]
fn ingest_after_compact_appends_to_fresh_wal_not_snapshot() {
    // After compact, the WAL is zero bytes. A subsequent ingest
    // appends to the WAL; a follow-up read returns BOTH the
    // pre-compact records (from snapshot) AND the post-compact
    // records (from WAL).
    let dir = temp_data_dir("ingest_after_compact");
    let tn = tenant("acme");

    ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(100, "pre-compact")]).into_bytes()),
        None,
    )
    .expect("first ingest");

    compact(&dir).expect("compact");
    let lumen_base = dir.join("lumen");
    assert_eq!(wal_size_bytes(&lumen_base), 0);

    ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(200, "post-compact")]).into_bytes()),
        None,
    )
    .expect("second ingest");
    assert!(wal_size_bytes(&lumen_base) > 0, "WAL has post-compact data");

    let mut buf: Vec<u8> = Vec::new();
    let count = read(&tn, &dir, &mut buf).expect("read");
    assert_eq!(count, 2);
    let out = String::from_utf8(buf).expect("utf8");
    let parsed: Vec<LogRecord> = out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse"))
        .collect();
    assert_eq!(parsed[0].body, "pre-compact");
    assert_eq!(parsed[1].body, "post-compact");
    cleanup(&dir);
}
