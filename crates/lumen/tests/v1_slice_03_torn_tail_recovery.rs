// Kaleidoscope Lumen v1 — slice 03 torn-tail recovery acceptance suite
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

//! Slice 03 — torn-tail recovery, lumen store-reopen path
//! (wal-torn-tail-recovery-v0, US-01).
//!
//! Feature: a crashed-then-restarted store recovers its intact acked
//! prefix and drops the torn tail. This suite exercises the
//! **store-reopen driving port**: `FileBackedLogStore::open` reopened on a
//! crashed tmp `pillar_root`, then queried through the `LogStore` trait.
//! The end-to-end binary + `GET /api/v1/logs` headline (verifier D04) and
//! the structured WARN (AC-3) live in
//! `crates/log-query-api/tests/slice_08_torn_tail_recovery.rs`, because
//! `CARGO_BIN_EXE_log-query-api` is only set for that crate's tests.
//!
//! ## I-O strategy: C (real local I/O)
//!
//! Real WAL files on a real tmp directory, real reopen, real query. No
//! external services, no containers. See
//! `docs/feature/wal-torn-tail-recovery-v0/distill/wave-decisions.md`
//! DWD-1.
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! Every scenario is `#[ignore]`d until its DELIVER slice removes the
//! marker one at a time (Outside-In). The tests drive ONLY existing public
//! APIs (`FileBackedLogStore::open` / `query`, on-disk WAL bytes), so they
//! COMPILE against today's code with no scaffold. They are RED because
//! today's `open` refuses a torn tail with `PersistenceFailed`; they are
//! never BROKEN. No reference to the not-yet-existing `crates/wal-recovery`
//! symbol appears here.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, LogStoreError, NoopRecorder, SeverityNumber,
    TimeRange,
};

// --------------------------------------------------------------------
// Helpers — match the established v1 file-backed convention exactly
// (std tmp dir + PID + nanos, manual cleanup; no new dev-dependency).
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, service: &str, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
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

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("lumen-torn-tail-{test_name}-{pid}-{nanos}"));
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

/// Seed a healthy store with `n` acked single-record batches, then close
/// it so the WAL is flushed to disk.
fn seed_acked_prefix(base: &Path, n: u64) {
    let store = FileBackedLogStore::open(base, Box::new(NoopRecorder)).expect("seed open");
    for i in 0..n {
        store
            .ingest(
                &tenant("acme-corp"),
                LogBatch::with_records(vec![record(100 + i, "checkout", &format!("order {i}"))]),
            )
            .expect("seed ingest");
    }
    drop(store);
}

/// Append a torn final line (partial JSON, NO trailing newline) to the
/// WAL, exactly as a `kill -9` between `write_all(bytes)` and
/// `write_all(b"\n")` leaves it. Returns the byte length of the appended
/// torn line.
fn append_torn_tail(base: &Path, torn: &str) -> usize {
    let wal = wal_path_of(base);
    let existing = fs::read_to_string(&wal).unwrap_or_default();
    fs::write(&wal, format!("{existing}{torn}")).expect("append torn tail");
    torn.len()
}

// --------------------------------------------------------------------
// AC-1 / AC-2 (store-reopen driving port): reopen FileBackedLogStore on a
// crashed pillar_root and query the recovered prefix directly through the
// LogStore trait. The torn tail is dropped, never repaired.
// --------------------------------------------------------------------

#[test]
#[ignore = "RED until DELIVER: wal-torn-tail-recovery-v0 slice 03 (AC-1 store reopen)"]
fn reopen_recovers_the_intact_prefix_and_drops_the_torn_tail() {
    // @real-io @adapter-integration @US-01 @AC-1 @AC-2
    let base = temp_base("reopen_prefix");
    seed_acked_prefix(&base, 5);
    append_torn_tail(
        &base,
        "{\"op\":\"ingest\",\"tenant\":\"acme-corp\",\"records\":[{\"body\":\"par",
    );

    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the intact prefix");
    let out = store
        .query(&tenant("acme-corp"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 5, "exactly the 5 acked records recover");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert_eq!(
        bodies,
        vec!["order 0", "order 1", "order 2", "order 3", "order 4"],
        "recovered records are in original observed_time order, none partial"
    );
    cleanup(&base);
}

#[test]
#[ignore = "RED until DELIVER: wal-torn-tail-recovery-v0 slice 03 (AC-1 N=1 boundary)"]
fn reopen_recovers_a_single_acked_record_before_the_torn_tail() {
    // @real-io @adapter-integration @US-01 @AC-1 (N=1 boundary)
    let base = temp_base("reopen_n1");
    seed_acked_prefix(&base, 1);
    append_torn_tail(
        &base,
        "{\"op\":\"ingest\",\"tenant\":\"acme-corp\",\"records\":[{\"bo",
    );

    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers N=1 prefix");
    let out = store
        .query(&tenant("acme-corp"), TimeRange::all())
        .expect("query");
    assert_eq!(out.len(), 1, "the single acked record recovers");
    assert_eq!(out[0].body, "order 0");
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-5 (NEGATIVE, lumen): a mid-file corrupt line (NOT the last line)
// keeps the store fail-closed; the intact-prefix path is NOT taken.
// --------------------------------------------------------------------

#[test]
#[ignore = "RED until DELIVER: wal-torn-tail-recovery-v0 slice 03 (AC-5 mid-file fail-closed)"]
fn mid_file_corruption_stays_fail_closed() {
    // @real-io @adapter-integration @US-01 @AC-5
    let base = temp_base("midfile");
    seed_acked_prefix(&base, 3);
    // Insert a malformed line in the MIDDLE (terminated by a newline) with
    // valid lines after it, so it is provably not the torn tail.
    let wal = wal_path_of(&base);
    let mut lines: Vec<String> = fs::read_to_string(&wal)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    lines.insert(1, "{not valid json".to_string());
    fs::write(&wal, format!("{}\n", lines.join("\n"))).expect("rewrite wal");

    let err = FileBackedLogStore::open(&base, Box::new(NoopRecorder))
        .expect_err("mid-file corruption must refuse");
    let LogStoreError::PersistenceFailed { reason } = err;
    assert!(
        reason.contains("line 2"),
        "the refusal names the offending line number; got {reason}"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-6 (NEGATIVE, lumen): a malformed FINAL line that DOES end in a
// trailing newline stays fail-closed. The trailing newline is the
// discriminator: a complete write, not a torn tear.
// --------------------------------------------------------------------

#[test]
#[ignore = "RED until DELIVER: wal-torn-tail-recovery-v0 slice 03 (AC-6 newline-terminated fail-closed)"]
fn newline_terminated_malformed_final_line_stays_fail_closed() {
    // @real-io @adapter-integration @US-01 @AC-6
    let base = temp_base("newline_malformed");
    seed_acked_prefix(&base, 2);
    // Malformed final line WITH a trailing newline.
    let wal = wal_path_of(&base);
    let existing = fs::read_to_string(&wal).unwrap();
    fs::write(&wal, format!("{existing}{{not valid json}}\n")).expect("append malformed line");

    let err = FileBackedLogStore::open(&base, Box::new(NoopRecorder))
        .expect_err("a complete-but-malformed final line must refuse");
    assert!(matches!(err, LogStoreError::PersistenceFailed { .. }));
    cleanup(&base);
}
