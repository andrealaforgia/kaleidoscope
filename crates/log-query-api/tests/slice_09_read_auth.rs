// Kaleidoscope log-query-api — slice 09 read-path per-request bearer auth
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

//! Slice 09 — read-path per-request bearer auth on the LOGS query API.
//!
//! Feature: `read-path-query-api-auth-v0` (ADR-0074, DD1-DD6). Part of
//! DESIGN slice 2 (log + trace parity, US-RAUTH-03 collapsed). REUSES the
//! `query-http-common` per-request capability verbatim — no per-crate auth
//! logic — and proves it on `log-query-api` (`GET /api/v1/logs`, logs /
//! Lumen). Mirrors `query-api/tests/slice_07_read_auth.rs` on the logs
//! surface (bare JSON array of LogRecords; `subject=log_query`;
//! `invalid_signature` is the WS-named reject control here, per US-RAUTH-03).
//!
//! ## Driving port + ephemeral ports
//!
//! The RUNNING `log-query-api` HTTP transport: `GET /api/v1/logs` with an
//! `Authorization: Bearer <jwt>` header. Every scenario binds the REAL
//! `log_query_api::router_with_auth(...)` on an EPHEMERAL port
//! (`127.0.0.1:0`, read back) and drives a REAL `reqwest` GET over loopback
//! (the fixed 9091 default is never bound — no flake). @real-io
//! @driving_adapter.
//!
//! ## Token-minting + RED-not-BROKEN
//!
//! Tokens minted in-suite with `jsonwebtoken::encode`, audience
//! `kaleidoscope-query`, mirroring aperture's `slice_10_ingest_auth.rs`.
//! Every reject scenario sets the env tenant to `acme-prod` so a fall-
//! through impl would serve its data — making each reject assertion FAIL
//! against the scaffold (the no-auth env path), which is the falsifiability
//! requirement (brief.md: "every reject AC must FAIL against an env-tenant
//! fall-through"). All auth scenarios are `#[ignore]`d RED until DELIVER;
//! the backward-compat guardrails are GREEN.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::{load_catalogue, TenantId, Validator, ValidatorConfig};
use jsonwebtoken::{encode, EncodingKey, Header};
use lumen::{FileBackedLogStore, LogStore};
use serde::Serialize;
use serde_json::Value;

use common::{open_durable_store, record, seed};

const ISSUER: &str = "acme-observability";
const AUDIENCE: &str = "kaleidoscope-query";
const INGEST_AUDIENCE: &str = "kaleidoscope-ingest";
const SECRET: &[u8] = b"slice-09-read-auth-test-secret-not-for-production";
const WRONG_SECRET: &[u8] = b"a-different-secret-that-must-not-validate-the-token";
const TENANT_A: &str = "acme-prod";
const TENANT_B: &str = "globex-staging";
const ROLE_VIEWER: &str = "viewer";

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
    encode(&header, claims, &EncodingKey::from_secret(secret)).expect("encode HS256 jwt")
}

fn valid_token_for(tenant: &str) -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: tenant,
            kaleidoscope_role: ROLE_VIEWER,
        },
        SECRET,
    )
}

fn ingest_audience_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: INGEST_AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: TENANT_A,
            kaleidoscope_role: ROLE_VIEWER,
        },
        SECRET,
    )
}

fn bad_signature_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: TENANT_A,
            kaleidoscope_role: ROLE_VIEWER,
        },
        WRONG_SECRET,
    )
}

/// Build the read-auth validator (audience `kaleidoscope-query`) over a
/// catalogue holding both tenants, loaded via the production `load_catalogue`
/// (real TOML I/O).
fn read_auth_validator() -> Arc<Validator> {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let cat_path = std::env::temp_dir().join(format!("log-query-api-read-auth-cat-{stamp}.toml"));
    std::fs::write(
        &cat_path,
        format!("[[tenants]]\nid = \"{TENANT_A}\"\n\n[[tenants]]\nid = \"{TENANT_B}\"\n"),
    )
    .expect("write catalogue");
    let catalogue = load_catalogue(&cat_path).expect("load catalogue");
    let _ = std::fs::remove_file(&cat_path);
    Arc::new(Validator::new(ValidatorConfig {
        issuer: ISSUER.to_string(),
        audience: AUDIENCE.to_string(),
        hs256_key: SECRET.to_vec(),
        catalogue,
    }))
}

struct Instance {
    addr: SocketAddr,
    _store: Arc<FileBackedLogStore>,
    _dir: std::path::PathBuf,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Seed one `acme-prod` log record and bind the auth-configured router on an
/// EPHEMERAL port. `env_tenant` lets the reject scenarios prove the no-
/// bearer-bypass / falsifiability (set to `acme-prod`).
async fn start_with_auth(env_tenant: Option<TenantId>, label: &str) -> Instance {
    let (store, dir) = open_durable_store(label);
    seed(
        &store,
        &TenantId(TENANT_A.to_string()),
        vec![record(1_717_000_100, "payments-api", "checkout ok")],
    );
    let router = log_query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn LogStore + Send + Sync>,
        env_tenant,
        Some(read_auth_validator()),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback port");
    let addr = listener.local_addr().expect("read back bound addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Instance {
        addr,
        _store: store,
        _dir: dir,
        handle,
    }
}

async fn get_logs(addr: SocketAddr, authorization: Option<&str>) -> (u16, String, String) {
    let url = format!("http://{addr}/api/v1/logs?start=1717000000&end=1717003600");
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(value) = authorization {
        req = req.header("Authorization", value);
    }
    let resp = req.send().await.expect("send GET /api/v1/logs");
    let status = resp.status().as_u16();
    let www = resp
        .headers()
        .get("WWW-Authenticate")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = resp.text().await.expect("read body");
    (status, www, body)
}

/// The number of log records in a logs success body (a bare JSON array).
fn record_count(body: &str) -> usize {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0)
}

// =========================================================================
// US-RAUTH-03 — valid token reads its own tenant's logs
// (@driving_port @real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

/// US-RAUTH-03 / AC a-valid-token-reads-its-own-tenant (logs). Nadia's
/// `acme-prod` token returns `acme-prod`'s log records with the existing bare-
/// array shape, scoped to her tenant.
///
/// FALSIFIABILITY: env tenant unset; against the scaffold the auth-configured
/// router refuses (no tenant resolvable), so the 200 + record-present
/// assertion FAILS. Green only once the query is scoped to the token tenant.
#[tokio::test(flavor = "multi_thread")]
async fn valid_token_reads_its_own_tenant_logs() {
    let instance = start_with_auth(None, "valid-own").await;
    let token = valid_token_for(TENANT_A);
    let (status, _www, body) = get_logs(instance.addr, Some(&format!("Bearer {token}"))).await;

    assert_eq!(
        status, 200,
        "a valid acme-prod token reads logs 200; body: {body}"
    );
    assert_eq!(
        record_count(&body),
        1,
        "acme-prod sees its own log record; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — TENANT ISOLATION (positive + negative control) on logs
// (@real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

/// US-RAUTH-03 / AC tenant-isolation positive control (logs).
#[tokio::test(flavor = "multi_thread")]
async fn isolation_positive_control_tenant_a_sees_its_logs() {
    let instance = start_with_auth(None, "iso-pos").await;
    let token = valid_token_for(TENANT_A);
    let (status, _www, body) = get_logs(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(
        record_count(&body),
        1,
        "positive control: acme-prod sees its log record"
    );
}

/// US-RAUTH-03 / AC tenant-isolation NEGATIVE control (logs, load-bearing): a
/// valid `globex-staging` token returns its (empty) logs — `acme-prod`'s
/// record is ABSENT.
///
/// FALSIFIABILITY: a non-isolating impl returns acme-prod's record regardless
/// of token; this assertion FAILS. Green only once scoped to globex-staging.
#[tokio::test(flavor = "multi_thread")]
async fn isolation_negative_control_tenant_b_cannot_read_tenant_a_logs() {
    let instance = start_with_auth(None, "iso-neg").await;
    let token = valid_token_for(TENANT_B);
    let (status, _www, body) = get_logs(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(
        record_count(&body),
        0,
        "negative control: globex-staging must NOT see acme-prod's logs; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — FAIL-CLOSED: no token 401 + WWW-Authenticate, store not read
// (@real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

/// US-RAUTH-03 / AC no-token-is-refused-401-before-the-store (logs). With the
/// env tenant set to acme-prod, a tokenless request must be the bearer-gate
/// 401 (with `WWW-Authenticate: Bearer`), NOT served the env tenant's logs.
///
/// FALSIFIABILITY: against the scaffold the env path serves acme-prod's
/// record 200 — the 401 assertion FAILS. Green only once the bearer gate
/// refuses.
#[tokio::test(flavor = "multi_thread")]
async fn no_token_is_refused_401_with_challenge_logs() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "no-token").await;
    let (status, www, body) = get_logs(instance.addr, None).await;
    assert_eq!(
        status, 401,
        "tokenless auth-on logs request must be 401; body: {body}"
    );
    assert!(
        www.contains("Bearer"),
        "401 must carry WWW-Authenticate: Bearer (RFC 6750); got {www:?}"
    );
    assert_eq!(
        record_count(&body),
        0,
        "no logs read on the refusal; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — reject reason: invalid signature (the WS-named control)
// (@real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

/// US-RAUTH-03 / AC invalid-token-refused-with-the-matching-reason (logs). A
/// bad-signature token returns 401; nothing read.
///
/// FALSIFIABILITY: env tenant set to acme-prod; against the scaffold the bad-
/// signature token is served acme-prod's logs 200 — the 401 assertion FAILS.
#[tokio::test(flavor = "multi_thread")]
async fn bad_signature_token_is_refused_401_logs() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "bad-sig").await;
    let token = bad_signature_token();
    let (status, _www, body) = get_logs(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(
        status, 401,
        "a bad-signature token must be refused 401; body: {body}"
    );
    assert_eq!(
        record_count(&body),
        0,
        "nothing read on the refusal; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — AUDIENCE FENCE on logs (cross-surface)
// (@real-io @driving_adapter @US-RAUTH-04)
// =========================================================================

/// US-RAUTH-04 / AC ingest-audience-token-rejected-wrong-audience (logs). An
/// ingest-audience token cannot read logs.
///
/// FALSIFIABILITY: env tenant set to acme-prod; against the scaffold the
/// ingest token is served acme-prod's logs 200 — the 401 assertion FAILS.
#[tokio::test(flavor = "multi_thread")]
async fn ingest_audience_token_is_rejected_on_logs() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "ingest-aud").await;
    let token = ingest_audience_token();
    let (status, _www, body) = get_logs(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(
        status, 401,
        "ingest-audience token must reject on logs read; body: {body}"
    );
    assert_eq!(record_count(&body), 0, "nothing read; body: {body}");
}

// =========================================================================
// US-RAUTH-03 — REDACTION on logs (the token never appears in the 401 body)
// (@real-io @driving_adapter — hard guardrail)
// =========================================================================

/// US-RAUTH-03 / AC the-secret-and-token-are-never-logged (logs wire body). A
/// bad-signature reject's 401 body never carries the raw token or the secret.
///
/// FALSIFIABILITY: env tenant set; the 401 gate FAILS against the scaffold
/// fall-through. Once rejected, the substring-absence assertions catch any
/// token/secret echo.
#[tokio::test(flavor = "multi_thread")]
async fn the_token_and_secret_never_appear_in_the_logs_401_body() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "redaction").await;
    let token = bad_signature_token();
    let (status, _www, body) = get_logs(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(status, 401, "body: {body}");
    assert!(
        !body.contains(&token),
        "raw token must NOT appear in the 401 body; body: {body}"
    );
    let secret_str = std::str::from_utf8(SECRET).expect("ascii secret");
    assert!(
        !body.contains(secret_str),
        "secret bytes must NOT appear; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-02 — BACKWARD COMPAT on logs (GUARDRAILS — GREEN, not ignored)
// (@real-io @driving_adapter @backward-compat @US-RAUTH-02)
// =========================================================================

/// US-RAUTH-02 / AC env-tenant-unchanged-when-auth-absent (logs). Omar's
/// legacy env-tenant log deployment is byte-for-byte today: auth off, env
/// tenant acme-prod, a stray Authorization header IGNORED, the env tenant
/// scopes the query.
#[tokio::test(flavor = "multi_thread")]
async fn auth_off_resolves_env_tenant_and_ignores_header_logs() {
    let (store, _dir) = open_durable_store("compat-env");
    seed(
        &store,
        &TenantId(TENANT_A.to_string()),
        vec![record(1_717_000_100, "payments-api", "checkout ok")],
    );
    let router = log_query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn LogStore + Send + Sync>,
        Some(TenantId(TENANT_A.to_string())),
        None,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let (status, _www, body) = get_logs(addr, Some("Bearer stray-token-must-be-ignored")).await;
    handle.abort();
    assert_eq!(
        status, 200,
        "auth-off env-tenant logs read 200; body: {body}"
    );
    assert_eq!(
        record_count(&body),
        1,
        "env tenant scopes the query, header ignored; body: {body}"
    );
}

/// US-RAUTH-02 / AC unset-env-tenant-still-refuses-401 (logs).
#[tokio::test(flavor = "multi_thread")]
async fn auth_off_unset_env_tenant_still_refuses_401_logs() {
    let (store, _dir) = open_durable_store("compat-unset");
    let router = log_query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn LogStore + Send + Sync>,
        None,
        None,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let (status, _www, body) = get_logs(addr, None).await;
    handle.abort();
    assert_eq!(status, 401, "auth-off + unset env tenant still refuses 401");
    assert!(
        body.contains("no tenant resolvable"),
        "the existing fail-closed reason is unchanged; body: {body}"
    );
}
