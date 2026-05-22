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
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use pulse::{MetricStore, TimeRange};
use serde::Deserialize;
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

/// The route path Prism's `buildUrl` targets: `backend.url` prefix
/// `/api/v1` + `/query_range` (verified in `queryRange.ts`).
const QUERY_RANGE_ROUTE: &str = "/api/v1/query_range";

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
    let state = ApiState { store, tenant };
    let api = Router::new()
        .route(QUERY_RANGE_ROUTE, get(handle_query_range))
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
    Query(params): Query<QueryRangeParams>,
) -> Response {
    // Fail-closed tenancy (DD7): refuse before touching the store.
    let tenant = match &state.tenant {
        Some(t) => t.clone(),
        None => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                "no tenant resolvable: the query service refuses unscoped requests",
            );
        }
    };

    let range = match parse_time_range(&params.start, &params.end) {
        Ok(range) => range,
        Err(reason) => return error_response(StatusCode::BAD_REQUEST, &reason),
    };

    let selector = match selector::parse(&params.query) {
        Ok(selector) => selector,
        Err(reason) => return error_response(StatusCode::BAD_REQUEST, &reason),
    };

    // Compile the regex matchers ONCE, before the row scan (ADR-0046
    // Decision 3). A compile failure is the single origin of the
    // invalid-regex 400; the reason names the matcher invalid and never
    // echoes the offending pattern, the raw query, or a forwarded header.
    let filter = match matrix::build_filter(&selector.matchers) {
        Ok(filter) => filter,
        Err(reason) => return error_response(StatusCode::BAD_REQUEST, &reason),
    };

    match state.store.query(&tenant, &selector.name, range) {
        Ok(mut rows) => {
            rows.retain(|(metric, point)| matrix::keep_row(metric, point, &filter));
            let result = matrix::to_matrix(rows);
            success_response(result)
        }
        Err(err) => {
            tracing::error!(event = "query.store.failed", reason = %err);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the backing metric store could not be read",
            )
        }
    }
}

/// Parse `start`/`end` epoch-seconds strings into a half-open nanosecond
/// [`TimeRange`]. Rejects non-numeric bounds and inverted bounds
/// (`end < start`). Prism emits floats (`buildUrl` does `.toString()`
/// on a `Date.getTime()/1000`), so a fractional `.0` suffix is tolerated
/// by parsing as `f64` then truncating to whole seconds.
fn parse_time_range(start: &str, end: &str) -> Result<TimeRange, String> {
    let start_secs = parse_epoch_seconds(start, "start")?;
    let end_secs = parse_epoch_seconds(end, "end")?;
    if end_secs < start_secs {
        return Err("invalid time bounds: end is earlier than start".to_string());
    }
    Ok(TimeRange::new(
        seconds_to_nanos(start_secs),
        seconds_to_nanos(end_secs),
    ))
}

/// Parse one epoch-seconds bound as a non-negative number of whole
/// seconds. The `field` name is named in the error but the raw value is
/// NOT echoed (DD6 redaction symmetry).
fn parse_epoch_seconds(raw: &str, field: &str) -> Result<u64, String> {
    let trimmed = raw.trim();
    let parsed: f64 = trimmed
        .parse()
        .map_err(|_| format!("invalid time bounds: {field} is not a number"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(format!("invalid time bounds: {field} is out of range"));
    }
    Ok(parsed as u64)
}

/// Whole seconds -> nanoseconds. Saturates rather than overflowing on an
/// implausibly large bound.
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

/// Serialise an error arm: `{status:'error', error:'<reason>'}` at the
/// given status code. The reason never echoes a forwarded header value
/// or the raw query (DD6).
fn error_response(status: StatusCode, reason: &str) -> Response {
    let body = json!({
        "status": "error",
        "error": reason,
    });
    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The acceptance suite reaches the bounds happy path, the
    // non-numeric-start reject, and the inverted-bounds reject. These
    // inline tests pin the remaining boundaries the acceptance suite
    // does not isolate one-by-one.

    #[test]
    fn equal_bounds_are_accepted_as_an_empty_half_open_range() {
        // start == end is a valid (empty) half-open range, NOT an
        // inverted-bounds rejection. Kills a `<` -> `<=` mutant on the
        // inversion check.
        let range = parse_time_range("100", "100").expect("equal bounds are valid");
        assert_eq!(range.start_unix_nano, 100_000_000_000);
        assert_eq!(range.end_unix_nano, 100_000_000_000);
    }

    #[test]
    fn fractional_epoch_seconds_truncate_to_whole_seconds() {
        // Prism emits floats; a `.5` fraction must parse and truncate.
        let range = parse_time_range("100.5", "200.9").expect("float bounds parse");
        assert_eq!(range.start_unix_nano, 100_000_000_000);
        assert_eq!(range.end_unix_nano, 200_000_000_000);
    }

    #[test]
    fn a_negative_bound_is_rejected_but_zero_is_accepted() {
        assert!(parse_time_range("-1", "100").is_err());
        // Zero is the epoch and a perfectly valid lower bound; the
        // out-of-range check is `< 0.0`, NOT `<= 0.0`. This pins that
        // boundary so a `<` -> `<=` mutant (which would reject a zero
        // start) is caught.
        let range = parse_time_range("0", "100").expect("zero is a valid bound");
        assert_eq!(range.start_unix_nano, 0);
    }

    #[test]
    fn a_non_numeric_end_is_rejected() {
        // The acceptance suite covers a non-numeric START; this pins the
        // END branch so a mutant that skips parsing `end` is caught.
        assert!(parse_time_range("100", "later").is_err());
    }

    #[test]
    fn the_bounds_error_never_echoes_the_raw_value() {
        let reason = parse_epoch_seconds("secretvalue", "start").expect_err("rejected");
        assert!(!reason.contains("secretvalue"));
    }
}
