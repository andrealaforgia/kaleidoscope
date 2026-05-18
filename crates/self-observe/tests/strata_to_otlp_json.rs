// Kaleidoscope self-observe — Strata → OTLP-JSON acceptance test
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

//! Cross-process bridge: Strata → OTLP-JSON NDJSON stream.
//! Parity with `lumen_to_otlp_json` and `ray_to_otlp_json`
//! (fixed-array attribute shape).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use aegis::TenantId;
use self_observe::StrataToOtlpJsonWriter;
use serde_json::Value;
use strata::{
    InMemoryProfileStore, Profile, ProfileBatch, ProfileStore, ServiceName,
    TimeRange as StrataTimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn profile(time: u64) -> Profile {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Profile {
        time_unix_nano: time,
        duration_nanos: 1_000_000_000,
        profile_type: "cpu".to_string(),
        sample_type: Vec::new(),
        samples: Vec::new(),
        locations: Vec::new(),
        functions: Vec::new(),
        mappings: Vec::new(),
        string_table: vec![String::new()],
        resource_attributes: resource,
        attributes: BTreeMap::new(),
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

fn build(buf: &Arc<Mutex<Vec<u8>>>) -> InMemoryProfileStore {
    let writer = StrataToOtlpJsonWriter::new(SharedBuf(buf.clone()));
    InMemoryProfileStore::new(Box::new(writer))
}

#[test]
fn strata_ingest_emits_one_otlp_json_line_with_profile_count_as_int() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let strata = build(&buf);
    strata
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(100), profile(200)]),
        )
        .expect("ingest");

    let lines = collect_lines(&buf);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0]["scopeMetrics"][0]["metrics"][0]["name"],
        "strata.ingest.count"
    );
    assert_eq!(
        lines[0]["scopeMetrics"][0]["scope"]["name"],
        "kaleidoscope.strata"
    );
    let dp = &lines[0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    assert_eq!(dp["asInt"], "2");
    assert_eq!(dp["attributes"][0]["value"]["stringValue"], "acme");
}

#[test]
fn strata_query_emits_a_second_distinct_otlp_metric_line() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let strata = build(&buf);
    let tn = tenant("acme");
    strata
        .ingest(
            &tn,
            ProfileBatch::with_profiles(vec![profile(100), profile(200)]),
        )
        .expect("ingest");
    let svc = ServiceName::new("checkout");
    let _ = strata
        .query(&tn, &svc, StrataTimeRange::new(0, u64::MAX))
        .expect("query");

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
    assert!(names.contains(&"strata.ingest.count"));
    assert!(names.contains(&"strata.query.count"));
}

#[test]
fn output_is_ndjson_one_record_per_line_with_trailing_newline() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let strata = build(&buf);
    strata
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(100)]),
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
    let strata = build(&buf);
    strata
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(100)]),
        )
        .expect("acme");
    strata
        .ingest(
            &tenant("globex"),
            ProfileBatch::with_profiles(vec![profile(100)]),
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
