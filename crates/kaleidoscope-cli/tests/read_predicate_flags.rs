// Kaleidoscope CLI — read --service / --min-severity acceptance test
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

//! `read` server-side filtering via Lumen's `query_with`. The
//! library function `read_filtered` is exercised directly here;
//! the binary's flag parsing is exercised end-to-end in the
//! shell smoke run before commit, and indirectly via the
//! `parse_severity` round-trip below.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, parse_severity, read_filtered, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, Predicate, SeverityNumber};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, service: &str, severity: SeverityNumber, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: severity,
        severity_text: "n/a".to_string(),
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
    p.push(format!("kal-cli-readpred-{name}-{pid}-{nanos}"));
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
    let records = vec![
        record(10, "checkout", SeverityNumber::INFO, "info-checkout"),
        record(20, "checkout", SeverityNumber::WARN, "warn-checkout"),
        record(30, "checkout", SeverityNumber::ERROR, "error-checkout"),
        record(40, "search", SeverityNumber::INFO, "info-search"),
        record(50, "search", SeverityNumber::ERROR, "error-search"),
        record(60, "payments", SeverityNumber::FATAL, "fatal-payments"),
    ];
    ingest(
        tn,
        dir,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&records).into_bytes()),
        None,
    )
    .expect("seed");
}

fn read_to_records(tn: &TenantId, dir: &std::path::Path, predicate: &Predicate) -> Vec<LogRecord> {
    let mut buf: Vec<u8> = Vec::new();
    read_filtered(tn, dir, predicate, &mut buf).expect("read");
    String::from_utf8(buf)
        .expect("utf8")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<LogRecord>(l).expect("parse"))
        .collect()
}

#[test]
fn read_with_empty_predicate_returns_every_tenant_record() {
    let dir = temp_data_dir("empty_pred");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let got = read_to_records(&tn, &dir, &Predicate::new());
    assert_eq!(got.len(), 6, "empty predicate matches read() exactly");
    cleanup(&dir);
}

#[test]
fn read_with_service_filter_returns_only_that_service() {
    let dir = temp_data_dir("svc_filter");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let predicate = Predicate::new().service("checkout");
    let got = read_to_records(&tn, &dir, &predicate);
    assert_eq!(got.len(), 3);
    for r in &got {
        assert_eq!(
            r.resource_attributes
                .get("service.name")
                .map(String::as_str),
            Some("checkout")
        );
    }
    cleanup(&dir);
}

#[test]
fn read_with_min_severity_filter_drops_lower_severities() {
    let dir = temp_data_dir("min_sev");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let predicate = Predicate::new().min_severity(SeverityNumber::ERROR);
    let got = read_to_records(&tn, &dir, &predicate);
    // ERROR or higher: error-checkout, error-search, fatal-payments.
    assert_eq!(got.len(), 3);
    let bodies: Vec<&str> = got.iter().map(|r| r.body.as_str()).collect();
    assert!(bodies.contains(&"error-checkout"));
    assert!(bodies.contains(&"error-search"));
    assert!(bodies.contains(&"fatal-payments"));
    cleanup(&dir);
}

#[test]
fn read_with_service_and_min_severity_is_conjunctive() {
    let dir = temp_data_dir("svc_and_sev");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let predicate = Predicate::new()
        .service("checkout")
        .min_severity(SeverityNumber::WARN);
    let got = read_to_records(&tn, &dir, &predicate);
    // checkout AND >= WARN: warn-checkout, error-checkout.
    assert_eq!(got.len(), 2);
    let bodies: Vec<&str> = got.iter().map(|r| r.body.as_str()).collect();
    assert!(bodies.contains(&"warn-checkout"));
    assert!(bodies.contains(&"error-checkout"));
    cleanup(&dir);
}

#[test]
fn read_predicate_that_matches_nothing_returns_zero_records_without_error() {
    let dir = temp_data_dir("no_match");
    let tn = tenant("acme");
    seed(&dir, &tn);
    let predicate = Predicate::new().service("does-not-exist");
    let got = read_to_records(&tn, &dir, &predicate);
    assert!(got.is_empty());
    cleanup(&dir);
}

#[test]
fn parse_severity_accepts_all_six_otlp_levels_case_insensitively() {
    assert_eq!(parse_severity("TRACE"), Some(SeverityNumber::TRACE));
    assert_eq!(parse_severity("debug"), Some(SeverityNumber::DEBUG));
    assert_eq!(parse_severity("Info"), Some(SeverityNumber::INFO));
    assert_eq!(parse_severity("WARN"), Some(SeverityNumber::WARN));
    assert_eq!(parse_severity("warning"), Some(SeverityNumber::WARN));
    assert_eq!(parse_severity("error"), Some(SeverityNumber::ERROR));
    assert_eq!(parse_severity("FATAL"), Some(SeverityNumber::FATAL));
    assert_eq!(parse_severity("nope"), None);
    assert_eq!(parse_severity(""), None);
}
