// Kaleidoscope consolidated runtime — Slice 7: live traces attribute filter
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

//! # Slice 7 — "find one customer's traces among many, live" (experimentable-stack-v0).
//!
//! Proves the consolidated runtime serves the new `attr_key`/`attr_value` span
//! attribute filter on the traces listing END TO END. The whole loop runs in
//! ONE process on EPHEMERAL `127.0.0.1:0` ports with the always-current demo
//! overlay OFF (`demo_overlay_enabled = false`), so the routers read EXACTLY the
//! RAW ingested data — no synthetic demo doubling the real push. Three traces
//! for the SAME service, each carrying a DIFFERENT `customer.id` span attribute,
//! are ingested over the REAL OTLP HTTP ingest path; once all three are visible
//! on the unfiltered listing, a single
//! `GET /api/v1/traces?service=...&attr_key=customer.id&attr_value=alice` over
//! loopback must carry ONLY Alice's trace (all of its spans) — Bob and Carol are
//! gone — so a caller finds the traces belonging to ONE identifier among many.

mod common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{
    poll_until, post_otlp, spawn_test_runtime_with, TestRuntime, SERVICE_NAME, TENANT_ACME,
    T_NANOS, T_SECONDS,
};
use kaleidoscope_runtime::ConsolidatedConfig;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;

/// The span-attribute key the caller searches by (a dotted key, carried
/// verbatim).
const ATTR_KEY: &str = "customer.id";

/// Three customers, one trace each (Alice's has two spans, only the root
/// tagged — so "all of the matching trace's spans" is observable live).
const ALICE_TRACE_BYTES: [u8; 16] = [0xA1; 16];
const ALICE_TRACE_HEX: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const BOB_TRACE_BYTES: [u8; 16] = [0xB2; 16];
const BOB_TRACE_HEX: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
const CAROL_TRACE_BYTES: [u8; 16] = [0xC3; 16];
const CAROL_TRACE_HEX: &str = "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

fn scope() -> InstrumentationScope {
    InstrumentationScope {
        name: "kaleidoscope.consolidated.test".to_string(),
        version: "0.0.0".to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    }
}

/// One server span on `trace`, service [`SERVICE_NAME`], around `T_NANOS`,
/// carrying `customer.id=customer` as a SPAN attribute iff `customer` is `Some`.
fn span(trace: [u8; 16], span_byte: u8, name: &str, customer: Option<&str>) -> Span {
    let attributes = customer
        .map(|value| vec![string_kv(ATTR_KEY, value)])
        .unwrap_or_default();
    Span {
        trace_id: trace.to_vec(),
        span_id: vec![span_byte; 8],
        trace_state: String::new(),
        parent_span_id: vec![],
        flags: 0,
        name: name.to_string(),
        kind: 2, // SPAN_KIND_SERVER
        start_time_unix_nano: T_NANOS,
        end_time_unix_nano: T_NANOS + 1_000,
        attributes,
        dropped_attributes_count: 0,
        events: vec![],
        dropped_events_count: 0,
        links: vec![],
        dropped_links_count: 0,
        status: None,
    }
}

/// An OTLP export body carrying `spans` for service [`SERVICE_NAME`].
fn encode_spans(spans: Vec<Span>) -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// GET the traces listing over loopback, narrowed by an attribute key/value.
async fn get_traces_by_attribute(addr: SocketAddr, key: &str, value: &str) -> (u16, String) {
    let start = T_SECONDS - 3_600;
    let end = T_SECONDS + 3_600;
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/v1/traces?service={SERVICE_NAME}&start={start}&end={end}&attr_key={key}&attr_value={value}"
        ))
        .send()
        .await
        .expect("GET traces over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read traces body");
    (status, body)
}

/// GET the UNFILTERED traces listing over loopback (no attribute params).
async fn get_traces_unfiltered(addr: SocketAddr) -> (u16, String) {
    let start = T_SECONDS - 3_600;
    let end = T_SECONDS + 3_600;
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/v1/traces?service={SERVICE_NAME}&start={start}&end={end}"
        ))
        .send()
        .await
        .expect("GET traces over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read traces body");
    (status, body)
}

/// The distinct trace_id strings in a bare span-array body.
fn trace_ids(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|spans| {
            spans
                .iter()
                .filter_map(|s| s["trace_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A fresh, empty pillar root under the OS temp dir, unique per call.
fn fresh_pillar_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "kal-consolidated-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("mkdir pillar root");
    path
}

/// Spawn a consolidated runtime on EPHEMERAL ports with the demo overlay OFF, so
/// the routers read EXACTLY the raw ingested data (no synthetic demo).
async fn spawn_raw_runtime(label: &str, tenant: &str) -> TestRuntime {
    let pillar_root = fresh_pillar_root(label);
    let mut config = ConsolidatedConfig::for_ephemeral_test(pillar_root, tenant);
    config.demo_overlay_enabled = false;
    spawn_test_runtime_with(label, config).await
}

/// experimentable-stack-v0: the live `attr_key`/`attr_value` filter surfaces
/// ONLY the traces belonging to one identifier, reachable by service + time.
/// @driving_port @real-io
///
/// ```gherkin
/// Scenario: attr_key/attr_value returns only one customer's traces, live
///   Given the consolidated runtime is running for tenant "acme" (demo overlay off)
///   And Andrea sends OTLP spans for "checkout" on three traces,
///       one carrying customer.id=alice, one bob, one carol
///   And all three traces are visible on the unfiltered listing
///   When Andrea GETs "/api/v1/traces?service=checkout&...&attr_key=customer.id&attr_value=alice"
///   Then only Alice's trace comes back, with all of its spans
///   And Bob's and Carol's traces are absent
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn attribute_filter_surfaces_only_one_customers_traces_live() {
    let rt = spawn_raw_runtime("attr-filter", TENANT_ACME).await;

    // Alice's trace: two spans, only the root carries customer.id=alice.
    assert_eq!(
        post_otlp(
            &rt.ingest_http_base(),
            "traces",
            encode_spans(vec![
                span(ALICE_TRACE_BYTES, 0x01, "alice-place-order", Some("alice")),
                span(ALICE_TRACE_BYTES, 0x02, "alice-charge-card", None),
            ]),
        )
        .await,
        200,
        "Alice's two spans are ingested"
    );
    assert_eq!(
        post_otlp(
            &rt.ingest_http_base(),
            "traces",
            encode_spans(vec![span(
                BOB_TRACE_BYTES,
                0x03,
                "bob-place-order",
                Some("bob")
            )]),
        )
        .await,
        200,
        "Bob's span is ingested"
    );
    assert_eq!(
        post_otlp(
            &rt.ingest_http_base(),
            "traces",
            encode_spans(vec![span(
                CAROL_TRACE_BYTES,
                0x04,
                "carol-place-order",
                Some("carol")
            )]),
        )
        .await,
        200,
        "Carol's span is ingested"
    );

    // Poll the UNFILTERED listing until ALL THREE traces are visible. This
    // proves Bob and Carol ARE in the store, so the subsequent attribute
    // exclusion is genuine, not a not-yet-ingested false green.
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || get_traces_unfiltered(rt.traces_addr()),
        |s, b| {
            let ids = trace_ids(b);
            s == 200
                && ids.iter().any(|id| id == ALICE_TRACE_HEX)
                && ids.iter().any(|id| id == BOB_TRACE_HEX)
                && ids.iter().any(|id| id == CAROL_TRACE_HEX)
        },
    )
    .await;
    assert_eq!(
        status, 200,
        "all three traces become visible on the unfiltered listing; body: {body}"
    );

    // Now the filtered listing: ONLY Alice's trace comes back.
    let (status, body) = get_traces_by_attribute(rt.traces_addr(), ATTR_KEY, "alice").await;
    assert_eq!(
        status, 200,
        "the attribute-filtered listing answers 200; body: {body}"
    );

    let ids = trace_ids(&body);
    assert!(!ids.is_empty(), "Alice's trace is reachable; body: {body}");
    assert!(
        ids.iter().all(|id| id == ALICE_TRACE_HEX),
        "every returned span belongs to Alice's trace; body: {body}"
    );
    // All of Alice's spans come back — including the untagged child.
    assert_eq!(
        ids.len(),
        2,
        "both of Alice's spans come back; body: {body}"
    );
    assert!(
        body.contains("alice-place-order") && body.contains("alice-charge-card"),
        "Alice's tagged root AND untagged child are both present; body: {body}"
    );
    assert!(
        !body.contains(BOB_TRACE_HEX)
            && !body.contains(CAROL_TRACE_HEX)
            && !body.contains("bob-place-order")
            && !body.contains("carol-place-order"),
        "Bob's and Carol's traces are excluded by the attribute filter; body: {body}"
    );
}
