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
use ray::{ServiceName, TimeRange, TraceId, TraceStore};
use serde::Deserialize;

// The cap constants and the four reason text literals now live in
// `query-http-common` (ADR-0054). `pub use` preserves the existing
// `trace_query_api::MAX_WINDOW_SECONDS` / `trace_query_api::MAX_RESULT_ROWS`
// downstream-readable path (US-01 backward compatibility AC).
pub use query_http_common::{MAX_RESULT_ROWS, MAX_WINDOW_SECONDS};

/// The route the operator and any future same-origin prism trace panel
/// target: `/api/v1` prefix (ADR-0043) + `/traces` (ADR-0048 Decision 3).
const TRACES_ROUTE: &str = "/api/v1/traces";

/// The sibling lookup-by-id route (ADR-0053 Decision 1). Mounted on the
/// same `Router` as `TRACES_ROUTE`; shares `ApiState { store, tenant }`.
const TRACES_BY_ID_ROUTE: &str = "/api/v1/traces/by_id";

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
        .route(TRACES_BY_ID_ROUTE, get(handle_traces_by_id))
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
    // Fail-closed tenancy: refuse before touching the store via the
    // shared seam in query-http-common (ADR-0054).
    let tenant = match query_http_common::resolve_tenant_or_refuse(&state.tenant, "the trace query")
    {
        Ok(t) => t.clone(),
        Err(resp) => return resp,
    };

    // The one structural divergence from logs (ADR-0048 Decision 1):
    // traces require an explicit `service`. Missing or empty is a 400
    // BEFORE the store is touched.
    let service = match read_required_service(&params.service) {
        Ok(s) => s,
        Err(reason) => return query_http_common::error_response(StatusCode::BAD_REQUEST, &reason),
    };

    // Parse and validate the window BEFORE the store is touched: a
    // malformed or inverted window is a 400 that never runs a query.
    // A missing bound (None) flows through as the empty string into the
    // shared parser, which rejects it as a non-numeric 400. The wire
    // status is unchanged from the pre-extraction path (BAD_REQUEST);
    // the reason text class drifts from "is required" to "is not a
    // number" but no acceptance scenario asserts the specific literal.
    let tr = match query_http_common::parse_time_range(
        params.start.as_deref().unwrap_or(""),
        params.end.as_deref().unwrap_or(""),
    ) {
        Ok(tr) => tr,
        Err(reason) => return query_http_common::error_response(StatusCode::BAD_REQUEST, reason),
    };
    let (start_secs, end_secs) = (tr.start_secs, tr.end_secs);

    // Window cap (ADR-0050 Decision 1 / D5): the span is computed in
    // whole seconds, BEFORE the nanosecond conversion, and BEFORE the
    // store is touched. The handler order is fixed (tenant -> service
    // -> parse -> window -> store -> result -> serialise), so the
    // missing-service 400 still fires first on a request whose
    // `service` is absent.
    if end_secs.saturating_sub(start_secs) > MAX_WINDOW_SECONDS {
        return query_http_common::error_response(
            StatusCode::BAD_REQUEST,
            query_http_common::REASON_WINDOW_TOO_LARGE,
        );
    }

    let range = TimeRange::new(seconds_to_nanos(start_secs), seconds_to_nanos(end_secs));

    match state.store.query(&tenant, &service, range) {
        Ok(spans) => {
            // Result-size cap (ADR-0050 Decision 2 / D5): measured on
            // the spans vector the store returned, BEFORE
            // serialisation. A count strictly over the cap is a 400;
            // serialisation never starts.
            if spans.len() > MAX_RESULT_ROWS {
                return query_http_common::error_response(
                    StatusCode::BAD_REQUEST,
                    query_http_common::REASON_TOO_MANY_ROWS,
                );
            }
            success_response(spans)
        }
        Err(err) => {
            tracing::error!(event = "traces.store.failed", reason = %err);
            query_http_common::error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the backing trace store could not be read",
            )
        }
    }
}

/// The lookup-by-id query parameter. ADR-0053 Decision 2: exactly 32
/// hex characters (case-insensitive). `trace_id` is `Option<String>` so
/// axum's `Query` extractor accepts a request whose parameter is
/// absent and the handler emits the contract's named 400 itself
/// (rather than axum's default rejection body), mirroring
/// `TracesParams`.
///
/// Scaffold for DISTILL Mandate 7 RED-not-BROKEN: the type is declared
/// so the acceptance suite compiles; the handler is `unimplemented!`
/// until DELIVER lands the parse + wire.
#[derive(Debug, Deserialize)]
pub struct TracesByIdParams {
    pub trace_id: Option<String>,
}

/// Handle `GET /api/v1/traces/by_id?trace_id=<32-hex>`. ADR-0053
/// Decision 1 mounts this as a sibling route on the same `Router` as
/// `handle_traces`; the two share `ApiState { store, tenant }`. The
/// orchestration the DELIVER wave implements is: resolve-tenant
/// (fail-closed 401) -> read required `trace_id` (presence 400 BEFORE
/// the store) -> parse `trace_id` (32-hex case-insensitive; format 400
/// BEFORE the store) -> `TraceStore::get_trace` -> result cap (400) ->
/// serialise the bare array (200, `[]` when empty) -> map
/// `PersistenceFailed` to 500. Every malformed-input arm returns the
/// single literal class label `"invalid trace_id"` and NEVER echoes
/// the raw parameter value (ADR-0053 Decision 2, ADR-0048 Decision 2
/// redaction extended).
///
/// Scaffold for DISTILL Mandate 7 RED-not-BROKEN: the handler is
/// `unimplemented!` so the suite compiles and every scenario fails
/// RED (panic on call) rather than BROKEN (compile error). DELIVER
/// implements the body per the architecture brief and the scenarios
/// go green one at a time per the outer-loop convention.
async fn handle_traces_by_id(
    State(state): State<ApiState>,
    Query(params): Query<TracesByIdParams>,
) -> Response {
    // Fail-closed tenancy: refuse before touching the store via the
    // shared seam in query-http-common (ADR-0054). The same 401 envelope
    // and reason text as `handle_traces` so the two sibling routes are
    // indistinguishable on the unscoped path (ADR-0053 Decision 1;
    // ADR-0048 Decision 2 redaction extended).
    let tenant = match query_http_common::resolve_tenant_or_refuse(&state.tenant, "the trace query")
    {
        Ok(t) => t.clone(),
        Err(resp) => return resp,
    };

    // The single structural divergence from the window arm: the lookup
    // arm requires a `trace_id` of EXACTLY 32 hex characters (ADR-0053
    // Decision 2). Missing, empty, wrong-length, or non-hex are all the
    // same class of malformed input and collapse to the single literal
    // reason "invalid trace_id". The raw value is NEVER echoed (the
    // redaction posture from ADR-0048 Decision 2 extended to the new
    // parameter; ADR-0053 Decision 2 forbids clever diagnostics that
    // would leak a property of the raw value into the error text).
    let raw = match params.trace_id.as_deref() {
        Some(s) => s,
        None => {
            return query_http_common::error_response(StatusCode::BAD_REQUEST, "invalid trace_id");
        }
    };
    let trace_id = match parse_trace_id(raw) {
        Ok(id) => id,
        Err(_) => {
            return query_http_common::error_response(StatusCode::BAD_REQUEST, "invalid trace_id");
        }
    };

    match state.store.get_trace(&tenant, &trace_id) {
        Ok(spans) => {
            // Result-size cap (ADR-0050 Decision 2 / ADR-0053 Decision
            // 3): the cap is uniform across the two read arms. Measured
            // on the spans vector the store returned, BEFORE
            // serialisation. A count strictly over the cap is a 400;
            // serialisation never starts.
            if spans.len() > MAX_RESULT_ROWS {
                return query_http_common::error_response(
                    StatusCode::BAD_REQUEST,
                    query_http_common::REASON_TOO_MANY_ROWS,
                );
            }
            success_response(spans)
        }
        Err(err) => {
            tracing::error!(event = "traces.lookup.store.failed", reason = %err);
            query_http_common::error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the backing trace store could not be read",
            )
        }
    }
}

/// Parse the `trace_id` wire value to a `TraceId`. ADR-0053 Decision 2
/// pins exactly 32 hex characters, case-insensitive (matching the OTel /
/// W3C trace context spec and the substrate codec at
/// `crates/ray/src/span.rs:42-60` which accepts both `a-f` and `A-F`).
///
/// Empty, wrong-length, and non-hex inputs all collapse to the same
/// `Err("invalid trace_id")`; the raw value is NEVER carried in the
/// returned reason text (redaction; ADR-0048 Decision 2 extended). The
/// clever diagnostic "expected 32 chars, got N" is rejected — it would
/// leak a property of the raw value into the error text.
fn parse_trace_id(raw: &str) -> Result<TraceId, String> {
    if raw.len() != 32 {
        return Err("invalid trace_id".to_string());
    }
    let mut bytes = [0u8; 16];
    let raw_bytes = raw.as_bytes();
    for (i, slot) in bytes.iter_mut().enumerate() {
        let hi = (raw_bytes[i * 2] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid trace_id".to_string())?;
        let lo = (raw_bytes[i * 2 + 1] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid trace_id".to_string())?;
        *slot = ((hi << 4) | lo) as u8;
    }
    Ok(TraceId(bytes))
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

/// Whole seconds -> nanoseconds. Saturates rather than overflowing on
/// an implausibly large bound. Pillar-specific (ray): kept per-consumer
/// because each consumer builds its pillar's nanosecond `TimeRange`
/// (ADR-0054 / ADR-0048 Decision 5).
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

#[cfg(test)]
mod tests {
    use super::*;

    // The inline tests for `parse_time_range`, `parse_epoch_seconds`,
    // `MAX_*` consts, and the cap reason literals now live canonically in
    // `crates/query-http-common/src/lib.rs` (ADR-0054 / Mikado step G).
    // The acceptance suite (`tests/*.rs`) is the byte-identity gate for
    // the rewire. Pillar-specific helpers `parse_trace_id` and
    // `read_required_service` stay per-consumer; their inline tests
    // stay here.

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
}
