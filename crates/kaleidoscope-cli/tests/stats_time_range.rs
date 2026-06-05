// Kaleidoscope CLI — `stats --since` / `--until` time-range filter acceptance test
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

//! # Acceptance tests — `stats` time-range filter (`--since` / `--until`)
//!
//! Feature: `cli-stats-time-range-v0`.
//!
//! Extends `kaleidoscope-cli stats` with two new optional flags
//! `--since <ISO 8601 UTC>` and `--until <ISO 8601 UTC>` so the
//! library `stats_with_tiers()` function can drive any
//! `lumen::TimeRange::new(since_ns, until_ns)` query instead of the
//! hard-coded `TimeRange::all()` at
//! `crates/kaleidoscope-cli/src/lib.rs:359-361`.
//!
//! ## Mapping to outcome KPIs
//!
//! - **OK1 — bounded-window record count (principal / North Star)**:
//!   test `bounded_window_returns_only_records_in_half_open_interval`
//!   exercises the closed-lower / open-upper boundary contract
//!   inherited from `lumen::TimeRange::contains`
//!   (`crates/lumen/src/record.rs:116-119`) on the `records=` line.
//! - **OK2 — windowed earliest/latest (leading)**: same test asserts
//!   the `earliest=` / `latest=` lines reflect the windowed min/max,
//!   not the global min/max of the seeded data.
//! - **OK3 — Cinder lines unchanged across time ranges (guardrail —
//!   D-CinderScope)**: test
//!   `cinder_lines_are_byte_identical_across_different_time_ranges`
//!   invokes `stats_with_tiers` twice with two materially different
//!   `TimeRange` values against the same `(tenant, data_dir)` pair
//!   and asserts the Cinder lines (`hot=` / `warm=` / `cold=`) are
//!   byte-identical between the two captures while the Lumen lines
//!   (`records=` / `earliest=` / `latest=`) differ. This is the
//!   source-level encoding of D-CinderScope as an executable
//!   guardrail.
//! - **OK4 — no-flag byte equivalence (guardrail)**: test
//!   `no_flag_default_is_byte_equivalent_to_time_range_all` asserts
//!   the library-direct call with `TimeRange::all()` produces stdout
//!   bytes equal to what the locked
//!   `tests/stats_cinder_tier_distribution.rs` already asserts on
//!   the same `(tenant, data_dir)` shape. The locked OK4 oracle files
//!   continue to pass under DESIGN DD4's mechanical 4th-arg update.
//! - **OK1 / OK2 half-bounded support**: tests
//!   `since_only_uses_u64_max_upper_bound` and
//!   `until_only_uses_zero_lower_bound` exercise the implicit-
//!   unbounded-side semantics, mirroring the predecessor's
//!   `read_time_range.rs` shape.
//! - **OK1 / D-EmptyWindow via subprocess**: test
//!   `empty_window_via_subprocess_emits_records_zero_then_cinder_lines`
//!   spawns the `kaleidoscope-cli` binary (mirroring
//!   `cli_binary_smoke.rs` and `read_time_range.rs` tests 5-6) and
//!   asserts on stdout shape for the empty-window case, exit code 0,
//!   and the D-EmptyWindow contract: stdout starts with `records=0\n`
//!   (no `earliest=` / no `latest=`) followed by whatever Cinder
//!   snapshot lines are present.
//!
//! ## RED state at v0
//!
//! These tests pass `TimeRange::new(s, e)` (or `TimeRange::all()`) as
//! the fourth argument to `kaleidoscope_cli::stats_with_tiers`. The
//! shipped signature today is 3 parameters
//! (`tenant, data_dir, mut writer`); the new parameter
//! `range: TimeRange` is the DESIGN DD1 extension that the DELIVER
//! crafter will add. The file will not compile against the current
//! `lib.rs` — that compile failure IS the RED gate for outside-in
//! TDD. The subprocess test will also fail because the binary does
//! not yet parse `--since` / `--until` on the `stats` subcommand.
//!
//! ## Witness timestamps
//!
//! Per DWD-04 in `distill/wave-decisions.md`, the bounded-window
//! tests use easy literal nanos `{100, 200, 300, 400, 500}` so a
//! reviewer can verify boundary inclusion / exclusion by inspection
//! against `TimeRange::new(200, 400)`: records at `200` and `300`
//! are included, `400` is excluded (open upper), `100` is excluded
//! (below lower), `500` is excluded (above upper). The subprocess
//! test #6 uses the existing cluster constant
//! `SEED_EARLIEST_NS = 1_779_062_400_000_000_000` (2026-05-18T00:00:00Z)
//! so the chosen subprocess window of 2030..2031 is unambiguously
//! empty.
//!
//! ## Harness duplication
//!
//! The harness helpers (`tenant`, `record`, `temp_root`, `cleanup`,
//! `ndjson`, `cinder_base`, `seed_cinder`) are duplicated inline at
//! v0 per DISCUSS D-Test-file / DESIGN DD5 last row / DEVOPS
//! mutation-kill-rate protocol. This is the SEVENTH inline
//! duplication in the cluster; rule-of-three extraction to
//! `tests/common/mod.rs` is overdue but is a separate refactoring
//! task and is NOT a deliverable of this feature.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use kaleidoscope_cli::{ingest, stats_with_tiers, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber, TimeRange};

// --------------------------------------------------------------------
// Helpers (duplicated inline per DISCUSS D-Test-file / DESIGN DD5
// last row; mirrors stats_cinder_tier_distribution.rs harness shape).
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
    p.push(format!("kal-cli-stats-time-range-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

fn ndjson(records: &[LogRecord]) -> String {
    records
        .iter()
        .map(|r| serde_json::to_string(r).expect("serialise"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mirrors the private helper at `crates/kaleidoscope-cli/src/lib.rs`
/// (`cinder_base(data_dir)`). The system under test will reopen
/// Cinder against exactly this path, so seeding helpers MUST join the
/// same suffix.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Opens a `FileBackedTieringStore` against `cinder_base(data_dir)` and
/// calls `place` once per `(item_id, tier)` triple. Drops the store so
/// any in-memory WAL flush lands before the SUT reopens it. Mirrors the
/// helper in `tests/stats_cinder_tier_distribution.rs`.
fn seed_cinder(data_dir: &Path, tenant: &TenantId, hot: usize, warm: usize, cold: usize) {
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .expect("open cinder for seeding");
    let mut seq = 0usize;
    for _ in 0..hot {
        let item = ItemId::new(format!("{}/seed-{:05}", tenant.0, seq));
        cinder
            .place(tenant, &item, Tier::Hot, SystemTime::now())
            .expect("place");
        seq += 1;
    }
    for _ in 0..warm {
        let item = ItemId::new(format!("{}/seed-{:05}", tenant.0, seq));
        cinder
            .place(tenant, &item, Tier::Warm, SystemTime::now())
            .expect("place");
        seq += 1;
    }
    for _ in 0..cold {
        let item = ItemId::new(format!("{}/seed-{:05}", tenant.0, seq));
        cinder
            .place(tenant, &item, Tier::Cold, SystemTime::now())
            .expect("place");
        seq += 1;
    }
    drop(cinder);
}

/// Pre-ingest the given records into `data_dir` under tenant `acme`
/// using the unmodified `ingest` library function. Mirrors the helper
/// in `tests/read_time_range.rs`.
fn seed_lumen(data_dir: &Path, t: &TenantId, records: &[LogRecord]) {
    let _ = ingest(
        t,
        data_dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(records).into_bytes()),
        None,
    )
    .expect("seed ingest");
}

/// Locate the compiled `kaleidoscope-cli` binary (mirrors
/// `cli_binary_smoke.rs`). Used only by the subprocess test #6.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

// --------------------------------------------------------------------
// Cluster-wide deterministic seeds (mirror stats_cinder_tier_distribution.rs
// and stats_subcommand.rs for byte-shape consistency across the test
// cluster).
// --------------------------------------------------------------------

/// `2026-05-18T00:00:00Z` in nanoseconds since the Unix epoch — used
/// only by the subprocess test #6 to seed records in a known-real year
/// so the chosen empty window (2030..2031) is unambiguous.
const SEED_EARLIEST_NS_2026: u64 = 1_779_062_400_000_000_000;

// --------------------------------------------------------------------
// Test #1 — OK1 + OK2 bounded window: closed-lower, open-upper.
//
// Witness nanos {100, 200, 300, 400, 500} against
// TimeRange::new(200, 400). Records at 200 and 300 must be counted in
// `records=`; 400 (open upper boundary), 500 (above upper), 100 (below
// lower) must be excluded. `earliest=` is the ISO 8601 rendering of
// nano 200; `latest=` is the rendering of nano 300 — windowed, not
// global. Cinder seeded with 5/12/47 so the hot/warm/cold lines appear
// unchanged after the Lumen lines.
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
    let acme = tenant("acme");
    seed_lumen(&data, &acme, &[r100, r200, r300, r400, r500]);

    // And Cinder has been seeded with placements for `acme` such that
    // list_by_tier counts are Hot=5, Warm=12, Cold=47. `ingest()`
    // already placed one Hot item as a side effect (one Hot per
    // batch — single batch here since 5 <= DEFAULT_BATCH_SIZE); we
    // seed 4 more Hot + 12 Warm + 47 Cold to reach the scenario's
    // 5/12/47 distribution. The exact totals are not load-bearing on
    // OK1/OK2 — they ground the hot=/warm=/cold= lines so the Lumen
    // assertions sit in the middle of a six-line stdout.
    seed_cinder(&data, &acme, 4, 12, 47);

    // When Priya invokes `stats_with_tiers` with TimeRange::new(200,
    // 400) and a captured stdout sink.
    let mut stdout = Vec::<u8>::new();
    let count =
        stats_with_tiers(&acme, &data, &mut stdout, TimeRange::new(200, 400)).expect("stats");

    // Then the returned count equals 2 (only r200 and r300 match the
    // half-open interval [200, 400)).
    assert_eq!(count, 2, "exactly two records match [200, 400)");

    // And the captured stdout begins with the three Lumen lines
    // reflecting ONLY the windowed records (OK1 + OK2), followed by
    // the Cinder snapshot lines (state-snapshot, D-CinderScope —
    // independent of the time range).
    let out = std::str::from_utf8(&stdout).expect("stdout is UTF-8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        6,
        "exactly six non-empty lines on stdout (got: {out:?})"
    );
    assert_eq!(
        lines[0], "records=2",
        "OK1: records= reports windowed count"
    );
    assert_eq!(
        lines[1], "earliest=1970-01-01T00:00:00.000000200Z",
        "OK2: earliest= is the ISO of the smallest observed_time_unix_nano IN the window (200), \
         NOT the global minimum (100)"
    );
    assert_eq!(
        lines[2], "latest=1970-01-01T00:00:00.000000300Z",
        "OK2: latest= is the ISO of the largest observed_time_unix_nano IN the window (300), \
         NOT the global maximum (500); also confirms r400 EXCLUDED (open upper bound)"
    );
    assert_eq!(
        lines[3], "hot=5",
        "Cinder hot= line unchanged by the time range"
    );
    assert_eq!(
        lines[4], "warm=12",
        "Cinder warm= line unchanged by the time range"
    );
    assert_eq!(
        lines[5], "cold=47",
        "Cinder cold= line unchanged by the time range"
    );
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK3 D-CinderScope guardrail: Cinder lines are byte-
// identical across two materially different TimeRange invocations on
// the same (tenant, data_dir) pair, while the Lumen lines differ.
//
// Witness nanos {100, 200, 300, 400, 500}.
//   First call:  TimeRange::new(100, 200) → records=1 (only 100)
//   Second call: TimeRange::new(300, 500) → records=2 (300 and 400)
//
// The Cinder slice of each stdout (the `hot=` / `warm=` / `cold=` lines)
// must be byte-identical between the two captures. This is the
// source-level encoding of D-CinderScope as an executable guardrail:
// the time range NEVER applies to the Cinder loop.
// --------------------------------------------------------------------

#[test]
fn cinder_lines_are_byte_identical_across_different_time_ranges() {
    // Given Priya has pre-ingested 5 records for tenant `acme` and
    // seeded Cinder with non-zero placements in all three tiers.
    let root = temp_root("ok3_cinder_invariance");
    let data = root.join("data");
    let acme = tenant("acme");
    let r100 = record(100, "a");
    let r200 = record(200, "b");
    let r300 = record(300, "c");
    let r400 = record(400, "d");
    let r500 = record(500, "e");
    seed_lumen(&data, &acme, &[r100, r200, r300, r400, r500]);
    seed_cinder(&data, &acme, 2, 3, 4);

    // When Priya invokes `stats_with_tiers` twice in succession with
    // two materially different TimeRange values.
    let mut stdout_first = Vec::<u8>::new();
    let count_first =
        stats_with_tiers(&acme, &data, &mut stdout_first, TimeRange::new(100, 200)).expect("first");
    let mut stdout_second = Vec::<u8>::new();
    let count_second = stats_with_tiers(&acme, &data, &mut stdout_second, TimeRange::new(300, 500))
        .expect("second");

    // Then the returned Lumen counts differ between the two calls (the
    // time range DOES filter Lumen).
    assert_eq!(count_first, 1, "[100, 200) matches one record (r100)");
    assert_eq!(
        count_second, 2,
        "[300, 500) matches two records (r300, r400)"
    );

    // And the Lumen lines (records=, earliest=, latest=) DIFFER
    // byte-for-byte between the two captures.
    let out_first = std::str::from_utf8(&stdout_first).expect("utf8");
    let out_second = std::str::from_utf8(&stdout_second).expect("utf8");
    let lumen_lines = |out: &str| -> Vec<String> {
        out.lines()
            .filter(|l| {
                l.starts_with("records=") || l.starts_with("earliest=") || l.starts_with("latest=")
            })
            .map(|l| l.to_string())
            .collect()
    };
    assert_ne!(
        lumen_lines(out_first),
        lumen_lines(out_second),
        "Lumen lines MUST differ between the two captures — the time range DOES filter Lumen"
    );

    // And the Cinder lines (hot=, warm=, cold=) are byte-identical
    // between the two captures (D-CinderScope: the time range does NOT
    // apply to the Cinder snapshot).
    let cinder_lines = |out: &str| -> Vec<String> {
        out.lines()
            .filter(|l| l.starts_with("hot=") || l.starts_with("warm=") || l.starts_with("cold="))
            .map(|l| l.to_string())
            .collect()
    };
    assert_eq!(
        cinder_lines(out_first),
        cinder_lines(out_second),
        "OK3 / D-CinderScope: Cinder lines MUST be byte-identical across different TimeRange \
         invocations on the same (tenant, data_dir) pair"
    );
    // And specifically the seeded counts surface unchanged in both
    // captures.
    assert_eq!(
        cinder_lines(out_first),
        vec!["hot=3", "warm=3", "cold=4"],
        "Cinder counts reflect the seeded distribution (2 placed Hot + 1 from ingest = 3)"
    );
}

// --------------------------------------------------------------------
// Test #3 — OK4 no-flag library equivalence: TimeRange::all() reproduces
// the pre-feature stdout bytes and return count.
//
// We seed Lumen + Cinder, invoke stats_with_tiers with TimeRange::all(),
// and assert on the six-line stdout shape. The locked OK4-protection
// file `stats_cinder_tier_distribution.rs` (under DESIGN DD4's
// mechanical 4th-arg update) is the byte-level oracle for the same
// shape; this in-feature test exists to keep OK4 within the new
// acceptance file's scope and to give the DELIVER crafter a sharp
// witness for the no-flag default.
// --------------------------------------------------------------------

#[test]
fn no_flag_default_is_byte_equivalent_to_time_range_all() {
    // Given Priya has pre-ingested 5 records for tenant `acme` with
    // observed_time_unix_nano values {100, 200, 300, 400, 500} and
    // Cinder has been seeded with 5/12/47 placements.
    let root = temp_root("ok4_no_flag");
    let data = root.join("data");
    let acme = tenant("acme");
    let r100 = record(100, "a");
    let r200 = record(200, "b");
    let r300 = record(300, "c");
    let r400 = record(400, "d");
    let r500 = record(500, "e");
    seed_lumen(&data, &acme, &[r100, r200, r300, r400, r500]);
    seed_cinder(&data, &acme, 4, 12, 47);

    // When Priya invokes `stats_with_tiers` with TimeRange::all() (the
    // no-flag default per DESIGN DD1).
    let mut stdout = Vec::<u8>::new();
    let count =
        stats_with_tiers(&acme, &data, &mut stdout, TimeRange::all()).expect("stats no-flag");

    // Then the returned count equals 5 (all ingested records match).
    assert_eq!(count, 5, "TimeRange::all() matches every ingested record");

    // And the captured stdout reflects the GLOBAL min/max
    // (earliest=...100Z, latest=...500Z) — the byte-equivalence the
    // locked OK4-protection test file asserts under its mechanical
    // 4th-arg update.
    let out = std::str::from_utf8(&stdout).expect("utf8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 6, "exactly six non-empty lines on stdout");
    assert_eq!(lines[0], "records=5", "no-flag default counts every record");
    assert_eq!(
        lines[1], "earliest=1970-01-01T00:00:00.000000100Z",
        "earliest= reflects the GLOBAL minimum under TimeRange::all()"
    );
    assert_eq!(
        lines[2], "latest=1970-01-01T00:00:00.000000500Z",
        "latest= reflects the GLOBAL maximum under TimeRange::all()"
    );
    assert_eq!(lines[3], "hot=5");
    assert_eq!(lines[4], "warm=12");
    assert_eq!(lines[5], "cold=47");
    assert!(out.ends_with('\n'), "stdout must end with `\\n`");

    // And a second invocation against the same (tenant, data_dir) with
    // TimeRange::all() produces byte-identical stdout — proves the
    // function is deterministic on a fixed input substrate (the
    // library-direct property the locked OK4 oracles already pin via
    // their assertion text against `stats_with_tiers` under
    // TimeRange::all()).
    let mut stdout_again = Vec::<u8>::new();
    let count_again = stats_with_tiers(&acme, &data, &mut stdout_again, TimeRange::all())
        .expect("stats no-flag repeat");
    assert_eq!(count_again, 5, "repeat call returns the same count");
    assert_eq!(
        stdout, stdout_again,
        "two no-flag-default invocations on the same substrate produce byte-identical stdout"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — `--since`-only window: TimeRange::new(200, u64::MAX).
//
// Witness nanos {100, 200, 300}. Expected count=2 (records at 200 and
// 300; 100 below lower bound EXCLUDED). Mirrors the half-bounded
// semantic in `read_time_range.rs` test #3.
// --------------------------------------------------------------------

#[test]
fn since_only_uses_u64_max_upper_bound() {
    // Given Priya has pre-ingested 3 records with
    // observed_time_unix_nano values {100, 200, 300}.
    let root = temp_root("since_only");
    let data = root.join("data");
    let acme = tenant("acme");
    let r100 = record(100, "a");
    let r200 = record(200, "b");
    let r300 = record(300, "c");
    seed_lumen(&data, &acme, &[r100, r200, r300]);

    // When Priya invokes `stats_with_tiers` with TimeRange::new(200,
    // u64::MAX) — the shape the binary constructs when `--since`
    // names a value parsing to 200 nanos and `--until` is absent.
    let mut stdout = Vec::<u8>::new();
    let count = stats_with_tiers(&acme, &data, &mut stdout, TimeRange::new(200, u64::MAX))
        .expect("stats since-only");

    // Then the returned count equals 2 (records at 200 and 300;
    // 100 EXCLUDED below the closed lower bound).
    assert_eq!(count, 2, "since-only matches records from since_ns onwards");

    // And the records= line reports that windowed count, and the
    // earliest= / latest= lines reflect the windowed min/max.
    let out = std::str::from_utf8(&stdout).expect("utf8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines[0], "records=2",
        "records= reflects half-bounded count"
    );
    assert_eq!(
        lines[1], "earliest=1970-01-01T00:00:00.000000200Z",
        "earliest= reflects the smallest observed_time_unix_nano in [200, u64::MAX)"
    );
    assert_eq!(
        lines[2], "latest=1970-01-01T00:00:00.000000300Z",
        "latest= reflects the largest observed_time_unix_nano in [200, u64::MAX)"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — `--until`-only window: TimeRange::new(0, 200).
//
// Witness nanos {100, 200, 300}. Expected count=1 (only the record at
// 100; 200 is the open upper bound EXCLUDED; 300 above upper EXCLUDED).
// Mirrors the half-bounded semantic in `read_time_range.rs` test #4.
// --------------------------------------------------------------------

#[test]
fn until_only_uses_zero_lower_bound() {
    // Given Priya has pre-ingested 3 records with
    // observed_time_unix_nano values {100, 200, 300}.
    let root = temp_root("until_only");
    let data = root.join("data");
    let acme = tenant("acme");
    let r100 = record(100, "a");
    let r200 = record(200, "b");
    let r300 = record(300, "c");
    seed_lumen(&data, &acme, &[r100, r200, r300]);

    // When Priya invokes `stats_with_tiers` with TimeRange::new(0,
    // 200) — the shape the binary constructs when `--until` names a
    // value parsing to 200 nanos and `--since` is absent.
    let mut stdout = Vec::<u8>::new();
    let count = stats_with_tiers(&acme, &data, &mut stdout, TimeRange::new(0, 200))
        .expect("stats until-only");

    // Then the returned count equals 1 (only the record at 100;
    // 200 EXCLUDED at the open upper bound).
    assert_eq!(
        count, 1,
        "until-only matches records strictly before until_ns"
    );

    // And the Lumen lines reflect the windowed single record (its
    // earliest equals its latest at nano 100).
    let out = std::str::from_utf8(&stdout).expect("utf8");
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines[0], "records=1",
        "records= reflects half-bounded count"
    );
    assert_eq!(
        lines[1], "earliest=1970-01-01T00:00:00.000000100Z",
        "earliest= reflects the only matching record at nano 100"
    );
    assert_eq!(
        lines[2], "latest=1970-01-01T00:00:00.000000100Z",
        "latest= reflects the only matching record at nano 100"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #6 — OK1 + D-EmptyWindow via subprocess: empty window at the
// binary boundary.
//
// Spawn CARGO_BIN_EXE_kaleidoscope-cli with argv list
// ["stats", "acme", "<data_dir>", "--since", "2030-01-01T00:00:00Z",
//  "--until", "2031-01-01T00:00:00Z"] against a data_dir populated
// with records seeded at 2026-05-18T00:00:00Z. The chosen window is
// entirely after every ingested record, so the bounded query result
// is empty.
//
// Assertions:
//   - exit code 0 (an empty window is a valid query result, not an
//     error — D-EmptyWindow / DISCUSS US-01 Scenario #3)
//   - stdout starts with `records=0\n` — no `earliest=` line, no
//     `latest=` line (D-EmptyWindow contract)
//   - stdout then contains the Cinder snapshot lines for whichever
//     tiers have non-zero per-tenant placements (D-CinderScope: the
//     Cinder lines are state-snapshot, independent of the time range).
//     ingest() places one Hot item per batch as a side effect; a
//     single batch was ingested, so exactly `hot=1` follows.
//
// Subprocess (not library-direct) because OK1 + D-EmptyWindow at the
// binary boundary exercises argv parsing of `--since` / `--until`
// AND the empty-window stdout shape AND the exit code in one probe —
// the same shape `read_time_range.rs` tests 5-6 use for the read
// feature's binary-boundary contract.
// --------------------------------------------------------------------

#[test]
fn empty_window_via_subprocess_emits_records_zero_then_cinder_lines() {
    // Given Priya has pre-ingested 3 records for tenant `acme` with
    // observed_time_unix_nano values seeded at 2026-05-18T00:00:00Z
    // (and the next two nanos) — entirely BEFORE the chosen 2030..2031
    // window. The Lumen-side bounded query result is therefore empty.
    let root = temp_root("empty_window_subprocess");
    let data = root.join("data");
    let acme = tenant("acme");
    let r0 = record(SEED_EARLIEST_NS_2026, "a");
    let r1 = record(SEED_EARLIEST_NS_2026 + 1, "b");
    let r2 = record(SEED_EARLIEST_NS_2026 + 2, "c");
    seed_lumen(&data, &acme, &[r0, r1, r2]);

    // When Priya invokes the binary with --since 2030-01-01T00:00:00Z
    // and --until 2031-01-01T00:00:00Z.
    let output = Command::new(bin())
        .arg("stats")
        .arg("acme")
        .arg(&data)
        .arg("--since")
        .arg("2030-01-01T00:00:00Z")
        .arg("--until")
        .arg("2031-01-01T00:00:00Z")
        .stdin(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-cli stats with empty window");

    // Then the process exits with code 0 (D-EmptyWindow: an empty
    // window is a valid query result, not an error).
    assert!(
        output.status.success(),
        "empty window must exit 0; got status {:?}, stderr={:?}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // And stdout starts with `records=0\n` (D-EmptyWindow contract:
    // exactly one Lumen line, no earliest=, no latest=).
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.starts_with("records=0\n"),
        "stdout must start with `records=0\\n` (D-EmptyWindow); got {stdout:?}"
    );

    // And stdout contains NO earliest= and NO latest= lines (the
    // (Some, Some) arm in stats_with_tiers fires only when the
    // windowed record vec is non-empty).
    assert!(
        !stdout.contains("earliest="),
        "no earliest= line for empty window (D-EmptyWindow); got {stdout:?}"
    );
    assert!(
        !stdout.contains("latest="),
        "no latest= line for empty window (D-EmptyWindow); got {stdout:?}"
    );

    // And after `records=0\n` the stdout continues with the Cinder
    // snapshot lines — `ingest()` places one Hot item per batch as a
    // side effect (lib.rs:243-244), so exactly `hot=1` is present
    // (Warm and Cold remain at zero so per Option B they emit no
    // lines). This proves D-CinderScope at the binary boundary: the
    // time range does not silence the Cinder lines.
    let after_records = &stdout["records=0\n".len()..];
    assert_eq!(
        after_records, "hot=1\n",
        "after the records=0 line the Cinder snapshot lines follow unchanged (D-CinderScope); \
         got: {stdout:?}"
    );

    cleanup(&root);
}
