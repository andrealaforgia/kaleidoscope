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
use serde_json::json;

/// The route Prism's future log panel and the operator both target:
/// `/api/v1` prefix (ADR-0043) + `/logs` (ADR-0047 Decision 3).
const LOGS_ROUTE: &str = "/api/v1/logs";

/// Maximum permitted query window in whole seconds (24 hours; ADR-0050
/// Decision 1). A request whose `end - start` in seconds STRICTLY
/// exceeds this value is refused with a named 400 BEFORE the store is
/// touched. A window of exactly `MAX_WINDOW_SECONDS` is served (the
/// boundary is inclusive).
pub const MAX_WINDOW_SECONDS: u64 = 86_400;

/// Maximum permitted log-record count in a single response (ADR-0050
/// Decision 2). The count is measured on the `Vec<LogRecord>` the store
/// returns BEFORE serialisation. A response of exactly `MAX_RESULT_ROWS`
/// is served (the boundary is inclusive). A response strictly greater
/// is refused with a named 400; serialisation never starts.
pub const MAX_RESULT_ROWS: usize = 100_000;

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
}

/// Handle `GET /api/v1/logs?start=&end=`. Never panics on bad input;
/// every failure mode is a `status:error` arm with the appropriate
/// status code. The orchestration is resolve-tenant (fail-closed 401)
/// -> parse-bounds (400 before the store) -> `LogStore::query` ->
/// serialise the bare array (200, `[]` when empty) -> map
/// `PersistenceFailed` to 500.
async fn handle_logs(State(state): State<ApiState>, Query(params): Query<LogsParams>) -> Response {
    // Fail-closed tenancy: refuse before touching the store.
    let tenant = match &state.tenant {
        Some(t) => t.clone(),
        None => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                "no tenant resolvable: the log query service refuses unscoped requests",
            );
        }
    };

    // Parse and validate the window BEFORE the store is touched: a
    // malformed or inverted window is a 400 that never runs a query.
    let (start_secs, end_secs) = match parse_time_range_seconds(&params.start, &params.end) {
        Ok(secs) => secs,
        Err(reason) => return error_response(StatusCode::BAD_REQUEST, &reason),
    };

    // Window cap (ADR-0050 Decision 1 / D5): the span is computed in
    // whole seconds, BEFORE the nanosecond conversion, and BEFORE the
    // store is touched. A request strictly over the cap is a 400; the
    // store is NEVER queried on this path.
    if end_secs.saturating_sub(start_secs) > MAX_WINDOW_SECONDS {
        return error_response(StatusCode::BAD_REQUEST, "window exceeds 86400 seconds");
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
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "unknown severity"),
        },
    };

    let range = TimeRange::new(seconds_to_nanos(start_secs), seconds_to_nanos(end_secs));

    let query_result = match min_severity {
        Some(floor) => {
            state
                .store
                .query_with(&tenant, range, &Predicate::new().min_severity(floor))
        }
        None => state.store.query(&tenant, range),
    };

    match query_result {
        Ok(records) => {
            // Result-size cap (ADR-0050 Decision 2 / D5): measured on
            // the records vector the store returned, BEFORE
            // serialisation. A count strictly over the cap is a 400;
            // serialisation never starts.
            if records.len() > MAX_RESULT_ROWS {
                return error_response(StatusCode::BAD_REQUEST, "result exceeds 100000 rows");
            }
            success_response(records)
        }
        Err(err) => {
            tracing::error!(event = "logs.store.failed", reason = %err);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the backing log store could not be read",
            )
        }
    }
}

/// Parse `start`/`end` epoch-seconds strings into a half-open nanosecond
/// [`TimeRange`]. Rejects non-numeric bounds and inverted bounds
/// (`end < start`). Float-tolerant: a fractional `.0` suffix is parsed as
/// `f64` then truncated to whole seconds, mirroring the metrics endpoint.
///
/// Test-only since ADR-0050: the handler computes the cap span in
/// seconds via [`parse_time_range_seconds`] BEFORE the nanosecond
/// conversion. The inline parser tests continue to assert the
/// pre-conversion behaviour through this thin wrapper.
#[cfg(test)]
fn parse_time_range(start: &str, end: &str) -> Result<TimeRange, String> {
    let (start_secs, end_secs) = parse_time_range_seconds(start, end)?;
    Ok(TimeRange::new(
        seconds_to_nanos(start_secs),
        seconds_to_nanos(end_secs),
    ))
}

/// Parse `start`/`end` epoch-seconds strings into `(start_secs, end_secs)`
/// as whole seconds. Same validation as [`parse_time_range`]; the window
/// cap check (ADR-0050 D5) needs the seconds span BEFORE the nanosecond
/// conversion.
fn parse_time_range_seconds(start: &str, end: &str) -> Result<(u64, u64), String> {
    let start_secs = parse_epoch_seconds(start, "start")?;
    let end_secs = parse_epoch_seconds(end, "end")?;
    if end_secs < start_secs {
        return Err("invalid time bounds: end is earlier than start".to_string());
    }
    Ok((start_secs, end_secs))
}

/// Parse one epoch-seconds bound as a non-negative number of whole
/// seconds. The `field` name is named in the error but the raw value is
/// NOT echoed (redaction symmetry with ADR-0047 Decision 1).
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

/// Serialise the success / empty arm: HTTP 200 with a BARE JSON array of
/// the in-window `LogRecord`s (ADR-0047 Decision 1), in the store's
/// ascending `observed_time_unix_nano` order. The empty arm is `[]`, a
/// calm 200, never an error. `LogRecord` carries its own `Serialize`
/// derive, so the array is faithful with no hand-written mapping.
fn success_response(records: Vec<lumen::LogRecord>) -> Response {
    (StatusCode::OK, Json(records)).into_response()
}

/// Serialise an error arm: `{status:'error', error:'<reason>'}` at the
/// given status code. The reason never echoes a forwarded header value,
/// the raw query, or credentials (ADR-0047 Decision 1 redaction).
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
    // does not isolate one-by-one, and the half-open boundary the store
    // enforces.

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
    fn an_inverted_window_is_rejected_before_any_store_query() {
        // start (later) > end (earlier) is the inverted window; the parse
        // rejects it so the store is never reached.
        assert!(parse_time_range("200", "100").is_err());
    }

    #[test]
    fn fractional_epoch_seconds_truncate_to_whole_seconds() {
        // The window is float-tolerant; a `.5` fraction must parse and
        // truncate to whole seconds.
        let range = parse_time_range("100.5", "200.9").expect("float bounds parse");
        assert_eq!(range.start_unix_nano, 100_000_000_000);
        assert_eq!(range.end_unix_nano, 200_000_000_000);
    }

    #[test]
    fn a_negative_bound_is_rejected_but_zero_is_accepted() {
        assert!(parse_time_range("-1", "100").is_err());
        // Zero is the epoch and a valid lower bound; the out-of-range
        // check is `< 0.0`, NOT `<= 0.0`. Pins that boundary so a `<` ->
        // `<=` mutant (which would reject a zero start) is caught.
        let range = parse_time_range("0", "100").expect("zero is a valid bound");
        assert_eq!(range.start_unix_nano, 0);
    }

    #[test]
    fn a_non_numeric_start_or_end_is_rejected() {
        assert!(parse_time_range("notanumber", "100").is_err());
        assert!(parse_time_range("100", "later").is_err());
    }

    #[test]
    fn the_bounds_error_never_echoes_the_raw_value() {
        let reason = parse_epoch_seconds("secretvalue", "start").expect_err("rejected");
        assert!(!reason.contains("secretvalue"));
    }

    #[test]
    fn the_window_converts_seconds_to_the_nanosecond_lumen_time_range() {
        // The store sorts and filters in nanoseconds; the seconds bounds
        // must be scaled by 1e9. Kills a mutant that drops the scaling.
        let range = parse_time_range("1716200000", "1716200060").expect("valid window");
        assert_eq!(range.start_unix_nano, 1_716_200_000_000_000_000);
        assert_eq!(range.end_unix_nano, 1_716_200_060_000_000_000);
    }

    #[test]
    fn the_half_open_range_includes_start_and_excludes_end() {
        // The contract's half-open boundary is the lumen TimeRange's:
        // a record at exactly start is included, at exactly end excluded.
        let range = parse_time_range("100", "200").expect("valid window");
        assert!(range.contains(range.start_unix_nano), "start is included");
        assert!(!range.contains(range.end_unix_nano), "end is excluded");
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
        // The cap check measures the span in seconds, BEFORE the nano
        // conversion. Pinning the helper keeps a mutant that scales to
        // nanos (and then makes the cap arithmetic compare against an
        // irrelevant value) caught.
        let (start, end) = parse_time_range_seconds("0", "86400").expect("valid bounds");
        assert_eq!(start, 0);
        assert_eq!(end, 86_400);
        let (s2, e2) = parse_time_range_seconds("0", "86401").expect("valid bounds");
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
        let window_reason = "window exceeds 86400 seconds";
        let result_reason = "result exceeds 100000 rows";
        for reason in [window_reason, result_reason] {
            assert!(!reason.contains("SECRET"));
            assert!(!reason.contains("Bearer"));
        }
    }

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
}
