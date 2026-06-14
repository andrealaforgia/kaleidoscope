// Kaleidoscope query-api — FIX-B.1 GET /help usage endpoint acceptance suite
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

//! FIX-B.1 — `GET /help` returns plain-text usage on the :9090 query API.
//!
//! Observable contract: `GET /help` answers 200 with `content-type:
//! text/plain`, a body listing four example curls (for
//! `/api/v1/query_range`, `/api/v1/logs`, `/api/v1/traces`
//! service-window, and `/api/v1/traces/by_id`) and the accepted time
//! format (both "RFC3339" and "unix"). The exact `/help` route WINS over
//! the SPA static fallback, so `GET /` must STILL serve the Prism SPA.
//!
//! All scenarios drive query-api through its single public driving port
//! (`query_api::router`) via `tower::ServiceExt::oneshot` — no network
//! port is bound. The `/help` route lives on the inner `api` router, so
//! it is present whether or not a static directory is mounted.
//!
//! RED-before / GREEN-after: before the route exists, `GET /help` with a
//! static bundle falls through to the SPA fallback (200, content-type
//! text/html, an index.html body) and without one is a 404 — either way
//! NOT text/plain carrying the curl body, so every assertion below fails
//! first.

use std::path::PathBuf;
use std::sync::Arc;

use aegis::TenantId;
use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use axum::Router;
use pulse::{FileBackedMetricStore, MetricStore, NoopRecorder};
use tower::ServiceExt; // for `oneshot`

/// Tenant constructor in the platform's aegis vocabulary.
fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

/// Open a fresh durable Pulse store in a unique tempdir. The /help and /
/// routes never touch the store, but the router constructor requires one;
/// a real durable store keeps the seam identical to the other slices.
fn open_durable_store(label: &str) -> (Arc<FileBackedMetricStore>, PathBuf) {
    let mut base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    base.push(format!("query-api-{label}-{}-{nanos}", std::process::id()));
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open store");
    (Arc::new(store), base)
}

/// The marker body of the SPA entry document, so the SPA regression arm
/// can prove it was index.html that was served at `/` (not the /help
/// body) by matching a string only index.html carries.
const INDEX_HTML_BODY: &str =
    "<!doctype html><html><head><title>Prism</title></head><body><div id=\"root\"></div></body></html>";

/// Lay down a temp static bundle containing an `index.html` with a known
/// marker, returning its path. Mirrors the tempdir prefix shape used by
/// `open_durable_store` so the suites are consistent.
fn static_bundle(label: &str) -> PathBuf {
    let mut base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    base.push(format!("query-api-{label}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&base).expect("create static bundle dir");
    std::fs::write(base.join("index.html"), INDEX_HTML_BODY).expect("write index.html");
    base
}

/// A plain `GET <path>` request with no query string.
fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .expect("build request")
}

/// Drive the router and return status, the `content-type` header (or the
/// empty string if absent), and the raw response body as a string.
async fn call_raw(router: Router, request: Request<Body>) -> (StatusCode, String, String) {
    let response = router.oneshot(request).await.expect("router responds");
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let body = String::from_utf8(bytes.to_vec()).expect("body is utf-8");
    (status, content_type, body)
}

// =====================================================================
// FIX-B.1 — /help answers plain-text usage and WINS over the SPA fallback
// =====================================================================

/// @driving_port @FIX-B.1
///
/// Given query-api is serving a static bundle (the SPA-mounted posture),
/// When an operator requests `GET /help`,
/// Then the exact /help route answers 200 with `content-type: text/plain`
/// — it wins over the SPA static fallback (which would answer text/html).
#[tokio::test]
async fn help_returns_plain_text_usage_and_wins_over_the_spa_fallback() {
    let (store, _base) = open_durable_store("help-plain-text");
    let t = tenant("acme-prod");
    let bundle = static_bundle("help-plain-text-bundle");

    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(t),
        Some(bundle),
    );
    let (status, content_type, _body) = call_raw(router, get("/help")).await;

    assert_eq!(status, StatusCode::OK, "GET /help is answered 200");
    assert!(
        content_type.starts_with("text/plain"),
        "GET /help answers text/plain (the exact route wins over the SPA fallback), got: {content_type}"
    );
}

/// @driving_port @FIX-B.1
///
/// Given query-api is serving a static bundle,
/// When an operator requests `GET /help`,
/// Then the body lists the four example curls (query_range, logs, traces
/// service-window, traces by_id) and the accepted time format (RFC3339
/// and unix seconds).
#[tokio::test]
async fn help_body_lists_the_four_example_curls_and_the_accepted_time_format() {
    let (store, _base) = open_durable_store("help-body");
    let t = tenant("acme-prod");
    let bundle = static_bundle("help-body-bundle");

    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(t),
        Some(bundle),
    );
    let (status, _content_type, body) = call_raw(router, get("/help")).await;

    assert_eq!(status, StatusCode::OK);
    for needle in [
        "/api/v1/query_range",
        "/api/v1/logs",
        "/api/v1/traces",
        "/api/v1/traces/by_id",
        "RFC3339",
        "unix",
    ] {
        assert!(
            body.contains(needle),
            "the /help body must mention {needle:?}, got:\n{body}"
        );
    }
    assert_eq!(
        body.matches("curl").count(),
        4,
        "the /help body carries four example curls, got:\n{body}"
    );
}

/// @driving_port @FIX-B.1
///
/// Given query-api is built with NO static directory (the API-only
/// posture),
/// When an operator requests `GET /help`,
/// Then it is still answered 200 with `content-type: text/plain` — /help
/// lives on the api router, so it is present in both configurations.
#[tokio::test]
async fn help_is_available_without_a_static_dir() {
    let (store, _base) = open_durable_store("help-no-static");
    let t = tenant("acme-prod");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let (status, content_type, body) = call_raw(router, get("/help")).await;

    assert_eq!(status, StatusCode::OK, "GET /help is answered 200");
    assert!(
        content_type.starts_with("text/plain"),
        "GET /help answers text/plain even with no static dir, got: {content_type}"
    );
    assert!(
        body.contains("/api/v1/query_range"),
        "the /help body carries the usage examples in the API-only posture"
    );
}

/// @driving_port @FIX-B.1
///
/// Given query-api is serving a static bundle AND now also exposes /help,
/// When the browser loads the root document,
/// Then the SPA index is STILL served at `/` (the new /help route does
/// not shadow the SPA static fallback).
#[tokio::test]
async fn root_still_serves_the_spa_index() {
    let (store, _base) = open_durable_store("help-spa-regression");
    let t = tenant("acme-prod");
    let bundle = static_bundle("help-spa-regression-bundle");

    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(t),
        Some(bundle),
    );
    let (status, content_type, body) = call_raw(router, get("/")).await;

    assert_eq!(status, StatusCode::OK, "GET / still serves the SPA");
    assert!(
        content_type.starts_with("text/html"),
        "GET / serves the SPA index document (text/html), got: {content_type}"
    );
    assert!(
        body.contains("id=\"root\""),
        "GET / serves index.html so the Prism SPA can take over: {body}"
    );
}
