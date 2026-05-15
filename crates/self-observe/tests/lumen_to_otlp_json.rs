// Kaleidoscope self-observe — Lumen → OTLP-JSON acceptance test
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

//! Cross-process bridge: Lumen → OTLP-JSON NDJSON stream.
//!
//! The writer emits one line of OTLP-JSON
//! `ResourceMetrics` per Lumen event. These tests capture the
//! stream into a `Vec<u8>`, parse each line as JSON, and
//! assert the OTLP-JSON shape collectors will read.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use aegis::TenantId;
use lumen::{InMemoryLogStore, LogBatch, LogRecord, LogStore, SeverityNumber, TimeRange};
use self_observe::LumenToOtlpJsonWriter;
use serde_json::Value;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn log_record(observed: u64, body: &str) -> LogRecord {
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

/// Shareable writer so the test can hold the buffer while
/// Lumen holds the recorder. `Arc<Mutex<Vec<u8>>>` implements
/// `Write` through a wrapper.
#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn collect_lines(buf: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse otlp-json"))
        .collect()
}

#[test]
fn lumen_ingest_emits_one_otlp_resource_metrics_line() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = LumenToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let lumen = InMemoryLogStore::new(Box::new(writer));

    lumen
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![
                log_record(100, "first"),
                log_record(200, "second"),
                log_record(300, "third"),
            ]),
        )
        .expect("ingest");

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 1, "exactly one OTLP line emitted");
    let line = &lines[0];

    // Resource-level tenant attribute.
    assert_eq!(line["resource"]["attributes"][0]["key"], "tenant_id");
    assert_eq!(
        line["resource"]["attributes"][0]["value"]["stringValue"],
        "acme"
    );

    // Scope.
    assert_eq!(
        line["scopeMetrics"][0]["scope"]["name"],
        "kaleidoscope.lumen"
    );

    // Metric.
    assert_eq!(
        line["scopeMetrics"][0]["metrics"][0]["name"],
        "lumen.ingest.count"
    );
    let sum = &line["scopeMetrics"][0]["metrics"][0]["sum"];
    assert_eq!(sum["isMonotonic"], true);
    assert_eq!(sum["aggregationTemporality"], 2);

    // The data point. OTLP-JSON encodes uint64 as string.
    let dp = &sum["dataPoints"][0];
    assert_eq!(dp["asInt"], "3");
    assert_eq!(dp["attributes"][0]["key"], "tenant_id");
    assert_eq!(dp["attributes"][0]["value"]["stringValue"], "acme");
    let time_str = dp["timeUnixNano"].as_str().expect("string");
    assert!(
        time_str.parse::<u64>().is_ok(),
        "timeUnixNano is uint64 string"
    );
}

#[test]
fn lumen_query_emits_a_second_distinct_otlp_metric_line() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = LumenToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let lumen = InMemoryLogStore::new(Box::new(writer));
    let tn = tenant("acme");

    lumen
        .ingest(
            &tn,
            LogBatch::with_records(vec![log_record(100, "a"), log_record(200, "b")]),
        )
        .expect("ingest");
    let _ = lumen.query(&tn, TimeRange::all()).expect("query");

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 2);

    let names: Vec<&str> = lines
        .iter()
        .map(|l| l["scopeMetrics"][0]["metrics"][0]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["lumen.ingest.count", "lumen.query.count"]);

    let ingest_dp = &lines[0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    let query_dp = &lines[1]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    assert_eq!(ingest_dp["asInt"], "2");
    assert_eq!(query_dp["asInt"], "2");
}

#[test]
fn two_tenants_emit_distinct_otlp_resource_attributes() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = LumenToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let lumen = InMemoryLogStore::new(Box::new(writer));

    lumen
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![log_record(100, "a")]),
        )
        .expect("acme");
    lumen
        .ingest(
            &tenant("globex"),
            LogBatch::with_records(vec![log_record(200, "g")]),
        )
        .expect("globex");

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0]["resource"]["attributes"][0]["value"]["stringValue"],
        "acme"
    );
    assert_eq!(
        lines[1]["resource"]["attributes"][0]["value"]["stringValue"],
        "globex"
    );
}

#[test]
fn output_is_ndjson_one_record_per_line_with_trailing_newline() {
    // OTLP-JSON over NDJSON is the interop format a sidecar
    // expects. This test pins the byte-level shape: every line
    // is valid JSON, lines are separated by '\n', and the
    // stream ends with '\n' (not the last line missing it).
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = LumenToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    let lumen = InMemoryLogStore::new(Box::new(writer));

    for i in 0..3u64 {
        lumen
            .ingest(
                &tenant("acme"),
                LogBatch::with_records(vec![log_record(i * 100, "x")]),
            )
            .expect("ingest");
    }

    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    assert!(s.ends_with('\n'), "stream ends with newline");
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.len(), 3);
    for line in lines {
        // Every line is independently parseable JSON.
        let _: Value = serde_json::from_str(line).expect("each line is JSON");
    }
}
