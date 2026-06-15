// Kaleidoscope log-query-api — trace_id query without a time window
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

//! The logs read path makes the time window OPTIONAL when a `trace_id` is
//! supplied. A trace id is globally unique, so a by-id logs query needs no
//! time window — the same posture the traces `by_id` endpoint already takes.
//!
//! Outcome under test: a `trace_id`-only query (no `start`/`end`) returns
//! ONLY that trace's logs across all of time with NO window cap; an unmatched
//! id is the calm empty array; a `trace_id` WITH a window still filters within
//! that window (the existing windowed PG2BYID path stays byte-unchanged);
//! supplying NEITHER a window NOR a trace_id is a 400 that names what is
//! required; a malformed trace_id with no window is a redacted format 400; and
//! a partial window (start without end) is a 400 that names both bounds.
//!
//! These scenarios drive the real router via `oneshot` (no network port
//! bound) over a real durable `FileBackedLogStore`, mirroring slice 13.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{call, is_error_envelope, open_durable_store, records_array, seed, tenant};
use lumen::{LogRecord, LogStore, SeverityNumber};
use std::collections::BTreeMap;

// Two distinct trace ids, each in its raw-bytes and 32-hex-lowercase form.
const TRACE_A_BYTES: [u8; 16] = [
    0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
];
const TRACE_A_HEX: &str = "0a0b0c0d0e0f10111213141516171819";
const TRACE_B_BYTES: [u8; 16] = [
    0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
];
const TRACE_B_HEX: &str = "aabbccddeeff00112233445566778899";

/// A log record carrying a known trace_id, observed at `secs`, tagged with a
/// distinguishing `body` so a scenario can assert WHICH records survived.
fn log_with_ids(secs: u64, trace: Option<[u8; 16]>, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: secs * 1_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: trace,
        span_id: None,
    }
}

/// GET the logs endpoint with ONLY a `trace_id` — no `start`/`end` at all.
fn logs_trace_only_request(trace_id: &str) -> Request<Body> {
    let uri = format!("/api/v1/logs?trace_id={trace_id}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// GET the logs endpoint with both a window and a `trace_id`.
fn logs_window_trace_request(start: &str, end: &str, trace_id: &str) -> Request<Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}&trace_id={trace_id}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// The `trace_id` strings of the returned records, in order.
fn returned_trace_ids(body: &serde_json::Value) -> Vec<String> {
    records_array(body)
        .iter()
        .map(|r| {
            r["trace_id"]
                .as_str()
                .expect("each returned record carries a hex trace_id string")
                .to_string()
        })
        .collect()
}

/// The `body` strings of the returned records, in order.
fn returned_bodies(body: &serde_json::Value) -> Vec<String> {
    records_array(body)
        .iter()
        .map(|r| r["body"].as_str().expect("body is a string").to_string())
        .collect()
}

// =====================================================================
// a. trace_id ONLY, no window -> 200 with ONLY that trace's logs, across
//    a span far wider than the window cap (proves no window/cap applied)
// =====================================================================

/// @driving_port @real-io
///
/// Given a tenant has two logs under trace A separated by far more than the
/// 86_400-second window cap, plus one log under trace B,
/// When the operator GETs the logs endpoint with ONLY trace A's id and no
/// start/end,
/// Then status is 200 and BOTH trace-A logs are returned (no window narrowed
/// the span, no cap rejected it) and the trace-B log never appears.
#[tokio::test]
async fn a_trace_id_only_query_returns_that_trace_across_all_of_time() {
    let (store, _base) = open_durable_store("nowindow-headline");
    let t = tenant("acme-prod");
    // Two trace-A records nearly two years apart: the span dwarfs the
    // 86_400s window cap, so any default-window or cap would drop one.
    seed(
        &store,
        &t,
        vec![
            log_with_ids(1_600_000_000, Some(TRACE_A_BYTES), "a-early"),
            log_with_ids(1_660_000_000, Some(TRACE_A_BYTES), "a-late"),
            log_with_ids(1_630_000_000, Some(TRACE_B_BYTES), "b-only"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_trace_only_request(TRACE_A_HEX);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "trace-id-only needs no window: {body}"
    );
    let trace_ids = returned_trace_ids(&body);
    assert_eq!(
        trace_ids.len(),
        2,
        "both trace-A logs survive with no window or cap: {body}"
    );
    assert!(
        trace_ids.iter().all(|id| id == TRACE_A_HEX),
        "every returned record carries trace A's id: {body}"
    );
    assert!(
        !trace_ids.iter().any(|id| id == TRACE_B_HEX),
        "no trace-B record leaks through: {body}"
    );
    let bodies = returned_bodies(&body);
    assert!(
        bodies.contains(&"a-early".to_string()) && bodies.contains(&"a-late".to_string()),
        "both the early and the late trace-A logs are present: {body}"
    );
}

// =====================================================================
// b. trace_id ONLY, no window, no match -> calm empty array
// =====================================================================

/// @driving_port @real-io
///
/// Given the seed carries only trace-A and trace-B logs,
/// When the operator GETs with ONLY a valid 32-hex id that no record carries
/// and no window,
/// Then status is 200 with an EMPTY array — never a 404 or 500.
#[tokio::test]
async fn a_trace_id_only_query_matching_nothing_is_the_calm_empty_array() {
    let (store, _base) = open_durable_store("nowindow-empty");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            log_with_ids(1_600_000_000, Some(TRACE_A_BYTES), "a-early"),
            log_with_ids(1_630_000_000, Some(TRACE_B_BYTES), "b-only"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let unmatched = "11112222333344445555666677778888";
    let request = logs_trace_only_request(unmatched);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "an unmatched id is a calm 200: {body}"
    );
    assert!(
        records_array(&body).is_empty(),
        "the body is the empty array, not an error: {body}"
    );
}

// =====================================================================
// c. trace_id + window -> still filters WITHIN the window (PG2BYID stays)
// =====================================================================

/// @driving_port @real-io
///
/// Given two trace-A logs, one inside a narrow window and one well outside it,
/// When the operator GETs with that window AND trace A's id,
/// Then status is 200 and ONLY the in-window trace-A log is returned — the
/// windowed by-id path is byte-unchanged.
#[tokio::test]
async fn a_trace_id_with_a_window_still_filters_within_that_window() {
    let (store, _base) = open_durable_store("nowindow-windowed");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            log_with_ids(1_716_200_010, Some(TRACE_A_BYTES), "a-in-window"),
            log_with_ids(1_600_000_000, Some(TRACE_A_BYTES), "a-out-of-window"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_window_trace_request("1716200000", "1716200060", TRACE_A_HEX);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let bodies = returned_bodies(&body);
    assert_eq!(
        bodies,
        vec!["a-in-window".to_string()],
        "only the in-window trace-A log survives the window: {body}"
    );
}

// =====================================================================
// d. neither trace_id nor window -> 400 naming what is required
// =====================================================================

/// @driving_port @real-io
///
/// Given any seed,
/// When the operator GETs with NEITHER a window NOR a trace_id,
/// Then status is 400, the body is the error envelope, the reason names BOTH
/// the trace_id and the window as the alternatives, and it does NOT echo a raw
/// query string.
#[tokio::test]
async fn neither_window_nor_trace_id_is_a_400_naming_what_is_required() {
    let (store, _base) = open_durable_store("nowindow-neither");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![log_with_ids(1_600_000_000, Some(TRACE_A_BYTES), "a-early")],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/logs")
        .body(Body::empty())
        .expect("build request");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        is_error_envelope(&body),
        "the body is the error envelope: {body}"
    );
    let reason = body["error"].as_str().expect("error text is a string");
    assert!(
        reason.contains("trace_id"),
        "the reason names trace_id as an alternative: {reason}"
    );
    assert!(
        reason.contains("window"),
        "the reason names the window as an alternative: {reason}"
    );
    assert!(
        !reason.contains("/api/v1/logs"),
        "the reason never echoes the raw query: {reason}"
    );
}

// =====================================================================
// e. malformed trace_id, no window -> redacted format 400
// =====================================================================

/// @driving_port @real-io
///
/// Given any seed,
/// When the operator GETs with a malformed trace_id (non-hex) and no window,
/// Then status is 400, the body is the error envelope, the reason names the
/// expected 32-character hex format, and it never echoes the raw value.
#[tokio::test]
async fn a_malformed_trace_id_with_no_window_is_a_redacted_format_400() {
    let (store, _base) = open_durable_store("nowindow-malformed");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![log_with_ids(1_600_000_000, Some(TRACE_A_BYTES), "a-early")],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    // 32 characters, but the final character `g` is not a hex digit.
    let non_hex = "0a0b0c0d0e0f1011121314151617181g";
    assert_eq!(non_hex.len(), 32, "the invalid value is the right length");
    let request = logs_trace_only_request(non_hex);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        is_error_envelope(&body),
        "the body is the error envelope: {body}"
    );
    let reason = body["error"].as_str().expect("error text is a string");
    assert!(
        reason.contains("32"),
        "the reason names the length: {reason}"
    );
    assert!(
        reason.contains("hex"),
        "the reason names the encoding: {reason}"
    );
    assert!(
        !reason.contains(non_hex),
        "the reason never echoes the raw invalid value: {reason}"
    );
}

// =====================================================================
// f. partial window (start only, no end, no trace_id) -> 400 naming both
// =====================================================================

/// @driving_port @real-io
///
/// Given any seed,
/// When the operator GETs with only `start` (no `end`, no trace_id),
/// Then status is 400, the body is the error envelope, and the reason names
/// that BOTH start and end are required.
#[tokio::test]
async fn a_partial_window_with_only_start_is_a_400_naming_both_bounds() {
    let (store, _base) = open_durable_store("nowindow-partial");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![log_with_ids(1_600_000_000, Some(TRACE_A_BYTES), "a-early")],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/logs?start=1716200000")
        .body(Body::empty())
        .expect("build request");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        is_error_envelope(&body),
        "the body is the error envelope: {body}"
    );
    let reason = body["error"].as_str().expect("error text is a string");
    assert!(
        reason.contains("start"),
        "the reason names start as required: {reason}"
    );
    assert!(
        reason.contains("end"),
        "the reason names end as required: {reason}"
    );
}
