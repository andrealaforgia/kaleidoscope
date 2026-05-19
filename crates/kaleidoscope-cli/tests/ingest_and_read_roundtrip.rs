// Kaleidoscope CLI — ingest + read roundtrip
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

//! Operator-flow acceptance test.
//!
//! Pipes NDJSON `LogRecord` lines through `ingest`, then runs
//! `read` against the same `data_dir`, then asserts the records
//! come back. Mirrors the real shell pipeline an operator
//! would write:
//!
//! ```text
//! cat input.ndjson | kaleidoscope-cli ingest acme ./data
//! kaleidoscope-cli read acme ./data | jq ...
//! ```

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, read, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber, TimeRange};

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

#[test]
fn ingest_then_read_round_trips_records_byte_stable() {
    let dir = temp_data_dir("roundtrip");
    let tn = tenant("acme");
    let records = vec![
        record(100, "first"),
        record(200, "second"),
        record(300, "third"),
    ];

    // Phase 1: ingest stdin → store.
    let input = ndjson(&records);
    let stats = ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(input.as_bytes()),
        None,
    )
    .expect("ingest");
    assert_eq!(stats.records_ingested, 3);
    assert_eq!(stats.batches_flushed, 1);
    assert_eq!(stats.tier_items_placed, 1);

    // Phase 2: read store → stdout. We capture stdout in a Vec.
    let mut buf: Vec<u8> = Vec::new();
    let count = read(&tn, &dir, &mut buf, None, TimeRange::all()).expect("read");
    assert_eq!(count, 3);

    // Parse the output back as NDJSON LogRecord and assert
    // byte-stable equality with the input.
    let output = String::from_utf8(buf).expect("utf8");
    let parsed: Vec<LogRecord> = output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse"))
        .collect();
    assert_eq!(parsed, records);
    cleanup(&dir);
}

#[test]
fn ingest_survives_a_simulated_restart_via_separate_read_call() {
    // The whole point of the CLI is that data persists between
    // invocations. This test simulates that by calling ingest
    // and read as separate functions that re-open the
    // file-backed adapters from the same data_dir.
    let dir = temp_data_dir("restart");
    let tn = tenant("acme");
    let records: Vec<LogRecord> = (0..250u64).map(|i| record(i, "body")).collect();

    let stats = ingest(
        &tn,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("ingest");
    // 250 records / 100 batch_size = 3 batches (100 + 100 + 50).
    assert_eq!(stats.records_ingested, 250);
    assert_eq!(stats.batches_flushed, 3);
    assert_eq!(stats.tier_items_placed, 3);

    // Separate read call — opens the file-backed adapters
    // afresh.
    let mut buf: Vec<u8> = Vec::new();
    let count = read(&tn, &dir, &mut buf, None, TimeRange::all()).expect("read");
    assert_eq!(count, 250);
    cleanup(&dir);
}

#[test]
fn two_tenants_data_is_isolated_in_the_same_data_dir() {
    let dir = temp_data_dir("tenants");
    let acme = tenant("acme");
    let globex = tenant("globex");
    let acme_records = vec![record(100, "a1"), record(200, "a2")];
    let globex_records = vec![record(150, "g1")];

    ingest(
        &acme,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&acme_records).into_bytes()),
        None,
    )
    .expect("acme ingest");
    ingest(
        &globex,
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&globex_records).into_bytes()),
        None,
    )
    .expect("globex ingest");

    let mut buf_a: Vec<u8> = Vec::new();
    let mut buf_g: Vec<u8> = Vec::new();
    read(&acme, &dir, &mut buf_a, None, TimeRange::all()).expect("acme read");
    read(&globex, &dir, &mut buf_g, None, TimeRange::all()).expect("globex read");

    let acme_out: Vec<LogRecord> = String::from_utf8(buf_a)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse"))
        .collect();
    let globex_out: Vec<LogRecord> = String::from_utf8(buf_g)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse"))
        .collect();

    assert_eq!(acme_out, acme_records);
    assert_eq!(globex_out, globex_records);
    cleanup(&dir);
}

#[test]
fn empty_stdin_produces_zero_records_zero_batches() {
    let dir = temp_data_dir("empty");
    let stats = ingest(
        &tenant("acme"),
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(&b""[..]),
        None,
    )
    .expect("ingest empty");
    assert_eq!(stats.records_ingested, 0);
    assert_eq!(stats.batches_flushed, 0);
    assert_eq!(stats.tier_items_placed, 0);
    cleanup(&dir);
}

#[test]
fn blank_lines_in_input_are_skipped() {
    let dir = temp_data_dir("blank_lines");
    let records = vec![record(100, "a"), record(200, "b")];
    let mut input = String::new();
    input.push('\n');
    input.push_str(&serde_json::to_string(&records[0]).unwrap());
    input.push_str("\n\n\n");
    input.push_str(&serde_json::to_string(&records[1]).unwrap());
    input.push('\n');

    let stats = ingest(
        &tenant("acme"),
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(input.into_bytes()),
        None,
    )
    .expect("ingest");
    assert_eq!(stats.records_ingested, 2);
    cleanup(&dir);
}

#[test]
fn malformed_json_line_returns_typed_error_with_line_number() {
    let dir = temp_data_dir("malformed");
    let good = serde_json::to_string(&record(100, "good")).unwrap();
    let input = format!("{good}\n{{not valid json}}\n");
    let err = ingest(
        &tenant("acme"),
        &dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(input.into_bytes()),
        None,
    )
    .unwrap_err();
    match err {
        kaleidoscope_cli::Error::ParseRecord { line, .. } => {
            assert_eq!(line, 2, "error reports the malformed line number");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    cleanup(&dir);
}

#[test]
fn small_batch_size_splits_into_multiple_batches() {
    let dir = temp_data_dir("small_batch");
    let records: Vec<LogRecord> = (0..10u64).map(|i| record(i, "x")).collect();
    let stats = ingest(
        &tenant("acme"),
        &dir,
        3, // 10 records / batch_size 3 = 4 batches (3+3+3+1)
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("ingest");
    assert_eq!(stats.records_ingested, 10);
    assert_eq!(stats.batches_flushed, 4);
    assert_eq!(stats.tier_items_placed, 4);
    cleanup(&dir);
}
