// Kaleidoscope CLI — `read --observe-otlp` flag acceptance test
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

//! # Acceptance tests — `read --observe-otlp` flag wiring
//!
//! When the operator passes `--observe-otlp <path>` to `read`, the
//! Lumen recorder inside `read()` is replaced by
//! `LumenToOtlpJsonWriter`. The file at that path receives exactly one
//! `lumen.query.count` OTLP-JSON line per `read()` invocation (because
//! `read()` calls `lumen.query(tenant, TimeRange::all())` exactly once,
//! per `crates/kaleidoscope-cli/src/lib.rs:258-260`).
//!
//! These tests drive the user-visible outcome of feature
//! `cli-read-observe-otlp-v0`:
//!
//! - **US-01 / OK1 (principal)**: one `lumen.query.count` line per
//!   `read()` invocation with the correct tenant, scope, and `asInt`
//!   (test #1 — happy path).
//! - **US-01 / OK2 (guardrail)**: no-flag non-regression. When
//!   `otlp_log_path = None`, no file is created and the returned count
//!   plus stdout bytes match the previously-ingested records exactly
//!   (test #2 — mirror of `no_observe_otlp_means_no_otlp_file_created`
//!   from `observe_otlp_flag.rs`).
//! - **US-01 / OK3 (leading)**: cross-subcommand symmetry. A single
//!   shell session that runs `ingest --observe-otlp <path>` then
//!   `read --observe-otlp <path>` against the same path produces a
//!   file whose metric-name set contains all three contracted names:
//!   `lumen.ingest.count`, `cinder.place.count`, `lumen.query.count`
//!   (test #3).
//!
//! Note on metric names: the wire-format Lumen query metric name
//! produced by `LumenToOtlpJsonWriter::record_query` is
//! `lumen.query.count` (see
//! `crates/self-observe/src/lumen_otlp_json.rs:205-207`). The scope
//! name is `kaleidoscope.lumen`. Drift between these tests' assertions
//! and the writer is a review failure (DISCUSS US-01 § System
//! Constraints).
//!
//! Note on Send+Sync witness: the compile-time witness for
//! `LumenToOtlpJsonWriter<File>: Send + Sync` is already discharged
//! by `tests/observe_otlp_cinder_wiring.rs::
//! cinder_writer_over_real_file_is_send_and_sync`. No re-probe here.
//!
//! Note on RED state at v0: these tests pass `Some(&path)` as the
//! fourth argument to `kaleidoscope_cli::read`. The shipped signature
//! today is 3 parameters (`tenant, data_dir, mut writer`); the new
//! parameter `otlp_log_path: Option<&Path>` is the DESIGN DD3
//! extension that the DELIVER crafter will add. The file will not
//! compile against the current `lib.rs` — that compile failure IS the
//! RED gate for outside-in TDD.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, read, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber};
use serde_json::Value;

// --------------------------------------------------------------------
// Helpers (mirror observe_otlp_flag.rs + observe_otlp_cinder_wiring.rs;
// rule-of-three deferral — extraction to tests/common.rs becomes
// warranted with this third test file, per DISCUSS D6 / DESIGN DD4
// last row, but is deferred to a follow-up refactor).
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
    p.push(format!("kal-cli-otlp-read-{name}-{pid}-{nanos}"));
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

/// Read the file, split on `\n`, return all non-empty lines parsed
/// as `serde_json::Value`. Used by every test below for the
/// per-line-JSON-validity check.
fn parse_ndjson_lines(path: &std::path::Path) -> Vec<Value> {
    let content = fs::read_to_string(path).expect("read otlp file");
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("each non-empty line parses as JSON"))
        .collect()
}

/// True if this OTLP-JSON line carries the given metric name in the
/// locked single-metric position `scopeMetrics[0].metrics[0].name`
/// (ADR-0039 §2).
fn line_has_metric(v: &Value, name: &str) -> bool {
    v["scopeMetrics"][0]["metrics"][0]["name"] == name
}

// --------------------------------------------------------------------
// Test #1 — OK1 happy path: `read` with `--observe-otlp` emits exactly
// one `lumen.query.count` line per invocation.
//
// Setup pre-ingests 6 records via an `ingest` call WITHOUT
// `--observe-otlp` (so the OTLP file remains clean of any ingest-side
// lines, isolating the assertion to the read-side wiring under test).
// --------------------------------------------------------------------

#[test]
fn read_with_observe_otlp_emits_one_lumen_query_count_line() {
    // Given Priya has pre-ingested 6 records for tenant `acme` into a
    // fresh data dir, with no `--observe-otlp` flag on the setup call.
    let root = temp_root("ok1_happy_path");
    let data = root.join("data");
    let otlp = root.join("otlp.ndjson");
    let records: Vec<LogRecord> = (0..6u64).map(|i| record(i, "x")).collect();

    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None, // setup ingest does NOT populate the OTLP file
    )
    .expect("setup ingest");

    // And the OTLP file does not exist yet (the setup call passed
    // `None`, so no side-channel file was created — guardrail for the
    // OK1 assertion below that the read call alone produces the 1
    // line).
    assert!(
        !otlp.exists(),
        "setup precondition: OTLP file must not exist before the read call"
    );

    // When Priya invokes `read` with
    // `otlp_log_path = Some(&otlp)` and the same tenant + data_dir.
    let mut stdout = Vec::<u8>::new();
    let count = read(&acme, &data, &mut stdout, Some(&otlp)).expect("read");

    // Then the returned `count` equals the number of pre-ingested
    // records (sanity: query matched everything under
    // `TimeRange::all()`).
    assert_eq!(count, 6, "read() returns matched record count");

    // And exactly 1 non-empty line exists in the OTLP file (one
    // `record_query` event per `read()` invocation, per DISCUSS
    // US-01 § Domain Examples #1).
    let lines = parse_ndjson_lines(&otlp);
    assert_eq!(
        lines.len(),
        1,
        "one read() call → one lumen.query.count line"
    );

    // And that line carries the contracted metric name, scope, tenant
    // resource attribute, and `asInt` count.
    let v = &lines[0];
    assert!(
        line_has_metric(v, "lumen.query.count"),
        "metric name on the wire is `lumen.query.count` (lumen_otlp_json.rs:205-207)"
    );
    assert_eq!(
        v["scopeMetrics"][0]["scope"]["name"], "kaleidoscope.lumen",
        "scope name is `kaleidoscope.lumen` (DISCUSS US-01 scope contract)"
    );
    assert_eq!(
        v["resource"]["attributes"][0]["value"]["stringValue"], "acme",
        "resource tenant_id is the read() tenant"
    );
    let dp = &v["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    assert_eq!(
        dp["asInt"], "6",
        "asInt equals matched record count (6 records under TimeRange::all())"
    );

    // And the file ends with `\n` (per-emission newline invariant from
    // the writer's locked single-write_all-per-event pattern).
    let raw = fs::read_to_string(&otlp).expect("read otlp file as bytes");
    assert!(
        raw.ends_with('\n'),
        "OTLP file must end with `\\n` (writer invariant)"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK2 no-flag non-regression: `read` without
// `--observe-otlp` creates no file and preserves existing stdout +
// return-value behaviour byte-equivalently.
//
// Mirrors `no_observe_otlp_means_no_otlp_file_created` from
// observe_otlp_flag.rs (ingest-side equivalent), re-asserted from the
// read-side test surface to prove the new match-arm wiring does not
// accidentally create a file when the flag is absent.
// --------------------------------------------------------------------

#[test]
fn read_without_observe_otlp_creates_no_file_and_preserves_stdout() {
    // Given Priya has pre-ingested 4 records for tenant `acme` into a
    // fresh data dir, with no OTLP flag.
    let root = temp_root("ok2_no_flag");
    let data = root.join("data");
    let otlp_would_be = root.join("otlp.ndjson");
    let records: Vec<LogRecord> = (10..14u64).map(|i| record(i, "y")).collect();
    let expected_ndjson = ndjson(&records);

    let acme = tenant("acme");
    let _ = ingest(
        &acme,
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(expected_ndjson.clone().into_bytes()),
        None,
    )
    .expect("setup ingest");

    // When Priya invokes `read` with `otlp_log_path = None` against a
    // captured stdout sink.
    let mut stdout = Vec::<u8>::new();
    let count = read(&acme, &data, &mut stdout, None).expect("read");

    // Then the returned `count` equals N (sanity: byte-equivalent to
    // pre-feature `read` behaviour).
    assert_eq!(
        count, 4,
        "read() returns the same matched record count as before"
    );

    // And the captured stdout bytes equal the pre-ingested records
    // re-serialised as NDJSON, one per line, terminated by `\n`.
    // (Lumen storage round-trips `LogRecord` by serde value equality;
    // this is the existing stdout contract.)
    let mut expected_stdout = expected_ndjson.into_bytes();
    expected_stdout.push(b'\n');
    assert_eq!(
        stdout, expected_stdout,
        "no-flag stdout bytes are byte-equivalent to pre-feature behaviour"
    );

    // And no file is created at the path the test would have specified
    // for the flag-set case (OK2 zero-side-channel invariant).
    assert!(
        !otlp_would_be.exists(),
        "OTLP file must not be created when --observe-otlp is absent"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK3 cross-subcommand symmetry: `ingest --observe-otlp
// <path>` followed by `read --observe-otlp <path>` against the same
// path in the same process leaves a file whose metric-name SET
// contains all three contracted names.
//
// Set-containment (rather than exact-count multiset) is the chosen
// assertion shape: it proves the cross-subcommand wiring contract
// without coupling the test to per-batch-flush emission cadence
// (which is already separately probed by `observe_otlp_flag.rs` and
// `observe_otlp_cinder_wiring.rs` on the ingest side, and by test #1
// above on the read side). Coupling to exact counts here would
// duplicate those probes without adding new signal and would brittlely
// break under any future per-batch flush refactor.
//
// Per-line JSON validity and trailing `\n` are asserted as the
// substrate invariants the operator's sidecar depends on.
// --------------------------------------------------------------------

#[test]
fn ingest_then_read_share_one_observe_otlp_file_in_one_session() {
    // Given Priya invokes `ingest --observe-otlp <path>` with 6
    // records, batch_size 3 (→ 2 batch flushes; ingest emits both
    // `lumen.ingest.count` and `cinder.place.count` lines per flush).
    let root = temp_root("ok3_symmetry");
    let data = root.join("data");
    let otlp = root.join("otlp.ndjson");
    let records: Vec<LogRecord> = (0..6u64).map(|i| record(i, "z")).collect();

    let acme = tenant("acme");
    let stats = ingest(
        &acme,
        &data,
        3,
        Cursor::new(ndjson(&records).into_bytes()),
        Some(&otlp),
    )
    .expect("ingest with --observe-otlp");
    assert_eq!(
        stats.batches_flushed, 2,
        "setup sanity: 6 records / batch_size 3 = 2 flushes"
    );

    // When Priya then invokes `read --observe-otlp <path>` against the
    // same tenant, same data_dir, and SAME otlp path. The append-mode
    // `OpenOptions` on both sides ensures the new line lands AFTER the
    // existing ingest-side content without truncation (DESIGN DD1
    // rationale 2).
    let mut stdout = Vec::<u8>::new();
    let count = read(&acme, &data, &mut stdout, Some(&otlp)).expect("read");
    assert_eq!(count, 6, "read() returns matched record count");

    // Then the file's metric-name SET contains all three contracted
    // names — full Lumen + Cinder lifecycle visible on one sidecar
    // configuration from one sequential shell session (OK3).
    let lines = parse_ndjson_lines(&otlp);
    let metric_names: BTreeSet<String> = lines
        .iter()
        .map(|v| {
            v["scopeMetrics"][0]["metrics"][0]["name"]
                .as_str()
                .expect("metric name is a string")
                .to_string()
        })
        .collect();

    assert!(
        metric_names.contains("lumen.ingest.count"),
        "metric-name set must contain `lumen.ingest.count` from the ingest call"
    );
    assert!(
        metric_names.contains("cinder.place.count"),
        "metric-name set must contain `cinder.place.count` from the ingest call (cli-cinder-otlp-wiring-v0)"
    );
    assert!(
        metric_names.contains("lumen.query.count"),
        "metric-name set must contain `lumen.query.count` from the read call (this feature)"
    );

    // And every non-empty line parses as `serde_json::Value` (the
    // `parse_ndjson_lines` helper above already asserts this on
    // parse). And the file ends with `\n` (per-emission newline
    // invariant; OK3 explicitly requires this for sidecar
    // line-by-line tail safety).
    let raw = fs::read_to_string(&otlp).expect("read otlp file as bytes");
    assert!(
        raw.ends_with('\n'),
        "OTLP file must end with `\\n` after ingest + read sequence"
    );

    // And no blank lines between OTLP records (sidecar invariant
    // inherited from OK6 of cli-cinder-otlp-wiring-v0).
    let raw_lines: Vec<&str> = raw.lines().collect();
    let non_empty: Vec<&&str> = raw_lines.iter().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        non_empty.len(),
        raw_lines.len(),
        "no blank lines between OTLP records across ingest + read"
    );

    cleanup(&root);
}
