//! Slice 10 — Aegis ingest authentication on the live aperture OTLP path.
//!
//! Feature: `aegis-ingest-auth-v0` (ADR-0068, DD1-DD7). This slice wires the
//! correct-but-unwired `aegis::Validator` onto aperture's ingest request path,
//! fail-closed. Every ingest request (this slice covers logs on gRPC + HTTP, the
//! walking-skeleton spine and the HTTP-parity leg) extracts a bearer token,
//! validates it, and either rejects (gRPC `UNAUTHENTICATED` / HTTP `401` +
//! `WWW-Authenticate: Bearer`, nothing stored, one deny audit line) or accepts
//! with the validated `tenant_id` riding the accepted record into the sink.
//!
//! Companion stories (DISCUSS `user-stories.md`):
//! US-AUTH-01 (walking skeleton: gRPC logs, fail-closed — the boundary);
//! US-AUTH-03 (HTTP transport parity: 401 + `WWW-Authenticate: Bearer`);
//! US-AUTH-05 (the reject-reason matrix — the 8 aegis `reason()` variants — and
//! one-audit-event-per-request). US-AUTH-02 refuse-to-start lives in its
//! companion subprocess suite `slice_10_ingest_auth_config_reject.rs`;
//! US-AUTH-04 traces/metrics parity is a follow-on slice that reuses this spine
//! (out of THIS slice's scope per the task delta — the logs spine is the
//! falsifiable boundary).
//!
//! ## Driving ports (black-box, brief.md "For Acceptance Designer")
//!
//! The running aperture instance, observed ONLY through: (1) gRPC
//! `authorization` metadata (`Bearer <jwt>`) on `ExportLogsServiceRequest`; (2)
//! HTTP `Authorization` header (`Bearer <jwt>`) on `POST /v1/logs`; (3) the
//! recording sink (accept -> drained record present + tenant-tagged; reject ->
//! sink empty); (4) structured stderr (`stderr_capture`) — exactly ONE decision
//! line per request.
//! No internal auth type is reached: `extract_bearer_*`, `reject_to_*`, the
//! `Validator` wiring, and `TenantScoped` are all crate-internal; the tests
//! drive the real binary's request path over real TCP and assert observable
//! outcomes only.
//!
//! ## The token-minting seam (MANDATORY for falsifiability)
//!
//! Tokens are minted IN-SUITE with `jsonwebtoken::encode` (the same engine aegis
//! uses), signed with the SAME secret bytes the test config's `secret_file`
//! points at, for a tenant present in the test `catalogue_path`, with `iss`/`aud`
//! matching the test config and a future `exp`. The negative-control mints (no
//! token, empty bearer, malformed, expired, bad-signature, wrong-issuer,
//! wrong-audience, unknown-tenant, unknown-role) each drive a distinct aegis
//! `reason`. This is the same seam aegis's own `slice_01_validate.rs` uses
//! (`jsonwebtoken::encode` + a hand-built `Claims`), grounded here against the
//! real aperture boundary rather than the validator in isolation.
//!
//! ## RED-not-BROKEN classification (Mandate 7)
//!
//! aperture + aegis both exist, so the harness, the `stderr_capture` seam, the
//! tonic/reqwest clients, and `jsonwebtoken::encode` all resolve and COMPILE
//! today. The auth WIRING does not exist yet, so every reject scenario is
//! behaviourally RED: against today's no-auth code a tokenless request is
//! ACCEPTED (gRPC OK / HTTP 200, the sink is NON-empty, no deny line), so the
//! "reject + nothing stored + one deny line" assertions FAIL. Each reject test
//! is therefore `#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]` so
//! `cargo test --workspace` stays green at the DISTILL commit; DELIVER removes
//! the ignores one at a time. The accept scenario is ALSO RED-by-config: today
//! `start_with_auth` builds a jwt-configured instance the production code cannot
//! yet honour (the `Config::builder().jwt_auth(..)` seam is the minimal scaffold
//! DELIVER replaces), so it is `#[ignore]`d too.
//!
//! Falsifiability is proven by RUNNING with `--ignored`: each reject test
//! panics on an assertion (the request was accepted / the sink was non-empty /
//! no deny line), NOT on a missing symbol — RED, not BROKEN.

mod common;

use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, EncodingKey, Header};
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use serde::Serialize;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Code;

use crate::common::{capture_stderr_events, encode_logs_request, wait_for, StderrEvent};

const RED: &str = "RED until DELIVER: aegis-ingest-auth-v0";

// =========================================================================
// Test fixtures — issuer / audience / secret / catalogued tenant
// =========================================================================
//
// These are the values the test auth config pins, and the values the in-suite
// minted tokens carry. The "valid" token is signed with SECRET for ISSUER /
// AUDIENCE / TENANT (catalogued) with a future exp. Each negative control
// perturbs exactly one axis.

const ISSUER: &str = "acme-observability";
const AUDIENCE: &str = "kaleidoscope-ingest";
const SECRET: &[u8] = b"slice-10-ingest-auth-test-secret-not-for-production";
const WRONG_SECRET: &[u8] = b"a-different-secret-that-must-not-validate-the-token";
const TENANT: &str = "acme-prod";
const ROLE_OPERATOR: &str = "operator";

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
    let header = Header::new(jsonwebtoken::Algorithm::HS256);
    let key = EncodingKey::from_secret(secret);
    encode(&header, claims, &key).expect("encode HS256 jwt")
}

/// A valid bearer token for the catalogued test tenant: correct
/// iss/aud/signature/tenant/role and a future `exp`.
fn valid_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: TENANT,
            kaleidoscope_role: ROLE_OPERATOR,
        },
        SECRET,
    )
}

// --- Negative-control mints (each perturbs ONE axis) ---------------------

fn expired_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() - 300, // 5 minutes in the past
            tenant_id: TENANT,
            kaleidoscope_role: ROLE_OPERATOR,
        },
        SECRET,
    )
}

fn wrong_issuer_token() -> String {
    sign(
        &Claims {
            iss: "evil-issuer",
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: TENANT,
            kaleidoscope_role: ROLE_OPERATOR,
        },
        SECRET,
    )
}

fn wrong_audience_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: "kaleidoscope-query", // the read-path audience, not ingest
            exp: now_secs() + 3600,
            tenant_id: TENANT,
            kaleidoscope_role: ROLE_OPERATOR,
        },
        SECRET,
    )
}

fn unknown_tenant_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: "acme-prod-evil", // NOT in the test catalogue
            kaleidoscope_role: ROLE_OPERATOR,
        },
        SECRET,
    )
}

fn unknown_role_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: TENANT,
            kaleidoscope_role: "auditor", // neither viewer nor operator
        },
        SECRET,
    )
}

fn invalid_signature_token() -> String {
    // Structurally valid JWT, signed with the WRONG key.
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: TENANT,
            kaleidoscope_role: ROLE_OPERATOR,
        },
        WRONG_SECRET,
    )
}

// =========================================================================
// Auth-configured instance fixture (the minimal DELIVER scaffold)
// =========================================================================
//
// Start a real aperture instance whose ingest path is authenticated against
// ISSUER/AUDIENCE/SECRET/TENANT. The auth config is supplied through a builder
// seam `jwt_auth(...)` that DELIVER lands on `ConfigBuilder` (DD1). At DISTILL
// the seam is a minimal scaffold (it does not wire the validator yet), so these
// instances behave like today's no-auth aperture — which is exactly why the
// reject scenarios driven against them are behaviourally RED.
//
// The secret + catalogue are written to real temp files (Strategy C real-local-
// IO): the seam takes a `secret_file` path + a `catalogue_path`, mirroring the
// production `[aperture.security.auth.jwt]` shape.

/// Drop-guard that removes the temp secret + catalogue files on every exit
/// path (success, assertion failure, panic) so no test litter leaks.
struct TempAuthFiles {
    secret_path: std::path::PathBuf,
    catalogue_path: std::path::PathBuf,
}

impl Drop for TempAuthFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.secret_path);
        let _ = std::fs::remove_file(&self.catalogue_path);
    }
}

fn write_auth_files(label: &str) -> TempAuthFiles {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    );
    let secret_path =
        std::env::temp_dir().join(format!("aperture-auth-secret-{label}-{stamp}.key"));
    let catalogue_path =
        std::env::temp_dir().join(format!("aperture-auth-catalogue-{label}-{stamp}.toml"));
    std::fs::write(&secret_path, SECRET).expect("write test secret file");
    std::fs::write(&catalogue_path, format!("[[tenants]]\nid = \"{TENANT}\"\n"))
        .expect("write test catalogue file");
    TempAuthFiles {
        secret_path,
        catalogue_path,
    }
}

// =========================================================================
// Audit-line assertion helper
// =========================================================================
//
// aegis emits exactly one structured decision event per validate call (info! on
// allow, warn! on deny; fields tenant_id/role/decision/subject/reason), and
// aperture emits the one pre-validate deny line for the no/empty/malformed bearer
// case. Both ride aperture's stderr stream and are captured as `StderrEvent`s.
// We assert on the structured fields (`decision`, `reason`, `subject`), not on
// the event message name, because the decision axis carries its semantics in
// fields (DD5). EXACTLY ONE decision line per request — never zero, never two.

fn field<'a>(e: &'a StderrEvent, key: &str) -> Option<&'a str> {
    e.fields.get(key).and_then(|v| v.as_str())
}

/// Count decision lines (allow or deny) — fields contain `decision`.
fn decision_lines(events: &[StderrEvent]) -> Vec<&StderrEvent> {
    events
        .iter()
        .filter(|e| field(e, "decision").is_some())
        .collect()
}

/// Assert exactly one decision line, with the given decision + reason +
/// subject, and return it.
fn expect_one_decision<'a>(
    events: &'a [StderrEvent],
    decision: &str,
    reason: &str,
    subject: &str,
) -> &'a StderrEvent {
    let lines = decision_lines(events);
    assert_eq!(
        lines.len(),
        1,
        "exactly one decision event per request (never zero, never duplicated); got {} : {:?}",
        lines.len(),
        lines
            .iter()
            .map(|e| (
                field(e, "decision"),
                field(e, "reason"),
                field(e, "subject")
            ))
            .collect::<Vec<_>>()
    );
    let line = lines[0];
    assert_eq!(
        field(line, "decision"),
        Some(decision),
        "decision field mismatch"
    );
    assert_eq!(field(line, "reason"), Some(reason), "reason field mismatch");
    assert_eq!(
        field(line, "subject"),
        Some(subject),
        "subject must name the ingest action"
    );
    line
}

// =========================================================================
// gRPC driving helpers
// =========================================================================

fn decode_request(bytes: Vec<u8>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}

/// Build a gRPC logs request carrying a `Bearer <token>` in the
/// `authorization` metadata.
fn logs_request_with_bearer(
    token: &str,
    service: &str,
    count: usize,
) -> tonic::Request<ExportLogsServiceRequest> {
    let mut req = tonic::Request::new(decode_request(encode_logs_request(service, count)));
    let value: MetadataValue<_> = format!("Bearer {token}")
        .parse()
        .expect("bearer metadata value parses");
    req.metadata_mut().insert("authorization", value);
    req
}

// =========================================================================
// US-AUTH-01 — WALKING SKELETON: gRPC logs, fail-closed (@walking_skeleton)
// =========================================================================
//
// The thinnest end-to-end slice that connects all activities on ONE transport
// (gRPC) for ONE signal (logs): present a token, authenticate at the boundary,
// reject the missing token (nothing stored), tag the accepted token's tenant,
// audit exactly one decision per request. The security boundary
// (reject-on-no-token, nothing-stored) is IN the skeleton — a happy-path-only
// slice is not shippable.

/// WS / US-AUTH-01 / AC no-token-is-rejected-unauthenticated-nothing-stored.
///
/// Mallory sends a gRPC logs export with NO authorization metadata, claiming
/// tenant "acme-prod" in the payload. aperture must reject UNAUTHENTICATED and
/// store nothing.
///
/// FALSIFIABILITY: against today's no-auth code the export is ACCEPTED (gRPC OK),
/// so `expect_err` panics — the test cannot pass on the bug. It passes only once
/// the bearer gate rejects the tokenless call.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_without_token_is_rejected_unauthenticated() {
    let _files = write_auth_files("grpc-no-token");
    let instance = start_with_auth(&_files).await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .expect("valid grpc endpoint")
        .connect()
        .await
        .expect("connect to aperture grpc listener");
    let mut client = LogsServiceClient::new(channel);

    // No `authorization` metadata at all.
    let req = tonic::Request::new(decode_request(encode_logs_request("acme-prod", 3)));
    let result = client.export(req).await;

    let err = result.expect_err("a tokenless gRPC export must be rejected");
    assert_eq!(
        err.code(),
        Code::Unauthenticated,
        "a missing bearer token must map to gRPC UNAUTHENTICATED"
    );
}

/// WS / US-AUTH-01 / AC no-token ... nothing-stored. The sink half: a tokenless
/// export reaches the sink with NOTHING.
///
/// FALSIFIABILITY: today the record is accepted and the sink is NON-empty; this
/// assertion fails on the bug and passes only when the reject stores nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_without_token_stores_nothing() {
    let _files = write_auth_files("grpc-no-token-sink");
    let instance = start_with_auth(&_files).await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let req = tonic::Request::new(decode_request(encode_logs_request("acme-prod", 3)));
    let _ = client.export(req).await;

    // Give aperture time to (incorrectly) hand off if it were going to.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        instance.sink.is_empty(),
        "a rejected (tokenless) request must store nothing"
    );
}

/// WS / US-AUTH-01 / AC no-token ... one deny audit line. Exactly ONE deny
/// decision event, reason `missing_claim`, subject `ingest_logs`.
///
/// FALSIFIABILITY: today no auth decision is taken, so there is no decision
/// line at all (zero), and `expect_one_decision` panics. It passes only when the
/// pre-validate gate emits exactly one deny line with reason missing_claim.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_without_token_emits_one_deny_audit_line_missing_claim() {
    let _files = write_auth_files("grpc-no-token-audit");
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_with_auth(&_files).await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let req = tonic::Request::new(decode_request(encode_logs_request("acme-prod", 3)));
        let _ = client.export(req).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        instance
    })
    .await;

    expect_one_decision(&events, "deny", "missing_claim", "ingest_logs");
}

/// WS / US-AUTH-01 / AC a-valid-token-ingests-tagged-with-its-tenant. Diego
/// presents a valid token for catalogued "acme-prod"; the export is accepted
/// AND exactly one ALLOW decision line is emitted (subject `ingest_logs`).
///
/// FALSIFIABILITY: the accept half is a guardrail (a valid token must ingest).
/// The falsifiable half is the allow decision line: today no auth decision is
/// taken, so there is zero decision line and `expect_one_decision(..,"allow"..)`
/// panics — it passes only once the validated request emits one allow line.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_valid_token_is_accepted_with_one_allow_line() {
    let _files = write_auth_files("grpc-valid");
    let (response_ok, events) = capture_stderr_events(|| async {
        let instance = start_with_auth(&_files).await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let ok = client
            .export(logs_request_with_bearer(&valid_token(), "payments-api", 3))
            .await
            .is_ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
        ok
    })
    .await;

    assert!(
        response_ok,
        "a valid bearer token for a catalogued tenant must be accepted"
    );
    expect_one_decision(&events, "allow", "allow", "ingest_logs");
}

/// WS / US-AUTH-01 / AC a-valid-token-ingests-tagged-with-its-tenant. The
/// tenant-tagging half, asserted through the OBSERVABLE allow audit line (the
/// `tenant_id` field), not by reaching into the `TenantScoped` payload type.
///
/// FALSIFIABILITY: today there is no allow decision line carrying a tenant_id;
/// once DELIVER tags the accepted record, the allow line names "acme-prod".
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_valid_token_tags_the_authenticated_tenant() {
    let _files = write_auth_files("grpc-valid-tag");
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_with_auth(&_files).await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(logs_request_with_bearer(&valid_token(), "payments-api", 3))
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        instance
    })
    .await;

    let allow = expect_one_decision(&events, "allow", "allow", "ingest_logs");
    assert_eq!(
        field(allow, "tenant_id"),
        Some(TENANT),
        "the accepted record must be tagged with the authenticated tenant"
    );
}

/// WS / US-AUTH-01 / AC a-valid-token-ingests-tagged. The sink half: a valid
/// token DOES reach the sink (the accept actually stores the record).
///
/// FALSIFIABILITY: RED-by-config (the jwt-auth instance is not yet honoured).
/// Once wired, exactly one record reaches the sink under the authenticated
/// tenant.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_valid_token_reaches_the_sink() {
    let _files = write_auth_files("grpc-valid-sink");
    let instance = start_with_auth(&_files).await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let _ = client
        .export(logs_request_with_bearer(&valid_token(), "payments-api", 3))
        .await;

    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    assert_eq!(
        instance.sink.len(),
        1,
        "a valid authenticated export must store exactly one record"
    );
}

// =========================================================================
// US-AUTH-01 — gRPC reject-reason matrix (expired is the WS-named control)
// =========================================================================

/// US-AUTH-01 / AC expired-token-rejected-with-the-matching-reason. An expired
/// token is rejected UNAUTHENTICATED with reason `expired`, nothing stored, one
/// deny line.
///
/// FALSIFIABILITY: today the export is accepted regardless of `exp`; the
/// `expect_err` + one-deny-line assertion both fail on the bug.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_expired_token_is_rejected_reason_expired() {
    assert_grpc_reject_reason(expired_token(), "expired", "grpc-expired").await;
}

// =========================================================================
// US-AUTH-03 — HTTP transport parity (@driving_port, the second front door)
// =========================================================================

/// US-AUTH-03 / AC no-token-is-rejected-unauthenticated-nothing-stored (HTTP). A
/// POST /v1/logs with NO Authorization header returns 401 with a
/// `WWW-Authenticate: Bearer` challenge and stores nothing.
///
/// FALSIFIABILITY: today the POST returns 200 and the sink is non-empty; the
/// status + header + sink-empty assertions all fail on the bug.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn http_logs_without_authorization_header_is_rejected_401() {
    let _files = write_auth_files("http-no-token");
    let instance = start_with_auth(&_files).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/logs", instance.http_base_url()))
        .header("Content-Type", "application/x-protobuf")
        .body(encode_logs_request("payments-api", 1))
        .send()
        .await
        .expect("POST /v1/logs");

    assert_eq!(
        response.status().as_u16(),
        401,
        "a tokenless HTTP logs POST must be rejected 401 Unauthorized"
    );
    let www = response
        .headers()
        .get("WWW-Authenticate")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        www.contains("Bearer"),
        "a 401 must carry a WWW-Authenticate: Bearer challenge (RFC 6750); got: {www:?}"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        instance.sink.is_empty(),
        "a tokenless HTTP POST must store nothing"
    );
}

/// US-AUTH-03 / AC no-token ... one deny audit line (HTTP). Exactly one deny
/// decision line, reason `missing_claim`, subject `ingest_logs`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn http_logs_without_authorization_header_emits_one_deny_line() {
    let _files = write_auth_files("http-no-token-audit");
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_with_auth(&_files).await;
        let client = reqwest::Client::new();
        let _ = client
            .post(format!("{}/v1/logs", instance.http_base_url()))
            .header("Content-Type", "application/x-protobuf")
            .body(encode_logs_request("payments-api", 1))
            .send()
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        instance
    })
    .await;

    expect_one_decision(&events, "deny", "missing_claim", "ingest_logs");
}

/// US-AUTH-03 / AC unknown-tenant-rejected-with-the-matching-reason (HTTP). A
/// correctly-signed token for a tenant NOT in the catalogue returns 401, reason
/// `unknown_tenant`, nothing stored.
///
/// FALSIFIABILITY: today the unknown-tenant payload is accepted and stored
/// (provenance is unauthenticated); the 401 + sink-empty + reason assertions all
/// fail on the bug.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn http_logs_with_unknown_tenant_token_is_rejected_reason_unknown_tenant() {
    let _files = write_auth_files("http-unknown-tenant");
    let (status, events, empty) = capture_stderr_events(|| async {
        let instance = start_with_auth(&_files).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/logs", instance.http_base_url()))
            .header("Content-Type", "application/x-protobuf")
            .header(
                "Authorization",
                format!("Bearer {}", unknown_tenant_token()),
            )
            .body(encode_logs_request("payments-api", 1))
            .send()
            .await
            .expect("POST /v1/logs");
        let status = resp.status().as_u16();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let empty = instance.sink.is_empty();
        (status, empty)
    })
    .await
    .pipe_split();

    assert_eq!(status, 401, "an unknown-tenant token must be rejected 401");
    assert!(empty, "an unknown-tenant token stores nothing");
    expect_one_decision(&events, "deny", "unknown_tenant", "ingest_logs");
}

/// US-AUTH-03 / AC a-valid-token-ingests-tagged-with-its-tenant (HTTP). A valid
/// header accepts with the existing 200 shape AND emits one allow decision line
/// tagged with the authenticated tenant.
///
/// FALSIFIABILITY: the 200 is a guardrail (a valid token must ingest). The
/// falsifiable half is the allow line carrying `tenant_id`: today no decision is
/// taken, so `expect_one_decision(..,"allow"..)` panics; it passes only once the
/// validated HTTP request emits one tenant-tagged allow line.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn http_logs_with_valid_token_is_accepted_200_with_tenant_tagged_allow_line() {
    let _files = write_auth_files("http-valid");
    let (status, events) = capture_stderr_events(|| async {
        let instance = start_with_auth(&_files).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/logs", instance.http_base_url()))
            .header("Content-Type", "application/x-protobuf")
            .header("Authorization", format!("Bearer {}", valid_token()))
            .body(encode_logs_request("payments-api", 1))
            .send()
            .await
            .expect("POST /v1/logs");
        let status = resp.status().as_u16();
        tokio::time::sleep(Duration::from_millis(100)).await;
        status
    })
    .await;

    assert_eq!(
        status, 200,
        "a valid HTTP bearer token must accept with the existing 200 shape"
    );
    let allow = expect_one_decision(&events, "allow", "allow", "ingest_logs");
    assert_eq!(
        field(allow, "tenant_id"),
        Some(TENANT),
        "the HTTP-accepted record must be tagged with the authenticated tenant"
    );
}

// =========================================================================
// US-AUTH-05 — the reject-reason matrix (each variant its own distinct reason)
// =========================================================================
//
// Each of the validation-failure causes surfaces with its matching, mutually-
// distinct aegis `reason()` in exactly one deny audit line, on the gRPC logs
// path. Driven over the real boundary with in-suite-minted tokens. These prove
// KPI-3 (reason-coverage; zero "unknown/other" bucket).

/// US-AUTH-05 / reason `invalid_signature` — a structurally-valid JWT signed
/// with the wrong key.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_bad_signature_token_reason_invalid_signature() {
    assert_grpc_reject_reason(invalid_signature_token(), "invalid_signature", "bad-sig").await;
}

/// US-AUTH-05 / reason `wrong_issuer` — `iss` does not match the configured
/// issuer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_wrong_issuer_token_reason_wrong_issuer() {
    assert_grpc_reject_reason(wrong_issuer_token(), "wrong_issuer", "wrong-iss").await;
}

/// US-AUTH-05 / reason `wrong_audience` — a token minted for the read-path
/// audience must not ingest.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_wrong_audience_token_reason_wrong_audience() {
    assert_grpc_reject_reason(wrong_audience_token(), "wrong_audience", "wrong-aud").await;
}

/// US-AUTH-05 / reason `unknown_role` — an otherwise-valid token whose role is
/// neither viewer nor operator.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_unknown_role_token_reason_unknown_role() {
    assert_grpc_reject_reason(unknown_role_token(), "unknown_role", "unknown-role").await;
}

/// US-AUTH-05 / reason `malformed` — a bearer value that is not a JWT at all,
/// distinct from invalid_signature (bad sig on a real JWT) and missing_claim
/// (no token).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_malformed_token_reason_malformed() {
    assert_grpc_reject_reason("not-a-jwt".to_string(), "malformed", "malformed").await;
}

/// US-AUTH-05 / reason `missing_claim` for an EMPTY bearer (the `Bearer ` with
/// no token case), decided at the extraction boundary — distinct from malformed.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn grpc_logs_with_empty_bearer_reason_missing_claim() {
    assert_grpc_reject_reason(String::new(), "missing_claim", "empty-bearer").await;
}

/// Shared reject-reason assertion: present `token` as the gRPC bearer, assert the
/// call is rejected UNAUTHENTICATED, nothing is stored, and exactly one deny line
/// carries `expected_reason` with subject `ingest_logs`.
///
/// FALSIFIABILITY: against today's no-auth code the export is accepted, the sink
/// is non-empty, and there is no deny line — every assertion fails on the bug.
async fn assert_grpc_reject_reason(token: String, expected_reason: &str, label: &str) {
    let files = write_auth_files(label);
    let (code_result, events, empty) = capture_stderr_events(|| async {
        let instance = start_with_auth(&files).await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let code = client
            .export(logs_request_with_bearer(&token, "payments-api", 1))
            .await
            .map(|_| ())
            .map_err(|e| e.code());
        tokio::time::sleep(Duration::from_millis(100)).await;
        let empty = instance.sink.is_empty();
        (code, empty)
    })
    .await
    .pipe_split();

    assert_eq!(
        code_result,
        Err(Code::Unauthenticated),
        "reason {expected_reason}: the request must be rejected UNAUTHENTICATED"
    );
    assert!(
        empty,
        "reason {expected_reason}: a rejected request must store nothing"
    );
    expect_one_decision(&events, "deny", expected_reason, "ingest_logs");
}

// =========================================================================
// SECRET-NEVER-LOGGED guardrail (System Constraint 4 — a CRITICAL guardrail)
// =========================================================================

/// secret-never-logged. Across a boot + a denied request + an accepted request,
/// the configured HS256 secret BYTES must NEVER appear anywhere in the captured
/// stderr/audit stream. A string-absence assertion on the whole capture.
///
/// FALSIFIABILITY: this is a guardrail — it must hold at every commit. It is
/// RED-by-config today only because it drives the jwt-auth instance (whose seam
/// is scaffolded); the SUBSTANTIVE assertion (no secret bytes in any line) must
/// be true once DELIVER wires auth, and a mutation that logs the secret is
/// caught here. The secret is ASCII so a substring scan of the rendered stream
/// is a faithful check.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]
async fn the_configured_secret_never_appears_in_any_log_line() {
    let _files = write_auth_files("secret-absence");
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_with_auth(&_files).await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        // A denied request (no token) ...
        let _ = client
            .export(tonic::Request::new(decode_request(encode_logs_request(
                "acme-prod",
                1,
            ))))
            .await;
        // ... and an accepted request (valid token).
        let _ = client
            .export(logs_request_with_bearer(&valid_token(), "payments-api", 1))
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        instance
    })
    .await;

    let secret_str = std::str::from_utf8(SECRET).expect("ascii secret");
    for e in &events {
        let rendered = format!("{} {} {}", e.level, e.event, e.fields);
        assert!(
            !rendered.contains(secret_str),
            "the HS256 secret bytes must NEVER appear in any log/audit line; leaked in: {rendered}"
        );
    }
}

// =========================================================================
// Auth-instance fixture + a tiny tuple-split helper
// =========================================================================

/// Start a real aperture instance with the ingest path authenticated against
/// the test issuer/audience/secret-file/catalogue. The `jwt_auth` builder seam
/// is the minimal DELIVER scaffold (DD1); at DISTILL it accepts the config but
/// does not yet wire the validator, which is why every scenario above is
/// `#[ignore]`d RED.
async fn start_with_auth(files: &TempAuthFiles) -> common::TestInstance {
    use aperture::config::Config;
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().expect("loopback parses"))
        .http_bind_addr("127.0.0.1:0".parse().expect("loopback parses"))
        .jwt_auth(
            ISSUER,
            AUDIENCE,
            files.secret_path.clone(),
            files.catalogue_path.clone(),
        )
        .build()
        .expect("auth-configured test config builds");
    common::start_with_recording_sink(config).await
}

/// Split a `((a, b), c)`-shaped capture result into `(a, c, b)` for the
/// reject-matrix call sites that thread a status/code + a sink-empty flag out of
/// the capture closure alongside the events. Keeps the call sites readable.
trait PipeSplit<A, B> {
    fn pipe_split(self) -> (A, Vec<StderrEvent>, B);
}

impl<A, B> PipeSplit<A, B> for ((A, B), Vec<StderrEvent>) {
    fn pipe_split(self) -> (A, Vec<StderrEvent>, B) {
        let ((a, b), events) = self;
        (a, events, b)
    }
}

/// Keep the shared ignore-reason constant referenced so it documents intent for
/// the whole suite even though each `#[ignore = "..."]` writes the literal.
#[test]
fn red_reason_is_documented() {
    assert_eq!(RED, "RED until DELIVER: aegis-ingest-auth-v0");
}
