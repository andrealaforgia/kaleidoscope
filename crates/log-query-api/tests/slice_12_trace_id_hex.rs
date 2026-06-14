// Kaleidoscope log-query-api — trace_id/span_id hex rendering + correlation
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

//! The logs read path renders `trace_id` (and `span_id`) as lowercase hex
//! strings, the SAME shape the traces read path uses, so a log emitted
//! inside a span and its trace carry the IDENTICAL `trace_id` string and
//! correlation by id works (PG-2).
//!
//! Outcome under test: Sara pulls a log from `/api/v1/logs`, copies its
//! `trace_id`, and looks the trace up on the traces API — the two ids are
//! the same string, byte for byte. Before this fix the logs API emitted
//! `trace_id` as a JSON byte array (`[10, 11, ...]`) while the traces API
//! emitted a 32-hex string, so the ids never matched and correlation broke.
//!
//! These scenarios drive BOTH real routers via `oneshot` (no network port
//! bound): `log_query_api::router` over a real durable `FileBackedLogStore`,
//! and `trace_query_api::router` over a real `InMemoryTraceStore`.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{call, logs_request, open_durable_store, records_array, seed, tenant};
use lumen::{LogRecord, LogStore, SeverityNumber};
use ray::{
    InMemoryTraceStore, NoopRecorder, Span, SpanBatch, SpanId, SpanKind, SpanStatus, TraceId,
    TraceStore,
};

// The shared id under test, in its three forms: the raw 16 bytes, the
// expected 32-hex lowercase string, and (for the span) the raw 8 bytes
// with their expected 16-hex lowercase string. Sequential, distinct
// nibbles per byte so a high/low-nibble swap is observable.
const TRACE_BYTES: [u8; 16] = [
    0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
];
const TRACE_HEX: &str = "0a0b0c0d0e0f10111213141516171819";
const SPAN_BYTES: [u8; 8] = [0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21];
const SPAN_HEX: &str = "1a1b1c1d1e1f2021";

/// A log record carrying a known trace_id / span_id, observed at `secs`.
fn log_with_ids(secs: u64, trace: Option<[u8; 16]>, span: Option<[u8; 8]>) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: secs * 1_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: "request handled".to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: trace,
        span_id: span,
    }
}

/// A minimal span carrying the same `trace_id`, for the traces API.
fn span_with_trace(trace: [u8; 16], span: [u8; 8], start_secs: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Span {
        trace_id: TraceId(trace),
        span_id: SpanId(span),
        parent_span_id: None,
        name: "GET /checkout".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start_secs * 1_000_000_000,
        end_time_unix_nano: (start_secs + 1) * 1_000_000_000,
        status: SpanStatus::default(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

// =====================================================================
// The logs API renders a populated trace_id / span_id as lowercase hex
// =====================================================================

/// @driving_port @real-io @PG-2
///
/// Given a tenant has a log carrying a known 16-byte trace_id and 8-byte
/// span_id,
/// When the operator GETs the logs endpoint over a covering window,
/// Then `trace_id` is the 32-char lowercase hex string and `span_id` is
/// the 16-char lowercase hex string (NOT a JSON byte array).
#[tokio::test]
async fn the_logs_api_renders_trace_id_and_span_id_as_lowercase_hex() {
    let (store, _base) = open_durable_store("hex-render");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![log_with_ids(
            1_716_200_005,
            Some(TRACE_BYTES),
            Some(SPAN_BYTES),
        )],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let records = records_array(&body);
    assert_eq!(records.len(), 1, "the single covering record is returned");
    let r = &records[0];

    assert_eq!(
        r["trace_id"].as_str(),
        Some(TRACE_HEX),
        "trace_id is the 32-hex lowercase string, not a byte array: {r}"
    );
    assert_eq!(
        r["span_id"].as_str(),
        Some(SPAN_HEX),
        "span_id is the 16-hex lowercase string, not a byte array: {r}"
    );
}

/// @driving_port @real-io @PG-2
///
/// Given a log whose trace_id has leading zero bytes,
/// When the operator GETs the logs endpoint,
/// Then the rendered hex preserves the leading zeros at full 32 chars
/// (a full 16-byte id is always 32 hex chars).
#[tokio::test]
async fn the_logs_api_preserves_leading_zeros_in_the_trace_id_hex() {
    let trace = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01,
    ];
    let (store, _base) = open_durable_store("hex-leading-zeros");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![log_with_ids(1_716_200_005, Some(trace), None)],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (_status, body) = call(router, request).await;
    let r = &records_array(&body)[0];

    assert_eq!(
        r["trace_id"].as_str(),
        Some("00000000000000000000000000000001"),
        "leading zeros preserved, full 32 chars: {r}"
    );
}

/// @driving_port @real-io @PG-2
///
/// Given a log with NO trace context,
/// When the operator GETs the logs endpoint,
/// Then `trace_id` and `span_id` are JSON null (absent context stays
/// absent, it is never rendered as an empty or zero hex string).
#[tokio::test]
async fn the_logs_api_renders_an_absent_trace_id_as_null() {
    let (store, _base) = open_durable_store("hex-absent");
    let t = tenant("acme-prod");
    seed(&store, &t, vec![log_with_ids(1_716_200_005, None, None)]);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (_status, body) = call(router, request).await;
    let r = &records_array(&body)[0];

    assert!(r["trace_id"].is_null(), "absent trace_id is null: {r}");
    assert!(r["span_id"].is_null(), "absent span_id is null: {r}");
}

// =====================================================================
// Correlation: a log inside a span and its trace share the IDENTICAL
// trace_id string across the two APIs
// =====================================================================

/// @driving_port @real-io @PG-2
///
/// Given the SAME 16-byte trace_id is carried by a log (in Lumen) and by
/// a span (in Ray),
/// When the operator GETs the log from the logs API and the trace from
/// the traces API,
/// Then both APIs return the EXACT SAME trace_id string — so an operator
/// can copy the id from a log and look up its trace by id.
#[tokio::test]
async fn a_log_and_its_trace_carry_the_identical_trace_id_string() {
    let t = tenant("acme-prod");

    // The logs side: ingest a log carrying the trace_id, GET it back.
    let (log_store, _base) = open_durable_store("correlation-logs");
    seed(
        &log_store,
        &t,
        vec![log_with_ids(
            1_716_200_005,
            Some(TRACE_BYTES),
            Some(SPAN_BYTES),
        )],
    );
    let log_router = log_query_api::router(
        log_store as Arc<dyn LogStore + Send + Sync>,
        Some(t.clone()),
    );
    let (_s1, log_body) = call(log_router, logs_request("1716200000", "1716200060")).await;
    let log_trace_id = records_array(&log_body)[0]["trace_id"]
        .as_str()
        .expect("logs API trace_id is a string")
        .to_string();

    // The traces side: ingest a span carrying the SAME trace_id, GET it by id.
    let trace_store = Arc::new(InMemoryTraceStore::new(Box::new(NoopRecorder)));
    trace_store
        .ingest(
            &t,
            SpanBatch::with_spans(vec![span_with_trace(
                TRACE_BYTES,
                SPAN_BYTES,
                1_716_200_005,
            )]),
        )
        .expect("seed trace store");
    let trace_router = trace_query_api::router(
        trace_store as Arc<dyn TraceStore + Send + Sync>,
        Some(t.clone()),
    );
    let by_id = format!("/api/v1/traces/by_id?trace_id={log_trace_id}");
    let (_s2, trace_body) = call(
        trace_router,
        Request::builder()
            .method("GET")
            .uri(by_id)
            .body(Body::empty())
            .expect("build request"),
    )
    .await;
    let trace_trace_id = trace_body.as_array().expect("traces body is a bare array")[0]["trace_id"]
        .as_str()
        .expect("traces API trace_id is a string")
        .to_string();

    assert_eq!(
        log_trace_id, trace_trace_id,
        "a log and its trace must carry the IDENTICAL trace_id string"
    );
    assert_eq!(
        log_trace_id, TRACE_HEX,
        "and that shared string is the OTel 32-hex lowercase form"
    );
}
