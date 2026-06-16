// Kaleidoscope trace-query-api — slice 10 traces listing attribute filter
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

//! Traces listing attribute filter — `GET /api/v1/traces?...&attr_key=&attr_value=`
//! returns ONLY the traces that have at least one span whose `attributes` map
//! carries `attr_key == attr_value` (exact string match), and ALL of that
//! trace's spans, within the same service + time-window scope that already
//! applies — so a caller can find the traces belonging to ONE identifier (e.g.
//! a specific `customer.id`) among many (experimentable-stack-v0).
//!
//! Both params absent (or both empty) is EXACTLY today's behaviour (no attribute
//! filtering). Exactly one of the two present is a 400 (both-or-neither) caught
//! BEFORE the store, consistent with the existing service/window/error param
//! validation. Composes with `error=true`: a trace must satisfy both filters.
//!
//! Every scenario drives trace-query-api through its public driving port
//! `trace_query_api::router(store, tenant)` via tower `oneshot` against a REAL
//! in-memory `TraceStore` fake (the happy/scope arms), or a failing double for
//! the both-or-neither-before-store proof. Port-to-port at the crate boundary:
//! assertions are on the HTTP status and the observable JSON span array.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use axum::body::Body;
use axum::http::{Request, StatusCode};

use common::{call, is_error_envelope, span_names, spans_array, tenant};
use ray::{
    InMemoryTraceStore, NoopRecorder, Span, SpanBatch, SpanId, SpanKind, SpanStatus,
    StatusCode as RayStatusCode, TraceId, TraceStore,
};

// ---------------------------------------------------------------------
// Fixtures — model "many traces, one identifier each" (the IDSEARCH shape).
// ---------------------------------------------------------------------

const WINDOW_START: &str = "1716200000";
const WINDOW_END: &str = "1716200060";

const ATTR_KEY: &str = "customer.id";

const ALICE_HEX: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const BOB_HEX: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

fn ok_status() -> SpanStatus {
    SpanStatus {
        code: RayStatusCode::Ok,
        message: String::new(),
    }
}

fn error_status(message: &str) -> SpanStatus {
    SpanStatus {
        code: RayStatusCode::Error,
        message: message.to_string(),
    }
}

/// A span on `trace_byte`*16 / `span_byte`*8, at `secs`, filed under `service`,
/// with `status`, carrying the given span-level `attributes`.
fn span(
    secs: u64,
    service: &str,
    trace_byte: u8,
    span_byte: u8,
    name: &str,
    status: SpanStatus,
    attributes: BTreeMap<String, String>,
) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    let start = secs * 1_000_000_000;
    Span {
        trace_id: TraceId([trace_byte; 16]),
        span_id: SpanId([span_byte; 8]),
        parent_span_id: None,
        name: name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 1_000_000,
        status,
        attributes,
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

/// `customer.id = value` as a span attribute map.
fn customer(value: &str) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    attrs.insert(ATTR_KEY.to_string(), value.to_string());
    attrs
}

fn store_with(spans: Vec<Span>, t: &TenantId) -> Arc<dyn TraceStore + Send + Sync> {
    let store = InMemoryTraceStore::new(Box::new(NoopRecorder));
    store
        .ingest(t, SpanBatch::with_spans(spans))
        .expect("seed trace store");
    Arc::new(store)
}

/// Five customer traces (one identifier each) for service "checkout" inside the
/// window, PLUS:
///   - Alice's trace has TWO spans: the root carries `customer.id=alice`, the
///     child carries NO `customer.id` — so "all spans of a matching trace" is
///     falsifiable (the child must still come back).
///   - one "anon" trace whose span has NO `customer.id` at all — so a mutant
///     that treats a missing key as a match (`unwrap_or(true)`) is killed.
fn five_customers_one_each() -> (TenantId, Vec<Span>) {
    let t = tenant("acme-prod");
    let spans = vec![
        // Alice — two spans, only the root tagged with the customer id.
        span(
            1_716_200_010,
            "checkout",
            0xA1,
            0x01,
            "alice-place-order",
            ok_status(),
            customer("alice"),
        ),
        span(
            1_716_200_011,
            "checkout",
            0xA1,
            0x02,
            "alice-charge-card",
            ok_status(),
            BTreeMap::new(),
        ),
        span(
            1_716_200_012,
            "checkout",
            0xB2,
            0x03,
            "bob-place-order",
            ok_status(),
            customer("bob"),
        ),
        span(
            1_716_200_013,
            "checkout",
            0xC3,
            0x04,
            "carol-place-order",
            ok_status(),
            customer("carol"),
        ),
        span(
            1_716_200_014,
            "checkout",
            0xD4,
            0x05,
            "dave-place-order",
            ok_status(),
            customer("dave"),
        ),
        span(
            1_716_200_015,
            "checkout",
            0xE5,
            0x06,
            "erin-place-order",
            ok_status(),
            customer("erin"),
        ),
        // Anon — no customer.id at all.
        span(
            1_716_200_016,
            "checkout",
            0x99,
            0x07,
            "anon-place-order",
            ok_status(),
            BTreeMap::new(),
        ),
    ];
    (t, spans)
}

/// The listing request the contract pins, with optional `error` / `attr_key` /
/// `attr_value` params appended verbatim when `Some`.
fn listing_request(
    service: &str,
    start: &str,
    end: &str,
    error: Option<&str>,
    attr_key: Option<&str>,
    attr_value: Option<&str>,
) -> Request<Body> {
    let mut uri = format!("/api/v1/traces?service={service}&start={start}&end={end}");
    if let Some(error) = error {
        uri.push_str(&format!("&error={error}"));
    }
    if let Some(attr_key) = attr_key {
        uri.push_str(&format!("&attr_key={attr_key}"));
    }
    if let Some(attr_value) = attr_value {
        uri.push_str(&format!("&attr_value={attr_value}"));
    }
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// The trace_id (lowercase hex) of every span in the response, in order.
fn trace_ids(body: &serde_json::Value) -> Vec<String> {
    spans_array(body)
        .iter()
        .filter_map(|s| s["trace_id"].as_str().map(str::to_string))
        .collect()
}

// =====================================================================
// (a) attr_key+attr_value returns ONLY the matching identifier's trace, and
// ALL of its spans (including the child span that does NOT carry the
// attribute), excluding the other four customers and the anon trace. The
// IDSEARCH discrimination: one identifier found among many.
// =====================================================================

#[tokio::test]
async fn attribute_filter_returns_only_the_matching_identifiers_full_trace() {
    let (t, spans) = five_customers_one_each();
    let store = store_with(spans, &t);

    let router = trace_query_api::router(store, Some(t));
    let request = listing_request(
        "checkout",
        WINDOW_START,
        WINDOW_END,
        None,
        Some(ATTR_KEY),
        Some("alice"),
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "the listing is 200: {body}");

    // Both of Alice's spans come back — the root that carries customer.id AND
    // the child that does not — so the matching trace is reachable IN FULL.
    assert_eq!(
        span_names(&body),
        vec![
            "alice-place-order".to_string(),
            "alice-charge-card".to_string(),
        ],
        "exactly Alice's two spans, the tagged root AND its untagged child: {body}"
    );

    // Every returned span belongs to Alice's trace; the other four customers
    // and the anon trace are gone.
    assert!(
        trace_ids(&body).iter().all(|id| id == ALICE_HEX),
        "every returned span belongs to Alice's trace: {body}"
    );
    let rendered = body.to_string();
    for absent in ["bob", "carol", "dave", "erin", "anon", BOB_HEX] {
        assert!(
            !rendered.contains(absent),
            "no other customer/trace appears ({absent}): {rendered}"
        );
    }
}

// =====================================================================
// (e) attr_key present, attr_value matches NOBODY -> empty list (a calm 200,
// not an error), even though every trace carries SOME customer.id. Kills a
// mutant that treats a missing/other key as a match.
// =====================================================================

#[tokio::test]
async fn attribute_filter_with_no_matching_value_returns_empty() {
    let (t, spans) = five_customers_one_each();
    let store = store_with(spans, &t);

    let router = trace_query_api::router(store, Some(t));
    let request = listing_request(
        "checkout",
        WINDOW_START,
        WINDOW_END,
        None,
        Some(ATTR_KEY),
        Some("nobody"),
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "no match is a calm 200: {body}");
    assert!(
        spans_array(&body).is_empty(),
        "no trace carries customer.id=nobody, so the list is empty: {body}"
    );
}

// =====================================================================
// (d) Both params absent -> EXACTLY today's behaviour: every in-window span for
// the service comes back, unfiltered. Kills a mutant that filters when the
// attribute filter is off.
// =====================================================================

#[tokio::test]
async fn no_attribute_params_leaves_the_listing_unfiltered() {
    let (t, spans) = five_customers_one_each();
    let store = store_with(spans, &t);

    let router = trace_query_api::router(store, Some(t));
    let request = listing_request("checkout", WINDOW_START, WINDOW_END, None, None, None);
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "the listing is 200: {body}");
    // All seven in-window spans (six traces) come back — no filtering.
    assert_eq!(
        span_names(&body),
        vec![
            "alice-place-order".to_string(),
            "alice-charge-card".to_string(),
            "bob-place-order".to_string(),
            "carol-place-order".to_string(),
            "dave-place-order".to_string(),
            "erin-place-order".to_string(),
            "anon-place-order".to_string(),
        ],
        "absent attribute params leave the listing unfiltered: {body}"
    );
}

// =====================================================================
// (b)+(c) BOTH-OR-NEITHER: exactly one of attr_key/attr_value present (the
// other absent OR empty) is a 400 caught BEFORE the store, with no echo of the
// raw value. Proven by a failing store double: a leaked query would lift the
// response to a 500. Both-empty is treated as absent (it stays a 200, asserted
// separately by the unfiltered arm via the runtime suite).
// =====================================================================

#[tokio::test]
async fn exactly_one_attribute_param_is_rejected_before_the_store() {
    // (key, value) pairs where exactly one side is meaningfully present.
    let cases: Vec<(Option<&str>, Option<&str>)> = vec![
        (Some("customer.id"), None),     // attr_key without attr_value
        (None, Some("alice")),           // attr_value without attr_key
        (Some("customer.id"), Some("")), // attr_key with empty attr_value
        (Some(""), Some("alice")),       // empty attr_key with attr_value
    ];
    for (attr_key, attr_value) in cases {
        let store: Arc<dyn TraceStore + Send + Sync> = Arc::new(common::FailingTraceStore);
        let router = trace_query_api::router(store, Some(tenant("acme-prod")));
        let request = listing_request(
            "checkout",
            WINDOW_START,
            WINDOW_END,
            None,
            attr_key,
            attr_value,
        );
        let (status, body) = call(router, request).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "exactly one of attr_key/attr_value ({attr_key:?}, {attr_value:?}) is a 400 before the store: {body}"
        );
        assert!(
            is_error_envelope(&body),
            "the rejection is an error envelope: {body}"
        );
        // The reason never echoes the raw key or value.
        let rendered = body.to_string();
        assert!(
            !rendered.contains("customer.id") && !rendered.contains("alice"),
            "the both-or-neither error never echoes the raw key/value: {rendered}"
        );
    }
}

// =====================================================================
// (f) The attribute filter narrows WITHIN the existing service + time-window
// scope: a matching trace outside the window, and a matching trace under a
// different service, are both excluded — only the in-scope matching trace
// survives.
// =====================================================================

#[tokio::test]
async fn attribute_filter_narrows_within_the_service_and_window_scope() {
    let t = tenant("acme-prod");
    let store = store_with(
        vec![
            // In-scope match (service checkout, inside the window).
            span(
                1_716_200_010,
                "checkout",
                0xA1,
                0x01,
                "in-scope-alice",
                ok_status(),
                customer("alice"),
            ),
            // In-scope, same service, DIFFERENT customer — only the attribute
            // filter (not the existing window/service scope) can exclude this,
            // so the scenario is genuinely RED without the new filter.
            span(
                1_716_200_011,
                "checkout",
                0xB2,
                0x04,
                "in-scope-bob",
                ok_status(),
                customer("bob"),
            ),
            // customer.id=alice OUTSIDE the window (same service) — excluded by
            // the window the listing already applies.
            span(
                1_716_300_000,
                "checkout",
                0xEF,
                0x02,
                "out-of-window-alice",
                ok_status(),
                customer("alice"),
            ),
            // customer.id=alice under a DIFFERENT service, inside the window —
            // excluded by the required service scope.
            span(
                1_716_200_015,
                "payments",
                0x77,
                0x03,
                "other-service-alice",
                ok_status(),
                customer("alice"),
            ),
        ],
        &t,
    );

    let router = trace_query_api::router(store, Some(t));
    let request = listing_request(
        "checkout",
        WINDOW_START,
        WINDOW_END,
        None,
        Some(ATTR_KEY),
        Some("alice"),
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "the listing is 200: {body}");
    assert_eq!(
        span_names(&body),
        vec!["in-scope-alice".to_string()],
        "only the in-scope match survives the service+window scope: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("in-scope-bob"),
        "an in-scope, same-service, different-customer trace is excluded by the attribute filter: {rendered}"
    );
    assert!(
        !rendered.contains("out-of-window-alice") && !rendered.contains("other-service-alice"),
        "matches outside the window or service are excluded: {rendered}"
    );
}

// =====================================================================
// (g) The attribute filter COMPOSES with error=true: a trace must satisfy BOTH
// (carry the attribute AND have an Error-status span). A failed trace for a
// DIFFERENT customer, and a healthy trace for the SAME customer, are both
// excluded — only the failed trace for the target customer survives.
// =====================================================================

#[tokio::test]
async fn attribute_filter_composes_with_the_error_filter() {
    let t = tenant("acme-prod");
    let store = store_with(
        vec![
            // FAILED + customer.id=alice — satisfies both filters.
            span(
                1_716_200_010,
                "checkout",
                0xA1,
                0x01,
                "alice-failed",
                error_status("boom"),
                customer("alice"),
            ),
            // FAILED but customer.id=bob — fails the attribute filter.
            span(
                1_716_200_011,
                "checkout",
                0xB2,
                0x02,
                "bob-failed",
                error_status("boom"),
                customer("bob"),
            ),
            // HEALTHY + customer.id=alice — fails the error filter.
            span(
                1_716_200_012,
                "checkout",
                0xC3,
                0x03,
                "alice-healthy",
                ok_status(),
                customer("alice"),
            ),
        ],
        &t,
    );

    let router = trace_query_api::router(store, Some(t));
    let request = listing_request(
        "checkout",
        WINDOW_START,
        WINDOW_END,
        Some("true"),
        Some(ATTR_KEY),
        Some("alice"),
    );
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "the listing is 200: {body}");
    assert_eq!(
        span_names(&body),
        vec!["alice-failed".to_string()],
        "only the failed trace for the target customer survives both filters: {body}"
    );
    let rendered = body.to_string();
    assert!(
        !rendered.contains("bob-failed") && !rendered.contains("alice-healthy"),
        "a failed-other-customer and a healthy-same-customer are both excluded: {rendered}"
    );
}
