// Kaleidoscope query-api — Prometheus query_range over Pulse
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

//! # query-api — the read side that closes the loop.
//!
//! Serves Prism's pinned `GET /api/v1/query_range` contract (ADR-0042 +
//! `apps/prism/src/lib/promql/queryRange.ts`) out of the durable Pulse
//! store, read-only. The single public driving port is [`router`]; the
//! thin binary (`src/main.rs`) is the composition root that opens the
//! store, resolves the tenant, and binds the listener.
//!
//! ## Public surface
//!
//! - [`router`] — build an axum `Router` over a `MetricStore` and an
//!   `Option<TenantId>`. `None` models fail-closed tenancy at the
//!   router seam: every request is refused with a `status:error` body.
//!
//! ## Architectural posture
//!
//! - Hexagonal: the `MetricStore` driven port (the `pulse` trait) and
//!   the tenant seam are the only collaborators. The parser and matrix
//!   translation carry the only mutable logic and are unit-testable in
//!   isolation under `selector` and `matrix`.
//! - Rust idiomatic: data + free functions; no inheritance, no `dyn`
//!   where generics suffice. The `Arc<dyn MetricStore>` indirection IS
//!   genuine polymorphism (the durable adapter in production, an
//!   in-memory or failing double in tests).
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

pub mod composition;
mod matrix;
mod selector;

use std::path::PathBuf;
use std::sync::Arc;

use aegis::TenantId;
use axum::extract::{Query, State};
use axum::http::header::{self, HeaderMap};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use pulse::{MetricStore, TimeRange};
use serde::Deserialize;
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

// The cap constants and the four reason text literals now live in
// `query-http-common` (ADR-0054). `pub use` preserves the existing
// `query_api::MAX_WINDOW_SECONDS` / `query_api::MAX_RESULT_ROWS`
// downstream-readable path (US-01 backward compatibility AC).
pub use query_http_common::{MAX_RESULT_ROWS, MAX_WINDOW_SECONDS};

/// The route path Prism's `buildUrl` targets: `backend.url` prefix
/// `/api/v1` + `/query_range` (verified in `queryRange.ts`).
const QUERY_RANGE_ROUTE: &str = "/api/v1/query_range";

/// The exact usage route (FIX-B.1). An operator hitting the :9090 query
/// API with `GET /help` gets a plain-text crib of the read endpoints and
/// the accepted time format. Registered as an exact `.route(...)` on the
/// inner `api` router so it WINS over the SPA static fallback (an exact
/// route takes precedence over `.fallback_service(...)`) and is present
/// whether or not a static bundle is mounted.
const HELP_ROUTE: &str = "/help";

/// The plain-text usage body served at `GET /help` (FIX-B.1). Lists one
/// example `curl` per read endpoint the platform exposes — metrics range
/// query, logs over a service window, traces over a service window, and a
/// single trace by id — and the accepted time format (RFC3339 or unix
/// seconds), so an operator can copy-paste a working request without
/// reaching for the docs.
const HELP_BODY: &str = "Kaleidoscope query API — usage\n\
\n\
Examples (replace host/port, tenant bearer, and arguments as needed):\n\
\n\
  # Metrics: range query over a service window\n\
  curl 'http://localhost:9090/api/v1/query_range?query=process_cpu_utilization&start=2026-06-14T00:00:00Z&end=2026-06-14T01:00:00Z&step=15s'\n\
\n\
  # Logs over a service window\n\
  curl 'http://localhost:9090/api/v1/logs?service=checkout&start=2026-06-14T00:00:00Z&end=2026-06-14T01:00:00Z'\n\
\n\
  # Traces over a service window\n\
  curl 'http://localhost:9090/api/v1/traces?service=checkout&start=2026-06-14T00:00:00Z&end=2026-06-14T01:00:00Z'\n\
\n\
  # A single trace by id\n\
  curl 'http://localhost:9090/api/v1/traces/by_id?trace_id=4bf92f3577b34da6a3ce929d0e0e4736'\n\
\n\
Accepted time format: RFC3339 (2026-06-14T00:00:00Z) or unix seconds (1718323200).\n";

/// The SPA entry document inside a served bundle. Unmatched non-API
/// paths fall back to this so the client-side router can take over
/// (DD6: SPA index fallback, NOT a 404).
const INDEX_HTML: &str = "index.html";

/// Application state shared with the handler. Two fields: the metric
/// store port and the resolved tenant (or `None` for fail-closed).
#[derive(Clone)]
struct ApiState {
    store: Arc<dyn MetricStore + Send + Sync>,
    tenant: Option<TenantId>,
    /// OPTIONAL per-request read-auth validator (read-path-query-api-auth-v0,
    /// ADR-0074). `Some` => the handler resolves the tenant from a validated
    /// `Authorization: Bearer <jwt>` (audience `kaleidoscope-query`),
    /// fail-closed before the store, and NEVER downgrades a missing/invalid
    /// bearer to the env `tenant` (the no-bearer-bypass). `None` => today's
    /// env-tenant path, header ignored (backward compatibility). The scaffold
    /// router stores it; the existing handler still ignores it (RED until
    /// DELIVER swaps `resolve_tenant_or_refuse` for
    /// `resolve_request_tenant_or_refuse`).
    auth: Option<Arc<aegis::Validator>>,
}

/// Build the query-api `Router`.
///
/// `store` is the `MetricStore` driven port (the durable
/// `FileBackedMetricStore` in production, a double in tests). `tenant`
/// models fail-closed tenancy at the router seam: `Some(t)` is a
/// resolved tenant; `None` is "no tenant resolvable" and every request
/// is refused with a `status:error` body. The production binary maps
/// `KALEIDOSCOPE_QUERY_TENANT` (set/non-empty -> `Some`, unset/empty ->
/// `None`) onto this same `Option`, so the fail-closed behaviour is
/// identical in tests and in production.
///
/// `static_dir` is the same-origin static-serving knob (DD3/DD6,
/// ADR-0043): `Some(dir)` mounts a `tower-http` `ServeDir` as the
/// router's fallback service so Prism's built bundle (its `config.json`,
/// `index.html`, and assets) is served from the same origin as
/// `/api/v1` — removing the need for CORS. The exact API route always
/// WINS over the static fallback (an exact `.route(...)` takes
/// precedence over `.fallback_service(...)`), and any unmatched non-API
/// path that is not an existing file falls back to `index.html` so the
/// SPA router can take over (NOT a 404). `None` is byte-for-byte
/// today's API-only router: with no fallback, an unknown path is a 404.
/// The production binary maps `KALEIDOSCOPE_QUERY_STATIC_DIR`
/// (set/non-empty -> `Some`, unset/empty -> `None`) onto this same
/// `Option`.
pub fn router(
    store: Arc<dyn MetricStore + Send + Sync>,
    tenant: Option<TenantId>,
    static_dir: Option<PathBuf>,
) -> Router {
    router_with_auth(store, tenant, None, static_dir)
}

/// Build the query-api `Router` with an OPTIONAL per-request read-auth
/// validator (read-path-query-api-auth-v0, ADR-0074). The additive sibling
/// of [`router`]: existing callers keep [`router`] (env-tenant only, header
/// ignored — backward compatibility, US-RAUTH-02); the auth acceptance
/// suite calls this with `Some(validator)` to exercise the per-request
/// bearer path.
///
/// `auth` is `Some(Arc<Validator>)` when the binary's read-auth config is
/// present and complete (the validator is built once at composition,
/// audience `kaleidoscope-query`), `None` otherwise.
///
/// ## DISTILL scaffold state (RED-not-BROKEN, Mandate 7)
///
/// The router stores `auth` on the state, but the handler still resolves
/// the tenant through the EXISTING env-tenant seam
/// (`resolve_tenant_or_refuse`), so an auth-configured router behaves like
/// today's env-tenant router — which is exactly why every auth scenario
/// driven against it is behaviourally RED (a tokenless request is NOT
/// refused 401 by the bearer gate; an isolation read is NOT scoped to the
/// token's tenant). DELIVER swaps the handler call for
/// `query_http_common::resolve_request_tenant_or_refuse` and the scenarios
/// go green one at a time.
pub fn router_with_auth(
    store: Arc<dyn MetricStore + Send + Sync>,
    tenant: Option<TenantId>,
    auth: Option<Arc<aegis::Validator>>,
    static_dir: Option<PathBuf>,
) -> Router {
    let state = ApiState {
        store,
        tenant,
        auth,
    };
    let api = Router::new()
        .route(QUERY_RANGE_ROUTE, get(handle_query_range))
        .route(HELP_ROUTE, get(handle_help))
        .with_state(state);
    match static_dir {
        Some(dir) => api.fallback_service(spa_static_service(dir)),
        None => api,
    }
}

/// Build the static-serving fallback: a `ServeDir` rooted at the bundle
/// that serves existing files (`config.json`, assets) directly, and
/// falls back to `index.html` (served with its natural 200, NOT a 404)
/// for any path it cannot resolve so the SPA router owns deep links
/// (DD6: SPA index fallback, not a 404).
fn spa_static_service(dir: PathBuf) -> ServeDir<ServeFile> {
    let index = dir.join(INDEX_HTML);
    ServeDir::new(dir).fallback(ServeFile::new(index))
}

/// The four query parameters the contract pins. `step` is accepted and
/// ignored at v0 (DD5: raw points, no re-stepping).
#[derive(Debug, Deserialize)]
struct QueryRangeParams {
    query: String,
    start: String,
    end: String,
    #[serde(default)]
    #[allow(dead_code)]
    step: Option<String>,
}

/// Handle `GET /api/v1/query_range`. Never panics on bad input; every
/// failure mode is a `status:error` arm with the appropriate status
/// code. The orchestration is parse-bounds -> parse-selector ->
/// resolve-tenant -> query -> translate -> serialise.
async fn handle_query_range(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(params): Query<QueryRangeParams>,
) -> Response {
    // Fail-closed tenancy (DD7 / ADR-0074 DD3): resolve the per-request
    // tenant through the shared seam BEFORE touching the store. When auth is
    // configured the tenant comes from the validated bearer (no env
    // fall-through, the no-bearer-bypass); when it is not, the existing
    // env-tenant path runs and the header is ignored (backward compatibility).
    let tenant = match query_http_common::resolve_request_tenant_or_refuse(
        state.auth.as_ref(),
        &headers,
        &state.tenant,
        "the query",
        "query_range",
        std::time::SystemTime::now(),
    ) {
        Ok(tenant) => tenant,
        Err(resp) => return resp,
    };

    let tr = match query_http_common::parse_time_range(&params.start, &params.end) {
        Ok(tr) => tr,
        Err(reason) => return query_http_common::error_response(StatusCode::BAD_REQUEST, reason),
    };
    let (start_secs, end_secs) = (tr.start_secs, tr.end_secs);

    // Window cap (ADR-0050 Decision 1 / D5): the span is computed in
    // whole seconds, BEFORE the nanosecond conversion, and BEFORE the
    // store is touched. A request strictly over the cap is a 400; the
    // store is NEVER queried on this path. The reason names the cap
    // value (86400) verbatim and never echoes the raw window values.
    if end_secs.saturating_sub(start_secs) > MAX_WINDOW_SECONDS {
        return query_http_common::error_response(
            StatusCode::BAD_REQUEST,
            query_http_common::REASON_WINDOW_TOO_LARGE,
        );
    }

    let range = TimeRange::new(seconds_to_nanos(start_secs), seconds_to_nanos(end_secs));

    let selector = match selector::parse(&params.query) {
        Ok(selector) => selector,
        Err(reason) => return query_http_common::error_response(StatusCode::BAD_REQUEST, &reason),
    };

    // Compile the regex matchers ONCE, before the row scan (ADR-0046
    // Decision 3). A compile failure is the single origin of the
    // invalid-regex 400; the reason names the matcher invalid and never
    // echoes the offending pattern, the raw query, or a forwarded header.
    let filter = match matrix::build_filter(&selector.matchers) {
        Ok(filter) => filter,
        Err(reason) => return query_http_common::error_response(StatusCode::BAD_REQUEST, &reason),
    };

    match state.store.query(&tenant, &selector.name, range) {
        Ok(mut rows) => {
            rows.retain(|(metric, point)| matrix::keep_row(metric, point, &filter));
            let result = matrix::to_matrix(rows);
            // Result-size cap (ADR-0050 Decision 2 / D5): measured on
            // the FINAL matrix-entry count, AFTER `to_matrix` and BEFORE
            // serialisation. The count is what the user observes in
            // `data.result.length`. A count strictly over the cap is a
            // 400; serialisation never starts.
            if result.len() > MAX_RESULT_ROWS {
                return query_http_common::error_response(
                    StatusCode::BAD_REQUEST,
                    query_http_common::REASON_TOO_MANY_ROWS,
                );
            }
            success_response(result)
        }
        Err(err) => {
            tracing::error!(event = "query.store.failed", reason = %err);
            query_http_common::error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the backing metric store could not be read",
            )
        }
    }
}

/// Handle `GET /help` (FIX-B.1). Stateless: returns the plain-text usage
/// crib verbatim with an explicit `text/plain; charset=utf-8`
/// content-type so a browser or terminal renders it as text rather than
/// guessing. Takes no extractors, so it composes with the api router's
/// state without needing it.
async fn handle_help() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        HELP_BODY,
    )
        .into_response()
}

/// Whole seconds -> nanoseconds. Saturates rather than overflowing on an
/// implausibly large bound. Pillar-specific (pulse): kept per-consumer
/// because each consumer builds its pillar's nanosecond `TimeRange`
/// (ADR-0054 / ADR-0048 Decision 5).
fn seconds_to_nanos(seconds: u64) -> u64 {
    seconds.saturating_mul(1_000_000_000)
}

/// Serialise the success / empty arm: HTTP 200,
/// `{status:'success', data:{resultType:'matrix', result:[...]}}`.
fn success_response(result: Vec<matrix::PromMatrixEntry>) -> Response {
    let body = json!({
        "status": "success",
        "data": {
            "resultType": "matrix",
            "result": result,
        }
    });
    (StatusCode::OK, Json(body)).into_response()
}

// The inline tests that targeted `parse_time_range`, `parse_epoch_seconds`,
// `MAX_*` consts, and the cap reason literals now live canonically in
// `crates/query-http-common/src/lib.rs` (ADR-0054 / Mikado step E). The
// acceptance suite (`tests/*.rs`) is the byte-identity gate for the
// rewire.
