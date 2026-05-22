// Kaleidoscope log-query-api — slice 01 logs-read acceptance suite
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

//! Logs read path — the operator reads a tenant's in-window logs over HTTP.
//!
//! Maps to `docs/feature/lumen-query-api-v0/discuss/user-stories.md`.
//! Stories: US-01 (in-window read + ordering + field fidelity), US-02
//! (calm empty), US-03 (tenant scoping, fail-closed), US-04 (bad window
//! 400 + store failure 500 + redaction). Contract pinned by ADR-0047.
//!
//! The user-centric outcome: Sara, on-call SRE for tenant "acme-prod",
//! GETs `/api/v1/logs?start=&end=` and finally READS the logs the gateway
//! has been writing into Lumen for weeks, scoped to her tenant, over a
//! window. Empty reads calm (200 `[]`), a fat-fingered window reads as a
//! 400 that names the fault, and a backend failure reads as a 500 that
//! never fabricates an empty success.
//!
//! Every scenario drives log-query-api through its single public driving
//! port `log_query_api::router(store, tenant)` via `oneshot` against a
//! REAL durable `FileBackedLogStore` (the success and isolation arms) or
//! a failing store double (the 500 arm).
//!
//! RED state (DISTILL Mandate 7): the lib ships a scaffold whose handler
//! is `unimplemented!`, so every scenario that sends a request fails RED
//! (the handler panics), never BROKEN (a compile error). The suite
//! COMPILES against the real lumen + axum + tower surfaces and asserts the
//! real ADR-0047 contract. DELIVER implements the handler and enables the
//! scenarios one at a time.
//!
//! One-at-a-time outer loop: the walking skeleton is enabled; every
//! following scenario is `#[ignore]`d and gets enabled one at a time as
//! the crafter drives each inward.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, logs_request, logs_request_with_auth, observed_times,
    open_durable_store, record, record_at_nanos, record_bodies, records_array, rich_record,
    secs_to_nanos, seed, tenant, FailingLogStore,
};
use lumen::{LogStore, SeverityNumber};

// =====================================================================
// US-01 — Walking skeleton: the operator reads the in-window logs
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has six log records seeded into a REAL
/// durable Lumen store, three inside the window [1716200000s, 1716200060s)
/// and three outside it (two earlier, one later),
/// When the operator GETs the logs endpoint for "acme-prod" over
/// [1716200000, 1716200060),
/// Then she sees exactly the three in-window records, in ascending
/// observed_time order, with their bodies intact and no out-of-window
/// record present.
///
/// This is the demo-able outcome of the slice: logs that were written and
/// invisible are finally readable, scoped to the tenant, over a window. It
/// seeds REAL durable storage (the same `FileBackedLogStore` adapter the
/// gateway writes through) so the skeleton proves wiring, the
/// `LogStore::query` read, the half-open window, and the bare-array shape
/// end to end.
#[tokio::test]
async fn operator_reads_the_in_window_logs_for_a_tenant() {
    let (store, _base) = open_durable_store("ws-in-window-read");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![
            // Two before the window.
            record(1_716_199_900, "checkout", "before: warming up"),
            record(1_716_199_990, "checkout", "before: still early"),
            // Three inside [1716200000, 1716200060).
            record(1_716_200_005, "checkout", "checkout: started"),
            record(1_716_200_030, "checkout", "checkout: slow upstream"),
            record(1_716_200_055, "checkout", "checkout: payment timeout"),
            // One after the window.
            record(1_716_200_100, "checkout", "after: too late"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        record_bodies(&body),
        vec![
            "checkout: started".to_string(),
            "checkout: slow upstream".to_string(),
            "checkout: payment timeout".to_string(),
        ],
        "exactly the three in-window records, in ascending observed_time order"
    );
    let times = observed_times(&body);
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(
        times, sorted,
        "records are in ascending observed_time order"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("before:") && !rendered.contains("after:"),
        "no out-of-window record appears: {rendered}"
    );
}

// =====================================================================
// US-01 — The half-open window includes start and excludes end (boundary)
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given tenant "acme-prod" has one record at exactly the window start and
/// one at exactly the window end,
/// When the operator GETs the logs endpoint over [start, end),
/// Then only the record at start is returned: the half-open window
/// includes start and excludes end.
#[tokio::test]
async fn the_half_open_window_includes_start_and_excludes_end() {
    let (store, _base) = open_durable_store("half-open-boundary");
    let t = tenant("acme-prod");
    let start_ns = secs_to_nanos(1_716_200_000);
    let end_ns = secs_to_nanos(1_716_200_060);
    seed(
        &store,
        &t,
        vec![
            record_at_nanos(start_ns, "checkout", "exactly at start"),
            record_at_nanos(end_ns, "checkout", "exactly at end"),
        ],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        record_bodies(&body),
        vec!["exactly at start".to_string()],
        "the record at start is included, the record at end is excluded"
    );
}

// =====================================================================
// US-01 — Every LogRecord field round-trips faithfully (boundary)
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given tenant "acme-prod" has one record carrying a body, severity text
/// and number, record attributes, resource attributes, and a populated
/// trace id and span id,
/// When the operator GETs the logs endpoint over a covering window,
/// Then the response carries every field of the record, none dropped or
/// renamed.
#[tokio::test]
async fn every_log_record_field_round_trips_in_the_response() {
    let (store, _base) = open_durable_store("field-fidelity");
    let t = tenant("acme-prod");
    seed(&store, &t, vec![rich_record(1_716_200_005)]);

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let records = records_array(&body);
    assert_eq!(records.len(), 1, "the single covering record is returned");
    let r = &records[0];
    assert_eq!(
        r["observed_time_unix_nano"].as_u64(),
        Some(secs_to_nanos(1_716_200_005))
    );
    assert_eq!(
        r["severity_number"].as_i64(),
        Some(SeverityNumber::ERROR.0 as i64)
    );
    assert_eq!(r["severity_text"], "ERROR");
    assert_eq!(r["body"], "db pool exhausted");
    assert_eq!(r["attributes"]["http.status_code"], "503");
    assert_eq!(r["resource_attributes"]["service.name"], "checkout");
    assert!(
        !r["trace_id"].is_null(),
        "the populated trace id is carried: {r}"
    );
    assert!(
        !r["span_id"].is_null(),
        "the populated span id is carried: {r}"
    );
}

// =====================================================================
// US-02 — A window with no logs returns a calm empty array (happy path)
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has logs but none whose observed_time falls in
/// the requested window,
/// When the operator GETs the logs endpoint over that empty window,
/// Then the status is 200 and the result is an empty array, not an error.
#[tokio::test]
async fn a_window_with_no_logs_returns_a_calm_empty_array() {
    let (store, _base) = open_durable_store("calm-empty-window");
    let t = tenant("acme-prod");
    // The earliest record is well after the queried window.
    seed(
        &store,
        &t,
        vec![record(1_716_300_000, "checkout", "much later")],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(t));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        records_array(&body).is_empty(),
        "an empty window is a calm empty array: {body}"
    );
    assert!(!is_error_envelope(&body), "empty is never an error: {body}");
}

// =====================================================================
// US-02 — An unknown tenant returns the empty array, not an error (edge)
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given the endpoint resolves tenant "globex-prod" which has never had
/// logs written,
/// When the operator GETs the logs endpoint over any window,
/// Then the status is 200 and the result is an empty array.
#[tokio::test]
async fn an_unknown_tenant_returns_the_empty_array_not_an_error() {
    // A real durable store that has never seen "globex-prod"; the store
    // returns Ok(Vec::new()) for an unknown tenant (store.rs:145).
    let (store, _base) = open_durable_store("calm-empty-unknown-tenant");
    let router = log_query_api::router(
        store as Arc<dyn LogStore + Send + Sync>,
        Some(tenant("globex-prod")),
    );
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        records_array(&body).is_empty(),
        "an unknown tenant yields the empty array: {body}"
    );
}

// =====================================================================
// US-03 — A query returns only the resolved tenant's records (happy path)
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given the endpoint resolves tenant "acme-prod" and the durable store
/// holds in-window logs for BOTH "acme-prod" and "globex-prod",
/// When the operator GETs the logs endpoint over the window,
/// Then the result contains only acme-prod's records and no globex-prod
/// record appears.
#[tokio::test]
async fn a_log_query_returns_only_the_resolved_tenants_records() {
    let (store, _base) = open_durable_store("tenant-isolation");
    let acme = tenant("acme-prod");
    let globex = tenant("globex-prod");
    seed(
        &store,
        &acme,
        vec![record(1_716_200_010, "checkout", "acme: checkout log")],
    );
    seed(
        &store,
        &globex,
        vec![record(
            1_716_200_020,
            "billing",
            "globex: secret billing log",
        )],
    );

    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, Some(acme));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        record_bodies(&body),
        vec!["acme: checkout log".to_string()],
        "only acme-prod's record is returned"
    );
    assert!(
        !body.to_string().contains("globex"),
        "no globex-prod record leaks across the tenant boundary: {body}"
    );
}

// =====================================================================
// US-03 — A request with no resolvable tenant is refused (boundary)
// =====================================================================

/// @driving_port @real-io @US-03
///
/// Given the endpoint has no configured tenant and the request carries no
/// tenant signal,
/// When the operator GETs the logs endpoint over any window,
/// Then the request is refused with a 401 error envelope and no log
/// records are returned.
#[tokio::test]
async fn a_request_with_no_resolvable_tenant_is_refused() {
    let (store, _base) = open_durable_store("fail-closed-no-tenant");
    // Seed records that MUST NOT leak: fail-closed means refused before
    // the store, so even a populated store returns nothing.
    seed(
        &store,
        &tenant("acme-prod"),
        vec![record(1_716_200_010, "checkout", "must not leak")],
    );

    // None models "no tenant resolvable" at the router seam.
    let router = log_query_api::router(store as Arc<dyn LogStore + Send + Sync>, None);
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no resolvable tenant is refused fail-closed"
    );
    assert!(
        is_error_envelope(&body),
        "the refusal is an error envelope: {body}"
    );
    assert!(
        !body.to_string().contains("must not leak"),
        "no records are returned when refused: {body}"
    );
}

// =====================================================================
// US-04 — An inverted window is rejected with no store query (error path)
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator submits a window where start is later than end,
/// When the endpoint validates the window,
/// Then the status is 400 with an error envelope, and (proven by a store
/// double that would panic if queried) no store query is run.
#[tokio::test]
async fn an_inverted_window_is_rejected_with_no_store_query() {
    // A failing store double: if the handler queried it, the 5xx arm would
    // fire instead of the 400. A 400 here proves the bad window is caught
    // BEFORE the store is touched.
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    // start (later) > end (earlier).
    let request = logs_request("1716200060", "1716200000");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an inverted window is a 400 caught before the store"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is an error envelope: {body}"
    );
}

// =====================================================================
// US-04 — A non-numeric window bound is rejected (error path)
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator submits a window with a non-numeric start bound,
/// When the endpoint validates the window,
/// Then the status is 400 with an error envelope.
#[tokio::test]
async fn a_non_numeric_window_bound_is_rejected() {
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request("notanumber", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        is_error_envelope(&body),
        "the rejection is an error envelope: {body}"
    );
}

// =====================================================================
// US-04 — A store read failure surfaces as a 500, never a fabricated empty
// =====================================================================

/// @infrastructure-failure @driving_port @US-04
///
/// Given the durable store fails to read for tenant "acme-prod" with a
/// persistence error,
/// When the operator GETs the logs endpoint over a valid window,
/// Then the status is a 500 server error with an error envelope, NOT a
/// fabricated empty array.
#[tokio::test]
async fn a_store_read_failure_surfaces_as_a_server_error_not_an_empty_result() {
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request("1716200000", "1716200060");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "a store failure is a 500, never a fabricated empty success"
    );
    assert!(
        is_error_envelope(&body),
        "the failure is an error envelope: {body}"
    );
    assert!(
        body.as_array().is_none(),
        "a store failure must never look like an empty array: {body}"
    );
}

// =====================================================================
// US-04 — A failure response never leaks a forwarded header value
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator's request carries a forwarded Authorization header
/// "Bearer SECRET" and the store fails to read,
/// When the endpoint returns an error response,
/// Then the error text does not contain "SECRET" or the header value.
#[tokio::test]
async fn a_failure_response_never_leaks_a_forwarded_header_value() {
    let store: Arc<dyn LogStore + Send + Sync> = Arc::new(FailingLogStore);
    let router = log_query_api::router(store, Some(tenant("acme-prod")));
    let request = logs_request_with_auth("1716200000", "1716200060", "Bearer SECRET");
    let (status, body) = call(router, request).await;

    // The exact error status is asserted elsewhere; here the point is the
    // redaction: whatever the error, it must not echo the credential.
    assert!(status.is_client_error() || status.is_server_error());
    let rendered = body.to_string();
    assert!(
        !rendered.contains("SECRET"),
        "the error text must never echo a forwarded credential: {rendered}"
    );
}
