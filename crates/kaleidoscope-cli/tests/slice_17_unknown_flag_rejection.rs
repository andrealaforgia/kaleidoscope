// Kaleidoscope CLI — unknown-flag rejection acceptance tests
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

//! # Acceptance tests — unknown-flag rejection (feature `cli-unknown-flag-rejection-v0`)
//!
//! These subprocess tests ARE the fresh anchor the EDD verifier
//! re-verifies defect K11 against (the previous anchor was dropped in
//! revert e3a8cad). They spawn the real binary via
//! `CARGO_BIN_EXE_kaleidoscope-cli` and assert on the observable
//! contract: exit code plus stderr, mirroring the harness shape already
//! used in `read_time_range.rs` (tests #5/#6) and `cli_binary_smoke.rs`.
//! Inline harness helpers per DISCUSS D7 (no shared `tests/common`).
//!
//! ## Driving port
//!
//! The driving port for every scenario here is the `kaleidoscope-cli`
//! binary's argv entry point (`fn main` in `src/main.rs`). The operator
//! invokes the binary from a shell; the observable outcome is the exit
//! code and the usage error on stderr. No library function is called
//! directly except `ingest`, used purely as a setup helper to seed a
//! readable data directory (mirroring `read_time_range.rs::seed`).
//!
//! ## AC -> user-story mapping
//!
//! | Test | US    | Status today |
//! |------|-------|--------------|
//! | AC-01 top-level unknown flag rejected     | US-01 | GREEN (already correct) |
//! | AC-02 subcommand unknown flag rejected    | US-02 | RED   (the silent-accept gap) |
//! | AC-03 unknown subcommand verb rejected    | US-03 | GREEN (already correct) |
//! | AC-04 valid subcommand flag NOT rejected  | US-04 | GREEN (regression guard) |
//!
//! ## Mandate 7: RED-not-BROKEN
//!
//! AC-02 is RED, not BROKEN. The binary compiles and runs; the test
//! fails on an assertion (it observes exit 0 / a non-2 exit where it
//! expects exit 2), because the subcommand flag scanners silently skip
//! `--bogus` today. There is no scaffold to write: the production change
//! is Crafty's shared `reject_unknown_flags` helper in `src/main.rs`. The
//! other three tests are GREEN against the shipped binary and pin the
//! contract that must not regress.
//!
//! ## AC-02 pre-commit safety
//!
//! AC-02 is `#[ignore]`d so the deterministic pre-commit hook stays green
//! while these tests are committed ahead of the fix (the DELIVER change is
//! atomic via Crafty). Crafty de-ignores AC-02 in DELIVER once
//! `reject_unknown_flags` lands, at which point it turns GREEN. Run it
//! locally before then with:
//!   `cargo test -p kaleidoscope-cli --test slice_17_unknown_flag_rejection -- --ignored`
//! Today that run is RED (observed exit 0), which is the correct
//! outside-in RED gate.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber};

// --------------------------------------------------------------------
// Helpers (duplicated inline per DISCUSS D7; no shared `tests/common`
// extraction in this feature).
// --------------------------------------------------------------------

/// Locate the compiled `kaleidoscope-cli` binary (mirrors
/// `cli_binary_smoke.rs` / `read_time_range.rs`).
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

fn temp_root(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("kal-cli-unknown-flag-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir temp_root");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, body: &str) -> LogRecord {
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: BTreeMap::new(),
        trace_id: None,
        span_id: None,
    }
}

fn ndjson(records: &[LogRecord]) -> String {
    records
        .iter()
        .map(|r| serde_json::to_string(r).expect("serialise"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Seed one record for tenant `acme` into `data_dir` using the
/// unmodified `ingest` library function (pure setup helper, mirroring
/// `read_time_range.rs::seed`). This makes a subsequent `read` reach a
/// real, openable Lumen store so that AC-02 observes the genuine
/// silent-accept exit 0 today (rather than masking it behind an I/O
/// error from a missing data directory).
fn seed_one_record(data_dir: &Path) {
    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        data_dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(1, "hi")]).into_bytes()),
        None,
    )
    .expect("seed ingest");
}

// --------------------------------------------------------------------
// AC-01 (US-01, re-anchor, GREEN today)
//
// A `--`-prefixed token in the first argument position that is not
// `--help` / `-h` is rejected by the existing top-level
// unknown-subcommand arm (main.rs:70-74): exit 2 plus a usage error on
// stderr naming the verbatim token. This re-anchors US-01's contract.
// --------------------------------------------------------------------

#[test]
fn ac01_top_level_unknown_flag_is_rejected_with_exit_2_and_usage() {
    // When the operator runs `kaleidoscope-cli --bogus`.
    let output = Command::new(bin())
        .arg("--bogus")
        .stdin(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-cli --bogus");

    // Then the process exits with code 2.
    assert_eq!(
        output.status.code(),
        Some(2),
        "top-level unknown flag must exit 2; got status {:?}",
        output.status
    );

    // And stderr carries a usage error that names the unknown token and
    // includes the usage block (the operator is told what they typed and
    // shown the valid invocations).
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("--bogus"),
        "stderr names the unknown token `--bogus`: {stderr:?}"
    );
    assert!(
        stderr.contains("kaleidoscope-cli ingest") && stderr.contains("kaleidoscope-cli read"),
        "stderr includes the usage block listing subcommands: {stderr:?}"
    );
}

// --------------------------------------------------------------------
// AC-02 (US-02, THE GAP, RED today — `#[ignore]` for pre-commit safety)
//
// `read acme <seeded_data_dir> --bogus` must reject `--bogus` with exit 2
// plus a usage error naming the token, and produce NO records on stdout.
//
// The data directory is seeded with one record so `read` reaches a real,
// openable Lumen store: today the subcommand flag scanners silently skip
// `--bogus` and `read` succeeds with exit 0 (the silent-accept gap). The
// unknown-flag rejection is intended to run during the argv parse, BEFORE
// any store is opened (the fail-before-store-open invariant the OK4 tests
// in `read_time_range.rs` already assert), so after the fix this exits 2
// with empty stdout even though the data dir is readable.
//
// RED today: observed exit 0 (records printed) instead of exit 2.
// Crafty de-ignores this test in DELIVER once `reject_unknown_flags`
// lands; it then turns GREEN.
// --------------------------------------------------------------------

#[test]
fn ac02_subcommand_unknown_flag_is_rejected_before_any_records_are_read() {
    let root = temp_root("ac02_subcommand_unknown_flag");
    let data = root.join("data");

    // Given a data directory with one ingested record for tenant `acme`,
    // so `read` can reach a real store (the gap is only observable when
    // the command would otherwise succeed).
    seed_one_record(&data);

    // When the operator runs `read acme <data_dir> --bogus`.
    let output = Command::new(bin())
        .arg("read")
        .arg("acme")
        .arg(&data)
        .arg("--bogus")
        .stdin(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-cli read with --bogus");

    // Then the process exits with code 2 (the same code the top-level
    // unknown-subcommand path uses, per DESIGN DD2).
    assert_eq!(
        output.status.code(),
        Some(2),
        "subcommand unknown flag must exit 2; got status {:?}",
        output.status
    );

    // And stderr names the unknown flag with the pinned wording
    // `unknown flag "--bogus"` (DESIGN DD3) plus the usage block.
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("unknown flag \"--bogus\""),
        "stderr names the unknown flag with the pinned wording: {stderr:?}"
    );
    assert!(
        stderr.contains("kaleidoscope-cli read"),
        "stderr includes the usage block: {stderr:?}"
    );

    // And no records are written to stdout (rejection happens before the
    // read runs).
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty when an unknown flag is rejected; got {} bytes",
        output.stdout.len()
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// AC-03 (US-03, re-anchor, GREEN today)
//
// A first-position token that is neither a known subcommand verb nor a
// help flag is rejected by the existing top-level arm: exit 2 plus a
// usage error naming the verbatim verb. Re-anchors US-03's contract.
// --------------------------------------------------------------------

#[test]
fn ac03_unknown_subcommand_verb_is_rejected_with_exit_2_and_usage() {
    // When the operator runs `kaleidoscope-cli bogus-subcommand`.
    let output = Command::new(bin())
        .arg("bogus-subcommand")
        .stdin(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-cli bogus-subcommand");

    // Then the process exits with code 2.
    assert_eq!(
        output.status.code(),
        Some(2),
        "unknown subcommand verb must exit 2; got status {:?}",
        output.status
    );

    // And stderr names the unknown verb and includes the usage block.
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("bogus-subcommand"),
        "stderr names the unknown subcommand verb: {stderr:?}"
    );
    assert!(
        stderr.contains("kaleidoscope-cli ingest") && stderr.contains("kaleidoscope-cli read"),
        "stderr includes the usage block listing subcommands: {stderr:?}"
    );
}

// --------------------------------------------------------------------
// AC-04 (US-04, regression guard, GREEN today)
//
// A VALID subcommand flag is NOT treated as unknown: `read acme
// <seeded_data_dir> --observe-otlp <path>` succeeds (exit 0, records on
// stdout, metric line appended). This proves the unknown-flag rejection
// is additive — a known value-taking flag and its value are consumed,
// never re-classified as an unknown flag.
//
// Chosen over a `--help`-style case because it positively exercises the
// value-taking-flag path (`--observe-otlp <path>`): the value token
// `<path>` must be consumed by the known flag, not mistaken for a
// positional or rejected. This is exactly the consumed-value rule
// (DESIGN DD-rule clause 1) that the future helper must honour, so the
// regression guard pins the load-bearing additive case rather than a
// no-flag invocation that exercises nothing new.
// --------------------------------------------------------------------

#[test]
fn ac04_valid_subcommand_flag_is_not_rejected_as_unknown() {
    let root = temp_root("ac04_valid_flag");
    let data = root.join("data");
    let metric_path = root.join("metrics.ndjson");

    // Given a data directory with one ingested record for tenant `acme`.
    seed_one_record(&data);

    // When the operator runs `read acme <data_dir> --observe-otlp <path>`
    // with a valid, value-taking known flag.
    let output = Command::new(bin())
        .arg("read")
        .arg("acme")
        .arg(&data)
        .arg("--observe-otlp")
        .arg(&metric_path)
        .stdin(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-cli read with --observe-otlp");

    // Then the process exits 0 (the valid flag is honoured, not rejected).
    assert_eq!(
        output.status.code(),
        Some(0),
        "valid known flag must not be rejected; got status {:?} stderr {:?}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // And the seeded record is written to stdout.
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("\"body\":\"hi\""),
        "stdout contains the seeded record: {stdout:?}"
    );

    // And the summary line confirms one record was read.
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("read ok: records=1"),
        "stderr summary line present: {stderr:?}"
    );

    // And the metric file received a query metric line (the value token
    // was consumed by `--observe-otlp`, proving the flag was honoured).
    let metric = fs::read_to_string(&metric_path).expect("read metric file");
    assert!(
        metric.contains("lumen.query.count"),
        "metric file contains the query metric line: {metric:?}"
    );

    cleanup(&root);
}
