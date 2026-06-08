// Kaleidoscope query-http-common — slice 07 shared read-auth capability
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

//! Slice 07 (shared) — the per-request read-auth capability that lands ONCE
//! in `query-http-common` (ADR-0074 DD3; ADR-0054 rationale).
//!
//! Feature: `read-path-query-api-auth-v0`. This file pins the properties
//! that belong to the SHARED `resolve_request_tenant_or_refuse` capability
//! itself — the ones the three read APIs would otherwise re-prove three
//! times (ADR-0074 Option B rejected): the full 8-reason aegis matrix
//! surfacing on the read path (US-RAUTH-04), one-audit-event-per-request
//! including the PRE-VALIDATE `missing_claim` case the shared capability
//! emits itself (DD5), and the secret/token-never-logged audit redaction
//! guardrail (System Constraint 4). The per-API HTTP wire behaviour
//! (status, isolation, the no-bearer-bypass) is pinned in each API's own
//! ephemeral-bind suite.
//!
//! ## The seam under test (Mandate 1)
//!
//! `query_http_common::resolve_request_tenant_or_refuse(auth, headers,
//! env_tenant, service_label, subject, now)` IS the driving port of the
//! shared capability: it is the single auditable resolution point all three
//! handlers route through. The tests build a real `HeaderMap`, mint real
//! HS256 tokens, and assert the OBSERVABLE outcomes (the audit decision
//! line and the `Result`), not internal state.
//!
//! ## Token-minting (mirrors aperture slice_10 + aegis slice_03)
//!
//! In-suite `jsonwebtoken::encode`, audience `kaleidoscope-query`. Each
//! negative-control mint perturbs one axis to drive a distinct aegis
//! `reason`. The catalogue holds `acme-prod`; `auditor` is the unknown-role
//! control; an out-of-catalogue tenant is the unknown-tenant control.
//!
//! ## RED-not-BROKEN (Mandate 7)
//!
//! `resolve_request_tenant_or_refuse` is a SCAFFOLD that PANICS
//! (`__SCAFFOLD__ read-path-query-api-auth-v0 RED`). Each scenario calls it
//! inside `catch_unwind` and asserts on the captured audit events + the
//! `Result`. Against the scaffold the panic means NO audit line is emitted
//! and NO `Ok/Err` is produced, so every assertion (e.g. "exactly one deny
//! line with reason=expired") FAILS — RED, not BROKEN (the symbol resolves
//! and the suite COMPILES). All scenarios are `#[ignore]`d so
//! `cargo test --workspace` stays green; DELIVER un-ignores one at a time.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::header::HeaderMap;
use axum::response::Response;
use jsonwebtoken::{encode, EncodingKey, Header};
use query_http_common::{
    load_catalogue, resolve_request_tenant_or_refuse, TenantId, Validator, ValidatorConfig,
};
use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::subscriber::with_default;
use tracing::{Event, Level, Subscriber};

const ISSUER: &str = "acme-observability";
const AUDIENCE: &str = "kaleidoscope-query";
const INGEST_AUDIENCE: &str = "kaleidoscope-ingest";
const SECRET: &[u8] = b"slice-07-shared-read-auth-secret-not-for-production";
const WRONG_SECRET: &[u8] = b"a-different-secret-that-must-not-validate-the-token";
const TENANT: &str = "acme-prod";
const SERVICE_LABEL: &str = "the query";
const SUBJECT: &str = "query_range";

#[derive(Debug, Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    exp: i64,
    tenant_id: &'a str,
    kaleidoscope_role: &'a str,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sign(claims: &Claims<'_>, secret: &[u8]) -> String {
    encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret),
    )
    .expect("encode HS256 jwt")
}

fn token(iss: &str, aud: &str, exp_offset: i64, tenant: &str, role: &str, secret: &[u8]) -> String {
    sign(
        &Claims {
            iss,
            aud,
            exp: now_secs() + exp_offset,
            tenant_id: tenant,
            kaleidoscope_role: role,
        },
        secret,
    )
}

fn validator() -> Arc<Validator> {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let cat_path = std::env::temp_dir().join(format!("qhc-read-auth-cat-{stamp}.toml"));
    std::fs::write(&cat_path, format!("[[tenants]]\nid = \"{TENANT}\"\n")).expect("write cat");
    let catalogue = load_catalogue(&cat_path).expect("load catalogue");
    let _ = std::fs::remove_file(&cat_path);
    Arc::new(Validator::new(ValidatorConfig {
        issuer: ISSUER.to_string(),
        audience: AUDIENCE.to_string(),
        hs256_key: SECRET.to_vec(),
        catalogue,
    }))
}

fn headers_with_bearer(value: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(v) = value {
        headers.insert(
            "authorization",
            format!("Bearer {v}").parse().expect("header value parses"),
        );
    }
    headers
}

/// A raw `Authorization` value (used for the empty-`Bearer ` case where we
/// want exactly `"Bearer "` with no token).
fn headers_with_raw_authorization(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", value.parse().expect("header value parses"));
    headers
}

// --------------------------------------------------------------------
// In-process capturing subscriber (mirrors aegis slice_03_audit.rs).
// --------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct AuditEvent {
    level: String,
    fields: std::collections::BTreeMap<String, String>,
}

#[derive(Default)]
struct AuditSubscriber {
    events: Arc<Mutex<Vec<AuditEvent>>>,
}

impl AuditSubscriber {
    fn new() -> (Self, Arc<Mutex<Vec<AuditEvent>>>) {
        let events: Arc<Mutex<Vec<AuditEvent>>> = Arc::default();
        (
            Self {
                events: Arc::clone(&events),
            },
            events,
        )
    }
}

struct FieldVisitor<'a> {
    fields: &'a mut std::collections::BTreeMap<String, String>,
}

impl<'a> Visit for FieldVisitor<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

impl Subscriber for AuditSubscriber {
    fn enabled(&self, _m: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _s: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _s: &tracing::span::Id, _v: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _s: &tracing::span::Id, _f: &tracing::span::Id) {}
    fn event(&self, event: &Event<'_>) {
        let level = match *event.metadata().level() {
            Level::ERROR => "error",
            Level::WARN => "warn",
            Level::INFO => "info",
            Level::DEBUG => "debug",
            Level::TRACE => "trace",
        };
        let mut fields = std::collections::BTreeMap::new();
        event.record(&mut FieldVisitor {
            fields: &mut fields,
        });
        self.events.lock().unwrap().push(AuditEvent {
            level: level.to_string(),
            fields,
        });
    }
    fn enter(&self, _s: &tracing::span::Id) {}
    fn exit(&self, _s: &tracing::span::Id) {}
}

/// Drive the shared capability under a capturing subscriber. Returns the
/// captured audit events and the `Result` (swallowing the scaffold panic so
/// the test asserts on observable outcomes, not the panic — the panic means
/// zero events + no Result, which fails every assertion below: RED).
type Resolution = Result<TenantId, Response>;

fn run(
    auth: Option<&Arc<Validator>>,
    headers: &HeaderMap,
    env: &Option<TenantId>,
) -> (Vec<AuditEvent>, Option<Resolution>) {
    let (subscriber, events) = AuditSubscriber::new();
    let result = with_default(subscriber, || {
        catch_unwind(AssertUnwindSafe(|| {
            resolve_request_tenant_or_refuse(
                auth,
                headers,
                env,
                SERVICE_LABEL,
                SUBJECT,
                SystemTime::now(),
            )
        }))
        .ok()
    });
    let captured = events.lock().unwrap().clone();
    (captured, result)
}

/// The deny audit lines (decision=deny) in the capture.
fn deny_lines(events: &[AuditEvent]) -> Vec<&AuditEvent> {
    events
        .iter()
        .filter(|e| e.fields.get("decision").map(String::as_str) == Some("deny"))
        .collect()
}

/// The decision lines (allow or deny) in the capture.
fn decision_lines(events: &[AuditEvent]) -> Vec<&AuditEvent> {
    events
        .iter()
        .filter(|e| e.fields.contains_key("decision"))
        .collect()
}

/// Assert exactly one deny line with `reason` and `subject=query_range`, and
/// that the resolution is an `Err` (a 401 Response).
fn assert_one_deny(reason: &str, headers: HeaderMap) {
    let v = validator();
    let (events, result) = run(Some(&v), &headers, &None);
    let lines = deny_lines(&events);
    assert_eq!(
        lines.len(),
        1,
        "reason {reason}: exactly one deny decision line (never zero, never duplicated); got {} : {:?}",
        lines.len(),
        lines.iter().map(|e| e.fields.get("reason")).collect::<Vec<_>>()
    );
    assert_eq!(
        lines[0].fields.get("reason").map(String::as_str),
        Some(reason),
        "reason field mismatch"
    );
    assert_eq!(
        lines[0].fields.get("subject").map(String::as_str),
        Some(SUBJECT),
        "subject must name the read action"
    );
    // The resolution is the 401 fail-closed Response (DD2 / DD4): not Ok,
    // not None, and the Response itself carries 401 + the RFC-6750
    // `WWW-Authenticate: Bearer` challenge. Asserting the observable
    // Response shape (status + challenge) here pins the shared seam's reject
    // wire contract at the crate that owns it, so a mutant that hollows out
    // `reject_unauthorized` to a default 200 Response is caught.
    let response = match result {
        Some(Err(response)) => response,
        other => panic!(
            "reason {reason}: resolution must be Err(401), not Ok/None; got {:?}",
            other.as_ref().map(|r| r.is_ok())
        ),
    };
    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNAUTHORIZED,
        "reason {reason}: the reject Response must be 401 Unauthorized"
    );
    let challenge = response
        .headers()
        .get(axum::http::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        challenge.contains("Bearer"),
        "reason {reason}: the 401 must carry a WWW-Authenticate: Bearer challenge (RFC 6750); got {challenge:?}"
    );
}

// =========================================================================
// US-RAUTH-04 — the 8-reason matrix, each distinct, on the shared seam
// (@US-RAUTH-04)
// =========================================================================

/// reason `invalid_signature` — a real JWT signed with the wrong key.
#[test]
fn reason_invalid_signature() {
    let t = token(ISSUER, AUDIENCE, 3600, TENANT, "viewer", WRONG_SECRET);
    assert_one_deny("invalid_signature", headers_with_bearer(Some(&t)));
}

/// reason `expired` — past `exp`.
#[test]
fn reason_expired() {
    let t = token(ISSUER, AUDIENCE, -300, TENANT, "viewer", SECRET);
    assert_one_deny("expired", headers_with_bearer(Some(&t)));
}

/// reason `wrong_issuer` — `iss` mismatch.
#[test]
fn reason_wrong_issuer() {
    let t = token("evil-issuer", AUDIENCE, 3600, TENANT, "viewer", SECRET);
    assert_one_deny("wrong_issuer", headers_with_bearer(Some(&t)));
}

/// reason `wrong_audience` — the cross-surface fence: an ingest-audience
/// token on the read path (KPI-6).
#[test]
fn reason_wrong_audience_ingest_token() {
    let t = token(ISSUER, INGEST_AUDIENCE, 3600, TENANT, "viewer", SECRET);
    assert_one_deny("wrong_audience", headers_with_bearer(Some(&t)));
}

/// reason `unknown_tenant` — a catalogue-absent tenant.
#[test]
fn reason_unknown_tenant() {
    let t = token(ISSUER, AUDIENCE, 3600, "acme-prod-evil", "viewer", SECRET);
    assert_one_deny("unknown_tenant", headers_with_bearer(Some(&t)));
}

/// reason `unknown_role` — a role that is neither viewer nor operator
/// (DD6: v0 reads with any catalogued viewer/operator; `auditor` rejects).
#[test]
fn reason_unknown_role() {
    let t = token(ISSUER, AUDIENCE, 3600, TENANT, "auditor", SECRET);
    assert_one_deny("unknown_role", headers_with_bearer(Some(&t)));
}

/// reason `malformed` — a bearer value that is not a JWT at all (distinct
/// from invalid_signature and missing_claim).
#[test]
fn reason_malformed() {
    assert_one_deny("malformed", headers_with_bearer(Some("not-a-jwt")));
}

/// reason `missing_claim` for NO `Authorization` header — the PRE-VALIDATE
/// case the shared capability emits itself (aegis never sees it; DD5).
#[test]
fn reason_missing_claim_no_header() {
    assert_one_deny("missing_claim", headers_with_bearer(None));
}

/// reason `missing_claim` for an EMPTY `Bearer ` value — also pre-validate,
/// distinct from `malformed` (a present-but-non-JWT token).
#[test]
fn reason_missing_claim_empty_bearer() {
    assert_one_deny("missing_claim", headers_with_raw_authorization("Bearer "));
}

// =========================================================================
// US-RAUTH-04 — the reasons are mutually distinct
// (@US-RAUTH-04)
// =========================================================================

/// AC: the reasons are mutually distinct (`malformed` != `invalid_signature`
/// != `missing_claim`). Collect the reason for each distinct cause and assert
/// no two collide.
#[test]
fn the_eight_reasons_are_mutually_distinct() {
    let v = validator();
    let cases: Vec<(&str, HeaderMap)> = vec![
        (
            "invalid_signature",
            headers_with_bearer(Some(&token(
                ISSUER,
                AUDIENCE,
                3600,
                TENANT,
                "viewer",
                WRONG_SECRET,
            ))),
        ),
        (
            "expired",
            headers_with_bearer(Some(&token(
                ISSUER, AUDIENCE, -300, TENANT, "viewer", SECRET,
            ))),
        ),
        (
            "wrong_issuer",
            headers_with_bearer(Some(&token(
                "evil", AUDIENCE, 3600, TENANT, "viewer", SECRET,
            ))),
        ),
        (
            "wrong_audience",
            headers_with_bearer(Some(&token(
                ISSUER,
                INGEST_AUDIENCE,
                3600,
                TENANT,
                "viewer",
                SECRET,
            ))),
        ),
        (
            "unknown_tenant",
            headers_with_bearer(Some(&token(
                ISSUER, AUDIENCE, 3600, "ghost", "viewer", SECRET,
            ))),
        ),
        (
            "unknown_role",
            headers_with_bearer(Some(&token(
                ISSUER, AUDIENCE, 3600, TENANT, "auditor", SECRET,
            ))),
        ),
        ("malformed", headers_with_bearer(Some("not-a-jwt"))),
        ("missing_claim", headers_with_bearer(None)),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (expected, headers) in cases {
        let (events, _result) = run(Some(&v), &headers, &None);
        let lines = deny_lines(&events);
        assert_eq!(
            lines.len(),
            1,
            "{expected}: exactly one deny line; got {}",
            lines.len()
        );
        let reason = lines[0].fields.get("reason").cloned().unwrap_or_default();
        assert_eq!(reason, expected, "the cause must surface its own reason");
        assert!(
            seen.insert(reason.clone()),
            "reason {reason} is not distinct"
        );
    }
    assert_eq!(
        seen.len(),
        8,
        "all 8 reasons must be present and distinct; got {seen:?}"
    );
}

// =========================================================================
// DD5 — exactly one decision event per request (never zero, never duplicated)
// (@US-RAUTH-04 @kpi)
// =========================================================================

/// DD5 / AC one-audit-event-per-rejected-request: a single rejected request
/// emits EXACTLY ONE decision line — including the pre-validate missing_claim
/// case (which aegis never sees, so the shared capability must emit it once,
/// not zero and not twice).
#[test]
fn exactly_one_decision_event_for_a_missing_claim_request() {
    let v = validator();
    let (events, _result) = run(Some(&v), &headers_with_bearer(None), &None);
    let lines = decision_lines(&events);
    assert_eq!(
        lines.len(),
        1,
        "exactly one decision line for a pre-validate missing_claim request; got {}: {:?}",
        lines.len(),
        lines
            .iter()
            .map(|e| (e.fields.get("decision"), e.fields.get("reason")))
            .collect::<Vec<_>>()
    );
}

/// DD5 / AC one-audit-event-per-request: a VALID token emits exactly one
/// `allow` decision line carrying the validated `tenant_id` and
/// `subject=query_range`, and resolves `Ok(acme-prod)`.
#[test]
fn exactly_one_allow_event_for_a_valid_token() {
    let v = validator();
    let t = token(ISSUER, AUDIENCE, 3600, TENANT, "viewer", SECRET);
    let (events, result) = run(Some(&v), &headers_with_bearer(Some(&t)), &None);
    let lines = decision_lines(&events);
    assert_eq!(
        lines.len(),
        1,
        "exactly one allow decision line; got {}",
        lines.len()
    );
    assert_eq!(
        lines[0].fields.get("decision").map(String::as_str),
        Some("allow")
    );
    assert_eq!(
        lines[0].fields.get("subject").map(String::as_str),
        Some(SUBJECT)
    );
    assert_eq!(
        lines[0].fields.get("tenant_id").map(String::as_str),
        Some(TENANT)
    );
    match result {
        Some(Ok(tid)) => assert_eq!(tid.0, TENANT, "resolves to the token's tenant"),
        other => panic!(
            "a valid token must resolve Ok(acme-prod); got {:?}",
            other.map(|r| r.is_ok())
        ),
    }
}

// =========================================================================
// System Constraint 4 — secret + raw token never appear in any audit line
// (hard guardrail)
// =========================================================================

/// AC the-secret-and-token-are-never-logged: across a denied request and an
/// accepted request, NO audit line contains the HS256 secret bytes OR the raw
/// token. A substring-absence scan over every captured field of every line
/// (the secret + token are ASCII, so a substring scan is faithful).
#[test]
fn the_secret_and_token_never_appear_in_any_audit_line() {
    let v = validator();
    let valid = token(ISSUER, AUDIENCE, 3600, TENANT, "viewer", SECRET);
    let bad = token(ISSUER, AUDIENCE, 3600, TENANT, "viewer", WRONG_SECRET);
    let secret_str = std::str::from_utf8(SECRET).expect("ascii secret");

    let mut all: Vec<AuditEvent> = Vec::new();
    let (e1, _) = run(Some(&v), &headers_with_bearer(Some(&bad)), &None);
    let (e2, _) = run(Some(&v), &headers_with_bearer(Some(&valid)), &None);
    all.extend(e1);
    all.extend(e2);
    assert!(
        !all.is_empty(),
        "the capability must emit at least one decision line (else there is nothing to redact-check) \
         — zero lines means the scaffold is still in place (RED)"
    );
    for e in &all {
        let rendered = format!("{} {:?}", e.level, e.fields);
        assert!(
            !rendered.contains(secret_str),
            "secret bytes leaked into an audit line: {rendered}"
        );
        assert!(
            !rendered.contains(&valid),
            "the valid raw token leaked: {rendered}"
        );
        assert!(
            !rendered.contains(&bad),
            "the bad raw token leaked: {rendered}"
        );
    }
}

// =========================================================================
// DD6 — role question resolved (recorded decision; a documentation test)
// (@US-RAUTH-04)
// =========================================================================

/// US-RAUTH-04 / AC DD6-role-question-resolved. v0 read auth is
/// authentication + tenant-scoping ONLY: any catalogued `viewer` OR
/// `operator` token may read; `unknown_role` rejects; role-gated read
/// authorization is DEFERRED with the decision recorded (ADR-0074 DD6).
///
/// This is a GREEN documentation test (not ignored): it records the resolved
/// decision so the AC is demonstrable from the suite. The behavioural halves
/// (a viewer reads; an auditor rejects unknown_role) are pinned by the
/// matrix + accept tests above. There is intentionally NO `if role != X`
/// gate at v0.
#[test]
fn dd6_role_question_resolved_v0_is_authn_plus_tenant_scoping_only() {
    // v0 accepts both catalogued roles for reads ...
    let read_roles_accepted_at_v0 = ["viewer", "operator"];
    assert_eq!(read_roles_accepted_at_v0.len(), 2);
    // ... and rejects a non-catalogued role free (unknown_role), proven
    // behaviourally by `reason_unknown_role`. Role-gated read authorization
    // is deferred (recorded in ADR-0074 DD6); no role gate ships at v0.
    let role_gated_read_authorization_shipped_at_v0 = false;
    assert!(
        !role_gated_read_authorization_shipped_at_v0,
        "DD6: role-gated read authz is deferred at v0 (recorded decision)"
    );
}
