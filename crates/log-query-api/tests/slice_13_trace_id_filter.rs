// Kaleidoscope log-query-api — trace_id query-filter on the logs read path
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

//! The logs read path gains an OPTIONAL `trace_id` query filter (PG-2). An
//! operator who has copied a `trace_id` out of one log (rendered as the
//! 32-char lowercase hex string by slice 12) can now hand it straight back
//! to `/api/v1/logs?trace_id=..` and receive ONLY that trace's logs for the
//! resolved tenant. The filter is the inverse of the hex rendering: the API
//! renders the id one way and now accepts the very same string back.
//!
//! Outcome under test: the filter narrows the tenant-scoped, in-window set
//! to one trace; a valid id that matches nothing is the calm empty array
//! (never a 404/500); a malformed id is a redacted 400 that names the
//! expected format and is refused BEFORE the store is touched; an absent
//! `trace_id` leaves the existing behaviour byte-unchanged; and the filter
//! never crosses the tenant boundary.
//!
//! These scenarios drive the real router via `oneshot` (no network port
//! bound) over a real durable `FileBackedLogStore`, mirroring slice 12.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{call, is_error_envelope, open_durable_store, records_array, seed, tenant};
use lumen::{LogRecord, LogStore, SeverityNumber};
use std::collections::BTreeMap;

// Two distinct trace ids, each in its raw-bytes and 32-hex-lowercase form.
// Sequential, distinct nibbles per byte so a high/low-nibble swap is
// observable. TRACE_B's hex is the byte-for-byte rendering of TRACE_B_BYTES.
const TRACE_A_BYTES: [u8; 16] = [
    0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
];
const TRACE_A_HEX: &str = "0a0b0c0d0e0f10111213141516171819";
const TRACE_B_BYTES: [u8; 16] = [
    0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
];
const TRACE_B_HEX: &str = "aabbccddeeff00112233445566778899";

// A covering window for every scenario: all seed records sit inside it.
const WINDOW_START: &str = "1716200000";
const WINDOW_END: &str = "1716200060";

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

/// Build the GET request with an explicit `trace_id` query parameter.
fn logs_trace_request(start: &str, end: &str, trace_id: &str) -> Request<Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}&trace_id={trace_id}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// Build the GET request WITHOUT a `trace_id` parameter (the prior contract).
fn logs_request(start: &str, end: &str) -> Request<Body> {
    let uri = format!("/api/v1/logs?start={start}&end={end}");
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

// =====================================================================
// a. The filter narrows the in-window set to ONLY the named trace
// =====================================================================

/// @driving_port @real-io @PG-2
///
/// Given a tenant has two logs under trace A and one log under trace B, all
/// inside the window,
/// When the operator GETs the logs endpoint filtering on trace A's 32-hex id,
/// Then status is 200 and ONLY the two trace-A logs are returned — every
/// returned record carries trace A's id and NONE carries trace B's id.
#[tokio::test]
async fn the_trace_id_filter_returns_only_logs_carrying_that_trace() {
    let (store, _base) = open_durable_store("trace-filter-a");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            log_with_ids(1_716_200_005, Some(TRACE_A_BYTES), "a-first"),
            log_with_ids(1_716_200_010, Some(TRACE_A_BYTES), "a-second"),
            log_with_ids(1_716_200_015, Some(TRACE_B_BYTES), "b-only"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_trace_request(WINDOW_START, WINDOW_END, TRACE_A_HEX);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let trace_ids = returned_trace_ids(&body);
    assert_eq!(
        trace_ids.len(),
        2,
        "only the two trace-A logs survive the filter: {body}"
    );
    assert!(
        trace_ids.iter().all(|id| id == TRACE_A_HEX),
        "every returned record carries trace A's id: {body}"
    );
    assert!(
        !trace_ids.iter().any(|id| id == TRACE_B_HEX),
        "no record carrying trace B's id leaks through: {body}"
    );
}

// =====================================================================
// b. A valid id that matches nothing is the calm empty array
// =====================================================================

/// @driving_port @real-io @PG-2
///
/// Given the same seed,
/// When the operator filters on a VALID 32-hex id that no record carries,
/// Then status is 200 with an EMPTY JSON array — never a 404 or a 500.
#[tokio::test]
async fn a_valid_trace_id_matching_no_record_is_the_calm_empty_array() {
    let (store, _base) = open_durable_store("trace-filter-empty");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            log_with_ids(1_716_200_005, Some(TRACE_A_BYTES), "a-first"),
            log_with_ids(1_716_200_015, Some(TRACE_B_BYTES), "b-only"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    // A well-formed 32-hex id carried by neither seed record.
    let unmatched = "11112222333344445555666677778888";
    let request = logs_trace_request(WINDOW_START, WINDOW_END, unmatched);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "an unmatched filter is a calm 200");
    assert!(
        records_array(&body).is_empty(),
        "the body is the empty array, not an error: {body}"
    );
}

// =====================================================================
// c. A malformed id is a redacted 400 that names the expected format
// =====================================================================

/// @driving_port @real-io @PG-2
///
/// Given the same seed,
/// When the operator filters on a 32-character value that contains a non-hex
/// character,
/// Then status is 400, the body is the error envelope, the error text names
/// the expected format (it contains "32" and "hex"), and it does NOT echo the
/// raw invalid value.
#[tokio::test]
async fn a_non_hex_trace_id_is_a_redacted_400_naming_the_format() {
    let (store, _base) = open_durable_store("trace-filter-nonhex");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![log_with_ids(1_716_200_005, Some(TRACE_A_BYTES), "a-first")],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    // 32 characters, but the final character `g` is not a hex digit.
    let non_hex = "0a0b0c0d0e0f1011121314151617181g";
    assert_eq!(non_hex.len(), 32, "the invalid value is the right length");
    let request = logs_trace_request(WINDOW_START, WINDOW_END, non_hex);
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

/// @driving_port @real-io @PG-2
///
/// Given the same seed,
/// When the operator filters on a value of the WRONG length (not 32 chars),
/// Then status is 400, the body is the error envelope, the error text names
/// the expected format (it contains "32" and "hex"), and it does NOT echo the
/// raw invalid value.
#[tokio::test]
async fn a_wrong_length_trace_id_is_a_redacted_400_naming_the_format() {
    let (store, _base) = open_durable_store("trace-filter-wronglen");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![log_with_ids(1_716_200_005, Some(TRACE_A_BYTES), "a-first")],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    // 31 characters: one short of the required 32.
    let short = "not-a-valid-trace-id-value-xxxx";
    assert_ne!(short.len(), 32, "the invalid value is the wrong length");
    let request = logs_trace_request(WINDOW_START, WINDOW_END, short);
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
        !reason.contains(short),
        "the reason never echoes the raw invalid value: {reason}"
    );
}

// =====================================================================
// d. An absent trace_id leaves the prior behaviour byte-unchanged
// =====================================================================

/// @driving_port @real-io @PG-2
///
/// Given the same seed of trace-A and trace-B logs,
/// When the operator GETs the logs endpoint with NO `trace_id` parameter,
/// Then the prior behaviour is unchanged: BOTH the trace-A and the trace-B
/// logs are returned (backward compatibility).
#[tokio::test]
async fn an_absent_trace_id_returns_every_in_window_log_unchanged() {
    let (store, _base) = open_durable_store("trace-filter-absent");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            log_with_ids(1_716_200_005, Some(TRACE_A_BYTES), "a-first"),
            log_with_ids(1_716_200_010, Some(TRACE_A_BYTES), "a-second"),
            log_with_ids(1_716_200_015, Some(TRACE_B_BYTES), "b-only"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request(WINDOW_START, WINDOW_END);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let trace_ids = returned_trace_ids(&body);
    assert_eq!(
        trace_ids.len(),
        3,
        "with no filter every in-window log is returned: {body}"
    );
    assert!(
        trace_ids.iter().any(|id| id == TRACE_A_HEX),
        "trace-A logs are present: {body}"
    );
    assert!(
        trace_ids.iter().any(|id| id == TRACE_B_HEX),
        "trace-B logs are present: {body}"
    );
}

// =====================================================================
// e. The filter never crosses the tenant boundary
// =====================================================================

/// @driving_port @real-io @PG-2
///
/// Given tenant X and tenant Y both have a log under the SAME trace A bytes
/// (in one durable store, distinguished only by tenant),
/// When a router resolved to tenant X filters on trace A,
/// Then ONLY tenant X's log is returned — tenant Y's log carrying the same id
/// is NEVER returned.
#[tokio::test]
async fn the_trace_id_filter_never_crosses_the_tenant_boundary() {
    let (store, _base) = open_durable_store("trace-filter-tenant");
    let tenant_x = tenant("acme-prod");
    let tenant_y = tenant("globex-prod");
    seed(
        &store,
        &tenant_x,
        vec![log_with_ids(1_716_200_005, Some(TRACE_A_BYTES), "x-log")],
    );
    seed(
        &store,
        &tenant_y,
        vec![log_with_ids(1_716_200_005, Some(TRACE_A_BYTES), "y-log")],
    );

    // The router is scoped to tenant X only.
    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(tenant_x));
    let request = logs_trace_request(WINDOW_START, WINDOW_END, TRACE_A_HEX);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let bodies: Vec<String> = records_array(&body)
        .iter()
        .map(|r| r["body"].as_str().expect("body is a string").to_string())
        .collect();
    assert_eq!(
        bodies,
        vec!["x-log".to_string()],
        "only tenant X's log: {body}"
    );
}
