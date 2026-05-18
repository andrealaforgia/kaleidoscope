// Kaleidoscope self-observe — Ray → OTLP-JSON acceptance test
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

//! Cross-process bridge: Ray → OTLP-JSON NDJSON stream.
//! Parity with `lumen_to_otlp_json` (fixed-array attribute
//! shape). Different scope name and metric names.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use aegis::TenantId;
use ray::{
    InMemoryTraceStore, ServiceName, Span, SpanBatch, SpanId, SpanKind, SpanStatus, StatusCode,
    TimeRange as RayTimeRange, TraceId, TraceStore,
};
use self_observe::RayToOtlpJsonWriter;
use serde_json::Value;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn span(trace: [u8; 16], span: [u8; 8]) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Span {
        trace_id: TraceId(trace),
        span_id: SpanId(span),
        parent_span_id: None,
        name: "GET /checkout".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: 100,
        end_time_unix_nano: 200,
        status: SpanStatus {
            code: StatusCode::Ok,
            message: String::new(),
        },
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
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

fn build(buf: &Arc<Mutex<Vec<u8>>>) -> InMemoryTraceStore {
    let writer = RayToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    InMemoryTraceStore::new(Box::new(writer))
}

#[test]
fn ray_ingest_emits_one_otlp_json_line_with_span_count_as_int() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let ray = build(&buf);
    ray.ingest(
        &tenant("acme"),
        SpanBatch::with_spans(vec![span([1; 16], [1; 8]), span([2; 16], [1; 8])]),
    )
    .expect("ingest");

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0]["scopeMetrics"][0]["metrics"][0]["name"],
        "ray.ingest.count"
    );
    assert_eq!(
        lines[0]["scopeMetrics"][0]["scope"]["name"],
        "kaleidoscope.ray"
    );
    let point = &lines[0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    assert_eq!(point["asInt"], "2");
    assert_eq!(
        point["attributes"][0]["value"]["stringValue"], "acme",
        "tenant_id point attribute"
    );
}

#[test]
fn ray_query_emits_a_second_distinct_otlp_metric_line() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let ray = build(&buf);
    let tn = tenant("acme");
    ray.ingest(
        &tn,
        SpanBatch::with_spans(vec![
            span([1; 16], [1; 8]),
            span([2; 16], [1; 8]),
            span([3; 16], [1; 8]),
        ]),
    )
    .expect("ingest");
    let svc = ServiceName::new("checkout");
    let _ = ray.query(&tn, &svc, RayTimeRange::all()).expect("query");

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 2);
    let names: Vec<&str> = lines
        .iter()
        .map(|l| {
            l["scopeMetrics"][0]["metrics"][0]["name"]
                .as_str()
                .unwrap_or("")
        })
        .collect();
    assert!(names.contains(&"ray.ingest.count"));
    assert!(names.contains(&"ray.query.count"));
}

#[test]
fn output_is_ndjson_one_record_per_line_with_trailing_newline() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let ray = build(&buf);
    ray.ingest(
        &tenant("acme"),
        SpanBatch::with_spans(vec![span([1; 16], [1; 8])]),
    )
    .expect("ingest");
    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    assert!(s.ends_with('\n'));
    assert_eq!(s.matches('\n').count(), 1);
}

#[test]
fn two_tenants_emit_distinct_otlp_resource_attributes() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let ray = build(&buf);
    ray.ingest(
        &tenant("acme"),
        SpanBatch::with_spans(vec![span([1; 16], [1; 8])]),
    )
    .expect("acme");
    ray.ingest(
        &tenant("globex"),
        SpanBatch::with_spans(vec![span([2; 16], [1; 8])]),
    )
    .expect("globex");
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
