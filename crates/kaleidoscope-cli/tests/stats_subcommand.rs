// Kaleidoscope CLI — `stats` subcommand acceptance test
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

//! # Acceptance tests — `stats` subcommand
//!
//! When the operator invokes `kaleidoscope-cli stats <tenant> <data_dir>`,
//! the library function `kaleidoscope_cli::stats(...)` queries Lumen
//! once via `LogStore::query(tenant, TimeRange::all())` and writes
//! plain-text key=value lines to the supplied writer:
//!
//! - Populated tenant (N > 0): three lines, in order —
//!   `records=N\n`, `earliest=<ISO 8601 UTC>\n`, `latest=<ISO 8601 UTC>\n`.
//! - Empty tenant (N == 0): one line — `records=0\n`. No `earliest=`,
//!   no `latest=` (`design/wave-decisions.md` DD5 / `discuss` D5).
//!
//! These tests drive the user-visible outcomes of feature
//! `cli-stats-subcommand-v0`:
//!
//! - **US-01 / OK1 (principal — record count correctness)**: the
//!   `records=N` line equals what `read()` would return for the same
//!   `(tenant, data_dir)`; tenant isolation honoured.
//! - **US-01 / OK2 (leading — time range correctness)**: the
//!   `earliest=` / `latest=` ISO 8601 UTC values equal the seeded
//!   min/max `observed_time_unix_nano`; single-record tenants yield
//!   byte-identical earliest/latest.
//! - **US-01 / OK3 (guardrail — empty-tenant unambiguity)**: a tenant
//!   with zero records writes exactly `records=0\n` and no
//!   timestamp lines.
//!
//! Note on the ISO 8601 contract: per DESIGN DD1 / DD6 the formatter is
//! hand-rolled and emits `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` — UTC, `Z`
//! suffix, always nine nanosecond digits. The byte-exact strings in
//! the assertions below derive directly from this contract; any drift
//! in the formatter surfaces as a byte-mismatch in test #1 or test #3.
//!
//! Note on the harness pattern: the `tenant`, `record`, `temp_root`,
//! `cleanup`, and `ndjson` helpers are duplicated inline at v0 per
//! `wave-decisions.md` DD9 (rule-of-three extraction deferred to a
//! follow-up refactor — this is the fifth test file using the same
//! shape, after the four `tests/observe_otlp_*.rs` siblings).
//!
//! Note on RED state at v0: every test below calls
//! `kaleidoscope_cli::stats(...)`. That function does not yet exist on
//! `lib.rs`. The file will not compile against the current crate —
//! that compile failure IS the RED gate for outside-in TDD (DELIVER
//! wave / Crafty adds the function).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, read, stats, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber};

// --------------------------------------------------------------------
// Helpers (mirror observe_otlp_read_flag.rs + observe_otlp_flag.rs;
// rule-of-three deferral confirmed by wave-decisions.md DD9).
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
    p.push(format!("kal-cli-stats-{name}-{pid}-{nanos}"));
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

// --------------------------------------------------------------------
// Deterministic timestamp seeds.
//
// The hand-rolled formatter (DESIGN DD1) renders `u64` nanos as
// `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ`. The constants below are the
// canonical Unix-epoch-nanos for round dates; the corresponding
// rendered strings are stated in the adjacent `EXPECTED_*` constants.
//
// Conversion (verified):
//   2026-05-18T00:00:00.000000000Z  ==  1_779_062_400_000_000_000 ns
//   2026-05-19T00:00:00.000000000Z  ==  1_779_148_800_000_000_000 ns
//   2026-05-11T00:00:00.000000000Z  ==  1_778_457_600_000_000_000 ns
//
// Derivation for 2026-05-18:
//   days from 1970-01-01 to 2026-01-01 = 56 * 365 + 14 leap days = 20_454
//   days from 2026-01-01 to 2026-05-18 = 31+28+31+30+17       =     137
//   total                                                    =  20_591 days
//   seconds = 20_591 * 86_400 = 1_779_062_400
//   nanos   = seconds * 1_000_000_000 = 1_779_062_400_000_000_000
// 2026-05-19 = 2026-05-18 + 86_400_000_000_000 ns.
// 2026-05-11 = 2026-05-18 - 7 * 86_400_000_000_000 ns.
//
// SEEDED_STEP_NS = 14_400_000_000_000 (4 hours) gives 7 evenly-spaced
// records across the 24-hour window:
//   t_0 = 2026-05-18T00:00:00.000000000Z
//   t_1 = 2026-05-18T04:00:00.000000000Z
//   t_2 = 2026-05-18T08:00:00.000000000Z
//   t_3 = 2026-05-18T12:00:00.000000000Z
//   t_4 = 2026-05-18T16:00:00.000000000Z
//   t_5 = 2026-05-18T20:00:00.000000000Z
//   t_6 = 2026-05-19T00:00:00.000000000Z
// --------------------------------------------------------------------

const SEED_EARLIEST_NS: u64 = 1_779_062_400_000_000_000;
// SEED_LATEST_NS documents the expected latest nanos derived from
// SEED_EARLIEST_NS + 6 * SEEDED_STEP_NS. Kept as a named constant for
// clarity in the test contract even though tests reference the ISO
// string literal directly (see EXPECTED_LATEST_LINE).
#[allow(dead_code)]
const SEED_LATEST_NS: u64 = 1_779_148_800_000_000_000;
const SEED_SINGLE_NS: u64 = 1_778_457_600_000_000_000;
const SEEDED_STEP_NS: u64 = 14_400_000_000_000;

const EXPECTED_EARLIEST_LINE: &str = "earliest=2026-05-18T00:00:00.000000000Z";
const EXPECTED_LATEST_LINE: &str = "latest=2026-05-19T00:00:00.000000000Z";
const EXPECTED_SINGLE_EARLIEST_LINE: &str = "earliest=2026-05-11T00:00:00.000000000Z";
const EXPECTED_SINGLE_LATEST_LINE: &str = "latest=2026-05-11T00:00:00.000000000Z";

// --------------------------------------------------------------------
// Test #1 — OK1 + OK2 happy path: populated tenant prints three lines
// in order with the correct count, earliest, and latest.
//
// Seeds 7 records for tenant `acme` evenly spaced at 4-hour intervals
// across the 24-hour window 2026-05-18T00:00:00Z .. 2026-05-19T00:00:00Z.
// Asserts exactly 3 non-empty lines, in order, byte-exact.
// --------------------------------------------------------------------

#[test]
fn stats_populated_tenant_emits_three_lines_in_order() {
    // Given Priya has pre-ingested 7 records for tenant `acme` whose
    // observed_time_unix_nano values span 2026-05-18T00:00:00Z (earliest)
    // to 2026-05-19T00:00:00Z (latest).
    let root = temp_root("ok1_ok2_populated");
    let data = root.join("data");
    let records: Vec<LogRecord> = (0..7u64)
        .map(|i| record(SEED_EARLIEST_NS + i * SEEDED_STEP_NS, "x"))
        .collect();

    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("setup ingest");

    // When Priya invokes `stats` with tenant `acme`, the data_dir, and
    // a captured stdout sink.
    let mut stdout = Vec::<u8>::new();
    let count = stats(&acme, &data, &mut stdout).expect("stats");

    // Then the returned count equals 7 (mirrors `read()`'s return shape).
    assert_eq!(count, 7, "stats() returns matched record count");

    // And the captured stdout contains exactly 3 non-empty lines, in
    // order: `records=7`, `earliest=<ISO 8601>`, `latest=<ISO 8601>`.
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3, "exactly three non-empty lines on stdout");
    assert_eq!(lines[0], "records=7", "line 1 is the records= line");
    assert_eq!(
        lines[1], EXPECTED_EARLIEST_LINE,
        "line 2 is the earliest= line, ISO 8601 UTC with nanosecond precision"
    );
    assert_eq!(
        lines[2], EXPECTED_LATEST_LINE,
        "line 3 is the latest= line, ISO 8601 UTC with nanosecond precision"
    );

    // And the stdout ends with `\n` (output-shape contract from
    // user-stories.md System Constraints).
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK3 empty tenant: writes exactly `records=0\n`, no
// timestamp lines.
//
// Pre-ingests 4 records for tenant `acme` then queries the
// never-ingested tenant `acmee` (the typo case from US-01 Domain
// Example #3) so that the data_dir exists and the Lumen store opens
// cleanly, but the queried tenant has zero records.
// --------------------------------------------------------------------

#[test]
fn stats_empty_tenant_emits_records_zero_and_no_timestamps() {
    // Given the Lumen store at the data_dir exists (because `acme` has
    // been ingested) but contains zero records for tenant `acmee`.
    let root = temp_root("ok3_empty");
    let data = root.join("data");
    let populated_records: Vec<LogRecord> = (0..4u64).map(|i| record(i, "y")).collect();

    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&populated_records).into_bytes()),
        None,
    )
    .expect("setup ingest for acme");

    // When Priya invokes `stats` with the never-ingested tenant `acmee`
    // and a captured stdout sink.
    let acmee = tenant("acmee");
    let mut stdout = Vec::<u8>::new();
    let count = stats(&acmee, &data, &mut stdout).expect("stats");

    // Then the returned count is 0 (empty-tenant is a valid query
    // result, not an error).
    assert_eq!(count, 0, "stats() returns 0 for never-ingested tenant");

    // And the captured stdout contains exactly 1 non-empty line equal to
    // `records=0`. No `earliest=` line. No `latest=` line.
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "empty-tenant stdout contains exactly one non-empty line"
    );
    assert_eq!(lines[0], "records=0", "the sole line is `records=0`");
    assert!(
        !out.contains("earliest="),
        "no `earliest=` line appears on stdout for empty tenant (D5)"
    );
    assert!(
        !out.contains("latest="),
        "no `latest=` line appears on stdout for empty tenant (D5)"
    );

    // And the stdout ends with `\n` (output-shape contract).
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK2 edge: single-record tenant yields byte-identical
// earliest and latest timestamp lines (degenerate one-instant window).
// --------------------------------------------------------------------

#[test]
fn stats_single_record_tenant_emits_identical_earliest_and_latest() {
    // Given Priya has pre-ingested exactly 1 record for tenant `globex`
    // whose observed_time_unix_nano corresponds to 2026-05-11T00:00:00Z.
    let root = temp_root("ok2_single_record");
    let data = root.join("data");
    let only_record = record(SEED_SINGLE_NS, "g");

    let globex = tenant("globex");
    let _ = ingest(
        &globex,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[only_record]).into_bytes()),
        None,
    )
    .expect("setup ingest");

    // When Priya invokes `stats` with tenant `globex` and a captured
    // stdout sink.
    let mut stdout = Vec::<u8>::new();
    let count = stats(&globex, &data, &mut stdout).expect("stats");

    // Then the returned count is 1.
    assert_eq!(count, 1, "stats() returns 1 for single-record tenant");

    // And the captured stdout contains exactly 3 non-empty lines:
    // `records=1`, `earliest=2026-05-11T00:00:00.000000000Z`,
    // `latest=2026-05-11T00:00:00.000000000Z`. The earliest and latest
    // values are byte-identical (single-record degenerate time window).
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3, "exactly three non-empty lines on stdout");
    assert_eq!(lines[0], "records=1", "line 1 is `records=1`");
    assert_eq!(
        lines[1], EXPECTED_SINGLE_EARLIEST_LINE,
        "earliest= renders the seeded single-record timestamp"
    );
    assert_eq!(
        lines[2], EXPECTED_SINGLE_LATEST_LINE,
        "latest= renders the seeded single-record timestamp"
    );

    // And the timestamp values (after stripping `earliest=` / `latest=`)
    // are byte-identical — the degenerate-window invariant.
    let earliest_ts = lines[1]
        .strip_prefix("earliest=")
        .expect("earliest= prefix");
    let latest_ts = lines[2].strip_prefix("latest=").expect("latest= prefix");
    assert_eq!(
        earliest_ts, latest_ts,
        "single-record tenant: earliest and latest timestamps are byte-identical"
    );

    // And the stdout ends with `\n` (output-shape contract).
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK1 tenant isolation: stats for `acme` does not count
// `globex` records that live in the same data_dir.
//
// Per-tenant isolation is a `LogStore` trait invariant
// (`crates/lumen/src/store.rs:67-70`). `stats()` inherits it because
// it calls `lumen.query(tenant, TimeRange::all())` exactly once with
// the queried tenant.
// --------------------------------------------------------------------

#[test]
fn stats_for_acme_does_not_count_globex_records_in_same_data_dir() {
    // Given Priya has pre-ingested 7 records for tenant `acme` and,
    // separately, 3 records for tenant `globex` into the same data_dir.
    let root = temp_root("ok1_tenant_isolation");
    let data = root.join("data");

    let acme_records: Vec<LogRecord> = (0..7u64)
        .map(|i| record(SEED_EARLIEST_NS + i * SEEDED_STEP_NS, "a"))
        .collect();
    // globex's records use timestamps OUTSIDE acme's 24-hour window so
    // that a hypothetical bug that returned the union would surface as
    // a different earliest/latest, not just a different count.
    let globex_records: Vec<LogRecord> =
        (0..3u64).map(|i| record(SEED_SINGLE_NS + i, "b")).collect();

    let acme = tenant("acme");
    let globex = tenant("globex");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&acme_records).into_bytes()),
        None,
    )
    .expect("setup ingest acme");
    let _ = ingest(
        &globex,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&globex_records).into_bytes()),
        None,
    )
    .expect("setup ingest globex");

    // When Priya invokes `stats` with tenant `acme` and a captured
    // stdout sink.
    let mut stdout = Vec::<u8>::new();
    let count = stats(&acme, &data, &mut stdout).expect("stats");

    // Then the returned count is 7 (NOT 10 — globex's 3 records must
    // not be counted).
    assert_eq!(
        count, 7,
        "stats(acme, ...) counts ONLY acme's records, never globex's"
    );

    // And the `records=` line on stdout shows 7.
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3, "populated-acme stdout has three lines");
    assert_eq!(lines[0], "records=7", "records= shows 7, not 10");

    // And the earliest/latest lines reflect ONLY acme's window
    // (2026-05-18T00:00:00Z .. 2026-05-19T00:00:00Z), NOT the union
    // with globex's 2026-05-11 records.
    assert_eq!(
        lines[1], EXPECTED_EARLIEST_LINE,
        "earliest= reflects acme's window only"
    );
    assert_eq!(
        lines[2], EXPECTED_LATEST_LINE,
        "latest= reflects acme's window only"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — OK1 cross-function consistency: the `records=N` line from
// `stats()` matches the line count of `read()`'s NDJSON output for the
// same (tenant, data_dir).
//
// `read()` returns the count as its return value AND writes one NDJSON
// line per record. Both views of N must agree with `stats()`'s
// `records=` line.
// --------------------------------------------------------------------

#[test]
fn stats_count_matches_read_count_for_same_tenant_and_data_dir() {
    // Given Priya has pre-ingested N = 5 records for tenant `acme` into
    // a fresh data_dir.
    let root = temp_root("ok1_consistency_with_read");
    let data = root.join("data");
    let n: usize = 5;
    let records: Vec<LogRecord> = (0..n as u64).map(|i| record(100 + i, "c")).collect();

    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("setup ingest");

    // When Priya invokes `read` against a captured stdout sink, yielding
    // both a returned count and N NDJSON lines on stdout.
    let mut read_stdout = Vec::<u8>::new();
    let read_count = read(&acme, &data, &mut read_stdout, None).expect("read");

    // And Priya then invokes `stats` against a SEPARATE captured stdout
    // sink for the same tenant and data_dir.
    let mut stats_stdout = Vec::<u8>::new();
    let stats_count = stats(&acme, &data, &mut stats_stdout).expect("stats");

    // Then `stats()`'s returned count equals `read()`'s returned count.
    assert_eq!(
        stats_count, read_count,
        "stats() and read() return the same count for the same (tenant, data_dir)"
    );
    assert_eq!(stats_count, n, "both return the seeded N");

    // And `read()`'s stdout has exactly N non-empty NDJSON lines.
    let read_out = std::str::from_utf8(&read_stdout).expect("read stdout is UTF-8");
    let read_lines = read_out.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(
        read_lines, n,
        "read() stdout has one non-empty NDJSON line per record"
    );

    // And the `records=` line from `stats()`'s stdout shows the same N.
    let stats_out = std::str::from_utf8(&stats_stdout).expect("stats stdout is UTF-8");
    let stats_lines: Vec<&str> = stats_out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        stats_lines[0],
        format!("records={n}"),
        "stats() `records=` line equals N — the cross-function consistency invariant"
    );

    cleanup(&root);
}
