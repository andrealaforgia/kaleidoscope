// Kaleidoscope query-http-common — shared read-side HTTP scaffolding
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

//! # query-http-common — shared HTTP scaffold for the Kaleidoscope read APIs.
//!
//! Workspace-internal library consumed by `query-api`, `log-query-api`,
//! and `trace-query-api`. It owns the four families of code that the
//! rule-of-three-and-a-bit (ADR-0048 Decision 6, ADR-0053 Decision 5,
//! ADR-0054) earned out of the three sibling read APIs:
//!
//! - the cap constants `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS`
//! - the literal reason texts those caps and the fail-closed seams emit
//! - the `parse_time_range` epoch-seconds parser (canonical shape)
//! - the `error_response` JSON envelope helper
//! - the `resolve_tenant_or_refuse` fail-closed tenant seam
//!
//! ## Public surface (delivered and green)
//!
//! All four free functions are implemented and live — each carries its
//! own "DELIVER state: implemented" note over a real body:
//!
//! - [`parse_time_range`] — the epoch-seconds window parser
//! - [`resolve_tenant_or_refuse`] — the fail-closed tenant seam
//! - [`error_response`] — the JSON error-envelope builder
//! - [`init_tracing`] — the tracing-subscriber initialiser
//!
//! The `#[cfg(test)] mod tests` block exercises that live surface: the
//! cap-constant values, the reason-text literals, [`ErrorBody`]
//! serialisation, and each function's behaviour against real inputs.
//!
//! ## Architectural posture
//!
//! - Pure data + free functions over `&str`, `Option<&str>`, and `TenantId`.
//! - No driven adapter; no filesystem, no network, no clock.
//! - Depends only on `axum`, `serde`, `serde_json`, `aegis`. Does NOT depend
//!   on the pillar stores (`pulse`, `lumen`, `ray`) — ADR-0048 Decision 5.
//! - `#![forbid(unsafe_code)]` mirrors the three consumers.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::OnceLock;
use std::time::SystemTime;

use axum::http::header::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

// Re-export the aegis auth surface the read-tier wiring (and the auth
// acceptance suites) build against, so the per-request resolution
// capability and its callers depend on ONE crate. The validator core is
// reused verbatim (ADR-0074); this crate only adds the per-request seam.
pub use aegis::{
    load_catalogue, TenantContext, TenantId, ValidationError, Validator, ValidatorConfig,
};

// =========================================================================
// Cap constants (ADR-0050)
// =========================================================================

/// Maximum permitted query window in whole seconds (24 hours; ADR-0050
/// Decision 1). A request whose `end - start` in seconds STRICTLY exceeds
/// this value is refused with a named 400 before the store is touched.
/// A window of exactly `MAX_WINDOW_SECONDS` is served (the boundary is
/// inclusive).
pub const MAX_WINDOW_SECONDS: u64 = 86_400;

/// Maximum permitted response-vector length (ADR-0050 Decision 2);
/// REFUSE-not-TRUNCATE. A response of exactly `MAX_RESULT_ROWS` is
/// served (the boundary is inclusive). A response strictly greater is
/// refused with a named 400; serialisation never starts.
pub const MAX_RESULT_ROWS: usize = 100_000;

// =========================================================================
// Reason text constants (ADR-0054)
// =========================================================================

/// The literal 400 reason for the inverted-bounds arm of [`parse_time_range`].
/// Verbatim byte-for-byte equal to the string today emitted by all three
/// consumer crates (`query-api`, `log-query-api`, `trace-query-api`).
pub const REASON_INVALID_TIME_RANGE: &str = "invalid time bounds: end is earlier than start";

/// The literal 400 reason for the window-cap arm. Pre-extraction call-site
/// count: 3 (one per consumer crate). The cap-arm consumer passes this
/// const verbatim to [`error_response`]. Byte-for-byte preserved.
pub const REASON_WINDOW_TOO_LARGE: &str = "window exceeds 86400 seconds";

/// The literal 400 reason for the result-cap arm. Pre-extraction call-site
/// count: 4 (`query-api` x1, `log-query-api` x1, `trace-query-api` x2 —
/// one per arm). Byte-for-byte preserved.
pub const REASON_TOO_MANY_ROWS: &str = "result exceeds 100000 rows";

/// The literal 401 reason prefix for the fail-closed tenant arm. Joined
/// inside [`resolve_tenant_or_refuse`] with the per-pillar `service_label`
/// (e.g. `"the query"`, `"the log query"`, `"the trace query"`) and the
/// literal suffix `" service refuses unscoped requests"`.
pub const REASON_MISSING_TENANT: &str = "no tenant resolvable: ";

// =========================================================================
// Data types
// =========================================================================

/// The error envelope body emitted by [`error_response`]. The wire shape
/// is the contract pinned across ADR-0042 (metrics), ADR-0047 (logs),
/// ADR-0048 (traces), and ADR-0053 (lookup-by-id):
/// `{"status":"error","error":"<reason>"}`. The `status` field is the
/// literal `"error"` discriminator; the `error` field carries the
/// per-arm reason text (one of the `REASON_*` consts above, or an
/// interpolated `String` from the parser).
///
/// Direct-construction byte-for-byte parity with the three consumers'
/// pre-extraction `json!({"status":"error","error":reason})` shape is
/// the K2 acceptance gate; this struct is the typed seam DELIVER will
/// thread through [`error_response`].
#[derive(Debug, Serialize)]
pub struct ErrorBody<'a> {
    /// The discriminator field; always the literal `"error"` for the
    /// envelope this struct serialises.
    pub status: &'static str,
    /// The per-arm reason text. Borrowed so both `&'static str` consts
    /// (the four `REASON_*` literals) and interpolated `&str` (the
    /// per-pillar tenant reason and the parser's four field-specific
    /// reasons) flow through unchanged.
    pub error: &'a str,
}

/// The half-open epoch-seconds time range returned by [`parse_time_range`].
/// `start_secs` is inclusive, `end_secs` is exclusive (a record at exactly
/// `end_secs` is NOT in range). The cap-arm consumer reads
/// `end_secs.saturating_sub(start_secs)` to test against
/// [`MAX_WINDOW_SECONDS`] BEFORE the nanosecond conversion.
///
/// The three consumer crates each keep their own pillar-specific
/// nanosecond `TimeRange` (`pulse::TimeRange`, `lumen::TimeRange`,
/// `ray::TimeRange`) and a private `seconds_to_nanos` helper. ADR-0048
/// Decision 5 cautions explicitly against forcing one of those types into
/// this crate; this `TimeRange` is the seconds-level pair instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    /// Lower bound in whole epoch seconds (inclusive).
    pub start_secs: u64,
    /// Upper bound in whole epoch seconds (exclusive on the consumer's
    /// pillar-specific nanosecond `TimeRange`).
    pub end_secs: u64,
}

// =========================================================================
// Free functions — DISTILL scaffold; DELIVER fills the bodies
// =========================================================================

/// Parse `start`/`end` epoch-seconds strings into a [`TimeRange`].
///
/// Signature mirrors the test-only `parse_time_range` wrapper today in
/// `crates/query-api/src/lib.rs:242`. The canonical implementation
/// DELIVER lands will:
///
/// - reject non-numeric input with `"invalid time bounds: <field> is not a number"`
/// - reject negative / non-finite input with `"invalid time bounds: <field> is out of range"`
/// - reject inverted bounds (`end < start`) with [`REASON_INVALID_TIME_RANGE`]
/// - accept equal bounds (empty half-open range)
/// - tolerate float input (Prism emits `.0` via `Date.getTime()/1000`),
///   truncating to whole seconds
/// - NEVER echo the raw value in the returned reason (redaction symmetry
///   pinned by ADR-0042 / ADR-0047 / ADR-0048 / ADR-0053)
///
/// DELIVER state: implemented. The four field-specific non-numeric and
/// out-of-range reasons are static literals (byte-for-byte equal to the
/// three consumer crates' pre-extraction `format!` output for the four
/// `(field, error-class)` combinations). The inverted-bounds reason is
/// [`REASON_INVALID_TIME_RANGE`].
pub fn parse_time_range(start: &str, end: &str) -> Result<TimeRange, &'static str> {
    let start_secs = parse_epoch_seconds(start, EpochField::Start)?;
    let end_secs = parse_epoch_seconds(end, EpochField::End)?;
    if end_secs < start_secs {
        return Err(REASON_INVALID_TIME_RANGE);
    }
    Ok(TimeRange {
        start_secs,
        end_secs,
    })
}

/// Which bound is being parsed; selects the static-literal reason for
/// the two field-specific error classes (non-numeric / out-of-range).
/// The `field` value never escapes the parser; only the four pinned
/// static literals do.
#[derive(Copy, Clone)]
enum EpochField {
    Start,
    End,
}

/// Parse one epoch-seconds bound as a non-negative number of whole
/// seconds. Returns one of four static-literal reasons on rejection;
/// the raw input is NEVER echoed (redaction symmetry).
fn parse_epoch_seconds(raw: &str, field: EpochField) -> Result<u64, &'static str> {
    let trimmed = raw.trim();
    let parsed: f64 = trimmed.parse().map_err(|_| match field {
        EpochField::Start => "invalid time bounds: start is not a number",
        EpochField::End => "invalid time bounds: end is not a number",
    })?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(match field {
            EpochField::Start => "invalid time bounds: start is out of range",
            EpochField::End => "invalid time bounds: end is out of range",
        });
    }
    Ok(parsed as u64)
}

/// Resolve the per-request tenant or refuse fail-closed with a 401.
///
/// Returns `Ok(t)` (a borrowed [`TenantId`]) when the router's
/// `Option<TenantId>` is `Some(t)`. Returns
/// `Err(error_response(UNAUTHORIZED, &<interpolated reason>))` when it
/// is `None`, where `<interpolated reason>` is
/// [`REASON_MISSING_TENANT`] + `service_label` + `" service refuses
/// unscoped requests"`. `service_label` is a static literal supplied by
/// each handler (`"the query"`, `"the log query"`, `"the trace query"`);
/// no untrusted input flows into the body.
///
/// DELIVER state: implemented. On `None`, builds the per-pillar 401
/// reason by joining [`REASON_MISSING_TENANT`] with `service_label` and
/// the literal suffix `" service refuses unscoped requests"`, then
/// emits the envelope via [`error_response`] at `UNAUTHORIZED`.
///
/// `clippy::result_large_err` is allowed here because the `Err`
/// variant IS the wire response we want to short-circuit through; the
/// alternative (boxing) would force every consumer call site through a
/// `*resp` dereference for zero behavioural gain.
#[allow(clippy::result_large_err)]
pub fn resolve_tenant_or_refuse<'a>(
    tenant: &'a Option<TenantId>,
    service_label: &'static str,
) -> Result<&'a TenantId, Response> {
    match tenant {
        Some(t) => Ok(t),
        None => {
            let reason =
                format!("{REASON_MISSING_TENANT}{service_label} service refuses unscoped requests");
            Err(error_response(StatusCode::UNAUTHORIZED, &reason))
        }
    }
}

/// Resolve the per-request tenant or refuse fail-closed with a 401 —
/// the per-request analogue of [`resolve_tenant_or_refuse`] that adds the
/// OPTIONAL per-request bearer path (`read-path-query-api-auth-v0`,
/// ADR-0074 DD3). The contract (the body is the crafter's; this is the
/// DISTILL scaffold):
///
/// 1. `auth` is `Some` AND a valid `Bearer <jwt>` rides `headers` →
///    `Ok(ctx.tenant_id)` (the query scopes to the TOKEN's tenant).
/// 2. `auth` is `Some` AND a missing / malformed / invalid bearer →
///    `Err(401)` BEFORE the store; `env_tenant` is **NEVER consulted in
///    this arm** (the no-bearer-bypass, R3 — there is no `else env_tenant`
///    fall-through after a validation failure).
/// 3. `auth` is `None` → today's env path via
///    [`resolve_tenant_or_refuse`]; the `Authorization` header is ignored
///    (backward compatibility, US-RAUTH-02).
///
/// On a reject the 401 carries `WWW-Authenticate: Bearer` (RFC 6750) and
/// the aegis `reason()` in the [`error_response`]/[`ErrorBody`] envelope;
/// neither the secret nor the raw token appears in any field. Exactly one
/// decision audit event per request: aegis emits it for every
/// validate-reached request; the shared capability emits the one
/// pre-validate `missing_claim` event itself for the no/empty/malformed
/// bearer case (DD5).
///
/// DELIVER state: implemented (read-path-query-api-auth-v0). The 3-arm
/// precedence above is the body. Arm 2 returns the 401 directly from the
/// validation-failure branch — there is no `else env_tenant` fall-through,
/// which is the no-bearer-bypass property (R3) by construction.
#[allow(clippy::result_large_err)]
pub fn resolve_request_tenant_or_refuse(
    auth: Option<&Arc<Validator>>,
    headers: &HeaderMap,
    env_tenant: &Option<TenantId>,
    service_label: &'static str,
    subject: &'static str,
    now: SystemTime,
) -> Result<TenantId, Response> {
    // Arm 3 — auth NOT configured: today's env-tenant path, verbatim. The
    // `Authorization` header is IGNORED in this arm (backward compatibility,
    // US-RAUTH-02). This branch is taken FIRST so the bearer path is only
    // ever reached when a validator is present.
    let Some(validator) = auth else {
        return resolve_tenant_or_refuse(env_tenant, service_label).cloned();
    };

    // Auth IS configured: the bearer is the sole tenant authority. The env
    // tenant is unreachable from here on — there is no path from a missing
    // or invalid bearer to `env_tenant` (the no-bearer-bypass, R3).
    let Some(token) = bearer_token(http_authorization(headers)) else {
        // Arm 2a — the bearer claim is absent or empty. This never reaches
        // aegis, so the shared capability emits the one pre-validate decision
        // line itself, in the same field shape aegis uses (DD5).
        tracing::warn!(
            tenant_id = "",
            role = "",
            decision = "deny",
            subject = subject,
            reason = "missing_claim",
            "read-path authz decision"
        );
        return Err(reject_unauthorized("missing_claim"));
    };

    // A present bearer reaches aegis, which validates signature / exp / issuer
    // / audience / tenant / role and emits the ONE decision line (allow on
    // success, deny on failure) carrying the matching `reason()` (DD5).
    match validator.validate_with_subject(token, now, subject) {
        // Arm 1 — valid bearer: the query scopes to the TOKEN's tenant.
        Ok(context) => Ok(context.tenant_id),
        // Arm 2b — a present-but-invalid bearer: fail-closed 401 with the
        // aegis reason. Still no env fall-through.
        Err(error) => Err(reject_unauthorized(error.reason())),
    }
}

/// Extract the bearer token from a raw `Authorization` header value
/// (`"Bearer <token>"`). Returns the non-empty token, or `None` when the
/// value is absent, not a `Bearer` scheme, or carries an empty token (the
/// `"Bearer "` case). The scheme match is case-insensitive per RFC 7235;
/// the token is returned verbatim (aegis classifies a non-JWT as
/// `malformed`). Mirrors `aperture::transport::bearer_token`.
fn bearer_token(raw: Option<&str>) -> Option<&str> {
    let raw = raw?;
    let rest = raw.strip_prefix("Bearer ").or_else(|| {
        let (scheme, rest) = raw.split_once(' ')?;
        scheme.eq_ignore_ascii_case("bearer").then_some(rest)
    })?;
    let token = rest.trim();
    if token.is_empty() {
        return None;
    }
    Some(token)
}

/// Read the raw `Authorization` header value as a `&str`, if present and
/// valid UTF-8. A non-UTF-8 header value is treated as absent (the auth
/// step then rejects with `missing_claim`). Mirrors
/// `aperture::transport::http_authorization`.
fn http_authorization(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
}

/// Build the HTTP 401 read-auth reject (ADR-0074 DD2, RFC 6750 §3):
/// `401 Unauthorized` with a `WWW-Authenticate: Bearer` challenge naming
/// the aegis `reason()` as `error_description`, over the shared
/// [`error_response`]/[`ErrorBody`] JSON envelope. Carries NEITHER the
/// token NOR the secret — only the stable aegis reason class. Mirrors
/// `aperture::transport::reject_http_unauthorized`.
fn reject_unauthorized(reason: &str) -> Response {
    let challenge = format!("Bearer error=\"invalid_token\", error_description=\"{reason}\"");
    let mut response = error_response(StatusCode::UNAUTHORIZED, reason);
    response.headers_mut().insert(
        axum::http::header::WWW_AUTHENTICATE,
        challenge
            .parse()
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("Bearer")),
    );
    response
}

/// Build the JSON error envelope at the given status code.
///
/// Returns `(status, Json({"status":"error","error":reason})).into_response()`
/// byte-for-byte equal to the three consumer crates' pre-extraction
/// helper. The `reason` parameter is `&'static str` to make accidental
/// echoing of request-derived input a type error at the call site (the
/// parser's interpolated `String` reasons flow through their own arm
/// via a sibling helper signature; see DELIVER's Mikado step C/E).
///
/// DELIVER state: implemented. Builds the byte-identical
/// `{"status":"error","error":"<reason>"}` envelope today emitted by
/// the three consumer crates. The `reason` parameter is `&str` (the
/// design pin in DD3) so both `&'static str` consts and interpolated
/// `String`s pass through (via auto-deref) without an `.as_str()`
/// indirection.
pub fn error_response(status: StatusCode, reason: &str) -> Response {
    let body = ErrorBody {
        status: "error",
        error: reason,
    };
    (status, Json(body)).into_response()
}

// =========================================================================
// Observability — shared tracing-subscriber install (read-api-tracing-subscriber-v0)
// =========================================================================

/// Install the read-tier tracing subscriber. Idempotent (`OnceLock`-guarded),
/// infallible, and safe to call from every `main` and from tests.
///
/// Called as the FIRST statement of each read binary's `main`
/// (`query-api`, `log-query-api`, `trace-query-api`), before any
/// `tracing::` call and before the earliest fallible startup steps
/// (`create_dir_all`, `*Store::open`, `resolve_addr`). This guarantees
/// every event from `*_starting` onward reaches stderr (DD2).
///
/// DELIVER will fill the body with aperture's subscriber builder (DD3):
/// a JSON `fmt` layer to stderr, flattened events, no target/span noise,
/// behind an `EnvFilter` keyed off `RUST_LOG` (the one deliberate
/// divergence from aperture's `APERTURE_LOG`), defaulting to `info`. The
/// rendered line shape matches aperture byte-for-byte so one JSON parser
/// covers all four read-tier binaries (US-05).
///
/// ## DISTILL scaffold state (read-api-tracing-subscriber-v0)
///
/// The body is a deliberate NO-OP. It compiles, installs nothing, and
/// NEVER panics, so the three binaries start exactly as they do today
/// (every lifecycle event still discarded). This is the RED-not-BROKEN
/// posture (Mandate 7): the new subprocess acceptance test
/// (`crates/log-query-api/tests/slice_07_tracing_subscriber.rs`) asserts
/// `health.startup.refused` / `*_starting` reach stderr and is therefore
/// RED against this no-op, while every EXISTING test that launches a
/// binary stays GREEN because the binary still boots. DELIVER replaces
/// the body with the real install and the acceptance test turns GREEN.
///
/// Mutation posture (C6): `init_tracing` is `OnceLock`-guarded
/// global-install wiring; it is exercised only by the black-box
/// subprocess run and carries the same unkillable-wiring posture as each
/// `main`. The killable in-process surface — the `OnceLock` idempotence
/// guard — is pinned by `test_init_tracing_is_idempotent_and_never_panics`.
/// The per-feature mutation run scopes this function out via the
/// `cargo-mutants` file/regex filter rather than the `#[mutants::skip]`
/// attribute, because `query-http-common` does not carry the `mutants`
/// no-op decorator crate as a dependency.
pub fn init_tracing() {
    // `OnceLock`-guarded so a second call (every `main` calls it once;
    // tests in a shared process may call it repeatedly) is a silent
    // no-op. Without the guard a second `try_init` would return `Err`;
    // the guard makes the helper infallible and idempotent.
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        // Aperture's posture (ADR-0009) verbatim, with the one deliberate
        // divergence DD3 pins: the filter keys off `RUST_LOG` (the
        // conventional operator-facing name the read tier uses) rather
        // than aperture's `APERTURE_LOG`. Everything else — JSON to
        // stderr, flattened events, `info` default, no target/span noise —
        // is identical, so the rendered line shape matches aperture and
        // one JSON parser covers all three read-tier binaries (US-05).
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(std::io::stderr)
                    .flatten_event(true)
                    .with_current_span(false)
                    .with_span_list(false)
                    .with_target(false),
            )
            .try_init();
    });
}

// =========================================================================
// Inline tests
// =========================================================================
//
// Two tiers:
//
// 1. Data-only tests (constants, struct serialisation) — already GREEN at
//    DISTILL close. These are the Mandate 7 ground truth: the data
//    surface of the public API is real, the literal reason texts are
//    byte-for-byte equal to the three consumers' pre-extraction code,
//    and the ErrorBody envelope serialises to the wire shape K2 pins.
//
// 2. Function-body tests — `#[ignore]`'d at DISTILL close so the
//    workspace pre-commit gate passes. Each one targets ONE behaviour
//    of one of the three `unimplemented!` functions; DELIVER's Crafty
//    de-ignores them ONE AT A TIME (outer-loop convention) and implements
//    the body until the test passes.

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Tier 1: data-only tests (GREEN at DISTILL close) -----

    #[test]
    fn test_max_window_seconds_value() {
        // The cap value is the byte-for-byte ADR-0050 Decision 1 value;
        // the three consumer crates each duplicate this assertion today.
        // Post-extraction the assertion lives once, here.
        assert_eq!(MAX_WINDOW_SECONDS, 86_400);
    }

    #[test]
    fn test_max_result_rows_value() {
        // ADR-0050 Decision 2; same single-source posture as above.
        assert_eq!(MAX_RESULT_ROWS, 100_000);
    }

    #[test]
    fn test_reason_constants_match_callsite_texts() {
        // Every literal here is byte-for-byte equal to a string today
        // emitted by one of the three consumer crates. The K2
        // byte-identity acceptance gate (DELIVER step E-G) verifies
        // the wire bytes through the existing acceptance suites; this
        // inline test verifies the constants themselves do not drift.
        assert_eq!(
            REASON_INVALID_TIME_RANGE,
            "invalid time bounds: end is earlier than start"
        );
        assert_eq!(REASON_WINDOW_TOO_LARGE, "window exceeds 86400 seconds");
        assert_eq!(REASON_TOO_MANY_ROWS, "result exceeds 100000 rows");
        assert_eq!(REASON_MISSING_TENANT, "no tenant resolvable: ");
    }

    #[test]
    fn test_reason_constants_never_contain_a_credential_marker() {
        // Redaction symmetry: every reason const is a literal that never
        // interpolates a request-derived value. The literal must not
        // contain a forwarded credential marker.
        for reason in [
            REASON_INVALID_TIME_RANGE,
            REASON_WINDOW_TOO_LARGE,
            REASON_TOO_MANY_ROWS,
            REASON_MISSING_TENANT,
        ] {
            assert!(!reason.contains("SECRET"), "reason leaks SECRET: {reason}");
            assert!(!reason.contains("Bearer"), "reason leaks Bearer: {reason}");
        }
    }

    #[test]
    fn test_error_body_serialises_to_expected_json() {
        // The wire-shape contract: {"status":"error","error":"<reason>"}.
        // Pinned across ADR-0042, ADR-0047, ADR-0048, ADR-0053. The K2
        // acceptance gate runs the three consumers' existing acceptance
        // suites; this inline test verifies the typed seam itself.
        let body = ErrorBody {
            status: "error",
            error: REASON_WINDOW_TOO_LARGE,
        };
        let json = serde_json::to_string(&body).expect("serialise");
        assert_eq!(
            json,
            r#"{"status":"error","error":"window exceeds 86400 seconds"}"#
        );
    }

    #[test]
    fn test_error_body_field_order_is_status_then_error() {
        // serde_json honours struct field declaration order. The three
        // consumers' pre-extraction `json!` calls happen to emit the
        // same order (`status` then `error`); the K2 byte-identity gate
        // would catch a drift, but this inline test pins it here too so
        // a mutation that reorders the ErrorBody fields is killed.
        let body = ErrorBody {
            status: "error",
            error: "any reason",
        };
        let json = serde_json::to_string(&body).expect("serialise");
        let status_pos = json.find("\"status\"").expect("status field present");
        let error_pos = json.find("\"error\"").expect("error field present");
        assert!(
            status_pos < error_pos,
            "status must precede error in the envelope: {json}"
        );
    }

    #[test]
    fn test_time_range_is_constructible_with_start_le_end() {
        // The TimeRange struct itself is data; once DELIVER fills the
        // parser body, this test still pins that the type accepts the
        // canonical `start <= end` shape. The parser-level rejection
        // of inverted bounds is covered by an ignored test below.
        let tr = TimeRange {
            start_secs: 100,
            end_secs: 200,
        };
        assert_eq!(tr.start_secs, 100);
        assert_eq!(tr.end_secs, 200);
    }

    // ----- Tier 2: function-body tests (RED via #[ignore] at DISTILL close) -----
    //
    // Each test asserts ONE behaviour of one of the three scaffolded
    // functions. The `#[ignore]` attribute is removed by DELIVER's Crafty
    // ONE AT A TIME as he implements each behaviour, per the outer-loop
    // Outside-In TDD convention (Mandate 7 RED-not-BROKEN: the test
    // exists, compiles, and identifies a failing observable behaviour;
    // the workspace pre-commit gate is unaffected because `#[ignore]`'d
    // tests are not run by default).

    // ----- parse_time_range -----

    #[test]
    fn test_parse_time_range_accepts_valid_integer_bounds() {
        let tr = parse_time_range("100", "200").expect("valid bounds");
        assert_eq!(tr.start_secs, 100);
        assert_eq!(tr.end_secs, 200);
    }

    #[test]
    fn test_parse_time_range_accepts_equal_bounds_as_empty_range() {
        // start == end is a valid empty half-open range, NOT an
        // inverted-bounds rejection.
        let tr = parse_time_range("100", "100").expect("equal bounds are valid");
        assert_eq!(tr.start_secs, 100);
        assert_eq!(tr.end_secs, 100);
    }

    #[test]
    fn test_parse_time_range_accepts_zero_as_lower_bound() {
        // Zero is the epoch and a perfectly valid lower bound.
        let tr = parse_time_range("0", "100").expect("zero is a valid bound");
        assert_eq!(tr.start_secs, 0);
        assert_eq!(tr.end_secs, 100);
    }

    #[test]
    fn test_parse_time_range_truncates_fractional_seconds() {
        // Prism emits floats; a `.5` fraction must parse and truncate.
        let tr = parse_time_range("100.5", "200.9").expect("float bounds parse");
        assert_eq!(tr.start_secs, 100);
        assert_eq!(tr.end_secs, 200);
    }

    #[test]
    fn test_parse_time_range_rejects_non_numeric_start() {
        assert!(parse_time_range("notanumber", "100").is_err());
    }

    #[test]
    fn test_parse_time_range_rejects_non_numeric_end() {
        assert!(parse_time_range("100", "later").is_err());
    }

    #[test]
    fn test_parse_time_range_rejects_negative_bounds() {
        assert!(parse_time_range("-1", "100").is_err());
    }

    #[test]
    fn test_parse_time_range_rejects_inverted_bounds_with_named_reason() {
        let err = parse_time_range("200", "100").expect_err("inverted is invalid");
        assert_eq!(err, REASON_INVALID_TIME_RANGE);
    }

    #[test]
    fn test_parse_time_range_error_never_echoes_raw_value() {
        // Redaction symmetry: the reason names the field class, never
        // the raw input string.
        let err = parse_time_range("secretvalue", "100").expect_err("rejected");
        assert!(!err.contains("secretvalue"), "reason leaks raw: {err}");
    }

    // ----- resolve_tenant_or_refuse -----

    #[test]
    fn test_resolve_tenant_or_refuse_returns_some_tenant_unchanged() {
        let tenant = Some(TenantId("acme".to_string()));
        let resolved = resolve_tenant_or_refuse(&tenant, "the query").expect("tenant present");
        assert_eq!(resolved.0, "acme");
    }

    #[tokio::test]
    async fn test_resolve_tenant_or_refuse_refuses_none_with_401() {
        let tenant: Option<TenantId> = None;
        let resp = resolve_tenant_or_refuse(&tenant, "the query").expect_err("None refused");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_resolve_tenant_or_refuse_uses_service_label_in_reason() {
        // The pillar-specific suffix is interpolated; the three consumers
        // each pass their own `"the query"` / `"the log query"` / `"the
        // trace query"` literal. The K2 gate verifies the full body
        // byte sequence pre/post; this inline test verifies the helper
        // honours the label by extracting the response body.
        let tenant: Option<TenantId> = None;
        let resp = resolve_tenant_or_refuse(&tenant, "the trace query").expect_err("None refused");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body extracts");
        let body = std::str::from_utf8(&bytes).expect("utf-8");
        assert!(
            body.contains("the trace query service refuses unscoped requests"),
            "body must carry the per-pillar reason: {body}"
        );
        // Redaction: never a forwarded credential marker.
        assert!(!body.contains("SECRET"), "body leaks SECRET: {body}");
        assert!(!body.contains("Bearer"), "body leaks Bearer: {body}");
    }

    // ----- error_response -----

    #[test]
    fn test_error_response_returns_given_status_code() {
        let resp = error_response(StatusCode::BAD_REQUEST, REASON_WINDOW_TOO_LARGE);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_error_response_body_is_byte_identical_json_envelope() {
        // The K2 acceptance gate in the three consumer crates is the
        // wire-bytes regression net; this inline test pins the typed
        // seam in isolation by extracting and comparing the body byte
        // sequence to the literal envelope.
        let resp = error_response(StatusCode::BAD_REQUEST, REASON_WINDOW_TOO_LARGE);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body extracts");
        let body = std::str::from_utf8(&bytes).expect("utf-8");
        assert_eq!(
            body,
            r#"{"status":"error","error":"window exceeds 86400 seconds"}"#
        );
    }

    #[test]
    fn test_error_response_content_type_is_application_json() {
        let resp = error_response(StatusCode::BAD_REQUEST, REASON_WINDOW_TOO_LARGE);
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .expect("content-type set")
            .to_str()
            .expect("ascii");
        assert!(ct.starts_with("application/json"), "got {ct}");
    }

    #[test]
    fn test_error_response_carries_unauthorized_status() {
        // The 401 arm of the three consumers; pinned here so a mutant
        // that hard-codes BAD_REQUEST is killed.
        let resp = error_response(StatusCode::UNAUTHORIZED, REASON_MISSING_TENANT);
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_error_response_carries_internal_server_error_status() {
        // The 500 arm of the three consumers (the "the backing store
        // could not be read" reason is per-pillar, not in this crate;
        // the consumer passes it as &'static str to error_response).
        let resp = error_response(StatusCode::INTERNAL_SERVER_ERROR, "any reason");
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ----- init_tracing (read-api-tracing-subscriber-v0) -----
    //
    // Option B of the pinned verification strategy: the in-process
    // idempotence contract. This is NOT a body-behaviour test (the
    // observable "events render to stderr" behaviour is asserted
    // black-box by the subprocess acceptance test, which is RED against
    // the no-op). It pins the ONE in-process invariant that holds for
    // BOTH the scaffold no-op AND the real DELIVER body: `init_tracing`
    // is `OnceLock`-guarded, so calling it more than once (every `main`
    // calls it once; tests may call it repeatedly across the shared
    // process) never panics. This test is GREEN now and stays GREEN
    // after DELIVER — it guards the idempotence contract, not the
    // unimplemented behaviour, so it is correctly NOT `#[ignore]`.

    #[test]
    fn test_init_tracing_is_idempotent_and_never_panics() {
        // First and second calls must both return without panicking; the
        // OnceLock guard makes the second a no-op even once DELIVER lands
        // the real `try_init` builder (a second global install would
        // otherwise be an error / panic without the guard).
        init_tracing();
        init_tracing();
    }
}
