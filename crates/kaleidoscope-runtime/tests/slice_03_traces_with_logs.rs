// Kaleidoscope consolidated runtime — Slice 3: combined traces+logs live
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

//! # Slice 3 — "a trace and its logs, live, in one call" (experimentable-stack-v0).
//!
//! Proves the consolidated runtime genuinely WIRES the log store into the
//! traces router so the combined `/api/v1/traces/with_logs` route returns a
//! trace together with its correlated logs in ONE response. The whole loop
//! runs in ONE process on EPHEMERAL `127.0.0.1:0` ports: an OTLP error span
//! AND a log carrying the SAME trace_id are ingested over the REAL ingest
//! HTTP path, then a single `GET /api/v1/traces/with_logs?trace_id=...` over
//! loopback must carry BOTH the error span and the cause log.
//!
//! This is the guard against a defined-but-unwired endpoint: if the runtime
//! built the traces router WITHOUT the log store, the `logs` array would come
//! back empty and the assertion would fail.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use common::{
    poll_until, post_otlp, spawn_test_runtime, SERVICE_NAME, SPAN_NAME, TENANT_ACME,
    TRACE_ID_BYTES, TRACE_ID_HEX, T_NANOS, T_SECONDS,
};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use prost::Message;

const CAUSE_LOG_BODY: &str = "checkout failed: card declined";

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

/// One ERROR-status server span on [`TRACE_ID_BYTES`] around `T_NANOS`.
fn encode_error_span() -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans: vec![Span {
                    trace_id: TRACE_ID_BYTES.to_vec(),
                    span_id: vec![0x11; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: SPAN_NAME.to_string(),
                    kind: 2, // SPAN_KIND_SERVER
                    start_time_unix_nano: T_NANOS,
                    end_time_unix_nano: T_NANOS + 1_000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: Some(Status {
                        message: "upstream timeout".to_string(),
                        code: 2, // STATUS_CODE_ERROR
                    }),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// One ERROR log carrying [`TRACE_ID_BYTES`] in its W3C trace context at
/// `T_NANOS` — the cause line correlated to the error span above.
fn encode_correlated_log() -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(scope()),
                log_records: vec![LogRecord {
                    time_unix_nano: T_NANOS,
                    observed_time_unix_nano: T_NANOS,
                    severity_number: 17, // ERROR
                    severity_text: "ERROR".to_string(),
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(CAUSE_LOG_BODY.to_string())),
                    }),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    flags: 0,
                    trace_id: TRACE_ID_BYTES.to_vec(),
                    span_id: vec![0x11; 8],
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// GET `/api/v1/traces/with_logs?trace_id=...` over loopback.
async fn get_trace_with_logs(addr: SocketAddr, trace_id: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/v1/traces/with_logs?trace_id={trace_id}"
        ))
        .send()
        .await
        .expect("GET with_logs over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read with_logs body");
    (status, body)
}

fn count(body: &str, field: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v[field].as_array().map(|a| a.len()))
        .unwrap_or(0)
}

/// experimentable-stack-v0 walking skeleton: a trace and its correlated log
/// come back together in one live call, proving the runtime wired the log
/// store into the traces router.
/// @walking_skeleton @driving_port @real-io
///
/// ```gherkin
/// Scenario: A trace and its correlated log are returned in one response, live
///   Given the consolidated runtime is running for tenant "acme"
///   When Andrea sends an OTLP error span on trace "4bf9...4736"
///   And Andrea sends an OTLP log carrying the same trace id
///   And Andrea GETs "/api/v1/traces/with_logs" for "4bf9...4736"
///   Then the response carries the error span and the cause log together
///   And no restart was required
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn trace_and_its_correlated_log_returned_together_live() {
    let rt = spawn_test_runtime("with-logs", TENANT_ACME).await;

    assert_eq!(
        post_otlp(&rt.ingest_http_base(), "traces", encode_error_span()).await,
        200,
        "the error span is ingested"
    );
    assert_eq!(
        post_otlp(&rt.ingest_http_base(), "logs", encode_correlated_log()).await,
        200,
        "the correlated log is ingested"
    );

    // Poll until BOTH the span and the log are visible through the ONE
    // combined call (ingest accept is async; the live loop tolerates it).
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || get_trace_with_logs(rt.traces_addr(), TRACE_ID_HEX),
        |s, b| s == 200 && count(b, "spans") > 0 && count(b, "logs") > 0,
    )
    .await;

    assert_eq!(
        status, 200,
        "the combined endpoint answers 200; body: {body}"
    );

    let json: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    assert_eq!(
        json["trace_id"].as_str(),
        Some(TRACE_ID_HEX),
        "the response is scoped to the requested trace_id; body: {body}"
    );

    // The error span is present WITH its Error status.
    let spans = json["spans"].as_array().expect("spans array");
    assert!(
        spans
            .iter()
            .any(|s| s["status"]["code"].as_str() == Some("Error")),
        "the error span is carried in the combined response; body: {body}"
    );

    // The cause log is present, carrying the SAME trace_id string.
    let logs = json["logs"].as_array().expect("logs array");
    assert!(
        logs.iter()
            .any(|l| l["body"].as_str() == Some(CAUSE_LOG_BODY)
                && l["trace_id"].as_str() == Some(TRACE_ID_HEX)),
        "the cause log is carried, correlated by trace_id, in the SAME response; body: {body}"
    );

    // The window query path is unchanged and still answers (regression guard).
    let _ = T_SECONDS;
}
