// Kaleidoscope self-observe — Augur → OTLP-JSON acceptance test
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

//! Cross-process bridge: Augur → OTLP-JSON NDJSON stream.
//! First writer that emits multiple OTLP metrics per source
//! event (anomaly → count + score), first writer that uses
//! `asDouble` data points (Gauge for the score).

use std::sync::{Arc, Mutex};

use aegis::TenantId;
use augur::MetricsRecorder as AugurRecorder;
use self_observe::AugurToOtlpJsonWriter;
use serde_json::Value;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

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

fn metric_name(v: &Value) -> &str {
    v["scopeMetrics"][0]["metrics"][0]["name"]
        .as_str()
        .unwrap_or("")
}

fn build(buf: &Arc<Mutex<Vec<u8>>>) -> AugurToOtlpJsonWriter<SharedBuf> {
    AugurToOtlpJsonWriter::new(SharedBuf(buf.clone()))
}

#[test]
fn augur_observation_emits_one_line_with_sum_as_int() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = build(&buf);
    writer.record_observation(&tenant("acme"));
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 1);
    assert_eq!(metric_name(&lines[0]), "augur.observation.count");
    assert_eq!(
        lines[0]["scopeMetrics"][0]["scope"]["name"],
        "kaleidoscope.augur"
    );
    let dp = &lines[0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    assert_eq!(dp["asInt"], "1");
    assert_eq!(dp["attributes"][0]["value"]["stringValue"], "acme");
}

#[test]
fn augur_anomaly_emits_two_lines_one_count_one_score() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = build(&buf);
    writer.record_anomaly(&tenant("acme"), 4.2);

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 2, "anomaly fires two distinct OTLP metrics");
    let names: Vec<&str> = lines.iter().map(metric_name).collect();
    assert!(names.contains(&"augur.anomaly.count"));
    assert!(names.contains(&"augur.anomaly.score"));
}

#[test]
fn augur_anomaly_score_lands_as_gauge_asdouble_not_asint() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = build(&buf);
    writer.record_anomaly(&tenant("acme"), 4.2);

    let lines = collect_lines(&buf);
    let score_line = lines
        .iter()
        .find(|l| metric_name(l) == "augur.anomaly.score")
        .expect("score line");
    // The score metric is a Gauge, not a Sum. The data point
    // carries asDouble as a JSON number (not a string like
    // asInt does, because uint64 doesn't fit a JSON number
    // safely but f64 does).
    let dp = &score_line["scopeMetrics"][0]["metrics"][0]["gauge"]["dataPoints"][0];
    assert!(dp["asDouble"].is_number(), "asDouble is a JSON number");
    assert!(
        (dp["asDouble"].as_f64().unwrap() - 4.2).abs() < 1e-9,
        "asDouble carries the score value"
    );
    assert!(
        score_line["scopeMetrics"][0]["metrics"][0]["sum"].is_null(),
        "Gauge metric does not carry a sum field"
    );
}

#[test]
fn augur_negative_anomaly_score_serializes_correctly() {
    // Negative z-scores are real signals (value far below the
    // baseline). Make sure they round-trip through serde + the
    // JSON encoding without sign loss.
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = build(&buf);
    writer.record_anomaly(&tenant("acme"), -3.7);

    let lines = collect_lines(&buf);
    let score_line = lines
        .iter()
        .find(|l| metric_name(l) == "augur.anomaly.score")
        .expect("score line");
    let dp = &score_line["scopeMetrics"][0]["metrics"][0]["gauge"]["dataPoints"][0];
    assert!((dp["asDouble"].as_f64().unwrap() - -3.7).abs() < 1e-9);
}

#[test]
fn output_is_ndjson_one_record_per_line_with_trailing_newline() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = build(&buf);
    writer.record_observation(&tenant("acme"));
    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    assert!(s.ends_with('\n'));
    assert_eq!(s.matches('\n').count(), 1);
}

#[test]
fn two_tenants_emit_distinct_otlp_resource_attributes() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = build(&buf);
    writer.record_observation(&tenant("acme"));
    writer.record_observation(&tenant("globex"));
    let lines = collect_lines(&buf);
    let resource_tenants: Vec<&str> = lines
        .iter()
        .map(|l| {
            l["resource"]["attributes"][0]["value"]["stringValue"]
                .as_str()
                .unwrap_or("")
        })
        .collect();
    assert!(resource_tenants.contains(&"acme"));
    assert!(resource_tenants.contains(&"globex"));
}
