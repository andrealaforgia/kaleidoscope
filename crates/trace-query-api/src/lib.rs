// Kaleidoscope trace-query-api — HTTP read path for traces over ray
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

//! # trace-query-api — the read side of the traces pillar.
//!
//! Serves `GET /api/v1/traces?service=&start=&end=` out of the durable
//! ray store, read-only, returning the in-window `Span`s for the resolved
//! tenant and service as a plain JSON array (ADR-0048). The single public
//! driving port is [`router`]; the thin binary (`src/main.rs`) is the
//! composition root that opens the store, resolves the tenant, runs the
//! Earned-Trust probe, and binds the listener.
//!
//! ## Public surface
//!
//! - [`router`] — build an axum `Router` over a `TraceStore` and an
//!   `Option<TenantId>`. `None` models fail-closed tenancy at the
//!   router seam: every request is refused with a `status:error` body
//!   at 401.
//!
//! ## Architectural posture
//!
//! - Hexagonal: the `ray::TraceStore` driven port and the tenant seam
//!   are the only collaborators. The window parse/validate and the
//!   required-`service` read carry the only mutable logic and are
//!   unit-testable in isolation.
//! - Rust idiomatic: data + free functions; no inheritance, no `dyn`
//!   where generics suffice. The `Arc<dyn TraceStore>` indirection IS
//!   genuine polymorphism (the durable adapter in production, an
//!   in-memory or failing double in tests).
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

pub mod composition;

use std::sync::Arc;

use aegis::TenantId;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use ray::{ServiceName, TimeRange, TraceStore};
use serde::Deserialize;
use serde_json::json;

/// The route the operator and any future same-origin prism trace panel
/// target: `/api/v1` prefix (ADR-0043) + `/traces` (ADR-0048 Decision 3).
const TRACES_ROUTE: &str = "/api/v1/traces";

/// Maximum permitted query window in whole seconds (24 hours; ADR-0050
/// Decision 1). A request whose `end - start` in seconds STRICTLY
/// exceeds this value is refused with a named 400 BEFORE the store is
/// touched. A window of exactly `MAX_WINDOW_SECONDS` is served (the
/// boundary is inclusive).
pub const MAX_WINDOW_SECONDS: u64 = 86_400;

/// Maximum permitted span count in a single response (ADR-0050 Decision
/// 2). The count is measured on the `Vec<Span>` the store returns
/// BEFORE serialisation. A response of exactly `MAX_RESULT_ROWS` is
/// served (the boundary is inclusive). A response strictly greater is
/// refused with a named 400; serialisation never starts.
pub const MAX_RESULT_ROWS: usize = 100_000;

/// Application state shared with the handler: the trace-store driven
/// port and the resolved tenant (or `None` for fail-closed).
#[derive(Clone)]
struct ApiState {
    store: Arc<dyn TraceStore + Send + Sync>,
    tenant: Option<TenantId>,
}

/// Build the trace-query-api `Router`.
///
/// `store` is the `ray::TraceStore` driven port (the durable
/// `FileBackedTraceStore` in production, a double in tests). `tenant`
/// models fail-closed tenancy at the router seam: `Some(t)` is a
/// resolved tenant; `None` is "no tenant resolvable" and every request
/// is refused with a `status:error` body at 401. The production binary
/// maps `KALEIDOSCOPE_TRACE_QUERY_TENANT` (set/non-empty -> `Some`,
/// unset/empty -> `None`) onto this same `Option`, so the fail-closed
/// behaviour is identical in tests and in production.
pub fn router(store: Arc<dyn TraceStore + Send + Sync>, tenant: Option<TenantId>) -> Router {
    let state = ApiState { store, tenant };
    Router::new()
        .route(TRACES_ROUTE, get(handle_traces))
        .with_state(state)
}

/// The three query parameters the contract pins: `service`, `start`,
/// and `end`. `start`/`end` are epoch seconds (float-tolerant, mirroring
/// the sibling endpoints). `service` is the one structural divergence
/// from logs (ADR-0048 Decision 1): a required parameter, missing or
/// empty is a 400 caught before the store.
///
/// All three fields are `Option<String>` so axum's `Query` extractor
/// accepts a request with the parameter absent and the handler emits
/// the contract's named 400 itself (rather than axum's default
/// rejection body). The handler enforces presence and non-emptiness.
#[derive(Debug, Deserialize)]
struct TracesParams {
    service: Option<String>,
    start: Option<String>,
    end: Option<String>,
}

/// Handle `GET /api/v1/traces?service=&start=&end=`. Never panics on
/// bad input; every failure mode is a `status:error` arm with the
/// appropriate status code. The orchestration is resolve-tenant
/// (fail-closed 401) -> read and validate `service` (400 on missing or
/// empty, before the store) -> parse-bounds (400 before the store) ->
/// `TraceStore::query` -> serialise the bare array (200, `[]` when
/// empty) -> map `PersistenceFailed` to 500.
async fn handle_traces(
    State(state): State<ApiState>,
    Query(params): Query<TracesParams>,
) -> Response {
    // Fail-closed tenancy: refuse before touching the store.
    let tenant = match &state.tenant {
        Some(t) => t.clone(),
        None => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                "no tenant resolvable: the trace query service refuses unscoped requests",
            );
        }
    };

    // The one structural divergence from logs (ADR-0048 Decision 1):
    // traces require an explicit `service`. Missing or empty is a 400
    // BEFORE the store is touched.
    let service = match read_required_service(&params.service) {
        Ok(s) => s,
        Err(reason) => return error_response(StatusCode::BAD_REQUEST, &reason),
    };

    // Parse and validate the window BEFORE the store is touched: a
    // malformed or inverted window is a 400 that never runs a query.
    let (start_secs, end_secs) =
        match parse_time_range_seconds(params.start.as_deref(), params.end.as_deref()) {
            Ok(secs) => secs,
            Err(reason) => return error_response(StatusCode::BAD_REQUEST, &reason),
        };

    // Window cap (ADR-0050 Decision 1 / D5): the span is computed in
    // whole seconds, BEFORE the nanosecond conversion, and BEFORE the
    // store is touched. The handler order is fixed (tenant -> service
    // -> parse -> window -> store -> result -> serialise), so the
    // missing-service 400 still fires first on a request whose
    // `service` is absent.
    if end_secs.saturating_sub(start_secs) > MAX_WINDOW_SECONDS {
        return error_response(StatusCode::BAD_REQUEST, "window exceeds 86400 seconds");
    }

    let range = TimeRange::new(seconds_to_nanos(start_secs), seconds_to_nanos(end_secs));

    match state.store.query(&tenant, &service, range) {
        Ok(spans) => {
            // Result-size cap (ADR-0050 Decision 2 / D5): measured on
            // the spans vector the store returned, BEFORE
            // serialisation. A count strictly over the cap is a 400;
            // serialisation never starts.
            if spans.len() > MAX_RESULT_ROWS {
                return error_response(StatusCode::BAD_REQUEST, "result exceeds 100000 rows");
            }
            success_response(spans)
        }
        Err(err) => {
            tracing::error!(event = "traces.store.failed", reason = %err);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the backing trace store could not be read",
            )
        }
    }
}

/// Read the required `service` parameter. The handler must see a
/// non-empty value; missing or empty are both 400. The reason names
/// the missing parameter WITHOUT echoing any raw value (the redaction
/// is stricter than logs: the body must contain neither "SECRET" nor
/// "Bearer", and never the raw `service`).
fn read_required_service(raw: &Option<String>) -> Result<ServiceName, String> {
    match raw {
        Some(s) if !s.is_empty() => Ok(ServiceName::new(s.clone())),
        Some(_) => Err("invalid request: service is empty".to_string()),
        None => Err("invalid request: service is required".to_string()),
    }
}

/// Parse `start`/`end` epoch-seconds strings into a half-open nanosecond
/// [`TimeRange`]. Rejects missing, non-numeric, and inverted bounds
/// (`end < start`). Float-tolerant: a fractional `.0` suffix is parsed
/// as `f64` then truncated to whole seconds, mirroring the sibling
/// endpoints.
///
/// Test-only since ADR-0050: the handler computes the cap span in
/// seconds via [`parse_time_range_seconds`] BEFORE the nanosecond
/// conversion. The inline parser tests continue to assert the
/// pre-conversion behaviour through this thin wrapper.
#[cfg(test)]
fn parse_time_range(start: Option<&str>, end: Option<&str>) -> Result<TimeRange, String> {
    let (start_secs, end_secs) = parse_time_range_seconds(start, end)?;
    Ok(TimeRange::new(
        seconds_to_nanos(start_secs),
        seconds_to_nanos(end_secs),
    ))
}

/// Parse `start`/`end` epoch-seconds into `(start_secs, end_secs)` as
/// whole seconds. Same validation as [`parse_time_range`]; the window
/// cap check (ADR-0050 D5) needs the seconds span BEFORE the nanosecond
/// conversion.
fn parse_time_range_seconds(start: Option<&str>, end: Option<&str>) -> Result<(u64, u64), String> {
    let start_secs = parse_epoch_seconds(start, "start")?;
    let end_secs = parse_epoch_seconds(end, "end")?;
    if end_secs < start_secs {
        return Err("invalid time bounds: end is earlier than start".to_string());
    }
    Ok((start_secs, end_secs))
}

/// Parse one epoch-seconds bound as a non-negative number of whole
/// seconds. The `field` name is named in the error but the raw value is
/// NOT echoed (redaction symmetry with ADR-0048 Decision 1). A missing
/// bound is a 400 too: the contract pins both `start` and `end`.
fn parse_epoch_seconds(raw: Option<&str>, field: &str) -> Result<u64, String> {
    let raw = raw.ok_or_else(|| format!("invalid time bounds: {field} is required"))?;
    let trimmed = raw.trim();
    let parsed: f64 = trimmed
        .parse()
        .map_err(|_| format!("invalid time bounds: {field} is not a number"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(format!("invalid time bounds: {field} is out of range"));
    }
    Ok(parsed as u64)
}

/// Whole seconds -> nanoseconds. Saturates rather than overflowing on
/// an implausibly large bound.
fn seconds_to_nanos(seconds: u64) -> u64 {
    seconds.saturating_mul(1_000_000_000)
}

/// Serialise the success / empty arm: HTTP 200 with a BARE JSON array
/// of the in-window `Span`s (ADR-0048 Decision 2), in the store's
/// ascending `start_time_unix_nano` order. The empty arm is `[]`, a
/// calm 200, never an error. `Span` carries its own `Serialize` derive,
/// so the array is faithful with no hand-written mapping.
fn success_response(spans: Vec<ray::Span>) -> Response {
    (StatusCode::OK, Json(spans)).into_response()
}

/// Serialise an error arm: `{status:'error', error:'<reason>'}` at the
/// given status code. The reason never echoes a forwarded header value,
/// the raw query, the raw `service`, or credentials (ADR-0048 Decision
/// 1 redaction; stricter than logs: the body must contain neither
/// "SECRET" nor "Bearer").
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

    // The acceptance suite reaches the missing/empty-service 400, the
    // non-numeric and inverted window 400 arms, the no-tenant 401, the
    // store-failure 500, the calm-empty 200, the half-open boundary,
    // and the redaction. These inline tests pin the remaining
    // boundaries one-by-one so mutation survivors at the parser, the
    // service reader, and the error envelope are killed.

    #[test]
    fn equal_bounds_are_accepted_as_an_empty_half_open_range() {
        // start == end is a valid (empty) half-open range, NOT an
        // inverted-bounds rejection. Kills a `<` -> `<=` mutant on the
        // inversion check.
        let range = parse_time_range(Some("100"), Some("100")).expect("equal bounds are valid");
        assert_eq!(range.start_unix_nano, 100_000_000_000);
        assert_eq!(range.end_unix_nano, 100_000_000_000);
    }

    #[test]
    fn an_inverted_window_is_rejected_before_any_store_query() {
        // start (later) > end (earlier) is the inverted window; the
        // parse rejects it so the store is never reached.
        assert!(parse_time_range(Some("200"), Some("100")).is_err());
    }

    #[test]
    fn fractional_epoch_seconds_truncate_to_whole_seconds() {
        // The window is float-tolerant; a `.5` fraction must parse and
        // truncate to whole seconds.
        let range = parse_time_range(Some("100.5"), Some("200.9")).expect("float bounds parse");
        assert_eq!(range.start_unix_nano, 100_000_000_000);
        assert_eq!(range.end_unix_nano, 200_000_000_000);
    }

    #[test]
    fn a_negative_bound_is_rejected_but_zero_is_accepted() {
        assert!(parse_time_range(Some("-1"), Some("100")).is_err());
        // Zero is the epoch and a valid lower bound; the out-of-range
        // check is `< 0.0`, NOT `<= 0.0`. Pins that boundary so a `<`
        // -> `<=` mutant (which would reject a zero start) is caught.
        let range = parse_time_range(Some("0"), Some("100")).expect("zero is a valid bound");
        assert_eq!(range.start_unix_nano, 0);
    }

    #[test]
    fn a_non_numeric_start_or_end_is_rejected() {
        assert!(parse_time_range(Some("notanumber"), Some("100")).is_err());
        assert!(parse_time_range(Some("100"), Some("later")).is_err());
    }

    #[test]
    fn a_missing_start_or_end_is_rejected() {
        assert!(parse_time_range(None, Some("100")).is_err());
        assert!(parse_time_range(Some("100"), None).is_err());
    }

    #[test]
    fn the_bounds_error_never_echoes_the_raw_value() {
        let reason = parse_epoch_seconds(Some("secretvalue"), "start").expect_err("rejected");
        assert!(!reason.contains("secretvalue"));
    }

    #[test]
    fn the_window_converts_seconds_to_the_nanosecond_ray_time_range() {
        // The store sorts and filters in nanoseconds; the seconds bounds
        // must be scaled by 1e9. Kills a mutant that drops the scaling.
        let range = parse_time_range(Some("1716200000"), Some("1716200060")).expect("valid window");
        assert_eq!(range.start_unix_nano, 1_716_200_000_000_000_000);
        assert_eq!(range.end_unix_nano, 1_716_200_060_000_000_000);
    }

    #[test]
    fn the_half_open_range_includes_start_and_excludes_end() {
        // The contract's half-open boundary is the ray TimeRange's:
        // a span at exactly start is included, at exactly end excluded.
        let range = parse_time_range(Some("100"), Some("200")).expect("valid window");
        assert!(range.contains(range.start_unix_nano), "start is included");
        assert!(!range.contains(range.end_unix_nano), "end is excluded");
    }

    #[test]
    fn a_missing_service_parameter_is_rejected_before_the_store() {
        // The one structural divergence from logs (ADR-0048 Decision 1).
        let err = read_required_service(&None).expect_err("rejected");
        assert!(
            err.contains("required"),
            "the reason names the missing parameter: {err}"
        );
    }

    #[test]
    fn an_empty_service_parameter_is_rejected_before_the_store() {
        let err = read_required_service(&Some(String::new())).expect_err("rejected");
        assert!(
            err.contains("empty"),
            "the reason names the empty value: {err}"
        );
    }

    #[test]
    fn the_service_error_never_echoes_the_raw_service_value_or_a_credential() {
        // The redaction is stricter than logs: the body must contain
        // neither "SECRET" nor "Bearer", and never the raw service.
        let empty_err = read_required_service(&Some(String::new())).expect_err("rejected");
        assert!(!empty_err.contains("SECRET"));
        assert!(!empty_err.contains("Bearer"));
        let missing_err = read_required_service(&None).expect_err("rejected");
        assert!(!missing_err.contains("SECRET"));
        assert!(!missing_err.contains("Bearer"));
    }

    #[test]
    fn a_non_empty_service_resolves_to_a_service_name() {
        let svc = read_required_service(&Some("checkout".to_string())).expect("resolves");
        assert_eq!(svc.as_str(), "checkout");
    }

    // ----- ADR-0050 cap-check inline tests -----

    #[test]
    fn the_window_cap_constant_matches_the_adr_value() {
        assert_eq!(MAX_WINDOW_SECONDS, 86_400);
    }

    #[test]
    fn the_result_cap_constant_matches_the_adr_value() {
        assert_eq!(MAX_RESULT_ROWS, 100_000);
    }

    #[test]
    fn parse_time_range_seconds_returns_the_unscaled_seconds_for_the_cap_check() {
        let (start, end) =
            parse_time_range_seconds(Some("0"), Some("86400")).expect("valid bounds");
        assert_eq!(start, 0);
        assert_eq!(end, 86_400);
        let (s2, e2) = parse_time_range_seconds(Some("0"), Some("86401")).expect("valid bounds");
        assert_eq!(e2 - s2, 86_401);
    }

    #[test]
    fn the_window_cap_reason_names_the_cap_class_and_the_cap_value() {
        let reason = "window exceeds 86400 seconds";
        assert!(reason.contains("window"));
        assert!(reason.contains("86400"));
    }

    #[test]
    fn the_result_cap_reason_names_the_cap_class_and_the_cap_value() {
        let reason = "result exceeds 100000 rows";
        assert!(reason.contains("result"));
        assert!(reason.contains("100000"));
    }

    #[test]
    fn the_cap_reasons_never_contain_a_forwarded_credential_marker() {
        // Stricter trace posture: never SECRET or Bearer anywhere in
        // the body. Both literal cap reasons honour that already.
        let window_reason = "window exceeds 86400 seconds";
        let result_reason = "result exceeds 100000 rows";
        for reason in [window_reason, result_reason] {
            assert!(!reason.contains("SECRET"));
            assert!(!reason.contains("Bearer"));
        }
    }
}
