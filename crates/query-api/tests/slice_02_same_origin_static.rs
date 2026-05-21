// Kaleidoscope query-api — prism-backend-wiring slice 01 acceptance suite
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

//! Slice 01 (prism-backend-wiring-v0) — same-origin static serving.
//!
//! Maps to
//! `docs/feature/prism-backend-wiring-v0/slices/slice-01-see-a-metric-in-a-browser.md`.
//! Stories: US-01 (the QueryPanel mounts against a valid served
//! config.json), US-02 (a browser-served Prism reaches query-api and
//! plots a real series). Design: DD1/DD3/DD6 of
//! `design/wave-decisions.md`, the C4 L3 of `design/application-architecture.md`,
//! and ADR-0043.
//!
//! The user-centric outcome: an on-call operator opens Prism in a
//! browser and her browser fetches the page, then `/config.json`, then
//! `/api/v1/query_range` all from ONE origin. query-api optionally serves
//! Prism's built bundle (its `dist/`, including `config.json` and
//! `index.html`) as a `tower-http` `ServeDir` FALLBACK behind the
//! `KALEIDOSCOPE_QUERY_STATIC_DIR` knob, while the `/api/v1` route always
//! wins. Same origin means zero CORS. With the knob unset the service is
//! byte-for-byte today's read-only API.
//!
//! ASSUMED query-api surface (the crafter MUST match this, or update the
//! call sites in the same slice-01 commit):
//!
//! ```ignore
//! // DD3/DD6: `router(...)` gains a THIRD parameter, an optional static
//! // directory. `Some(dir)` attaches a ServeDir fallback under the
//! // existing /api/v1/query_range route (the API route wins; any
//! // unmatched path falls through to the static files, with index.html
//! // as the SPA fallback). `None` is byte-for-byte today's API-only
//! // router. The production binary maps KALEIDOSCOPE_QUERY_STATIC_DIR
//! // (unset/empty -> None) onto this same Option via a pure
//! // `composition::resolve_static_dir`.
//! pub fn router(
//!     store: std::sync::Arc<dyn pulse::MetricStore + Send + Sync>,
//!     tenant: Option<aegis::TenantId>,
//!     static_dir: Option<std::path::PathBuf>,
//! ) -> axum::Router;
//! ```
//!
//! This is a BREAKING change to the two-arg `router(store, tenant)` the
//! `query-range-api-v0` slice-01 suite calls. The crafter MUST update
//! those existing call sites (pass `None` for the API-only behaviour) in
//! the same commit, or these two suites will not both compile. The
//! `ServeDir` wiring does not yet exist, so this file is RED.
//!
//! All scenarios drive query-api through its single public driving port
//! (`query_api::router`) via `tower::ServiceExt::oneshot` (see
//! `common/mod.rs`), no network port bound. The unknown-path behaviour
//! asserted here is the SPA index.html fallback the design pinned (DD3,
//! the L3 component view, and the request-flow sequence), NOT a 404.
//!
//! One-at-a-time outer loop: the walking skeleton is enabled; every
//! following scenario is `#[ignore]`d and is enabled one at a time as the
//! crafter drives each inward.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use pulse::{MetricBatch, MetricStore};
use tower::ServiceExt; // for `oneshot`

use common::{
    call, gauge, open_durable_store, point, prism_accepts_success, query_range_request,
    result_series, secs_to_nanos, tenant,
};

// =====================================================================
// Local helpers — a temp static bundle, and plain GET requests.
//
// These mirror `common/mod.rs` (the slice-01 query-range suite): the
// same tempdir prefix shape (`query-api-{label}-{pid}-{nanos}`) and the
// same oneshot `call` driver. They live here because they are specific
// to the same-origin static-serving behaviour this suite pins, and
// `common/mod.rs` is the shared seam the crafter must not have to edit
// to keep the existing suite green.
// =====================================================================

/// The committed config.json value the design pins (DD2/DD4): an
/// origin-relative `backend.url` carrying `/api/v1`, the "Pulse
/// (durable)" label, and the slice-01 prism version. This is the exact
/// body the served file is expected to contain.
const SLICE_01_CONFIG_JSON: &str = r#"{
  "backend": { "url": "/api/v1", "label": "Pulse (durable)" },
  "prism": { "version": "0.1.0" }
}
"#;

/// The marker body of the SPA entry document, so the index-fallback
/// assertions can prove it was index.html that was served (and not, say,
/// an API body) by matching on a string only index.html carries.
const INDEX_HTML_BODY: &str =
    "<!doctype html><html><head><title>Prism</title></head><body><div id=\"root\"></div></body></html>";

/// Lay down a temp `dist/`-shaped static bundle containing `config.json`
/// and `index.html`, returning its path. Mirrors the tempdir prefix
/// shape used by `open_durable_store` so the two suites are consistent.
/// The returned dir is the value the crafter's `ServeDir` is pointed at.
fn static_bundle(label: &str) -> PathBuf {
    let mut base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    base.push(format!("query-api-{label}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&base).expect("create static bundle dir");
    std::fs::write(base.join("config.json"), SLICE_01_CONFIG_JSON).expect("write config.json");
    std::fs::write(base.join("index.html"), INDEX_HTML_BODY).expect("write index.html");
    base
}

/// A plain `GET <path>` request with no query string, for the static
/// paths (`/`, `/config.json`, an unknown SPA route). The `/api/v1`
/// requests still use the contract-shaped `query_range_request` helper
/// from `common`.
fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .expect("build request")
}

/// Drive the router and return the HTTP status plus the raw response
/// body as a string (the static files are not JSON, so the JSON `call`
/// helper does not fit the static arms).
async fn call_raw(router: Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.oneshot(request).await.expect("router responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let body = String::from_utf8(bytes.to_vec()).expect("body is utf-8");
    (status, body)
}

// =====================================================================
// US-01 — Walking skeleton: the browser loads Prism and its config from
//          one origin
// =====================================================================

/// @walking_skeleton @driving_port @real-io @adapter-integration @US-01
///
/// Given query-api is serving Prism's built bundle from a single origin,
/// When the operator's browser loads the page and then its configuration,
/// Then the page document is served and the configuration declares the
/// backend label "Pulse (durable)" so Prism's QueryPanel can mount.
///
/// This is the demo-able same-origin closure: it points the ServeDir at
/// a REAL temp bundle on the filesystem (the same shape Vite copies into
/// `dist/`) so the skeleton proves wiring, path resolution, and the
/// served bytes end to end, not an in-memory stand-in. It exercises the
/// ServeDir adapter against a real (temp) filesystem.
#[tokio::test]
async fn the_browser_loads_prism_and_its_config_from_one_origin() {
    let (store, _base) = open_durable_store("same-origin-walking");
    let t = tenant("acme-prod");
    let bundle = static_bundle("same-origin-walking-bundle");

    // The browser's first request: the page itself.
    let router = query_api::router(
        store.clone() as Arc<dyn MetricStore + Send + Sync>,
        Some(t.clone()),
        Some(bundle.clone()),
    );
    let (page_status, page_body) = call_raw(router, get("/")).await;
    assert_eq!(page_status, StatusCode::OK, "the page document is served");
    assert!(
        page_body.contains("id=\"root\""),
        "GET / serves the Prism index document: {page_body}"
    );

    // The browser's second request: its runtime configuration.
    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(t),
        Some(bundle),
    );
    let (config_status, config_body) = call_raw(router, get("/config.json")).await;
    assert_eq!(config_status, StatusCode::OK, "config.json is served");
    let config: serde_json::Value =
        serde_json::from_str(&config_body).expect("config.json is valid JSON");
    assert_eq!(
        config["backend"]["label"], "Pulse (durable)",
        "the served config declares the backend label so the panel mounts: {config_body}"
    );
    assert_eq!(
        config["backend"]["url"], "/api/v1",
        "the served config carries the origin-relative /api/v1 backend url"
    );
}

// =====================================================================
// US-01 — The configuration file is served verbatim from the bundle
// =====================================================================

/// @driving_port @real-io @adapter-integration @US-01
///
/// Given query-api is serving a bundle whose config.json holds the
/// slice-01 value,
/// When the browser fetches the configuration,
/// Then the served bytes are exactly the committed file's bytes, so
/// Prism's own loader validates them.
#[tokio::test]
async fn the_configuration_is_served_verbatim_from_the_bundle() {
    let (store, _base) = open_durable_store("config-verbatim");
    let t = tenant("acme-prod");
    let bundle = static_bundle("config-verbatim-bundle");

    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(t),
        Some(bundle),
    );
    let (status, body) = call_raw(router, get("/config.json")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body, SLICE_01_CONFIG_JSON,
        "the static fallback serves the config.json file's bytes verbatim"
    );
}

// =====================================================================
// US-02 — The API route wins over the static fallback
// =====================================================================

/// @driving_port @real-io @adapter-integration @US-02
///
/// Given query-api is serving a static bundle AND has metric data for the
/// tenant,
/// When the operator's browser queries a metric over a covering range,
/// Then the query is answered by the API handler with a success matrix
/// (the API route wins over the static fallback), never shadowed by an
/// index.html body.
#[tokio::test]
async fn the_api_route_wins_over_the_static_fallback() {
    let (store, _base) = open_durable_store("api-route-wins");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 0.4, &[])],
            )]),
        )
        .expect("seed durable store");
    let bundle = static_bundle("api-route-wins-bundle");

    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(t),
        Some(bundle),
    );
    let request = query_range_request("process_cpu_utilization", "1716200000", "1716200015");
    let (status, body) = call(router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the /api/v1 route is served by the handler, not the static fallback"
    );
    assert!(
        prism_accepts_success(&body),
        "the API answers the contract matrix shape, not an index.html body: {body}"
    );
    let series = result_series(&body);
    assert_eq!(series.len(), 1, "the operator sees the queried series");
    assert_eq!(series[0]["metric"]["__name__"], "process_cpu_utilization");
}

// =====================================================================
// US-01 — An unknown path falls back to the SPA index document
// =====================================================================

/// @driving_port @real-io @adapter-integration @US-01
///
/// Given query-api is serving a static bundle,
/// When the browser requests a client-side route that is neither an
/// /api/v1 path nor an existing static file (a deep link the SPA router
/// owns),
/// Then the page document (index.html) is served so the SPA can route
/// it client-side.
///
/// The design (DD3, the L3 component view, and the request-flow
/// sequence) pins index.html SPA fallback for unmatched paths, NOT a 404.
#[tokio::test]
async fn an_unknown_path_falls_back_to_the_spa_index() {
    let (store, _base) = open_durable_store("spa-fallback");
    let t = tenant("acme-prod");
    let bundle = static_bundle("spa-fallback-bundle");

    let router = query_api::router(
        store as Arc<dyn MetricStore + Send + Sync>,
        Some(t),
        Some(bundle),
    );
    let (status, body) = call_raw(router, get("/dashboards/checkout")).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "an unknown SPA path falls back to the index document, not a 404"
    );
    assert!(
        body.contains("id=\"root\""),
        "the SPA fallback serves index.html so the client router can take over: {body}"
    );
}

// =====================================================================
// US-02 — Default-off: with no static dir, the API still works
// =====================================================================

/// @driving_port @US-02
///
/// Given query-api is built with NO static directory (the default-off
/// posture, the existing read-only API),
/// When the operator queries a metric,
/// Then the query is answered exactly as today: read-only behaviour is
/// unchanged when static serving is off.
#[tokio::test]
async fn with_no_static_dir_the_api_behaviour_is_unchanged() {
    let (store, _base) = open_durable_store("default-off-api");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "checkout",
                vec![point(secs_to_nanos(1_716_200_000), 0.4, &[])],
            )]),
        )
        .expect("seed durable store");

    // No static dir: the third argument is None.
    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let request = query_range_request("process_cpu_utilization", "1716200000", "1716200015");
    let (status, body) = call(router, request).await;

    assert_eq!(status, StatusCode::OK, "the read-only API is unchanged");
    assert!(
        prism_accepts_success(&body),
        "the API still answers the contract matrix shape: {body}"
    );
    assert_eq!(result_series(&body).len(), 1);
}

// =====================================================================
// US-01 — Default-off: with no static dir, no static files are served
// =====================================================================

/// @driving_port @US-01
///
/// Given query-api is built with NO static directory,
/// When the browser fetches the configuration file,
/// Then nothing is served at that path: static serving is off by default,
/// so the existing API-only surface does not regress into serving files.
#[tokio::test]
async fn with_no_static_dir_the_config_path_is_not_served() {
    let (store, _base) = open_durable_store("default-off-config");
    let t = tenant("acme-prod");

    let router = query_api::router(store as Arc<dyn MetricStore + Send + Sync>, Some(t), None);
    let (status, _body) = call_raw(router, get("/config.json")).await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "with static serving off, /config.json is not served"
    );
}

// =====================================================================
// US-01 — The committed config.json validates against Prism's loader
// =====================================================================

/// @US-01
///
/// Given the committed config asset at `apps/prism/public/config.json`
/// (which DELIVER creates),
/// When its shape is checked against Prism's own `isRuntimeConfig`
/// contract,
/// Then it is valid JSON of shape
/// `{ backend: { url: "/api/v1", label: <string> }, prism: { version: <string> } }`,
/// so Prism's loader returns `{ kind: 'ok' }` and the QueryPanel mounts.
///
/// This encodes Prism's `isRuntimeConfig` guard
/// (`apps/prism/src/lib/config/loader.ts`) as a Rust assertion against
/// the committed file, locating the test alongside the query-api process
/// that serves the file at runtime. DELIVER must create
/// `apps/prism/public/config.json` to satisfy this; until then the file
/// is absent and this scenario is RED. (A parallel prism-side Vitest test
/// that runs the real `isRuntimeConfig` over the same bytes is the
/// natural complement on the frontend gate; this Rust arm guards the
/// shape from the serving side.)
#[tokio::test]
async fn the_committed_config_json_validates_against_prisms_loader() {
    // Resolve the repository's apps/prism/public/config.json relative to
    // this crate (CARGO_MANIFEST_DIR is crates/query-api).
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir
        .join("../../apps/prism/public/config.json")
        .canonicalize()
        .expect(
            "the committed config asset must exist at apps/prism/public/config.json \
             (DELIVER creates it)",
        );

    let raw = std::fs::read_to_string(&config_path).expect("read committed config.json");
    let config: serde_json::Value =
        serde_json::from_str(&raw).expect("the committed config.json is valid JSON");

    // Mirror of Prism's isRuntimeConfig: backend and prism are objects;
    // backend.url, backend.label, prism.version are strings.
    let backend = config
        .get("backend")
        .and_then(serde_json::Value::as_object)
        .expect("backend is an object");
    let prism = config
        .get("prism")
        .and_then(serde_json::Value::as_object)
        .expect("prism is an object");
    assert!(
        backend
            .get("url")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "backend.url is a string"
    );
    assert!(
        backend
            .get("label")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "backend.label is a string"
    );
    assert!(
        prism
            .get("version")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "prism.version is a string"
    );

    // The slice-01 reconciliation: backend.url MUST carry the /api/v1
    // segment so buildUrl lands on query-api's /api/v1/query_range route.
    assert_eq!(
        backend.get("url").and_then(serde_json::Value::as_str),
        Some("/api/v1"),
        "backend.url carries the origin-relative /api/v1 segment"
    );
}
