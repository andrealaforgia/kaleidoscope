// Kaleidoscope CLI — `read --since` / `--until` time-range filter acceptance test
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

//! # Acceptance tests — `read` time-range filter (`--since` / `--until`)
//!
//! Feature: `cli-read-time-range-v0`.
//!
//! Extends `kaleidoscope-cli read` with two new optional flags
//! `--since <ISO 8601 UTC>` and `--until <ISO 8601 UTC>` so the
//! library `read()` function can drive any
//! `lumen::TimeRange::new(since_ns, until_ns)` query instead of the
//! hard-coded `TimeRange::all()` at
//! `crates/kaleidoscope-cli/src/lib.rs:283-285`.
//!
//! ## Mapping to outcome KPIs
//!
//! - **OK1 — bounded-window-filter (principal / North Star)**: test
//!   `bounded_window_returns_only_records_in_half_open_interval`
//!   exercises the closed-lower / open-upper boundary contract
//!   inherited from `lumen::TimeRange::contains`
//!   (`crates/lumen/src/record.rs:116-119`).
//! - **OK2 — no-flag byte equivalence (guardrail)**: test
//!   `no_flag_default_is_byte_equivalent_to_time_range_all` asserts
//!   library-direct call with `TimeRange::all()` produces stdout bytes
//!   and return count equal to pre-feature behaviour. The locked
//!   subprocess tests `observe_otlp_read_flag.rs` and
//!   `observe_otlp_flag.rs` continue to pass byte-equivalently because
//!   their argv lists do NOT mention `--since` / `--until`, so they
//!   hit the no-flag default.
//! - **OK3 — half-bounded support (leading)**: tests
//!   `since_only_uses_u64_max_upper_bound` and
//!   `until_only_uses_zero_lower_bound` exercise the implicit-
//!   unbounded-side semantics.
//! - **OK4 — invalid ISO 8601 fails fast (guardrail)**: tests
//!   `invalid_since_value_fails_fast_naming_flag_in_stderr` and
//!   `invalid_until_value_fails_fast_naming_flag_in_stderr` spawn the
//!   binary as a subprocess (mirroring `cli_binary_smoke.rs`) and
//!   assert on exit code, stderr content, stdout emptiness, and
//!   filesystem-absence of the Lumen store (fail-before-store-open
//!   invariant).
//!
//! ## RED state at v0
//!
//! These tests pass `TimeRange::new(s, e)` as the fifth argument to
//! `kaleidoscope_cli::read`. The shipped signature today is 4
//! parameters (`tenant, data_dir, mut writer, otlp_log_path`); the
//! new parameter `range: TimeRange` is the DESIGN DD1 extension that
//! the DELIVER crafter will add. The file will not compile against
//! the current `lib.rs` — that compile failure IS the RED gate for
//! outside-in TDD. The subprocess tests will also fail because the
//! binary does not yet parse `--since` / `--until`.
//!
//! ## Witness timestamps
//!
//! Per DWD-04 in `distill/wave-decisions.md`, the bounded-window
//! test uses easy literal nanos `{100, 200, 300, 400, 500}` so a
//! reviewer can verify boundary inclusion / exclusion by inspection
//! against `TimeRange::new(200, 400)`: records at `200` and `300`
//! are included, `400` is excluded (open upper), `100` is excluded
//! (below lower), `500` is excluded (above upper).
//!
//! ## Harness duplication
//!
//! The harness helpers (`tenant`, `record`, `temp_root`, `cleanup`,
//! `ndjson`) are duplicated inline at v0 per DISCUSS D7 / DESIGN DD4
//! last row. Rule-of-three extraction to `tests/common/mod.rs` is a
//! separate refactoring task and is NOT a deliverable of this
//! feature.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, read, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber, TimeRange};

// --------------------------------------------------------------------
// Helpers (duplicated inline per DISCUSS D7 / DESIGN DD4 last row).
// --------------------------------------------------------------------

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

fn temp_root(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("kal-cli-read-time-range-{name}-{pid}-{nanos}"));
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

/// Pre-ingest the given records into `data_dir` under tenant `acme`
/// using the unmodified `ingest` library function (no `--observe-otlp`,
/// no flags — pure setup helper).
fn seed(data_dir: &std::path::Path, records: &[LogRecord]) {
    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        data_dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(records).into_bytes()),
        None,
    )
    .expect("seed ingest");
}

/// Locate the compiled `kaleidoscope-cli` binary (mirrors
/// `cli_binary_smoke.rs`). Used by the OK4 subprocess tests.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Test #1 — OK1 bounded-window: closed-lower, open-upper inclusion.
//
// Witness nanos {100, 200, 300, 400, 500} against TimeRange::new(200,
// 400). Records at 200 and 300 must be present; 100 (below lower),
// 400 (open upper boundary), 500 (above upper) must be absent.
// --------------------------------------------------------------------

#[test]
fn bounded_window_returns_only_records_in_half_open_interval() {
    // Given Priya has pre-ingested 5 records for tenant `acme` with
    // observed_time_unix_nano values {100, 200, 300, 400, 500}.
    let root = temp_root("ok1_bounded_window");
    let data = root.join("data");
    let r100 = record(100, "a");
    let r200 = record(200, "b");
    let r300 = record(300, "c");
    let r400 = record(400, "d");
    let r500 = record(500, "e");
    seed(
        &data,
        &[
            r100.clone(),
            r200.clone(),
            r300.clone(),
            r400.clone(),
            r500.clone(),
        ],
    );

    // When Priya invokes `read` with TimeRange::new(200, 400) and a
    // captured stdout sink.
    let acme = tenant("acme");
    let mut stdout = Vec::<u8>::new();
    let count = read(&acme, &data, &mut stdout, None, TimeRange::new(200, 400)).expect("read");

    // Then the returned count equals 2 (only r200 and r300 match the
    // half-open interval [200, 400)).
    assert_eq!(count, 2, "exactly two records match [200, 400)");

    // And the captured stdout bytes equal the records r200 and r300
    // re-serialised as NDJSON, one per line, terminated by `\n`.
    let mut expected = ndjson(&[r200, r300]).into_bytes();
    expected.push(b'\n');
    assert_eq!(
        stdout, expected,
        "stdout contains exactly the records at 200 and 300, in order, NDJSON-terminated"
    );

    // And the boundary records are correctly handled:
    //   - r200 (== since_ns) INCLUDED  → closed lower bound
    //   - r400 (== until_ns) EXCLUDED  → open upper bound
    let stdout_str = String::from_utf8(stdout).expect("utf8");
    assert!(
        stdout_str.contains("\"observed_time_unix_nano\":200"),
        "record at exactly since_ns (200) is INCLUDED (closed lower bound)"
    );
    assert!(
        !stdout_str.contains("\"observed_time_unix_nano\":400"),
        "record at exactly until_ns (400) is EXCLUDED (open upper bound)"
    );
    assert!(
        !stdout_str.contains("\"observed_time_unix_nano\":100"),
        "record below since_ns (100) is EXCLUDED"
    );
    assert!(
        !stdout_str.contains("\"observed_time_unix_nano\":500"),
        "record above until_ns (500) is EXCLUDED"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK2 no-flag byte equivalence: TimeRange::all() preserves
// the pre-feature stdout bytes and return count.
//
// The locked subprocess tests observe_otlp_read_flag.rs and
// observe_otlp_flag.rs continue to pass byte-equivalently because
// their argv lists do NOT include --since / --until, hitting the
// no-flag default which DESIGN DD1 pins to TimeRange::all().
// --------------------------------------------------------------------

#[test]
fn no_flag_default_is_byte_equivalent_to_time_range_all() {
    // Given Priya has pre-ingested 4 records for tenant `acme`.
    let root = temp_root("ok2_no_flag");
    let data = root.join("data");
    let records: Vec<LogRecord> = (10..14u64).map(|i| record(i, "y")).collect();
    let expected_ndjson = ndjson(&records);
    seed(&data, &records);

    // When Priya invokes `read` with TimeRange::all() (the no-flag
    // default per DESIGN DD1).
    let acme = tenant("acme");
    let mut stdout = Vec::<u8>::new();
    let count = read(&acme, &data, &mut stdout, None, TimeRange::all()).expect("read");

    // Then the returned count equals N (all records matched).
    assert_eq!(count, 4, "TimeRange::all() matches every record");

    // And the captured stdout bytes are byte-equivalent to the
    // pre-ingested records re-serialised as NDJSON, one per line,
    // terminated by `\n` — same shape today's `read()` produces.
    let mut expected_stdout = expected_ndjson.into_bytes();
    expected_stdout.push(b'\n');
    assert_eq!(
        stdout, expected_stdout,
        "no-flag default stdout bytes are byte-equivalent to pre-feature behaviour"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK3a half-bounded: --since alone uses u64::MAX as the
// implicit upper bound.
//
// Witness nanos {100, 200, 300, 400}. TimeRange::new(200, u64::MAX)
// must match records at 200, 300, 400 (closed lower at 200, open upper
// at u64::MAX so the maximum ingested value is included).
// --------------------------------------------------------------------

#[test]
fn since_only_uses_u64_max_upper_bound() {
    // Given Priya has pre-ingested 3 records with
    // observed_time_unix_nano values {100, 200, 300}.
    let root = temp_root("ok3_since_only");
    let data = root.join("data");
    let r100 = record(100, "a");
    let r200 = record(200, "b");
    let r300 = record(300, "c");
    seed(&data, &[r100, r200.clone(), r300.clone()]);

    // When Priya invokes `read` with TimeRange::new(200, u64::MAX) —
    // the shape the binary constructs when `--since 200-equivalent` is
    // present and `--until` is absent.
    let acme = tenant("acme");
    let mut stdout = Vec::<u8>::new();
    let count = read(
        &acme,
        &data,
        &mut stdout,
        None,
        TimeRange::new(200, u64::MAX),
    )
    .expect("read");

    // Then the returned count equals 2 (records at 200 and 300).
    assert_eq!(count, 2, "since-only matches records from since_ns onwards");

    // And the captured stdout contains exactly r200 and r300 as NDJSON.
    let mut expected = ndjson(&[r200, r300]).into_bytes();
    expected.push(b'\n');
    assert_eq!(
        stdout, expected,
        "stdout contains records at 200 and 300 in order"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK3b half-bounded: --until alone uses 0 as the implicit
// lower bound.
//
// Witness nanos {100, 200, 300}. TimeRange::new(0, 200) must match
// the record at 100 only (closed lower at 0 includes every value
// from the earliest ingested; open upper at 200 excludes the record
// at exactly 200).
// --------------------------------------------------------------------

#[test]
fn until_only_uses_zero_lower_bound() {
    // Given Priya has pre-ingested 3 records with
    // observed_time_unix_nano values {100, 200, 300}.
    let root = temp_root("ok3_until_only");
    let data = root.join("data");
    let r100 = record(100, "a");
    let r200 = record(200, "b");
    let r300 = record(300, "c");
    seed(&data, &[r100.clone(), r200, r300]);

    // When Priya invokes `read` with TimeRange::new(0, 200) — the
    // shape the binary constructs when `--until 200-equivalent` is
    // present and `--since` is absent.
    let acme = tenant("acme");
    let mut stdout = Vec::<u8>::new();
    let count = read(&acme, &data, &mut stdout, None, TimeRange::new(0, 200)).expect("read");

    // Then the returned count equals 1 (only the record at 100;
    // 200 is the open upper bound so it is EXCLUDED).
    assert_eq!(
        count, 1,
        "until-only matches records strictly before until_ns"
    );

    // And the captured stdout contains exactly r100 as NDJSON.
    let mut expected = ndjson(&[r100]).into_bytes();
    expected.push(b'\n');
    assert_eq!(stdout, expected, "stdout contains the record at 100 only");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — OK4a invalid --since fails fast at the binary boundary.
//
// Spawn the CARGO_BIN_EXE_kaleidoscope-cli binary with argv list
// ["read", "acme", "<data_dir>", "--since", "not-an-iso"]. Assert:
//   - exit code != 0
//   - stderr contains both `--since` and the verbatim bad value
//     `not-an-iso`
//   - stdout is empty (no records were written)
//   - the Lumen store under data_dir was NOT opened (no `lumen.*`
//     files exist post-call) — filesystem-absence probe per DEVOPS
//     KPI-instrumentation OK4 ("fail-before-store-open invariant").
//
// Subprocess (not library-direct) because OK4 is the
// binary-boundary fail-fast contract: the binary's argv parser
// must surface the error before reaching the library `read()`.
// --------------------------------------------------------------------

#[test]
fn invalid_since_value_fails_fast_naming_flag_in_stderr() {
    let root = temp_root("ok4_invalid_since");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya invokes the binary with a malformed --since value.
    let output = Command::new(bin())
        .arg("read")
        .arg("acme")
        .arg(&data)
        .arg("--since")
        .arg("not-an-iso")
        .stdin(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-cli read with bad --since");

    // Then the process exits non-zero (ExitCode::FAILURE per
    // DISCUSS D3 / DESIGN DD3 fail-fast contract).
    assert!(
        !output.status.success(),
        "invalid --since must exit non-zero; got status {:?}",
        output.status
    );

    // And stderr names the offending flag AND the verbatim bad value.
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("--since"),
        "stderr names the offending flag `--since`: {stderr:?}"
    );
    assert!(
        stderr.contains("not-an-iso"),
        "stderr contains the verbatim bad value `not-an-iso`: {stderr:?}"
    );

    // And stdout is empty (no records were written).
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on parse failure; got {} bytes",
        output.stdout.len()
    );

    // And the Lumen store under data_dir was NOT opened
    // (filesystem-absence probe — the substrate IS the test oracle).
    // No `lumen.*` files should exist because the parser failed BEFORE
    // `FileBackedLogStore::open` was called.
    let lumen_files: Vec<_> = fs::read_dir(&data)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("lumen"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        lumen_files.is_empty(),
        "fail-fast contract: no Lumen store files must exist post-call; got {lumen_files:?}"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #6 — OK4b invalid --until fails fast at the binary boundary.
//
// Symmetric to test #5: the argv list includes a valid --since AND a
// malformed --until value (`2026-13-32T25:99:99Z` — out-of-range
// month, day, hour, minute, second; passes the shape check by length
// but fails calendar-range validation per DESIGN DD3). Same assertions
// on exit code, stderr content, stdout emptiness, and Lumen store
// absence.
// --------------------------------------------------------------------

#[test]
fn invalid_until_value_fails_fast_naming_flag_in_stderr() {
    let root = temp_root("ok4_invalid_until");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya invokes the binary with a calendar-out-of-range
    // --until value.
    let bad_until = "2026-13-32T25:99:99Z";
    let output = Command::new(bin())
        .arg("read")
        .arg("acme")
        .arg(&data)
        .arg("--since")
        .arg("2026-05-18T00:00:00Z")
        .arg("--until")
        .arg(bad_until)
        .stdin(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-cli read with bad --until");

    // Then the process exits non-zero.
    assert!(
        !output.status.success(),
        "invalid --until must exit non-zero; got status {:?}",
        output.status
    );

    // And stderr names the offending flag AND the verbatim bad value.
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("--until"),
        "stderr names the offending flag `--until`: {stderr:?}"
    );
    assert!(
        stderr.contains(bad_until),
        "stderr contains the verbatim bad value `{bad_until}`: {stderr:?}"
    );

    // And stdout is empty.
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on parse failure; got {} bytes",
        output.stdout.len()
    );

    // And the Lumen store under data_dir was NOT opened.
    let lumen_files: Vec<_> = fs::read_dir(&data)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("lumen"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        lumen_files.is_empty(),
        "fail-fast contract: no Lumen store files must exist post-call; got {lumen_files:?}"
    );

    cleanup(&root);
}
