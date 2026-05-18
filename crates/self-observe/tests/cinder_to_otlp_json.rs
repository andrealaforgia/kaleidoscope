// Kaleidoscope self-observe — Cinder → OTLP-JSON acceptance test
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

//! Cross-process bridge: Cinder → OTLP-JSON NDJSON stream.
//! Parity with `lumen_to_otlp_json` but for Cinder's three
//! events, with the tier-topology attributes that distinguish
//! the Cinder shape from the Lumen shape.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use aegis::TenantId;
use cinder::{InMemoryTieringStore, ItemId, Tier, TierPolicy, TieringStore};
use self_observe::CinderToOtlpJsonWriter;
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

fn first_data_point(value: &Value) -> &Value {
    &value["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]
}

fn attr<'a>(point: &'a Value, key: &str) -> Option<&'a str> {
    point["attributes"]
        .as_array()?
        .iter()
        .find(|a| a["key"] == key)?["value"]["stringValue"]
        .as_str()
}

fn build(buf: &Arc<Mutex<Vec<u8>>>) -> InMemoryTieringStore {
    let writer = CinderToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    InMemoryTieringStore::new(Box::new(writer))
}

#[test]
fn cinder_place_emits_otlp_json_line_with_tier_point_attribute() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let cinder = build(&buf);
    cinder.place(
        &tenant("acme"),
        &ItemId::new("w-1"),
        Tier::Hot,
        SystemTime::UNIX_EPOCH,
    );
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 1);
    let point = first_data_point(&lines[0]);
    assert_eq!(point["asInt"], "1");
    assert_eq!(attr(point, "tenant_id"), Some("acme"));
    assert_eq!(attr(point, "tier"), Some("hot"));
    assert_eq!(
        lines[0]["scopeMetrics"][0]["metrics"][0]["name"],
        "cinder.place.count"
    );
    assert_eq!(
        lines[0]["scopeMetrics"][0]["scope"]["name"],
        "kaleidoscope.cinder"
    );
}

#[test]
fn cinder_migrate_emits_otlp_json_line_with_from_and_to_point_attributes() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let cinder = build(&buf);
    let tn = tenant("acme");
    let item = ItemId::new("w-1");
    cinder.place(&tn, &item, Tier::Hot, SystemTime::UNIX_EPOCH);
    cinder
        .migrate(
            &tn,
            &item,
            Tier::Warm,
            SystemTime::UNIX_EPOCH + Duration::from_secs(60),
        )
        .expect("migrate");

    let lines = collect_lines(&buf);
    // 1 line for place, 1 line for migrate.
    assert_eq!(lines.len(), 2);
    let migrate = &lines[1];
    assert_eq!(
        migrate["scopeMetrics"][0]["metrics"][0]["name"],
        "cinder.migrate.count"
    );
    let point = first_data_point(migrate);
    assert_eq!(attr(point, "from"), Some("hot"));
    assert_eq!(attr(point, "to"), Some("warm"));
    assert_eq!(attr(point, "tenant_id"), Some("acme"));
}

#[test]
fn cinder_evaluate_emits_one_line_per_tenant_with_migrated_count_as_int() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let cinder = build(&buf);
    let tn = tenant("acme");
    cinder.place(&tn, &ItemId::new("w-1"), Tier::Hot, SystemTime::UNIX_EPOCH);
    cinder.place(&tn, &ItemId::new("w-2"), Tier::Hot, SystemTime::UNIX_EPOCH);

    let policy = TierPolicy::age_based(Duration::from_secs(60), Duration::from_secs(300));
    cinder.evaluate_at(SystemTime::UNIX_EPOCH + Duration::from_secs(120), &policy);

    let lines = collect_lines(&buf);
    // 2 places + 2 migrate (Hot->Warm for each item) + 1 evaluate
    assert_eq!(lines.len(), 5);
    let names: Vec<&str> = lines
        .iter()
        .map(|l| {
            l["scopeMetrics"][0]["metrics"][0]["name"]
                .as_str()
                .unwrap_or("")
        })
        .collect();
    let eval_count = names
        .iter()
        .filter(|n| **n == "cinder.evaluate.migrated.count")
        .count();
    assert_eq!(eval_count, 1);
    let eval = lines
        .iter()
        .find(|l| l["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.evaluate.migrated.count")
        .expect("eval line");
    let point = first_data_point(eval);
    assert_eq!(point["asInt"], "2");
}

#[test]
fn output_is_ndjson_one_record_per_line_with_trailing_newline() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let cinder = build(&buf);
    cinder.place(
        &tenant("acme"),
        &ItemId::new("w-1"),
        Tier::Hot,
        SystemTime::UNIX_EPOCH,
    );
    let bytes = buf.lock().unwrap().clone();
    let s = String::from_utf8(bytes).expect("utf8");
    assert!(s.ends_with('\n'), "trailing newline so tail -f sees it");
    assert_eq!(
        s.matches('\n').count(),
        1,
        "exactly one record-terminator per event"
    );
}

#[test]
fn two_tenants_emit_distinct_otlp_resource_attributes() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let cinder = build(&buf);
    cinder.place(
        &tenant("acme"),
        &ItemId::new("a-1"),
        Tier::Hot,
        SystemTime::UNIX_EPOCH,
    );
    cinder.place(
        &tenant("globex"),
        &ItemId::new("g-1"),
        Tier::Cold,
        SystemTime::UNIX_EPOCH,
    );
    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 2);
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
