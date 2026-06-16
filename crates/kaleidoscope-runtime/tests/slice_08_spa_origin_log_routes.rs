// Kaleidoscope consolidated runtime — Slice 8: SPA origin serves log routes
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

//! # Slice 8 — "the SPA origin serves the log query route, same-origin" (experimentable-stack-v0).
//!
//! ADR-0077 F4 made the metrics router (:9090) ALSO serve Prism's static
//! bundle same-origin, and ADR-0078 merged the TRACE query routes onto that
//! same origin, so the browser-served SPA reaches `/api/v1/query_range` and
//! `/api/v1/traces*` with relative paths and no CORS. But the LOG query route
//! still lived ONLY on the standalone logs listener (:9091): a same-origin
//! `GET /api/v1/logs` on the metrics origin fell through to the SPA
//! `index.html` static fallback (200 text/html, NOT the log JSON), so Prism's
//! Logs view — which fetches `/api/v1/logs` same-origin — got HTML and threw
//! "Unexpected token '<'".
//!
//! This slice proves the metrics/SPA origin now ALSO serves the log query
//! route, same-origin, WITHOUT breaking metrics, the trace routes, or the SPA
//! static fallback. The whole loop runs in ONE process on EPHEMERAL
//! `127.0.0.1:0` ports with a REAL Prism-bundle directory on the filesystem (a
//! temp dir holding an `index.html`): every assertion below drives the SAME
//! port that serves `query_range`, the trace routes, and the SPA
//! (`metrics_addr`), never the standalone :9091.
//!
//! The demo overlay is ON (the production default, ADR-0079) and the runtime is
//! configured for the demo tenant `acme`, so the always-current demo log set is
//! synthesised at query time — including EXACTLY ONE declined ERROR cause log.
//! The four observable outcomes asserted, all on the ONE metrics/SPA origin:
//!   1. `GET /api/v1/logs?...&body_regex=(?i)declined` -> `application/json`
//!      (NOT `text/html`) carrying EXACTLY the one declined demo log — the
//!      precise surface Prism's Logs view fetches, now answering JSON.
//!   2. `GET /api/v1/query_range` -> the metric still answers JSON (no regression).
//!   3. `GET /api/v1/traces?...` -> the trace routes still answer JSON (no regression).
//!   4. `GET /<client-route>` (unknown non-API path) -> the SPA `index.html`
//!      (the static fallback still catches client-side routes).
//!
//! FALSIFIABILITY: before the log route is merged onto the metrics router,
//! outcome (1) falls through to the SPA static fallback and returns the
//! `index.html` HTML (`text/html`, no log array) — outcome (1) FAILS while
//! (2)/(3)/(4) still pass, which is exactly the regression-free RED that
//! isolates the new same-origin log gateway behaviour (the Customer's cold-run
//! bug: `:9090/api/v1/logs` -> text/html -> "Unexpected token '<'").

mod common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use common::{
    fresh_pillar_root, metrics_contains_value, metrics_status_success, poll_until,
    spawn_test_runtime_with, TENANT_ACME,
};
use kaleidoscope_runtime::ConsolidatedConfig;

/// The demo `service.name` the overlay synthesises under (ADR-0079 identity).
const DEMO_SERVICE: &str = "kaleidoscope-demo";
/// The readable failure message on the one declined demo cause log (ADR-0079).
const DEMO_ERROR_MESSAGE: &str = "checkout failed: card declined";
/// A marker baked into the temp Prism `index.html`, so the SPA-fallback
/// assertion (and the RED-state HTML detection) proves the bytes came from the
/// static bundle, NOT a route.
const SPA_MARKER: &str = "<!doctype html><title>KALEIDOSCOPE_SPA_MARKER</title>";

/// A fresh temp dir holding a Prism-bundle `index.html` carrying [`SPA_MARKER`].
/// This is the REAL static bundle the metrics/SPA router serves; before the log
/// route is merged, `/api/v1/logs` returns THESE bytes (the bug).
fn fresh_prism_bundle() -> PathBuf {
    let dir = fresh_pillar_root("prism-bundle-logs");
    std::fs::write(dir.join("index.html"), SPA_MARKER).expect("write prism index.html");
    dir
}

/// GET `path` against the metrics/SPA origin over loopback, returning the
/// status, the `content-type` header (lowercased, empty if absent), and the
/// body. The content-type is load-bearing: the bug served `text/html`, the fix
/// serves `application/json`.
async fn get(addr: SocketAddr, path: &str) -> (u16, String, String) {
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}{path}"))
        .send()
        .await
        .expect("GET against the metrics/SPA origin over loopback");
    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let body = resp.text().await.expect("read response body");
    (status, content_type, body)
}

/// A now-centred window in epoch seconds wide enough to cover every demo log
/// (the demo logs sit within ~70s of now).
fn now_window() -> (u64, u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    (now - 3_600, now + 3_600)
}

/// The bodies of the records in a bare JSON log array (empty if not JSON).
fn log_bodies(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|records| {
            records
                .iter()
                .filter_map(|r| r["body"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// experimentable-stack-v0: the metrics/SPA origin (:9090) ALSO serves the log
/// query route same-origin, so Prism's Logs view reaches `/api/v1/logs` with a
/// relative path and gets JSON (not the SPA `index.html` HTML) — without
/// breaking metrics, the trace routes, or the SPA static fallback.
/// @driving_port @real-io
///
/// ```gherkin
/// Scenario: The SPA origin serves the log query route same-origin, live
///   Given the consolidated runtime is running for tenant "acme" with a Prism bundle served on the metrics origin
///   And the always-current demo overlay is on (the production default)
///   When Andrea GETs "/api/v1/logs?...&body_regex=(?i)declined" on the metrics origin
///   Then the response is application/json (not text/html) carrying exactly the one declined demo log
///   And "/api/v1/query_range" on that origin still returns the metric as JSON
///   And "/api/v1/traces" on that origin still returns the traces as JSON
///   And an unknown non-API path on that origin still returns the SPA index.html
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn metrics_spa_origin_also_serves_log_query_route_same_origin() {
    let root = fresh_pillar_root("spa-origin-logs");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.static_dir = Some(fresh_prism_bundle());
    // The demo overlay defaults ON; the demo tenant is "acme" (== TENANT_ACME),
    // so the synthesised demo logs — incl. the one declined cause log — are read
    // back over the SAME read handle the :9091 logs router and the overlay use.
    let rt = spawn_test_runtime_with("spa-origin-logs", config).await;

    // The SPA origin is the SAME port that serves query_range, the trace
    // routes, and the static bundle — never the standalone :9091.
    let origin = rt.metrics_addr();
    let (start, end) = now_window();

    // (1) The log route answers JSON on the SPA origin, NOT the index.html
    // fallback. With the demo overlay on, a body_regex=(?i)declined narrows to
    // EXACTLY the one declined demo cause log. Poll until the declined log is
    // visible through the metrics origin's /api/v1/logs, then read it once more
    // for the content-type + exact-body assertions.
    let logs_path = format!("/api/v1/logs?start={start}&end={end}&body_regex=(?i)declined");
    poll_until(
        Duration::from_secs(10),
        || async {
            let (s, _ct, b) = get(origin, &logs_path).await;
            (s, b)
        },
        |s, b| s == 200 && !log_bodies(b).is_empty(),
    )
    .await;
    let (status, content_type, body) = get(origin, &logs_path).await;
    assert_eq!(
        status, 200,
        "/api/v1/logs answers 200 on the metrics/SPA origin; content-type: {content_type}; body: {body}"
    );
    assert!(
        content_type.starts_with("application/json"),
        "/api/v1/logs on the SPA origin is application/json, NOT text/html (the bug); content-type: {content_type}; body: {body}"
    );
    assert!(
        !body.contains("KALEIDOSCOPE_SPA_MARKER"),
        "/api/v1/logs is the real log handler, NOT the SPA index.html fallback; body: {body}"
    );
    let bodies = log_bodies(&body);
    assert_eq!(
        bodies.as_slice(),
        [DEMO_ERROR_MESSAGE.to_string()].as_slice(),
        "body_regex=(?i)declined returns EXACTLY the one declined demo log as JSON on the SPA origin; body: {body}"
    );

    // (2) Metrics still answer JSON on the SAME origin (no regression). The demo
    // overlay synthesises the request_count point over the now-window.
    let query_range_path =
        format!("/api/v1/query_range?query=request_count&start={start}&end={end}");
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || async {
            let (s, _ct, b) = get(origin, &query_range_path).await;
            (s, b)
        },
        |s, b| s == 200 && metrics_status_success(b) && metrics_contains_value(b, "1"),
    )
    .await;
    assert_eq!(
        status, 200,
        "query_range still answers 200 on the metrics origin; body: {body}"
    );
    assert!(
        metrics_status_success(&body) && metrics_contains_value(&body, "1"),
        "the metric still comes back as JSON on the metrics origin; body: {body}"
    );

    // (3) The trace routes still answer JSON on the SAME origin (no regression).
    let (status, content_type, body) = get(
        origin,
        &format!("/api/v1/traces?service={DEMO_SERVICE}&start={start}&end={end}"),
    )
    .await;
    assert_eq!(
        status, 200,
        "the traces listing still answers 200 on the SPA origin; body: {body}"
    );
    assert!(
        content_type.starts_with("application/json"),
        "the traces listing is still JSON on the SPA origin; content-type: {content_type}; body: {body}"
    );
    let spans: serde_json::Value =
        serde_json::from_str(&body).expect("traces listing is JSON on the SPA origin, not HTML");
    assert!(
        spans.as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "the demo traces still ride the SPA origin; body: {body}"
    );

    // (4) The SPA static fallback still catches unknown non-API client routes.
    let (status, _content_type, body) = get(origin, "/some/client/route").await;
    assert_eq!(
        status, 200,
        "an unknown non-API path is the SPA index.html (200, not 404); body: {body}"
    );
    assert!(
        body.contains("KALEIDOSCOPE_SPA_MARKER"),
        "the SPA index.html is still served for client routes; body: {body}"
    );
}
