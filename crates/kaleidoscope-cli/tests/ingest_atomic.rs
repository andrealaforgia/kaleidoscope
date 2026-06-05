// Kaleidoscope CLI — all-or-nothing ingest on parse error
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

//! Operator-flow acceptance test — all-or-nothing ingest on a parse error.
//!
//! Priya the platform operator pipes an NDJSON file into the CLI. When a
//! line partway through the file is malformed, the command must commit
//! NOTHING (the store count is unchanged), name the offending line, and
//! survive her reflex re-run without double-counting. After she fixes the
//! named line, the corrected file ingests every record exactly once.
//!
//! This mirrors the shell pipeline an operator writes:
//!
//! ```text
//! cat acme-logs.ndjson | kaleidoscope-cli ingest acme /tmp/data
//! kaleidoscope-cli: parse record at line 4: ...
//! kaleidoscope-cli stats acme /tmp/data   # records=0  (committed nothing)
//! ```
//!
//! ## Test strategy (@real-io)
//!
//! Every test drives the CLI through its library entry point `ingest(...)`
//! — the in-process equivalent of spawning the binary and piping NDJSON on
//! stdin (ADR-0064 DD-6). The store is a REAL `FileBackedLogStore` on a
//! per-test tmp `data_dir`; the committed store count is read back through
//! the shipped `read(...)` surface against the SAME `data_dir` (the
//! in-process equivalent of `kaleidoscope-cli read`/`stats`). No private
//! helper is touched; no subprocess, no signals, no wall-clock — the
//! observables are a typed `Result` and a committed-state count.
//!
//! ## RED/GREEN status at the DISTILL commit (verified by running)
//!
//! The all-or-nothing behaviour does NOT exist yet — today's `ingest`
//! flushes each full batch DURING the read, so a parse error after a
//! flushed batch leaves a partial commit. The tests that assert the NEW
//! commit-nothing / no-double behaviour therefore FAIL against today's code
//! and are `#[ignore]`d until DELIVER re-orders ingest to
//! parse-all-then-flush-all. The negative controls that already hold today
//! (valid-file, malformed-first-line) stay GREEN as guardrails. The
//! corrected-file test is ALSO RED today — although its corrected input is
//! valid, it builds on a store dirtied by the failed run's partial commit
//! (count 7, not 4), so it too depends on the new behaviour. Un-ignore all
//! ignored tests once the re-ordering lands.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, read, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber, TimeRange};

// --- Harness (mirrors tests/ingest_and_read_roundtrip.rs; inline at v0,
// rule-of-three extraction deferred per cluster precedent). ---

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
    p.push(format!("kal-cli-{name}-{pid}-{nanos}"));
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

/// The committed store count, read back through the shipped `read(...)`
/// surface against the same `data_dir` — the in-process equivalent of
/// `kaleidoscope-cli read <tenant> <data_dir>` / `stats` reporting
/// `records=N`. This is the only way the tests observe committed state;
/// no private Lumen file is inspected.
fn stored_count(tn: &TenantId, dir: &std::path::Path) -> usize {
    let mut sink: Vec<u8> = Vec::new();
    read(tn, dir, &mut sink, None, TimeRange::all()).expect("read store count")
}

/// An NDJSON input of `n` valid `LogRecord`s followed by one malformed
/// line — the malformed line is at 1-based line number `n + 1`.
fn valid_prefix_then_malformed(n: u64) -> String {
    let mut s = ndjson(&(0..n).map(|i| record(i, "valid")).collect::<Vec<_>>());
    s.push('\n');
    s.push_str("{not valid json}");
    s
}

// =========================================================================
// AC1 — parse-error-commits-nothing (the headline footgun).
//
// 3 valid records + a malformed line 4, ingested at batch_size=3 so the
// first batch (lines 1-3) WOULD flush before line 4 under non-atomic
// behaviour. The call must return Err(ParseRecord{line:4}) AND the store
// count must be UNCHANGED at 0 (no partial commit).
//
// RED today: today the first batch flushes -> stored_count == 3 -> the
// `count == 0` assertion fails. Ignored until DELIVER.
// =========================================================================
#[test]
#[ignore = "RED until DELIVER: cli-ingest-atomic-v0 (today commits first batch -> count 3)"]
fn parse_error_commits_nothing() {
    let dir = temp_data_dir("commits_nothing");
    let tn = tenant("acme");

    // Given: a fresh empty store (zero records), and 3 valid records
    // followed by a malformed line 4, ingested at batch_size=3 so a batch
    // would flush before the bad line under non-atomic behaviour.
    assert_eq!(
        stored_count(&tn, &dir),
        0,
        "precondition: store starts empty"
    );
    let input = valid_prefix_then_malformed(3);

    // When: Priya ingests the malformed file.
    let err = ingest(&tn, &dir, 3, Cursor::new(input.into_bytes()), None).unwrap_err();

    // Then: the call names the malformed line 4 ...
    match err {
        kaleidoscope_cli::Error::ParseRecord { line, .. } => {
            assert_eq!(line, 4, "error names the 1-based malformed line number");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    // ... and the store committed NOTHING (count unchanged from the
    // pre-ingest zero — the first batch that would have flushed is held
    // back by the all-or-nothing discipline).
    assert_eq!(
        stored_count(&tn, &dir),
        0,
        "all-or-nothing: a parse error commits zero records"
    );
    cleanup(&dir);
}

// =========================================================================
// AC2 — re-run-no-double.
//
// Running the SAME still-malformed input a second time must again error
// and leave the count UNCHANGED at 0 — no prefix from the first run to
// double, no new partial from the second.
//
// RED today: today run 1 -> count 3, run 2 -> count 6 -> the `count == 0`
// assertion fails. Ignored until DELIVER.
// =========================================================================
#[test]
#[ignore = "RED until DELIVER: cli-ingest-atomic-v0 (today run1->3, run2->6)"]
fn re_run_of_still_malformed_input_does_not_double_count() {
    let dir = temp_data_dir("no_double");
    let tn = tenant("acme");
    let input = valid_prefix_then_malformed(3);

    // Given: the first ingest of the malformed input has already failed,
    // leaving the store count at 0.
    let first = ingest(&tn, &dir, 3, Cursor::new(input.clone().into_bytes()), None).unwrap_err();
    matches!(first, kaleidoscope_cli::Error::ParseRecord { line: 4, .. })
        .then_some(())
        .expect("first run names line 4");
    assert_eq!(
        stored_count(&tn, &dir),
        0,
        "first failed run committed nothing"
    );

    // When: Priya re-runs the same still-malformed input (her reflex).
    let second = ingest(&tn, &dir, 3, Cursor::new(input.into_bytes()), None).unwrap_err();

    // Then: the second call also names line 4 ...
    match second {
        kaleidoscope_cli::Error::ParseRecord { line, .. } => assert_eq!(line, 4),
        other => panic!("unexpected error: {other:?}"),
    }
    // ... and the store count is STILL 0 — the re-run of a still-bad input
    // is a no-op on the count (no double).
    assert_eq!(
        stored_count(&tn, &dir),
        0,
        "re-running a still-malformed input does not double-count"
    );
    cleanup(&dir);
}

// =========================================================================
// AC3 — corrected-file-ingests-once.
//
// After Priya fixes the named line (input now 4 valid records at
// batch_size=3), the ingest commits every record exactly once and exits
// Ok. Asserted against a store that already saw failed ingests of the
// malformed input, to prove no stale partial leaks in.
//
// Classification verified by running: although the corrected input itself
// is fully valid, this test builds on a store that already saw a FAILED
// ingest of the malformed input. Today that failed run commits a partial
// first batch of 3, so the corrected 4-record ingest lands on a dirty
// store and the count is 3 + 4 = 7, not 4. The "exactly once on a store
// left clean by the failed run" contract therefore depends on the NEW
// all-or-nothing behaviour. RED today -> ignored until DELIVER.
// =========================================================================
#[test]
#[ignore = "RED until DELIVER: cli-ingest-atomic-v0 (failed run's partial commit dirties store -> count 7 not 4)"]
fn corrected_file_ingests_every_record_exactly_once() {
    let dir = temp_data_dir("corrected");
    let tn = tenant("acme");

    // Given: the store count is 0 after a failed ingest naming line 4 ...
    let malformed = valid_prefix_then_malformed(3);
    let _ = ingest(&tn, &dir, 3, Cursor::new(malformed.into_bytes()), None).unwrap_err();
    // ... and Priya has corrected line 4 so the input is now 4 valid
    // records, ingested at batch_size=3 (3 + 1 = 2 batches).
    let corrected = ndjson(&(0..4u64).map(|i| record(i, "valid")).collect::<Vec<_>>());

    // When: Priya ingests the corrected file.
    let stats = ingest(&tn, &dir, 3, Cursor::new(corrected.into_bytes()), None).expect("ingest ok");

    // Then: every record committed exactly once, exit Ok.
    assert_eq!(stats.records_ingested, 4);
    assert_eq!(
        stats.batches_flushed, 2,
        "3 + 1 = 2 batches at batch_size=3"
    );
    assert_eq!(stats.tier_items_placed, 2);
    // And the store holds exactly 4 — not 0, not 8 (no stale partial from
    // the failed run, no double).
    assert_eq!(
        stored_count(&tn, &dir),
        4,
        "corrected file commits exactly once on a store left clean by the failed run"
    );
    cleanup(&dir);
}

// =========================================================================
// AC4 — valid-file-negative-control (no-regression guardrail).
//
// A fully-valid file of 250 records at DEFAULT_BATCH_SIZE=100 commits
// every record exactly once with the byte-equivalent IngestStats
// (records=250, batches=3, tier_items=3) — 100 + 100 + 50, identical to
// the existing restart test. This MUST pass today; it is the safety net
// proving the all-valid path does not regress.
//
// GREEN today (the all-valid path already commits correctly). NOT ignored.
// =========================================================================
#[test]
fn fully_valid_file_ingests_every_record_exactly_once_no_regression() {
    let dir = temp_data_dir("valid_control");
    let tn = tenant("acme");

    // Given: a fresh empty store and 250 valid records, no malformed line.
    assert_eq!(
        stored_count(&tn, &dir),
        0,
        "precondition: store starts empty"
    );
    let records: Vec<LogRecord> = (0..250u64).map(|i| record(i, "valid")).collect();

    // When: Priya ingests the fully-valid file at DEFAULT_BATCH_SIZE.
    let stats = ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("ingest ok");

    // Then: every record committed exactly once; IngestStats is
    // byte-equivalent to today (250 / 100 = 3 batches: 100 + 100 + 50).
    assert_eq!(
        stats,
        kaleidoscope_cli::IngestStats {
            records_ingested: 250,
            batches_flushed: 3,
            tier_items_placed: 3,
        },
        "all-valid IngestStats is byte-equivalent to pre-change behaviour"
    );
    assert_eq!(
        stored_count(&tn, &dir),
        250,
        "every record committed exactly once"
    );
    cleanup(&dir);
}

// =========================================================================
// AC5 — malformed-first-line boundary.
//
// A file whose VERY FIRST line is malformed: Err(ParseRecord{line:1}) and
// count 0. This is the degenerate all-or-nothing case — no batch ever
// could have flushed — so both old and new code commit nothing here.
//
// Classification verified by running: nothing flushes before line 1
// fails, so the count is 0 today; this PASSES today and stays a guardrail
// (NOT ignored). It nails the "names the bad line" half at the boundary.
// =========================================================================
#[test]
fn malformed_first_line_commits_nothing_and_names_line_one() {
    let dir = temp_data_dir("first_line");
    let tn = tenant("acme");

    // Given: a fresh empty store and input whose first line is malformed,
    // no valid prefix at all.
    assert_eq!(
        stored_count(&tn, &dir),
        0,
        "precondition: store starts empty"
    );
    let input = "{not valid json}\n";

    // When: Priya ingests the file.
    let err = ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(input.as_bytes()),
        None,
    )
    .unwrap_err();

    // Then: the error names line 1 ...
    match err {
        kaleidoscope_cli::Error::ParseRecord { line, .. } => {
            assert_eq!(line, 1, "error names the malformed first line");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    // ... and the store committed nothing.
    assert_eq!(stored_count(&tn, &dir), 0, "no batch ever flushed");
    cleanup(&dir);
}
