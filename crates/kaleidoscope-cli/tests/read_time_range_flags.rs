// Kaleidoscope CLI — read --since / --until acceptance test
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

//! `read` time-range filtering via Lumen's `query_with(range, &Predicate)`.
//! The CLI accepts unix-seconds bounds and the library scales to
//! nanoseconds. The helpers `parse_unix_seconds_to_nanos` and
//! `build_time_range` are exercised directly here; the binary's
//! flag-shape is exercised end-to-end in the shell smoke before
//! commit.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{
    build_time_range, ingest, parse_unix_seconds_to_nanos, read_filtered, DEFAULT_BATCH_SIZE,
};
use lumen::{LogRecord, Predicate, SeverityNumber, TimeRange};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed_secs: u64, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed_secs * 1_000_000_000,
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
    p.push(format!("kal-cli-readrange-{name}-{pid}-{nanos}"));
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

fn seed(dir: &std::path::Path, tn: &TenantId) {
    // Six records spaced 10s apart from t=100s to t=150s. Bounds
    // chosen so off-by-one between [start, end) inclusive/exclusive
    // is visible: a window of [110, 130) must yield exactly two
    // (t=110 included, t=130 excluded).
    let records: Vec<LogRecord> = (0..6u64)
        .map(|i| record(100 + i * 10, &format!("t-{}", 100 + i * 10)))
        .collect();
    ingest(
        tn,
        dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("seed");
}

fn read_to_records(
    tn: &TenantId,
    dir: &std::path::Path,
    range: TimeRange,
    predicate: &Predicate,
) -> Vec<LogRecord> {
    let mut buf: Vec<u8> = Vec::new();
    read_filtered(tn, dir, range, predicate, &mut buf).expect("read");
    String::from_utf8(buf)
        .expect("utf8")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<LogRecord>(l).expect("parse"))
        .collect()
}

#[test]
fn read_with_since_only_excludes_records_before_the_bound() {
    let dir = temp_data_dir("since_only");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let range = build_time_range(parse_unix_seconds_to_nanos("120"), None).expect("range");
    let got = read_to_records(&tn, &dir, range, &Predicate::new());
    // t=120, 130, 140, 150 included.
    assert_eq!(got.len(), 4);
    let bodies: Vec<&str> = got.iter().map(|r| r.body.as_str()).collect();
    assert!(!bodies.contains(&"t-100"));
    assert!(!bodies.contains(&"t-110"));
    assert!(bodies.contains(&"t-120"));
    assert!(bodies.contains(&"t-150"));
    cleanup(&dir);
}

#[test]
fn read_with_until_only_excludes_records_at_or_after_the_bound() {
    let dir = temp_data_dir("until_only");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let range = build_time_range(None, parse_unix_seconds_to_nanos("130")).expect("range");
    let got = read_to_records(&tn, &dir, range, &Predicate::new());
    // [0, 130 * 1e9): t=100, 110, 120. t=130 EXCLUDED (half-open).
    assert_eq!(got.len(), 3);
    let bodies: Vec<&str> = got.iter().map(|r| r.body.as_str()).collect();
    assert!(bodies.contains(&"t-100"));
    assert!(bodies.contains(&"t-110"));
    assert!(bodies.contains(&"t-120"));
    assert!(!bodies.contains(&"t-130"));
    cleanup(&dir);
}

#[test]
fn read_with_since_and_until_returns_half_open_window() {
    let dir = temp_data_dir("window");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let range = build_time_range(
        parse_unix_seconds_to_nanos("110"),
        parse_unix_seconds_to_nanos("130"),
    )
    .expect("range");
    let got = read_to_records(&tn, &dir, range, &Predicate::new());
    // [110, 130): t=110, 120. t=130 EXCLUDED.
    assert_eq!(got.len(), 2);
    let bodies: Vec<&str> = got.iter().map(|r| r.body.as_str()).collect();
    assert_eq!(bodies, vec!["t-110", "t-120"]);
    cleanup(&dir);
}

#[test]
fn read_with_window_that_matches_nothing_returns_zero_records() {
    let dir = temp_data_dir("no_match");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let range = build_time_range(
        parse_unix_seconds_to_nanos("200"),
        parse_unix_seconds_to_nanos("300"),
    )
    .expect("range");
    let got = read_to_records(&tn, &dir, range, &Predicate::new());
    assert!(got.is_empty());
    cleanup(&dir);
}

#[test]
fn parse_unix_seconds_to_nanos_round_trips_normal_values() {
    assert_eq!(parse_unix_seconds_to_nanos("0"), Some(0));
    assert_eq!(parse_unix_seconds_to_nanos("1"), Some(1_000_000_000));
    assert_eq!(
        parse_unix_seconds_to_nanos("1717200000"),
        Some(1_717_200_000_000_000_000)
    );
}

#[test]
fn parse_unix_seconds_to_nanos_rejects_non_numeric_input() {
    assert_eq!(parse_unix_seconds_to_nanos(""), None);
    assert_eq!(parse_unix_seconds_to_nanos("now"), None);
    assert_eq!(parse_unix_seconds_to_nanos("-1"), None);
    assert_eq!(parse_unix_seconds_to_nanos("1.5"), None);
    assert_eq!(parse_unix_seconds_to_nanos("2026-05-17T22:00:00Z"), None);
}

#[test]
fn parse_unix_seconds_to_nanos_rejects_values_that_overflow_u64_when_scaled() {
    // u64::MAX seconds * 1e9 overflows. The function MUST return
    // None rather than wrap silently.
    assert_eq!(parse_unix_seconds_to_nanos(&u64::MAX.to_string()), None);
}

#[test]
fn build_time_range_defaults_to_all_when_both_bounds_omitted() {
    let range = build_time_range(None, None).expect("range");
    assert_eq!(range, TimeRange::all());
}

#[test]
fn build_time_range_rejects_empty_window_with_since_at_or_above_until() {
    // [200, 100) is empty — operator typo, not silent zero.
    assert!(build_time_range(Some(200), Some(100)).is_none());
    // [100, 100) is also empty — half-open conventions.
    assert!(build_time_range(Some(100), Some(100)).is_none());
}

#[test]
fn build_time_range_composes_correctly_with_explicit_bounds() {
    let r = build_time_range(Some(1_000_000_000), Some(2_000_000_000)).expect("range");
    assert_eq!(r.start_unix_nano, 1_000_000_000);
    assert_eq!(r.end_unix_nano, 2_000_000_000);
}
