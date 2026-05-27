// Kaleidoscope trace-query-api — slice 03 lookup-by-id acceptance suite
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

//! Trace lookup-by-id — `/api/v1/traces/by_id?trace_id=<32-hex>` reads
//! the spans of one trace for the resolved tenant, in one HTTP call.
//!
//! Maps to `docs/feature/trace-lookup-by-id-v0/discuss/user-stories.md`
//! (US-01 happy path; US-02 unknown trace_id is 200 `[]`; US-03 tenant
//! fail-closed 401; US-04 malformed trace_id 400 with no echo; US-05
//! cross-tenant isolation 200 `[]`). Contract pinned by ADR-0053
//! (`docs/product/architecture/adr-0053-trace-lookup-by-id.md`),
//! cap interaction by ADR-0050, envelope and redaction by ADR-0048.
//!
//! The user-centric outcome: Sara, on-call SRE for tenant "acme-prod",
//! holding a trace_id from a log line or an alert payload, GETs
//! `/api/v1/traces/by_id?trace_id=<32-hex>` and finally pivots from "I
//! have this trace_id" to "here are the spans of this trace for my
//! tenant" in ONE HTTP call, without naming a service or estimating a
//! window. The substrate seam `ray::TraceStore::get_trace(&tenant,
//! &trace_id)` already exists at `crates/ray/src/store.rs:72`; the
//! slice is parse + wire on the HTTP boundary.
//!
//! Every scenario drives trace-query-api through its single public
//! driving port `trace_query_api::router(store, tenant)` via tower
//! `oneshot` against a REAL durable `FileBackedTraceStore` (the seeded
//! scenarios) or a `FailingTraceStore` double (the no-store-call
//! arms). Mandate 1 (hexagonal boundary) and Mandate 3 (user-journey
//! completeness) are honoured.
//!
//! RED state (DISTILL Mandate 7): the lib ships a scaffold whose
//! `handle_traces_by_id` is `unimplemented!`, so every scenario that
//! sends a request fails RED with the panic message
//! `__SCAFFOLD__ trace-lookup-by-id-v0 RED`, never BROKEN (a compile
//! error). The suite COMPILES against the real ray + axum + tower
//! surfaces and asserts the real ADR-0053 contract. DELIVER lands the
//! parse + wire and the scenarios go green one at a time per the
//! outer-loop convention.
//!
//! One-at-a-time outer loop: the walking skeleton (AC-01) is enabled;
//! every following scenario is `#[ignore]`d and gets enabled one at a
//! time as the crafter drives each inward.
//!
//! Result-cap (`MAX_RESULT_ROWS = 100_000`) note (ADR-0053 Decision 3):
//! the cap applies uniformly to the lookup arm. `MAX_RESULT_ROWS` is a
//! `pub const` in `trace_query_api::lib.rs:78` and is NOT
//! test-overridable cleanly; a 100_001-span fixture is too expensive
//! for the acceptance suite. The uniform interaction is verified
//! structurally at the parse-handler layer (the same `if spans.len() >
//! MAX_RESULT_ROWS` check the existing arm uses, ADR-0050 Decision 2)
//! and is covered by `--in-diff` mutation tests against
//! `slice_02_caps.rs` once DELIVER lands the handler body. An
//! `#[ignore]`d placeholder scenario at the end of this file documents
//! the decision in-line; ADR-0053 Decision 3 is the durable record.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, is_error_envelope, open_durable_store, rich_span, seed, span_with_ids, spans_array,
    tenant, FailingTraceStore,
};
use ray::TraceStore;

// ---------------------------------------------------------------------
// Local helper — build the GET request for the by-id endpoint.
//
// The slice 01 / slice 02 shared `traces_request(service, start, end)`
// helper in `common/mod.rs` deliberately targets the window arm; this
// slice's by-id URL shape is local to the lookup acceptance file so
// the shared helpers stay unchanged (matching the
// `log-query-api/tests/slice_03_severity_filter.rs` posture).
//
// The query parameter is `trace_id`; the value is passed through
// verbatim so a malformed value (non-hex, wrong length, empty) reaches
// the handler unaltered for the 400 arms to assert against.
// ---------------------------------------------------------------------

fn traces_by_id_request(trace_id: &str) -> axum::http::Request<axum::body::Body> {
    let uri = format!("/api/v1/traces/by_id?trace_id={trace_id}");
    axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Build a GET request with NO `trace_id` parameter at all, for the
/// missing-parameter 400 arm.
fn traces_by_id_request_without_trace_id() -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/traces/by_id")
        .body(axum::body::Body::empty())
        .expect("build request")
}

/// Seed `n` spans sharing one `trace_id` (all under the same service)
/// for the happy-path scenario. Each span carries a distinct
/// `span_byte` so the returned vector reads as three distinct spans of
/// one trace, in ascending `start_time` order.
fn seed_trace_with_spans(
    store: &Arc<ray::FileBackedTraceStore>,
    t: &aegis::TenantId,
    trace_byte: u8,
    base_secs: u64,
    n: usize,
) {
    let spans: Vec<ray::Span> = (0..n)
        .map(|i| {
            span_with_ids(
                base_secs + i as u64,
                "checkout",
                &format!("step-{i}"),
                trace_byte,
                0x10 + i as u8,
            )
        })
        .collect();
    seed(store, t, spans);
}

// =====================================================================
// AC-01 (US-01) — Walking skeleton: a known trace_id returns ALL its
// spans, scoped to the resolved tenant
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given tenant "acme-prod" has three spans persisted into a REAL
/// durable ray store, all sharing trace_id
/// "abc123abc123abc123abc123abc12345" (a 32-char lowercase-hex id),
/// When the on-call SRE GETs `/api/v1/traces/by_id?trace_id=<that id>`
/// for "acme-prod",
/// Then she sees exactly three spans in the response, all carrying the
/// requested trace_id, in ascending observed-time order, served from
/// the real durable store via `TraceStore::get_trace`.
///
/// This is the demo-able outcome of the slice: the operator pivots
/// from "I have a trace_id" to "here are the spans of this trace for
/// my tenant" in ONE HTTP call, without naming a service or
/// estimating a window. It exercises wiring, the new route, the
/// parse helper, the substrate seam, and the bare-array shape end to
/// end.
#[tokio::test]
async fn ac_01_known_trace_id_returns_all_spans() {
    let (store, _base) = open_durable_store("ws-known-trace-id");
    let t = tenant("acme-prod");
    seed_trace_with_spans(&store, &t, 0xAB, 1_716_200_005, 3);

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_by_id_request("abababababababababababababababab");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the known trace_id returns 200 with its spans: {body}"
    );
    let spans = spans_array(&body);
    assert_eq!(
        spans.len(),
        3,
        "exactly the three seeded spans of that trace are returned: {body}"
    );
    let times: Vec<u64> = spans
        .iter()
        .filter_map(|s| s["start_time_unix_nano"].as_u64())
        .collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(times, sorted, "the spans are in ascending start_time order");
    for span in spans {
        assert_eq!(
            span["trace_id"].as_str(),
            Some("abababababababababababababababab"),
            "every returned span carries the requested trace_id: {span}"
        );
    }
}

// =====================================================================
// AC-01 (US-01) — Field fidelity: every Span field round-trips on the
// lookup arm (no field is dropped, none is renamed)
// =====================================================================

/// @driving_port @real-io @US-01
///
/// Given tenant "acme-prod" has one rich span seeded under trace_id
/// `aa...aa` carrying `name`, `kind`, `status` (Error + message),
/// `attributes`, `resource_attributes`, a populated `parent_span_id`,
/// one event, and one link,
/// When the operator GETs the lookup endpoint with that trace_id,
/// Then the returned JSON object carries every Span field (`trace_id`,
/// `span_id`, `parent_span_id`, `name`, `kind`, `start_time_unix_nano`,
/// `end_time_unix_nano`, `status`, `attributes`, `resource_attributes`,
/// `events`, `links`) with no field dropped or renamed.
#[tokio::test]
async fn ac_01_known_trace_id_carries_every_span_field() {
    let (store, _base) = open_durable_store("ws-field-fidelity");
    let t = tenant("acme-prod");
    // `rich_span` uses trace_byte 0xAA so the trace_id is aa...aa
    // (32 hex chars). See `common/mod.rs` `rich_span` constructor.
    seed(&store, &t, vec![rich_span(1_716_200_005)]);

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_by_id_request("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK);
    let spans = spans_array(&body);
    assert_eq!(spans.len(), 1, "the single rich span is returned: {body}");
    let span = &spans[0];

    // Every documented Span field is carried with no rename. A missing
    // field on the returned span means the boundary dropped it; a
    // renamed field means the boundary mapped it (forbidden by
    // ADR-0048 Decision 2; the bare-array shape rides Span's own
    // Serialize derive).
    for field in [
        "trace_id",
        "span_id",
        "parent_span_id",
        "name",
        "kind",
        "start_time_unix_nano",
        "end_time_unix_nano",
        "status",
        "attributes",
        "resource_attributes",
        "events",
        "links",
    ] {
        assert!(
            span.get(field).is_some(),
            "the returned span carries field {field:?}: {span}"
        );
    }
    assert_eq!(
        span["name"].as_str(),
        Some("place-order"),
        "the name is unchanged: {span}"
    );
    assert_eq!(
        span["status"]["code"].as_str(),
        Some("Error"),
        "the status code round-trips: {span}"
    );
    assert_eq!(
        span["status"]["message"].as_str(),
        Some("upstream timeout"),
        "the status message round-trips: {span}"
    );
}

// =====================================================================
// AC-02 (US-02) — Unknown trace_id returns the calm empty arm (200 [],
// NEVER 404)
// =====================================================================

/// @driving_port @real-io @US-02
///
/// Given tenant "acme-prod" has no spans persisted under trace_id
/// "00000000000000000000000000000000",
/// When the operator GETs the lookup endpoint with that trace_id,
/// Then the response is HTTP 200 with the bare empty JSON array `[]`,
/// NEVER a 404 (the endpoint exists and is responding; the trace is
/// what was not found). The empty arm is the same shape ADR-0048
/// Decision 2 chose for the window arm; ADR-0053 Decision 1 honours it
/// on the lookup arm.
#[tokio::test]
async fn ac_02_unknown_trace_id_returns_empty_array() {
    let (store, _base) = open_durable_store("unknown-trace-id");
    let t = tenant("acme-prod");
    // Seed something else under a different trace_id so the store is
    // populated; the query is for a trace_id the store has never seen.
    seed_trace_with_spans(&store, &t, 0xFF, 1_716_200_005, 2);

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    let request = traces_by_id_request("00000000000000000000000000000000");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "an unknown trace_id is the calm empty arm, NEVER a 404: {body}"
    );
    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "the endpoint exists and is responding; 404 is the wrong code"
    );
    let spans = spans_array(&body);
    assert!(
        spans.is_empty(),
        "the body is the bare empty array []: {body}"
    );
    assert!(
        body.as_array().is_some(),
        "the empty arm is a bare JSON array, never an error envelope: {body}"
    );
}

// =====================================================================
// AC-03 (US-03) — No resolvable tenant: lookup refused (401, no store
// call, no leak of trace_id)
// =====================================================================

/// @driving_port @US-03
///
/// Given the endpoint has no configured tenant (`tenant = None`) and
/// the request carries a perfectly-formed trace_id,
/// When an unscoped caller GETs the lookup endpoint,
/// Then the response is HTTP 401 with the existing error envelope, the
/// `FailingTraceStore` double's `get_trace` was NEVER called (proven
/// by the clean 401 — a leaked call would lift the response to 500),
/// and the response body NEVER contains the raw trace_id value.
#[tokio::test]
async fn ac_03_missing_tenant_returns_401_with_no_store_call_and_no_leak() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, None);
    let request = traces_by_id_request("abcdef0123456789abcdef0123456789");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no resolvable tenant returns 401; the store is NEVER touched: {body}"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("abcdef0123456789abcdef0123456789"),
        "the body must never echo the raw trace_id value: {rendered}"
    );
}

// =====================================================================
// AC-04a (US-04) — Missing trace_id parameter: 400 with the literal
// class label "invalid trace_id", no store call
// =====================================================================

/// @driving_port @US-04
///
/// Given the request URL contains no `trace_id` parameter at all,
/// When the endpoint validates the request,
/// Then the response is HTTP 400 with the literal error envelope
/// `{"status":"error","error":"invalid trace_id"}` (ADR-0053 Decision
/// 2 collapses every malformed arm to one class label), and the
/// `FailingTraceStore` double's `get_trace` was NEVER called (a leaked
/// call would lift the response to 500).
#[tokio::test]
async fn ac_04a_missing_trace_id_returns_400() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_by_id_request_without_trace_id();
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a missing trace_id parameter is a 400, never a 500: {body}"
    );
    assert!(
        is_error_envelope(&body),
        "the rejection is the existing error envelope: {body}"
    );
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid trace_id",
        "the reason text is the single literal class label"
    );
}

// =====================================================================
// AC-04b (US-04) — Empty trace_id parameter: 400, no store call
// =====================================================================

/// @driving_port @US-04
///
/// Given the request URL carries `trace_id=` (empty string),
/// When the endpoint validates the trace_id,
/// Then the response is HTTP 400 with the same `"invalid trace_id"`
/// envelope (ADR-0053 Decision 2 treats empty as a class of the same
/// malformed-input fault), and the store is NEVER touched.
#[tokio::test]
async fn ac_04b_empty_trace_id_returns_400() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    let request = traces_by_id_request("");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an empty trace_id is a 400, never a 500: {body}"
    );
    assert!(is_error_envelope(&body));
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid trace_id",
        "the reason text is the single literal class label"
    );
}

// =====================================================================
// AC-04c (US-04) — Wrong-length trace_id (31 chars and 33 chars): 400
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator submits a trace_id of length 31 (one short of
/// the OTel-pinned 32-char width — a paste truncation),
/// When the endpoint validates the trace_id,
/// Then the response is HTTP 400 with `"invalid trace_id"`, and the
/// store is NEVER touched. Kills a `!= 32` -> `> 32` mutant on the
/// length check (per ADR-0053 / DEVOPS handoff mutation targets).
#[tokio::test]
async fn ac_04c_trace_id_31_chars_returns_400() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    // 31 hex characters — one short of the cap.
    let too_short = "0123456789abcdef0123456789abcde";
    assert_eq!(too_short.len(), 31, "fixture is 31 chars");
    let request = traces_by_id_request(too_short);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a 31-char trace_id is a 400: {body}"
    );
    assert!(is_error_envelope(&body));
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(message, "invalid trace_id");
}

/// @driving_port @US-04
///
/// Given the operator submits a trace_id of length 33 (one over the
/// OTel-pinned 32-char width — a stray-character paste),
/// When the endpoint validates the trace_id,
/// Then the response is HTTP 400 with `"invalid trace_id"`, and the
/// store is NEVER touched. Kills a `!= 32` -> `< 32` mutant on the
/// length check.
#[tokio::test]
async fn ac_04c_trace_id_33_chars_returns_400() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    // 33 hex characters — one over the cap.
    let too_long = "0123456789abcdef0123456789abcdef0";
    assert_eq!(too_long.len(), 33, "fixture is 33 chars");
    let request = traces_by_id_request(too_long);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a 33-char trace_id is a 400: {body}"
    );
    assert!(is_error_envelope(&body));
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(message, "invalid trace_id");
}

// =====================================================================
// AC-04d (US-04) — Non-hex character in trace_id: 400, no echo of the
// raw value
// =====================================================================

/// @driving_port @US-04
///
/// Given the operator submits a 32-character trace_id whose final
/// position is the letter `g` (NOT a hex digit; the substrate codec
/// at `ray::span::hex::decode::<16>` rejects any non-[0-9a-fA-F]
/// byte),
/// When the endpoint validates the trace_id,
/// Then the response is HTTP 400 with `"invalid trace_id"`, the
/// response body does NOT echo the raw trace_id value (redaction;
/// ADR-0053 Decision 2), and the store is NEVER touched.
#[tokio::test]
async fn ac_04d_non_hex_trace_id_returns_400_with_no_echo() {
    let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(FailingTraceStore);
    let router = trace_query_api::router(store, Some(tenant("acme-prod")));
    // 32 chars, last one is `g` (non-hex).
    let non_hex = "0123456789abcdef0123456789abcdeg";
    assert_eq!(non_hex.len(), 32, "fixture is 32 chars (one non-hex)");
    let request = traces_by_id_request(non_hex);
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a non-hex character in the trace_id is a 400: {body}"
    );
    assert!(is_error_envelope(&body));
    let message = body["error"].as_str().expect("error is a string");
    assert_eq!(
        message, "invalid trace_id",
        "the reason text is the single literal class label, NOT a clever diagnostic"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains(non_hex),
        "the body must never echo the raw trace_id value: {rendered}"
    );
    assert!(
        !rendered.contains("SECRET"),
        "the body must not contain SECRET anywhere: {rendered}"
    );
    assert!(
        !rendered.contains("Bearer"),
        "the body must not contain Bearer anywhere: {rendered}"
    );
}

// =====================================================================
// AC-04e (US-04) — Uppercase hex trace_id: 200 (case-insensitive
// accept; ADR-0053 Decision 2)
// =====================================================================

/// @driving_port @real-io @US-04
///
/// Given tenant "acme-prod" has three spans seeded under trace_id
/// `abababababababababababababababab` (32 lowercase hex chars),
/// When the operator GETs the lookup endpoint with the SAME trace_id
/// rendered entirely in uppercase (`ABABABABABABABABABABABABABABABAB`),
/// Then the response is HTTP 200 with the three seeded spans, because
/// the boundary's case-insensitive parse maps the uppercase form to
/// the same `TraceId` bytes as the lowercase form (matching the
/// substrate's hex codec, ADR-0053 Decision 2). Kills a mutant that
/// compares case-sensitively on the hex digits.
#[tokio::test]
async fn ac_04e_uppercase_trace_id_resolves_to_the_same_bytes() {
    let (store, _base) = open_durable_store("uppercase-trace-id");
    let t = tenant("acme-prod");
    // Seed under the lowercase form; query under the uppercase form.
    seed_trace_with_spans(&store, &t, 0xAB, 1_716_200_005, 3);

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(t));
    // Uppercase A and B map to the same nibbles as lowercase a and b.
    let request = traces_by_id_request("ABABABABABABABABABABABABABABABAB");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "uppercase hex is accepted case-insensitively: {body}"
    );
    let spans = spans_array(&body);
    assert_eq!(
        spans.len(),
        3,
        "the same three spans seeded under lowercase ab... are returned for the uppercase query: {body}"
    );
}

// =====================================================================
// AC-05 (US-05) — Cross-tenant isolation: a trace_id present under
// tenant A returns 200 `[]` when the endpoint resolves tenant B
// =====================================================================

/// @driving_port @real-io @US-05
///
/// Given tenant "acme-prod" has three spans persisted under trace_id
/// `cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd`, and tenant "globex-prod" has
/// NO spans under that trace_id,
/// When the endpoint resolving "globex-prod" GETs the lookup endpoint
/// with that trace_id,
/// Then the response is HTTP 200 with the bare empty JSON array `[]`,
/// and the body does NOT contain any acme-prod span name, attribute,
/// or identifier (cross-tenant isolation on the `(tenant, trace_id)`
/// key, ADR-0053 Decision 1 inheriting the substrate property at
/// `ray::store::InMemoryTraceStore::get_trace`).
#[tokio::test]
async fn ac_05_cross_tenant_isolation_on_the_lookup_arm() {
    let (store, _base) = open_durable_store("cross-tenant-isolation");
    let acme = tenant("acme-prod");
    let globex = tenant("globex-prod");
    // Seed under acme-prod; query under globex-prod.
    seed_trace_with_spans(&store, &acme, 0xCD, 1_716_200_005, 3);

    let router = trace_query_api::router(store as Arc<dyn TraceStore + Send + Sync>, Some(globex));
    let request = traces_by_id_request("cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the cross-tenant arm is 200, never a leak of acme-prod spans: {body}"
    );
    let spans = spans_array(&body);
    assert!(
        spans.is_empty(),
        "globex-prod sees the calm empty arm for a trace_id present only under acme-prod: {body}"
    );
    let rendered = body.to_string();
    // The seed used `name = "step-0", "step-1", "step-2"`. Verify
    // none of these names leaks across tenants.
    for name in ["step-0", "step-1", "step-2"] {
        assert!(
            !rendered.contains(name),
            "the cross-tenant response must not echo acme-prod span name {name}: {rendered}"
        );
    }
}

// =====================================================================
// AC-CAP (FLAG 3 PIN, ADR-0053 Decision 3) — Result cap applies
// uniformly on the lookup arm
// =====================================================================

/// @driving_port @US-04 @cap
///
/// Documented decision (DISTILL wave-decisions.md): `MAX_RESULT_ROWS`
/// is a `pub const` (`crates/trace-query-api/src/lib.rs:78`) and is
/// NOT test-overridable cleanly. A 100_001-span fixture would be
/// expensive in CI and offer little incremental signal beyond the
/// existing window-arm cap scenario in
/// `slice_02_caps.rs::a_result_one_row_over_the_cap_is_refused_with_a_named_400`,
/// which already pins the same `if spans.len() > MAX_RESULT_ROWS`
/// branch (the lookup-arm handler reuses the same constant and the
/// same reason text per ADR-0053 Decision 3). The structural
/// uniformity is verified at the parse-handler layer (the handler
/// body in DELIVER includes the same `if` arm with the same reason
/// text `"result exceeds 100000 rows"`) and is covered by
/// `--in-diff` mutation tests against `slice_02_caps.rs` once
/// DELIVER lands the lookup-arm handler body. This `#[ignore]`d
/// placeholder is the in-line breadcrumb for the decision; the
/// durable record is ADR-0053 Decision 3.
#[ignore = "Result cap is verified structurally at the parse-handler layer; same MAX_RESULT_ROWS path as the existing window arm (see slice_02_caps.rs::a_result_one_row_over_the_cap_is_refused_with_a_named_400). A 100_001-span fixture is too expensive for the acceptance suite; uniform interaction is pinned by ADR-0053 Decision 3 and covered by --in-diff mutation tests."]
#[tokio::test]
async fn ac_cap_result_cap_applies_uniformly_on_the_lookup_arm() {
    // Intentionally left as a documentation breadcrumb. The DELIVER
    // wave's mutation suite against `crates/trace-query-api/src/lib.rs`
    // exercises the same `if spans.len() > MAX_RESULT_ROWS { ... }`
    // branch on both handlers; a mutant that drops the check on the
    // lookup arm is killed by `--in-diff` regression on the modified
    // file. ADR-0053 Decision 3 is the durable record of the
    // uniform-cap decision.
}
