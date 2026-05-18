// Kaleidoscope self-observe — Sluice → OTLP-JSON acceptance test
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

//! Cross-process bridge: Sluice → OTLP-JSON NDJSON stream.
//! Parity with `cinder_to_otlp_json`. The interesting shape
//! is `sluice.enqueue.count` carrying the `accepted` attribute
//! that distinguishes successful enqueues from
//! capacity-rejection events.

use std::sync::{Arc, Mutex};

use aegis::TenantId;
use self_observe::SluiceToOtlpJsonWriter;
use serde_json::Value;
use sluice::{InMemoryQueue, Queue};

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

fn attr<'a>(point: &'a Value, key: &str) -> Option<&'a str> {
    point["attributes"]
        .as_array()?
        .iter()
        .find(|a| a["key"] == key)?["value"]["stringValue"]
        .as_str()
}

fn build(buf: &Arc<Mutex<Vec<u8>>>, cap: usize) -> InMemoryQueue {
    let writer = SluiceToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    InMemoryQueue::new(cap, Box::new(writer))
}

#[test]
fn sluice_enqueue_emits_otlp_json_line_with_accepted_true_attribute() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let queue = build(&buf, 10);
    queue.enqueue(&tenant("acme"), b"x".to_vec()).expect("ok");
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0]["scopeMetrics"][0]["metrics"][0]["name"],
        "sluice.enqueue.count"
    );
    assert_eq!(
        lines[0]["scopeMetrics"][0]["scope"]["name"],
        "kaleidoscope.sluice"
    );
    let point = &lines[0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    assert_eq!(point["asInt"], "1");
    assert_eq!(attr(point, "tenant_id"), Some("acme"));
    assert_eq!(attr(point, "accepted"), Some("true"));
}

#[test]
fn sluice_enqueue_at_capacity_emits_accepted_false_in_otlp_stream() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let queue = build(&buf, 1);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"a".to_vec()).expect("first ok");
    let _ = queue.enqueue(&tn, b"b".to_vec()).expect_err("full");

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 2);
    let accepted_values: Vec<&str> = lines
        .iter()
        .map(|l| {
            let pt = &l["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
            pt["attributes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["key"] == "accepted")
                .unwrap()["value"]["stringValue"]
                .as_str()
                .unwrap()
        })
        .collect();
    assert!(accepted_values.contains(&"true"));
    assert!(accepted_values.contains(&"false"));
}

#[test]
fn sluice_full_cycle_emits_enqueue_dequeue_ack_lines() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let queue = build(&buf, 10);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"x".to_vec()).expect("enqueue");
    let msg = queue.dequeue(&tn).expect("dequeue");
    queue.ack(msg.id);

    let lines = collect_lines(&buf);
    let names: Vec<&str> = lines
        .iter()
        .map(|l| {
            l["scopeMetrics"][0]["metrics"][0]["name"]
                .as_str()
                .unwrap_or("")
        })
        .collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"sluice.enqueue.count"));
    assert!(names.contains(&"sluice.dequeue.count"));
    assert!(names.contains(&"sluice.ack.count"));
}

#[test]
fn sluice_nack_emits_nack_line_distinct_from_ack() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let queue = build(&buf, 10);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"x".to_vec()).expect("enqueue");
    let msg = queue.dequeue(&tn).expect("dequeue");
    queue.nack(msg.id);

    let lines = collect_lines(&buf);
    let names: Vec<&str> = lines
        .iter()
        .map(|l| {
            l["scopeMetrics"][0]["metrics"][0]["name"]
                .as_str()
                .unwrap_or("")
        })
        .collect();
    assert!(names.contains(&"sluice.nack.count"));
    assert!(!names.contains(&"sluice.ack.count"));
}

#[test]
fn output_is_ndjson_one_record_per_line_with_trailing_newline() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let queue = build(&buf, 10);
    queue.enqueue(&tenant("acme"), b"x".to_vec()).expect("ok");
    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    assert!(s.ends_with('\n'), "trailing newline for tail -f");
    assert_eq!(s.matches('\n').count(), 1, "exactly one terminator");
}

#[test]
fn two_tenants_emit_distinct_otlp_resource_attributes() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let queue = build(&buf, 10);
    queue.enqueue(&tenant("acme"), b"a".to_vec()).expect("a");
    queue.enqueue(&tenant("globex"), b"g".to_vec()).expect("g");
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
