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
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use lumen::{LogStore, Predicate, SeverityNumber, TimeRange};
use serde::Deserialize;

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
    let state = ApiState { store, tenant };
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
}

/// Maximum permitted byte length of a `body_contains` value (ADR-0055
/// Decision 5 / DD6). A non-empty value of exactly 1024 bytes is
/// served; 1025 bytes or more is refused with the literal envelope.
const MAX_BODY_CONTAINS_LEN: usize = 1024;

/// Handle `GET /api/v1/logs?start=&end=`. Never panics on bad input;
/// every failure mode is a `status:error` arm with the appropriate
/// status code. The orchestration is resolve-tenant (fail-closed 401)
/// -> parse-bounds (400 before the store) -> `LogStore::query` ->
/// serialise the bare array (200, `[]` when empty) -> map
/// `PersistenceFailed` to 500.
async fn handle_logs(State(state): State<ApiState>, Query(params): Query<LogsParams>) -> Response {
    // Fail-closed tenancy: refuse before touching the store via the
    // shared seam in query-http-common (ADR-0054).
    let tenant = match query_http_common::resolve_tenant_or_refuse(&state.tenant, "the log query") {
        Ok(t) => t.clone(),
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

    let range = TimeRange::new(seconds_to_nanos(start_secs), seconds_to_nanos(end_secs));

    // Dispatch (ADR-0055 Decision 7): four arms by the cross-product
    // of `min_severity` x `body_contains`. When either filter is
    // present, build the composed predicate and call `query_with`;
    // when both are absent, fall through to `query` (the slice-prior
    // backward-compat path).
    let query_result = match (min_severity, body_contains) {
        (None, None) => state.store.query(&tenant, range),
        (Some(floor), None) => {
            state
                .store
                .query_with(&tenant, range, &Predicate::new().min_severity(floor))
        }
        (None, Some(target)) => {
            state
                .store
                .query_with(&tenant, range, &Predicate::new().body_contains(target))
        }
        (Some(floor), Some(target)) => state.store.query_with(
            &tenant,
            range,
            &Predicate::new().min_severity(floor).body_contains(target),
        ),
    };

    match query_result {
        Ok(records) => {
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
            success_response(records)
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

/// Serialise the success / empty arm: HTTP 200 with a BARE JSON array of
/// the in-window `LogRecord`s (ADR-0047 Decision 1), in the store's
/// ascending `observed_time_unix_nano` order. The empty arm is `[]`, a
/// calm 200, never an error. `LogRecord` carries its own `Serialize`
/// derive, so the array is faithful with no hand-written mapping.
fn success_response(records: Vec<lumen::LogRecord>) -> Response {
    (StatusCode::OK, Json(records)).into_response()
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
}
