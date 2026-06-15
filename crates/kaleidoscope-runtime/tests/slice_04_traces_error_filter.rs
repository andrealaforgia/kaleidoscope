// Kaleidoscope consolidated runtime — Slice 4: live traces error filter
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

//! # Slice 4 — "reach the failed trace, live" (experimentable-stack-v0).
//!
//! Proves the consolidated runtime serves the new `error=true` filter on the
//! traces listing END TO END. The whole loop runs in ONE process on EPHEMERAL
//! `127.0.0.1:0` ports: an OTLP ERROR span (the failed trace) AND a healthy
//! span on a DIFFERENT trace are ingested over the REAL ingest HTTP path for
//! the same service; once BOTH are visible on the unfiltered listing, a single
//! `GET /api/v1/traces?service=...&error=true` over loopback must carry ONLY
//! the failed trace's spans — the healthy trace is gone — so a newcomer reaches
//! the failed trace and tells it IS the failed one without opening every trace.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use common::{
    poll_until, post_otlp, spawn_test_runtime, SERVICE_NAME, TENANT_ACME, T_NANOS, T_SECONDS,
};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use prost::Message;

/// The failed trace's id (16 bytes / 32 hex chars) and its lowercase hex.
const FAILED_TRACE_BYTES: [u8; 16] = [0xAB; 16];
const FAILED_TRACE_HEX: &str = "abababababababababababababababab";
/// A healthy trace on a DIFFERENT id, same service, same window.
const HEALTHY_TRACE_BYTES: [u8; 16] = [0xCD; 16];
const HEALTHY_TRACE_HEX: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

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
/// with an Error status iff `error`.
fn encode_span(trace: [u8; 16], span_byte: u8, name: &str, error: bool) -> Vec<u8> {
    let status = error.then(|| Status {
        message: "upstream timeout".to_string(),
        code: 2, // STATUS_CODE_ERROR
    });
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans: vec![Span {
                    trace_id: trace.to_vec(),
                    span_id: vec![span_byte; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: name.to_string(),
                    kind: 2, // SPAN_KIND_SERVER
                    start_time_unix_nano: T_NANOS,
                    end_time_unix_nano: T_NANOS + 1_000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status,
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// GET the traces listing over loopback with an explicit `error` value.
async fn get_traces(addr: SocketAddr, error: &str) -> (u16, String) {
    let start = T_SECONDS - 3_600;
    let end = T_SECONDS + 3_600;
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/v1/traces?service={SERVICE_NAME}&start={start}&end={end}&error={error}"
        ))
        .send()
        .await
        .expect("GET traces over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read traces body");
    (status, body)
}

/// The set of distinct trace_id strings in a bare span-array body.
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

/// experimentable-stack-v0: the live `error=true` filter surfaces ONLY the
/// failed trace, reachable by service + time, distinguishable as failed.
/// @driving_port @real-io
///
/// ```gherkin
/// Scenario: error=true returns only the failed trace's spans, live
///   Given the consolidated runtime is running for tenant "acme"
///   And Andrea sends an OTLP error span on the failed trace for "checkout"
///   And Andrea sends an OTLP healthy span on a different trace for "checkout"
///   And both traces are visible on the unfiltered listing
///   When Andrea GETs "/api/v1/traces?service=checkout&...&error=true"
///   Then only the failed trace's spans come back, carrying Error status
///   And the healthy trace is absent
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn error_true_listing_surfaces_only_the_failed_trace_live() {
    let rt = spawn_test_runtime("error-filter", TENANT_ACME).await;

    assert_eq!(
        post_otlp(
            &rt.ingest_http_base(),
            "traces",
            encode_span(FAILED_TRACE_BYTES, 0x11, "place-order", true),
        )
        .await,
        200,
        "the failed trace's error span is ingested"
    );
    assert_eq!(
        post_otlp(
            &rt.ingest_http_base(),
            "traces",
            encode_span(HEALTHY_TRACE_BYTES, 0x22, "healthy-order", false),
        )
        .await,
        200,
        "the healthy trace's span is ingested"
    );

    // Poll the UNFILTERED listing until BOTH traces are visible. This proves
    // the healthy span IS in the store, so the subsequent error=true
    // exclusion is genuine, not a not-yet-ingested false green.
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || get_traces(rt.traces_addr(), "false"),
        |s, b| {
            let ids = trace_ids(b);
            s == 200
                && ids.iter().any(|id| id == FAILED_TRACE_HEX)
                && ids.iter().any(|id| id == HEALTHY_TRACE_HEX)
        },
    )
    .await;
    assert_eq!(
        status, 200,
        "both traces become visible on the unfiltered listing; body: {body}"
    );

    // Now the filtered listing: ONLY the failed trace's spans come back.
    let (status, body) = get_traces(rt.traces_addr(), "true").await;
    assert_eq!(
        status, 200,
        "the error=true listing answers 200; body: {body}"
    );

    let ids = trace_ids(&body);
    assert!(
        !ids.is_empty(),
        "the failed trace is reachable; body: {body}"
    );
    assert!(
        ids.iter().all(|id| id == FAILED_TRACE_HEX),
        "every returned span belongs to the failed trace; body: {body}"
    );
    assert!(
        !body.contains(HEALTHY_TRACE_HEX) && !body.contains("healthy-order"),
        "the healthy trace is excluded by error=true; body: {body}"
    );

    // The failed trace is distinguishable AS failed: its Error status rides.
    let json: serde_json::Value = serde_json::from_str(&body).expect("body is a JSON array");
    let spans = json.as_array().expect("bare span array");
    assert!(
        spans
            .iter()
            .any(|s| s["status"]["code"].as_str() == Some("Error")),
        "the failed trace is told apart by its Error status; body: {body}"
    );
}
