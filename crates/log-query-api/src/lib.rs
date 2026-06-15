// Kaleidoscope log-query-api — HTTP read path for logs over Lumen
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

//! # log-query-api — the read side of the logs pillar.
//!
//! Serves `GET /api/v1/logs?start=&end=` out of the durable Lumen store,
//! read-only, returning the in-window `LogRecord`s for the resolved
//! tenant as a plain JSON array (ADR-0047). The single public driving
//! port is [`router`]; the thin binary (`src/main.rs`) is the
//! composition root that opens the store, resolves the tenant, runs the
//! Earned-Trust probe, and binds the listener.
//!
//! ## Public surface
//!
//! - [`router`] — build an axum `Router` over a `LogStore` and an
//!   `Option<TenantId>`. `None` models fail-closed tenancy at the
//!   router seam: every request is refused with a `status:error` body
//!   at 401.
//!
//! ## Architectural posture
//!
//! - Hexagonal: the `lumen::LogStore` driven port and the tenant seam
//!   are the only collaborators. The window parse/validate carries the
//!   only mutable logic and is unit-testable in isolation.
//! - Rust idiomatic: data + free functions; no inheritance, no `dyn`
//!   where generics suffice. The `Arc<dyn LogStore>` indirection IS
//!   genuine polymorphism (the durable adapter in production, an
//!   in-memory or failing double in tests).
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

pub mod composition;

use std::sync::Arc;

use aegis::TenantId;
use axum::extract::{Query, State};
use axum::http::header::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use lumen::{LogStore, Predicate, SeverityNumber, TimeRange};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// The cap constants and the four reason text literals now live in
// `query-http-common` (ADR-0054). `pub use` preserves the existing
// `log_query_api::MAX_WINDOW_SECONDS` / `log_query_api::MAX_RESULT_ROWS`
// downstream-readable path (US-01 backward compatibility AC).
pub use query_http_common::{MAX_RESULT_ROWS, MAX_WINDOW_SECONDS};

/// The route Prism's future log panel and the operator both target:
/// `/api/v1` prefix (ADR-0043) + `/logs` (ADR-0047 Decision 3).
const LOGS_ROUTE: &str = "/api/v1/logs";

/// Application state shared with the handler: the log-store driven port
/// and the resolved tenant (or `None` for fail-closed).
#[derive(Clone)]
struct ApiState {
    store: Arc<dyn LogStore + Send + Sync>,
    tenant: Option<TenantId>,
    /// OPTIONAL per-request read-auth validator (read-path-query-api-auth-v0,
    /// ADR-0074). See `query-api`'s `ApiState::auth` for the contract; the
    /// scaffold stores it, the existing handler still ignores it (RED until
    /// DELIVER swaps in `resolve_request_tenant_or_refuse`).
    auth: Option<Arc<aegis::Validator>>,
}

/// Build the log-query-api `Router`.
///
/// `store` is the `lumen::LogStore` driven port (the durable
/// `FileBackedLogStore` in production, a double in tests). `tenant`
/// models fail-closed tenancy at the router seam: `Some(t)` is a resolved
/// tenant; `None` is "no tenant resolvable" and every request is refused
/// with a `status:error` body at 401. The production binary maps
/// `KALEIDOSCOPE_LOG_QUERY_TENANT` (set/non-empty -> `Some`, unset/empty
/// -> `None`) onto this same `Option`, so the fail-closed behaviour is
/// identical in tests and in production.
pub fn router(store: Arc<dyn LogStore + Send + Sync>, tenant: Option<TenantId>) -> Router {
    router_with_auth(store, tenant, None)
}

/// Build the log-query-api `Router` with an OPTIONAL per-request read-auth
/// validator (read-path-query-api-auth-v0, ADR-0074). The additive sibling
/// of [`router`]; see `query_api::router_with_auth` for the full contract
/// and the RED-not-BROKEN scaffold posture. The auth acceptance suite calls
/// this with `Some(validator)`; existing callers keep [`router`].
pub fn router_with_auth(
    store: Arc<dyn LogStore + Send + Sync>,
    tenant: Option<TenantId>,
    auth: Option<Arc<aegis::Validator>>,
) -> Router {
    let state = ApiState {
        store,
        tenant,
        auth,
    };
    Router::new()
        .route(LOGS_ROUTE, get(handle_logs))
        .with_state(state)
}

/// The query parameters the contract pins: `start` and `end` in epoch
/// seconds (float-tolerant, mirroring the metrics endpoint), plus the
/// optional `min_severity` floor introduced by ADR-0052.
///
/// `min_severity` is an additive optional parameter: a missing value
/// deserialises as `None` and the handler keeps its prior unfiltered
/// behaviour. A present value (including the empty string `""`) is
/// `Some(_)` and runs through `parse_min_severity`; an unknown name is
/// rejected with the redacted 400 envelope BEFORE the store is touched.
#[derive(Debug, Deserialize)]
struct LogsParams {
    start: String,
    end: String,
    min_severity: Option<String>,
    /// ADR-0055 (log-body-text-search-v0). The `body_contains`
    /// optional parameter narrows the response to records whose
    /// `body` field contains the supplied substring (byte-wise,
    /// case-sensitive). Validated via [`parse_body_contains`].
    body_contains: Option<String>,
    /// ADR-0056 (log-body-regex-search-v0). The `body_regex`
    /// optional parameter narrows the response to records whose
    /// `body` field is matched by the supplied regular expression
    /// (`Regex::is_match`, unanchored, byte-wise case-sensitive by
    /// default). Validated and compiled via [`parse_body_regex`].
    /// Mutually exclusive with `body_contains` at slice 01 (DD4).
    body_regex: Option<String>,
    /// ADR-0057 (log-query-pagination-v0). The `limit` optional
    /// parameter bounds the response to at most the first `n` records
    /// of the ordered, post-filter result set. Validated via
    /// [`parse_limit`]: `0`, negative, non-numeric, and over-cap values
    /// are each rejected with the literal `"invalid limit"` 400 BEFORE
    /// the store is touched.
    limit: Option<String>,
    /// ADR-0057 (log-query-pagination-v0). The `offset` optional
    /// parameter skips the first `n` records of the ordered,
    /// post-filter result set before `limit` is applied. Validated via
    /// [`parse_offset`]: `0` is VALID (the first page); negative and
    /// non-numeric values are rejected with the literal
    /// `"invalid offset"` 400. There is NO upper cap; an offset past
    /// the end yields the calm empty page `[]` (HTTP 200), not a 400.
    offset: Option<String>,
    /// PG-2 (log-trace-id-filter). The `trace_id` optional parameter
    /// narrows the response to records whose `trace_id` equals the
    /// supplied id. The wire value is the SAME 32-char lowercase hex
    /// string the response RENDERS via `to_lowercase_hex` (case-insensitive
    /// on input), so an operator can copy an id out of one log and hand it
    /// straight back. Validated via [`parse_trace_id`]: a value that is not
    /// exactly 32 hex characters is rejected with the redacted 400 envelope
    /// BEFORE the store is touched. A missing parameter is `None` and the
    /// handler keeps its prior unfiltered behaviour; a valid id that matches
    /// no record is the calm empty array (HTTP 200), never a 404.
    trace_id: Option<String>,
}

/// Maximum permitted byte length of a `body_contains` value (ADR-0055
/// Decision 5 / DD6). A non-empty value of exactly 1024 bytes is
/// served; 1025 bytes or more is refused with the literal envelope.
const MAX_BODY_CONTAINS_LEN: usize = 1024;

/// Maximum permitted byte length of a `body_regex` value (ADR-0056
/// Decision 5 / DD3). A non-empty value of exactly 1024 bytes is
/// served; 1025 bytes or more is refused with the literal envelope.
/// Mirrors `MAX_BODY_CONTAINS_LEN` exactly so operators learn ONE
/// rule for every body-related parameter.
const MAX_BODY_REGEX_LEN: usize = 1024;

/// Handle `GET /api/v1/logs?start=&end=`. Never panics on bad input;
/// every failure mode is a `status:error` arm with the appropriate
/// status code. The orchestration is resolve-tenant (fail-closed 401)
/// -> parse-bounds (400 before the store) -> `LogStore::query` ->
/// serialise the bare array (200, `[]` when empty) -> map
/// `PersistenceFailed` to 500.
async fn handle_logs(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(params): Query<LogsParams>,
) -> Response {
    // Fail-closed tenancy (ADR-0074 DD3): resolve the per-request tenant
    // through the shared seam BEFORE touching the store. Auth configured ->
    // the tenant comes from the validated bearer (no env fall-through, the
    // no-bearer-bypass); auth absent -> the existing env path, header
    // ignored (backward compatibility, US-RAUTH-02).
    let tenant = match query_http_common::resolve_request_tenant_or_refuse(
        state.auth.as_ref(),
        &headers,
        &state.tenant,
        "the log query",
        "log_query",
        std::time::SystemTime::now(),
    ) {
        Ok(tenant) => tenant,
        Err(resp) => return resp,
    };

    // Parse and validate the window BEFORE the store is touched: a
    // malformed or inverted window is a 400 that never runs a query.
    let tr = match query_http_common::parse_time_range(&params.start, &params.end) {
        Ok(tr) => tr,
        Err(reason) => return query_http_common::error_response(StatusCode::BAD_REQUEST, reason),
    };
    let (start_secs, end_secs) = (tr.start_secs, tr.end_secs);

    // Window cap (ADR-0050 Decision 1 / D5): the span is computed in
    // whole seconds, BEFORE the nanosecond conversion, and BEFORE the
    // store is touched. A request strictly over the cap is a 400; the
    // store is NEVER queried on this path.
    if end_secs.saturating_sub(start_secs) > MAX_WINDOW_SECONDS {
        return query_http_common::error_response(
            StatusCode::BAD_REQUEST,
            query_http_common::REASON_WINDOW_TOO_LARGE,
        );
    }

    // Severity parse (ADR-0052 D5 / D8): runs AFTER the window cap and
    // BEFORE the store is touched. An unknown name is the named 400 and
    // the store is NEVER queried on this path. A missing parameter is
    // `None` and the handler keeps its prior unfiltered behaviour. A
    // present empty value (`?min_severity=`) is `Some("")` and is
    // rejected as unknown (NOT a missing-parameter shortcut).
    let min_severity = match params.min_severity.as_deref() {
        None => None,
        Some(raw) => match parse_min_severity(raw) {
            Ok(sev) => Some(sev),
            Err(_) => {
                return query_http_common::error_response(
                    StatusCode::BAD_REQUEST,
                    "unknown severity",
                );
            }
        },
    };

    // Body-contains parse (ADR-0055 Decision 7 / DD4-DD6): runs AFTER
    // the severity parse and BEFORE the store is touched. An empty
    // value or an over-cap value is the named 400 and the store is
    // NEVER queried on this path. A missing parameter is `None` and
    // the handler keeps its prior dispatch behaviour.
    let body_contains = match params.body_contains.as_deref() {
        None => None,
        Some(raw) => match parse_body_contains(raw) {
            Ok(target) => Some(target),
            Err(reason) => {
                return query_http_common::error_response(StatusCode::BAD_REQUEST, reason);
            }
        },
    };

    // Mutual-exclusion check (ADR-0056 Decision 7 / DD4): runs AFTER
    // `parse_body_contains` (so its own empty / over-cap 400 surfaces
    // first) and BEFORE `parse_body_regex` (so an honest cross-check
    // 400 is not masked by a downstream compile-failure 400 when both
    // values are syntactically valid but mutually-exclusively
    // present). Store is NEVER touched on this path. The reason text
    // is a static literal; the raw values are NEVER echoed.
    if body_contains.is_some() && params.body_regex.is_some() {
        return query_http_common::error_response(
            StatusCode::BAD_REQUEST,
            "specify body_regex or body_contains, not both",
        );
    }

    // Body-regex parse (ADR-0056 Decision 8 / DD1, DD2, DD3): runs
    // AFTER the mutual-exclusion check and BEFORE the store is
    // touched. An empty value, an over-cap value, or a
    // compile-failure value is the named 400 and the store is NEVER
    // queried on this path. A missing parameter is `None` and the
    // handler keeps its prior dispatch behaviour. The raw value is
    // NEVER echoed in the reason text.
    let body_regex = match params.body_regex.as_deref() {
        None => None,
        Some(raw) => match parse_body_regex(raw) {
            Ok(re) => Some(re),
            Err(reason) => {
                return query_http_common::error_response(StatusCode::BAD_REQUEST, reason);
            }
        },
    };

    // Pagination parse (ADR-0057 Decision 5, 6 / log-query-pagination-v0):
    // runs AFTER the filter parses and BEFORE the store is touched, so an
    // invalid `limit` or `offset` is a parse-time 400 that NEVER queries the
    // store (the no-store-call invariant). A missing `limit` is `None` (take
    // all, the cap is the backstop); a missing `offset` defaults to `0`. The
    // raw value is NEVER echoed: each reason is a static literal. The page
    // slice itself runs LATER, after the result-cap check, on the
    // post-filter, PRE-slice vector (cap-then-slice order, Decision 6).
    let limit = match params.limit.as_deref() {
        None => None,
        Some(raw) => match parse_limit(raw) {
            Ok(n) => Some(n),
            Err(reason) => {
                return query_http_common::error_response(StatusCode::BAD_REQUEST, reason);
            }
        },
    };
    let offset = match params.offset.as_deref() {
        None => 0,
        Some(raw) => match parse_offset(raw) {
            Ok(n) => n,
            Err(reason) => {
                return query_http_common::error_response(StatusCode::BAD_REQUEST, reason);
            }
        },
    };

    // Trace-id parse (PG-2 / log-trace-id-filter): runs AFTER the
    // pagination parse and BEFORE the store is touched, so an invalid
    // value is a parse-time 400 that NEVER queries the store (the
    // no-store-call invariant shared by every other parameter). A
    // missing parameter is `None` and the handler keeps its prior
    // unfiltered behaviour. The raw value is NEVER echoed: the reason is
    // a static literal naming the expected format.
    let trace_id_filter = match params.trace_id.as_deref() {
        None => None,
        Some(raw) => match parse_trace_id(raw) {
            Ok(bytes) => Some(bytes),
            Err(reason) => {
                return query_http_common::error_response(StatusCode::BAD_REQUEST, reason)
            }
        },
    };

    let range = TimeRange::new(seconds_to_nanos(start_secs), seconds_to_nanos(end_secs));

    // Dispatch (ADR-0056 Decision 8 / application-architecture.md
    // Combinations Table): six reachable arms by the cross product
    // `min_severity x exactly-one-of {none, body_contains,
    // body_regex}`. The two arms in which BOTH `body_contains` AND
    // `body_regex` would be `Some` are pruned by the
    // mutual-exclusion check above and are therefore unreachable
    // here. When all three filters are absent, fall through to
    // `query` (the slice-prior backward-compat path).
    let query_result = match (min_severity, body_contains, body_regex) {
        (None, None, None) => state.store.query(&tenant, range),
        (Some(floor), None, None) => {
            state
                .store
                .query_with(&tenant, range, &Predicate::new().min_severity(floor))
        }
        (None, Some(target), None) => {
            state
                .store
                .query_with(&tenant, range, &Predicate::new().body_contains(target))
        }
        (None, None, Some(re)) => {
            state
                .store
                .query_with(&tenant, range, &Predicate::new().body_regex(re))
        }
        (Some(floor), Some(target), None) => state.store.query_with(
            &tenant,
            range,
            &Predicate::new().min_severity(floor).body_contains(target),
        ),
        (Some(floor), None, Some(re)) => state.store.query_with(
            &tenant,
            range,
            &Predicate::new().min_severity(floor).body_regex(re),
        ),
        // The two (None, Some, Some) and (Some, Some, Some) arms are
        // UNREACHABLE — pruned by the mutual-exclusion check above.
        // The check returned 400 before the dispatch was reached.
        (_, Some(_), Some(_)) => {
            unreachable!("mutual-exclusion check pruned both-body-filters arms")
        }
    };

    match query_result {
        Ok(records) => {
            // Trace-id filter (PG-2 / log-trace-id-filter): applied as the
            // FIRST step inside the success arm, BEFORE the result-size cap
            // check, so the cap is measured on the trace-filtered set and
            // pagination composes (filter-before-page). The store applied the
            // time-range and tenant scope already; this narrows the returned
            // set to records carrying the requested id. `LogRecord.trace_id`
            // is `Option<[u8; 16]>`, so the comparison is a direct array
            // equality. An absent filter (`None`) leaves the set unchanged.
            let records = match trace_id_filter {
                None => records,
                Some(id) => records
                    .into_iter()
                    .filter(|record| record.trace_id == Some(id))
                    .collect(),
            };

            // Result-size cap (ADR-0050 Decision 2 / D5): measured on
            // the records vector the store returned, BEFORE
            // serialisation. A count strictly over the cap is a 400;
            // serialisation never starts.
            if records.len() > MAX_RESULT_ROWS {
                return query_http_common::error_response(
                    StatusCode::BAD_REQUEST,
                    query_http_common::REASON_TOO_MANY_ROWS,
                );
            }

            // Page slice (ADR-0057 Decision 1, 6 / log-query-pagination-v0):
            // runs AFTER the result-cap check (the cap is measured on the
            // post-filter, PRE-slice vector) and BEFORE serialisation. The
            // `limit` and `offset` values were parsed BEFORE the store
            // dispatch (so an invalid value is a parse-time 400 that never
            // queries the store); here they are applied as
            // `records.into_iter().skip(offset).take(limit).collect()` over
            // the stable-ordered, per-tenant, post-filter vector, so
            // filter-before-page and tenant-scope-before-page are automatic
            // (US-06, US-07). When neither parameter was present the page is
            // `skip(0).take(usize::MAX)`, byte-unchanged from the
            // pre-pagination response (US-03 backward compatibility): the cap
            // (at most `MAX_RESULT_ROWS` records) is the backstop, so the
            // `usize::MAX` take never overflows.
            let limit = limit.unwrap_or(usize::MAX);
            let page: Vec<lumen::LogRecord> =
                records.into_iter().skip(offset).take(limit).collect();

            success_response(page)
        }
        Err(err) => {
            tracing::error!(event = "logs.store.failed", reason = %err);
            query_http_common::error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the backing log store could not be read",
            )
        }
    }
}

/// Whole seconds -> nanoseconds. Saturates rather than overflowing on an
/// implausibly large bound. Pillar-specific (lumen): kept per-consumer
/// because each consumer builds its pillar's nanosecond `TimeRange`
/// (ADR-0054 / ADR-0048 Decision 5).
fn seconds_to_nanos(seconds: u64) -> u64 {
    seconds.saturating_mul(1_000_000_000)
}

/// Parse the `min_severity` wire value to a lumen [`SeverityNumber`].
///
/// Case-insensitive on the six OTel names (TRACE, DEBUG, INFO, WARN,
/// ERROR, FATAL) per ADR-0052 Decision 2. Aliases (e.g. `"WARNING"`,
/// `"WARN+"`) are NOT accepted; the empty string is NOT a missing-value
/// shortcut and is rejected as unknown.
///
/// Returns `Err("unknown severity")` on any unrecognised input. The
/// reason text is the literal class label; the raw value is NEVER
/// echoed (the redaction inherited from ADR-0047 Decision 1, mirroring
/// `parse_epoch_seconds`).
fn parse_min_severity(raw: &str) -> Result<SeverityNumber, String> {
    if raw.is_empty() {
        return Err("unknown severity".to_string());
    }
    if raw.eq_ignore_ascii_case("TRACE") {
        Ok(SeverityNumber::TRACE)
    } else if raw.eq_ignore_ascii_case("DEBUG") {
        Ok(SeverityNumber::DEBUG)
    } else if raw.eq_ignore_ascii_case("INFO") {
        Ok(SeverityNumber::INFO)
    } else if raw.eq_ignore_ascii_case("WARN") {
        Ok(SeverityNumber::WARN)
    } else if raw.eq_ignore_ascii_case("ERROR") {
        Ok(SeverityNumber::ERROR)
    } else if raw.eq_ignore_ascii_case("FATAL") {
        Ok(SeverityNumber::FATAL)
    } else {
        Err("unknown severity".to_string())
    }
}

/// Parse the `body_contains` wire value to an owned `String`.
///
/// ADR-0055 Decision 9 / parse-helper-spec: rejects the empty string
/// and any value whose byte length strictly exceeds
/// [`MAX_BODY_CONTAINS_LEN`] (1024 bytes). Both rejections return the
/// SAME literal reason `"invalid body_contains"`; the raw parameter
/// value is NEVER interpolated (DD5 anti-echo). Returns an owned
/// `String` on success — a fresh copy of the operator's input,
/// byte-for-byte preserved (no trim, no case folding, no
/// normalisation).
fn parse_body_contains(raw: &str) -> Result<String, &'static str> {
    if raw.is_empty() {
        return Err("invalid body_contains");
    }
    if raw.len() > MAX_BODY_CONTAINS_LEN {
        return Err("invalid body_contains");
    }
    Ok(raw.to_string())
}

/// Parse the `body_regex` wire value to a compiled `regex::Regex`.
///
/// ADR-0056 Decision 5 / Decision 6 / Decision 3 and
/// `design/parse-helper-spec.md`: rejects the empty string, any
/// value whose byte length strictly exceeds [`MAX_BODY_REGEX_LEN`]
/// (1024 bytes), and any value the `regex` crate refuses to
/// compile. All three rejections return the SAME literal reason
/// `"invalid body_regex"`; the raw parameter value is NEVER
/// interpolated (the `regex::Error::Display` impl is NEVER called).
/// On success, returns the compiled `Regex`; the handler hands it to
/// `Predicate::body_regex(re)` and the per-record match call is
/// `re.is_match(&record.body)`. No normalisation is applied (no
/// trim, no case folding, no Unicode flag override); the `regex`
/// crate's default behaviour governs the grammar.
///
/// Order of checks: empty -> over-cap -> compile. The cap is a
/// budget on parse work, not on match work; the cap rejection
/// precedes the compile call so a 1025-byte pattern never pays the
/// parse cost.
fn parse_body_regex(raw: &str) -> Result<Regex, &'static str> {
    if raw.is_empty() {
        return Err("invalid body_regex");
    }
    if raw.len() > MAX_BODY_REGEX_LEN {
        return Err("invalid body_regex");
    }
    Regex::new(raw).map_err(|_| "invalid body_regex")
}

/// Parse the `limit` wire value to a bounded, strictly-positive `usize`.
///
/// ADR-0057 Decision 2 / Decision 5 and
/// `design/parse-helper-spec.md`: rejects `0` (PIN 6: a page of zero
/// records carries no information an absent request would not), any
/// non-numeric or negative value (a leading `-` makes the string
/// non-parseable as `usize`, so this is the same rejection arm as
/// non-numeric; no separate sign check is needed), and any value
/// strictly greater than [`MAX_RESULT_ROWS`] (100000; the boundary is
/// `>`, INCLUSIVE at the cap). All rejections return the SAME literal
/// reason `"invalid limit"`; the raw parameter value is NEVER
/// interpolated (the anti-echo posture symmetric with ADR-0052 /
/// ADR-0055 / ADR-0056). On success, returns `Ok(n)` for
/// `1 <= n <= 100000`.
///
/// Check order: parse to `usize` (rejects non-numeric and negative) ->
/// reject `0` -> reject `> MAX_RESULT_ROWS` -> `Ok(n)`.
fn parse_limit(raw: &str) -> Result<usize, &'static str> {
    let n: usize = raw.parse().map_err(|_| "invalid limit")?;
    if n == 0 {
        return Err("invalid limit");
    }
    if n > MAX_RESULT_ROWS {
        return Err("invalid limit");
    }
    Ok(n)
}

/// Parse the `offset` wire value to a non-negative `usize`.
///
/// ADR-0057 Decision 3 / Decision 5 and
/// `design/parse-helper-spec.md`: `0` is VALID (the first page; NOT
/// rejected). Rejects any non-numeric or negative value (a leading `-`
/// makes the string non-parseable as `usize`, the same rejection arm as
/// non-numeric) with the literal reason `"invalid offset"`. There is NO
/// upper cap on `offset`: a large offset (for example past the result
/// set) is `Ok(n)`; the empty page is produced by the slice
/// (`skip(n)` yields an empty iterator), NOT by a parse error (PIN 4).
/// The raw parameter value is NEVER interpolated.
///
/// Check order: parse to `usize` (rejects non-numeric and negative) ->
/// `Ok(n)`. No zero check, no upper-cap check.
fn parse_offset(raw: &str) -> Result<usize, &'static str> {
    raw.parse::<usize>().map_err(|_| "invalid offset")
}

/// Parse the `trace_id` wire value to the raw 16 bytes of a trace id
/// (PG-2 / log-trace-id-filter). This is the INVERSE of [`to_lowercase_hex`]
/// in this file: the response renders an id one way and this accepts the
/// very same string back. It mirrors the established discipline of
/// `trace_query_api::parse_trace_id` and the substrate codec at
/// `crates/ray/src/span.rs:42-60` — exactly 32 characters, hex decoded via
/// `char::to_digit(16)` so the input is case-insensitive (`a-f` and `A-F`
/// both accepted), with nibble math `((hi << 4) | lo) as u8`.
///
/// Unlike the traces API, the return type is the raw `[u8; 16]` (NOT a
/// `TraceId`), because the handler compares it directly against
/// `lumen::LogRecord.trace_id` (`Option<[u8; 16]>`).
///
/// Both the wrong-length arm AND the non-hex arm collapse to the SAME
/// literal reason `"invalid trace_id: expected a 32-character hex string"`;
/// the raw value is NEVER carried in the returned reason text (redaction,
/// symmetric with `parse_min_severity` / `parse_body_contains`). The reason
/// names the expected format (it contains "32" and "hex") without leaking
/// any property of the raw value.
fn parse_trace_id(raw: &str) -> Result<[u8; 16], &'static str> {
    if raw.len() != 32 {
        return Err("invalid trace_id: expected a 32-character hex string");
    }
    let mut bytes = [0u8; 16];
    let raw_bytes = raw.as_bytes();
    for (i, slot) in bytes.iter_mut().enumerate() {
        let hi = (raw_bytes[i * 2] as char)
            .to_digit(16)
            .ok_or("invalid trace_id: expected a 32-character hex string")?;
        let lo = (raw_bytes[i * 2 + 1] as char)
            .to_digit(16)
            .ok_or("invalid trace_id: expected a 32-character hex string")?;
        *slot = ((hi << 4) | lo) as u8;
    }
    Ok(bytes)
}

/// Lowercase fixed-width hex encoding of a byte identifier, matching the
/// rendering the traces API uses for `trace_id` / `span_id` (the
/// `ray::TraceId` / `ray::SpanId` `Serialize` impls in
/// `crates/ray/src/span.rs`: lowercase, two chars per byte, leading zeros
/// preserved). Reproduced here as a free function over the bytes so the
/// logs query and the traces query agree EXACTLY on the id string and
/// correlation by id works (PG-2). This renders ONLY the query response;
/// lumen ingest and on-disk storage keep the raw `[u8; N]` unchanged.
fn to_lowercase_hex(bytes: &[u8]) -> String {
    let mut hex = String::new();
    for byte in bytes {
        hex.push(char::from_digit(u32::from(byte >> 4), 16).expect("high nibble is 0..=15"));
        hex.push(char::from_digit(u32::from(byte & 0x0f), 16).expect("low nibble is 0..=15"));
    }
    hex
}

/// JSON view of a [`lumen::LogRecord`] for the query response. Byte-for-byte
/// identical to `LogRecord`'s own derive for EVERY field except `trace_id`
/// and `span_id`, which render as lowercase-hex strings (32 / 16 chars) in
/// place of the raw byte arrays — the SAME shape the traces API uses
/// (PG-2). The field set and order mirror `LogRecord` exactly so the
/// response is otherwise unchanged. An absent id stays `null` (it is never
/// rendered as an empty or zero-filled string).
#[derive(Serialize)]
struct LogRecordView<'a> {
    observed_time_unix_nano: u64,
    severity_number: SeverityNumber,
    severity_text: &'a str,
    body: &'a str,
    attributes: &'a BTreeMap<String, String>,
    resource_attributes: &'a BTreeMap<String, String>,
    trace_id: Option<String>,
    span_id: Option<String>,
}

impl<'a> From<&'a lumen::LogRecord> for LogRecordView<'a> {
    fn from(record: &'a lumen::LogRecord) -> Self {
        Self {
            observed_time_unix_nano: record.observed_time_unix_nano,
            severity_number: record.severity_number,
            severity_text: &record.severity_text,
            body: &record.body,
            attributes: &record.attributes,
            resource_attributes: &record.resource_attributes,
            trace_id: record.trace_id.map(|bytes| to_lowercase_hex(&bytes)),
            span_id: record.span_id.map(|bytes| to_lowercase_hex(&bytes)),
        }
    }
}

/// Serialise the success / empty arm: HTTP 200 with a BARE JSON array of
/// the in-window `LogRecord`s (ADR-0047 Decision 1), in the store's
/// ascending `observed_time_unix_nano` order. The empty arm is `[]`, a
/// calm 200, never an error. Each record is rendered through
/// [`LogRecordView`], which carries every field faithfully and renders
/// `trace_id` / `span_id` as lowercase hex so the logs query agrees with
/// the traces query on the id string (PG-2).
fn success_response(records: Vec<lumen::LogRecord>) -> Response {
    let views: Vec<LogRecordView> = records.iter().map(LogRecordView::from).collect();
    (StatusCode::OK, Json(views)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The inline tests for `parse_time_range`, `parse_epoch_seconds`,
    // `MAX_*` consts, and the cap reason literals now live canonically in
    // `crates/query-http-common/src/lib.rs` (ADR-0054 / Mikado step F).
    // The acceptance suite (`tests/*.rs`) is the byte-identity gate for
    // the rewire.

    // ----- ADR-0052 parse_min_severity inline tests -----
    //
    // The acceptance suite covers the behavioural outcomes (the floor
    // filter, the boundary, the case-insensitive identity, the
    // unknown-severity 400, the no-store-call assertion). These inline
    // tests pin the per-name mapping and the empty-string rejection
    // one-by-one so a single-name drop or a fold-to-eq mutant is caught.

    #[test]
    fn parse_min_severity_accepts_each_otel_name_in_lowercase() {
        assert_eq!(parse_min_severity("trace").unwrap(), SeverityNumber::TRACE);
        assert_eq!(parse_min_severity("debug").unwrap(), SeverityNumber::DEBUG);
        assert_eq!(parse_min_severity("info").unwrap(), SeverityNumber::INFO);
        assert_eq!(parse_min_severity("warn").unwrap(), SeverityNumber::WARN);
        assert_eq!(parse_min_severity("error").unwrap(), SeverityNumber::ERROR);
        assert_eq!(parse_min_severity("fatal").unwrap(), SeverityNumber::FATAL);
    }

    #[test]
    fn parse_min_severity_accepts_each_otel_name_in_uppercase() {
        assert_eq!(parse_min_severity("TRACE").unwrap(), SeverityNumber::TRACE);
        assert_eq!(parse_min_severity("DEBUG").unwrap(), SeverityNumber::DEBUG);
        assert_eq!(parse_min_severity("INFO").unwrap(), SeverityNumber::INFO);
        assert_eq!(parse_min_severity("WARN").unwrap(), SeverityNumber::WARN);
        assert_eq!(parse_min_severity("ERROR").unwrap(), SeverityNumber::ERROR);
        assert_eq!(parse_min_severity("FATAL").unwrap(), SeverityNumber::FATAL);
    }

    #[test]
    fn parse_min_severity_accepts_each_otel_name_in_mixed_case() {
        assert_eq!(parse_min_severity("Trace").unwrap(), SeverityNumber::TRACE);
        assert_eq!(parse_min_severity("Debug").unwrap(), SeverityNumber::DEBUG);
        assert_eq!(parse_min_severity("Info").unwrap(), SeverityNumber::INFO);
        assert_eq!(parse_min_severity("Warn").unwrap(), SeverityNumber::WARN);
        assert_eq!(parse_min_severity("Error").unwrap(), SeverityNumber::ERROR);
        assert_eq!(parse_min_severity("Fatal").unwrap(), SeverityNumber::FATAL);
    }

    #[test]
    fn parse_min_severity_rejects_the_empty_string_as_unknown() {
        // `?min_severity=` arrives as `Some("")` from serde and MUST be
        // rejected, NOT treated as a missing-parameter shortcut. Kills a
        // mutant that uses `is_empty()` to fall back to the unfiltered
        // path.
        let reason = parse_min_severity("").expect_err("empty is unknown");
        assert_eq!(reason, "unknown severity");
    }

    #[test]
    fn parse_min_severity_rejects_aliases_like_warning() {
        // ADR-0052 Decision 2: only the six OTel names; no aliases. Kills
        // a mutant that adds `"WARNING"` -> `WARN` (or any other alias).
        assert!(parse_min_severity("WARNING").is_err());
        assert!(parse_min_severity("WARN+").is_err());
        assert!(parse_min_severity("CRITICAL").is_err());
        // `UNSPECIFIED` (SeverityNumber::UNSPECIFIED = 0) is NOT an
        // accepted wire value either.
        assert!(parse_min_severity("UNSPECIFIED").is_err());
    }

    #[test]
    fn parse_min_severity_is_case_insensitive_identity_for_warn() {
        // The Predicate is constructed from the parsed SeverityNumber
        // (not the raw string), so the three case forms produce the same
        // SeverityNumber. Kills a fold-to-`eq` mutant.
        let lower = parse_min_severity("warn").unwrap();
        let upper = parse_min_severity("WARN").unwrap();
        let mixed = parse_min_severity("Warn").unwrap();
        assert_eq!(lower, upper);
        assert_eq!(upper, mixed);
        assert_eq!(lower, SeverityNumber::WARN);
    }

    #[test]
    fn parse_min_severity_error_reason_is_the_literal_class_label() {
        // The reason MUST be the literal `"unknown severity"`; it MUST
        // NOT echo the raw value (redaction symmetry with the bounds
        // parser, ADR-0047 Decision 1).
        let reason = parse_min_severity("hunter2").expect_err("rejected");
        assert_eq!(reason, "unknown severity");
        assert!(!reason.contains("hunter2"));
    }

    // ----- ADR-0055 parse_body_contains inline tests -----
    //
    // The acceptance suite covers the behavioural outcomes (the
    // substring filter, the calm empty arm, the empty-string and
    // over-cap 400s, the case-sensitive pin, the no-store-call
    // assertion, the cross-tenant isolation). These inline tests pin
    // the inclusive-1024 / strict-1025 boundary one byte at a time so
    // a `>` -> `>=` length-cap mutant is caught.

    #[test]
    fn parse_body_contains_accepts_exactly_1024_bytes() {
        // The cap is INCLUSIVE at 1024 bytes (ADR-0055 Decision 5 /
        // DD6): a 1024-byte value is served. Kills a `>` -> `>=`
        // mutant that would refuse the inclusive boundary.
        let at_cap = "A".repeat(MAX_BODY_CONTAINS_LEN);
        assert_eq!(at_cap.len(), 1024);
        let parsed = parse_body_contains(&at_cap).expect("1024 bytes is at-cap, served");
        assert_eq!(parsed, at_cap);
    }

    #[test]
    fn parse_body_contains_rejects_1025_bytes_with_literal_reason() {
        // 1025 bytes is STRICTLY over the cap and rejected with the
        // literal reason. Kills a `>` -> `>=` mutant on the other side.
        let over_cap = "A".repeat(MAX_BODY_CONTAINS_LEN + 1);
        assert_eq!(over_cap.len(), 1025);
        let reason = parse_body_contains(&over_cap).expect_err("1025 bytes is over-cap");
        assert_eq!(reason, "invalid body_contains");
    }

    #[test]
    fn parse_body_contains_rejects_the_empty_string_with_literal_reason() {
        // `?body_contains=` arrives as `Some("")` from serde and MUST
        // be rejected, NOT treated as a missing-parameter shortcut.
        let reason = parse_body_contains("").expect_err("empty is invalid");
        assert_eq!(reason, "invalid body_contains");
    }

    #[test]
    fn parse_body_contains_preserves_the_raw_value_byte_for_byte() {
        // No trim, no case folding, no normalisation: the parser
        // returns a fresh copy of the operator's input.
        let raw = "Kafka Timeout  \t  ";
        let parsed = parse_body_contains(raw).expect("non-empty under-cap is served");
        assert_eq!(parsed, raw);
    }

    #[test]
    fn parse_body_contains_error_reason_never_echoes_the_raw_value() {
        // The reason MUST be the literal `"invalid body_contains"`;
        // it MUST NOT echo the raw value (DD5 anti-echo, redaction
        // symmetry with the bounds parser, ADR-0047 Decision 1).
        let oversize = format!("SECRET-{}", "A".repeat(MAX_BODY_CONTAINS_LEN));
        let reason = parse_body_contains(&oversize).expect_err("over-cap is rejected");
        assert_eq!(reason, "invalid body_contains");
        assert!(!reason.contains("SECRET-"));
    }

    // ----- PG-2 to_lowercase_hex inline tests -----
    //
    // The acceptance suite proves the end-to-end response shape and the
    // cross-API correlation. These inline tests pin the per-nibble encoding
    // one case at a time so a high/low-nibble swap, a base/radix change, or
    // a leading-zero drop is caught at the unit boundary.

    #[test]
    fn to_lowercase_hex_encodes_each_byte_as_two_lowercase_chars() {
        // Distinct high and low nibbles per byte: a high<->low swap, an
        // `>> 4` -> `<< 4`, or an `& 0x0f` flip changes the output.
        assert_eq!(to_lowercase_hex(&[0x1a]), "1a");
        assert_eq!(to_lowercase_hex(&[0xab]), "ab");
        assert_eq!(to_lowercase_hex(&[0xf0]), "f0");
        assert_eq!(to_lowercase_hex(&[0x0f]), "0f");
    }

    #[test]
    fn to_lowercase_hex_preserves_leading_zero_bytes_at_full_width() {
        // A zero byte renders as "00", never collapsed: the width is two
        // chars per byte regardless of value.
        assert_eq!(to_lowercase_hex(&[0x00]), "00");
        assert_eq!(to_lowercase_hex(&[0x00, 0x00, 0x01]), "000001");
    }

    #[test]
    fn to_lowercase_hex_of_a_full_16_byte_id_is_32_chars() {
        let id = [
            0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19,
        ];
        let hex = to_lowercase_hex(&id);
        assert_eq!(hex, "0a0b0c0d0e0f10111213141516171819");
        assert_eq!(hex.len(), 32);
    }

    #[test]
    fn to_lowercase_hex_of_an_empty_slice_is_the_empty_string() {
        assert_eq!(to_lowercase_hex(&[]), "");
    }

    #[test]
    fn log_record_view_renders_ids_as_hex_and_absent_ids_as_none() {
        use std::collections::BTreeMap;
        let mut record = lumen::LogRecord {
            observed_time_unix_nano: 7,
            severity_number: SeverityNumber::INFO,
            severity_text: "INFO".to_string(),
            body: "b".to_string(),
            attributes: BTreeMap::new(),
            resource_attributes: BTreeMap::new(),
            trace_id: Some([
                0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
                0x18, 0x19,
            ]),
            span_id: Some([0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21]),
        };
        let view = LogRecordView::from(&record);
        assert_eq!(
            view.trace_id.as_deref(),
            Some("0a0b0c0d0e0f10111213141516171819")
        );
        assert_eq!(view.span_id.as_deref(), Some("1a1b1c1d1e1f2021"));

        record.trace_id = None;
        record.span_id = None;
        let absent = LogRecordView::from(&record);
        assert!(absent.trace_id.is_none());
        assert!(absent.span_id.is_none());
    }

    // ----- PG-2 parse_trace_id inline tests -----
    //
    // The acceptance suite covers the behavioural outcomes (the filter
    // narrows to one trace, the calm empty arm, the redacted 400s, the
    // backward-compat absent arm, the cross-tenant isolation, the
    // no-store-call assertion). These inline tests pin the per-nibble
    // decoding, the case-insensitivity, the length and hex rejections, and
    // the anti-echo redaction one at a time so a nibble swap, a base/radix
    // change, a length-check flip, or a reason-text drift is caught at the
    // unit boundary. `parse_trace_id` is the inverse of `to_lowercase_hex`.

    #[test]
    fn parse_trace_id_decodes_a_32_hex_string_to_the_exact_bytes() {
        // Distinct high and low nibbles per byte: a high<->low swap, an
        // `<< 4` -> `>> 4`, or an `| lo` drop changes the output. This is
        // the inverse of `to_lowercase_hex_of_a_full_16_byte_id_is_32_chars`.
        let bytes = parse_trace_id("0a0b0c0d0e0f10111213141516171819").expect("valid 32-hex");
        assert_eq!(
            bytes,
            [
                0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
                0x18, 0x19,
            ]
        );
    }

    #[test]
    fn parse_trace_id_accepts_the_uppercase_form_as_the_same_bytes() {
        // Case-insensitive on input (matching the substrate codec): the
        // uppercase form of the same id yields the IDENTICAL bytes. Kills a
        // mutant that lowercases the radix or rejects `A-F`.
        let lower = parse_trace_id("0a0b0c0d0e0f10111213141516171819").expect("lowercase");
        let upper = parse_trace_id("0A0B0C0D0E0F10111213141516171819").expect("uppercase");
        assert_eq!(lower, upper);
    }

    #[test]
    fn parse_trace_id_rejects_wrong_length_values_with_the_literal_reason() {
        // 31 and 33 characters are both the wrong length and rejected with
        // the named-format literal. Kills a `!=` -> `<` / `>` length mutant.
        let too_short = parse_trace_id("0a0b0c0d0e0f101112131415161718").expect_err("31 chars");
        assert_eq!(
            too_short,
            "invalid trace_id: expected a 32-character hex string"
        );
        let too_long = parse_trace_id("0a0b0c0d0e0f1011121314151617181900").expect_err("34 chars");
        assert_eq!(
            too_long,
            "invalid trace_id: expected a 32-character hex string"
        );
    }

    #[test]
    fn parse_trace_id_rejects_a_non_hex_character_with_the_literal_reason() {
        // A 32-char value whose final character is not a hex digit is
        // rejected with the SAME literal reason as the wrong-length arm.
        let non_hex = parse_trace_id("0a0b0c0d0e0f1011121314151617181g").expect_err("'g'");
        assert_eq!(
            non_hex,
            "invalid trace_id: expected a 32-character hex string"
        );
        let also_non_hex = parse_trace_id("zz0b0c0d0e0f10111213141516171819").expect_err("'z'");
        assert_eq!(
            also_non_hex,
            "invalid trace_id: expected a 32-character hex string"
        );
    }

    #[test]
    fn parse_trace_id_reason_names_the_format_and_never_echoes_the_raw_value() {
        // The reason MUST name the expected format (contain "32" and "hex")
        // and MUST NOT echo the raw value (redaction symmetry with the other
        // parsers, ADR-0047 Decision 1).
        let raw = "SECRETSECRETSECRETSECRETSECRETXX";
        let reason = parse_trace_id(raw).expect_err("non-hex, rejected");
        assert!(reason.contains("32"), "reason names the length: {reason}");
        assert!(
            reason.contains("hex"),
            "reason names the encoding: {reason}"
        );
        assert!(
            !reason.contains("SECRET"),
            "reason never echoes the raw value: {reason}"
        );
    }
}
