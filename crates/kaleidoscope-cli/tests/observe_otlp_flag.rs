// Kaleidoscope CLI — --observe-otlp flag acceptance test
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

//! When `ingest` is called with `otlp_log_path = Some(...)`,
//! both the Lumen and Cinder recorders are replaced by their
//! OTLP-JSON writer variants. The file at that path receives
//! one NDJSON OTLP-JSON line per Lumen ingest event AND one
//! per Cinder place event (which is one place per batch flush
//! in the current ingest implementation). Operators can `tail
//! -f` the file and a sidecar can forward to a real OTLP/HTTP
//! collector.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber};
use serde_json::Value;

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
    p.push(format!("kal-cli-otlp-{name}-{pid}-{nanos}"));
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
fn observe_otlp_writes_one_lumen_and_one_cinder_line_per_batch_flush() {
    let root = temp_root("one_line_per_batch");
    let data = root.join("data");
    let otlp = root.join("otlp.ndjson");

    // Ingest two batches of three records each (batch_size = 3).
    let records: Vec<LogRecord> = (0..6u64).map(|i| record(i, "x")).collect();
    let stats = ingest(
        &tenant("acme"),
        &data,
        3,
        Cursor::new(ndjson(&records).into_bytes()),
        Some(&otlp),
    )
    .expect("ingest");
    assert_eq!(stats.batches_flushed, 2);

    let content = fs::read_to_string(&otlp).expect("read otlp file");
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    // Each batch flush produces one lumen.ingest.count line
    // (record_count = 3 records per batch) plus one
    // cinder.place.count line (one tier item placed per batch).
    assert_eq!(lines.len(), 4, "two batches → 2 Lumen + 2 Cinder lines");

    let parsed: Vec<Value> = lines
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).expect("parse"))
        .collect();

    let lumen_lines: Vec<&Value> = parsed
        .iter()
        .filter(|v| v["scopeMetrics"][0]["metrics"][0]["name"] == "lumen.ingest.count")
        .collect();
    let cinder_lines: Vec<&Value> = parsed
        .iter()
        .filter(|v| v["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.place.count")
        .collect();
    assert_eq!(lumen_lines.len(), 2, "one lumen ingest event per batch");
    assert_eq!(cinder_lines.len(), 2, "one cinder place event per batch");

    for line in &lumen_lines {
        let dp = &line["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
        assert_eq!(dp["asInt"], "3", "each batch carried 3 records");
        assert_eq!(
            line["resource"]["attributes"][0]["value"]["stringValue"],
            "acme"
        );
        assert_eq!(
            line["scopeMetrics"][0]["scope"]["name"],
            "kaleidoscope.lumen"
        );
    }
    for line in &cinder_lines {
        // Cinder place carries tier as a point attribute. The
        // CLI places every batch in Hot.
        let dp = &line["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
        assert_eq!(dp["asInt"], "1", "each place event counts 1");
        let attrs = dp["attributes"].as_array().expect("attrs array");
        let tier = attrs
            .iter()
            .find(|a| a["key"] == "tier")
            .expect("tier attr")["value"]["stringValue"]
            .as_str()
            .expect("tier string");
        assert_eq!(tier, "hot");
        assert_eq!(
            line["scopeMetrics"][0]["scope"]["name"],
            "kaleidoscope.cinder"
        );
    }
    cleanup(&root);
}

#[test]
fn no_observe_otlp_means_no_otlp_file_created() {
    let root = temp_root("no_flag");
    let data = root.join("data");
    let otlp_would_be = root.join("otlp.ndjson");

    let _ = ingest(
        &tenant("acme"),
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(100, "x")]).into_bytes()),
        None,
    )
    .expect("ingest");

    assert!(
        !otlp_would_be.exists(),
        "OTLP file must not be created when flag is absent"
    );
    cleanup(&root);
}

#[test]
fn observe_otlp_file_is_appended_to_across_multiple_ingest_calls() {
    // The OTLP writer opens the file in append mode, so a
    // second ingest call against the same data_dir + otlp_path
    // adds more lines without truncating the first call's
    // output.
    let root = temp_root("append");
    let data = root.join("data");
    let otlp = root.join("otlp.ndjson");

    ingest(
        &tenant("acme"),
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(100, "a")]).into_bytes()),
        Some(&otlp),
    )
    .expect("first ingest");
    ingest(
        &tenant("acme"),
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(200, "b")]).into_bytes()),
        Some(&otlp),
    )
    .expect("second ingest");

    let content = fs::read_to_string(&otlp).expect("read");
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    // Each ingest call: 1 batch of 1 record => 1 Lumen ingest
    // line + 1 Cinder place line. Two calls => 4 lines, all
    // appended (never truncated).
    assert_eq!(
        lines.len(),
        4,
        "two ingest calls → 2 Lumen + 2 Cinder lines, appended"
    );
    let names: Vec<String> = lines
        .iter()
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse");
            v["scopeMetrics"][0]["metrics"][0]["name"]
                .as_str()
                .unwrap_or("")
                .to_string()
        })
        .collect();
    let lumen_count = names.iter().filter(|n| *n == "lumen.ingest.count").count();
    let cinder_count = names.iter().filter(|n| *n == "cinder.place.count").count();
    assert_eq!(lumen_count, 2);
    assert_eq!(cinder_count, 2);
    cleanup(&root);
}
