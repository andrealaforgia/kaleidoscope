// Kaleidoscope trace-query-api — slice 05 read-path per-request bearer auth
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

//! Slice 05 — read-path per-request bearer auth on the TRACES query API,
//! including the trace LOOKUP-BY-ID path (ADR-0053).
//!
//! Feature: `read-path-query-api-auth-v0` (ADR-0074, DD1-DD6). Part of
//! DESIGN slice 2 (log + trace parity, US-RAUTH-03 collapsed). REUSES the
//! `query-http-common` per-request capability verbatim and proves it on
//! `trace-query-api` for BOTH the window route (`GET /api/v1/traces`) AND
//! the lookup-by-id route (`GET /api/v1/traces/by_id?trace_id=<32-hex>`).
//! The lookup-by-id path MUST also be isolated (DD4, ADR-0053); this file
//! pins that negative control specifically.
//!
//! ## Driving port + ephemeral ports
//!
//! The RUNNING `trace-query-api` HTTP transport. Both routes share one
//! `ApiState { store, tenant, auth }`, so the validator wired once covers
//! both. Every scenario binds the REAL
//! `trace_query_api::router_with_auth(...)` on an EPHEMERAL port
//! (`127.0.0.1:0`, read back; the fixed 9092 default is never bound) and
//! drives a REAL `reqwest` GET over loopback. @real-io @driving_adapter.
//!
//! ## Token-minting + RED-not-BROKEN
//!
//! In-suite `jsonwebtoken::encode` mints, audience `kaleidoscope-query`,
//! mirroring aperture's `slice_10_ingest_auth.rs`. Reject scenarios set the
//! env tenant to `acme-prod` so a fall-through impl would serve its data,
//! making each reject assertion FAIL against the scaffold (falsifiability).
//! All auth scenarios `#[ignore]`d RED until DELIVER; backward-compat green.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::{load_catalogue, TenantId, Validator, ValidatorConfig};
use jsonwebtoken::{encode, EncodingKey, Header};
use ray::{FileBackedTraceStore, TraceStore};
use serde::Serialize;
use serde_json::Value;

use common::{open_durable_store, rich_span, seed};

const ISSUER: &str = "acme-observability";
const AUDIENCE: &str = "kaleidoscope-query";
const INGEST_AUDIENCE: &str = "kaleidoscope-ingest";
const SECRET: &[u8] = b"slice-05-read-auth-test-secret-not-for-production";
const WRONG_SECRET: &[u8] = b"a-different-secret-that-must-not-validate-the-token";
const TENANT_A: &str = "acme-prod";
const TENANT_B: &str = "globex-staging";
const ROLE_VIEWER: &str = "viewer";
/// `rich_span` uses trace_byte 0xAA, so the seeded trace_id is `aa..aa`
/// (32 hex chars). See `common/mod.rs::rich_span`.
const SEEDED_TRACE_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SERVICE: &str = "checkout";

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

fn read_auth_validator() -> Arc<Validator> {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let cat_path = std::env::temp_dir().join(format!("trace-query-api-read-auth-cat-{stamp}.toml"));
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
    _store: Arc<FileBackedTraceStore>,
    _dir: std::path::PathBuf,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Seed one rich span (trace `aa..aa`, service `checkout`) for `acme-prod`
/// and bind the auth-configured router on an EPHEMERAL port.
async fn start_with_auth(env_tenant: Option<TenantId>, label: &str) -> Instance {
    let (store, dir) = open_durable_store(label);
    seed(
        &store,
        &TenantId(TENANT_A.to_string()),
        vec![rich_span(1_717_000)],
    );
    let router = trace_query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn TraceStore + Send + Sync>,
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

/// GET the WINDOW route `/api/v1/traces?service=checkout&...`.
async fn get_traces(addr: SocketAddr, authorization: Option<&str>) -> (u16, String, String) {
    let start = (1_717_000 - 100).to_string();
    let end = (1_717_000 + 100).to_string();
    let url = format!("http://{addr}/api/v1/traces?service={SERVICE}&start={start}&end={end}");
    drive(addr, &url, authorization).await
}

/// GET the LOOKUP-BY-ID route `/api/v1/traces/by_id?trace_id=<32-hex>`.
async fn get_trace_by_id(
    addr: SocketAddr,
    trace_id: &str,
    authorization: Option<&str>,
) -> (u16, String, String) {
    let url = format!("http://{addr}/api/v1/traces/by_id?trace_id={trace_id}");
    drive(addr, &url, authorization).await
}

async fn drive(_addr: SocketAddr, url: &str, authorization: Option<&str>) -> (u16, String, String) {
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(value) = authorization {
        req = req.header("Authorization", value);
    }
    let resp = req.send().await.expect("send GET");
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

/// The number of spans in a traces success body (a bare JSON array).
fn span_count(body: &str) -> usize {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0)
}

// =========================================================================
// US-RAUTH-03 — valid token reads its own tenant's traces (window route)
// (@driving_port @real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

/// US-RAUTH-03 / AC a-valid-token-reads-its-own-tenant (traces window).
#[tokio::test(flavor = "multi_thread")]
async fn valid_token_reads_its_own_tenant_traces() {
    let instance = start_with_auth(None, "valid-own").await;
    let token = valid_token_for(TENANT_A);
    let (status, _www, body) = get_traces(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(
        status, 200,
        "valid acme-prod token reads traces 200; body: {body}"
    );
    assert_eq!(
        span_count(&body),
        1,
        "acme-prod sees its own span; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — TENANT ISOLATION on the WINDOW route (positive + negative)
// (@real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn isolation_positive_control_tenant_a_sees_its_traces() {
    let instance = start_with_auth(None, "iso-pos").await;
    let token = valid_token_for(TENANT_A);
    let (status, _www, body) = get_traces(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(
        span_count(&body),
        1,
        "positive control: acme-prod sees its span"
    );
}

/// US-RAUTH-03 / AC tenant-isolation NEGATIVE control (traces window): a valid
/// `globex-staging` token sees `acme-prod`'s span ABSENT.
#[tokio::test(flavor = "multi_thread")]
async fn isolation_negative_control_tenant_b_cannot_read_tenant_a_traces() {
    let instance = start_with_auth(None, "iso-neg").await;
    let token = valid_token_for(TENANT_B);
    let (status, _www, body) = get_traces(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(
        span_count(&body),
        0,
        "negative control: globex-staging must NOT see acme-prod's span; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — TENANT ISOLATION on the LOOKUP-BY-ID route (ADR-0053)
// The load-bearing extra path: the lookup-by-id path MUST also be isolated.
// (@real-io @driving_adapter @US-RAUTH-03 @lookup-by-id)
// =========================================================================

/// US-RAUTH-03 / AC tenant-isolation ... INCLUDING the trace lookup-by-id
/// path (positive control). `acme-prod`'s token looks up trace `aa..aa` and
/// gets its span(s).
#[tokio::test(flavor = "multi_thread")]
async fn lookup_by_id_positive_control_tenant_a_finds_its_trace() {
    let instance = start_with_auth(None, "byid-pos").await;
    let token = valid_token_for(TENANT_A);
    let (status, _www, body) = get_trace_by_id(
        instance.addr,
        SEEDED_TRACE_ID,
        Some(&format!("Bearer {token}")),
    )
    .await;
    assert_eq!(
        status, 200,
        "acme-prod looks up its trace 200; body: {body}"
    );
    assert_eq!(
        span_count(&body),
        1,
        "acme-prod finds its trace's span; body: {body}"
    );
}

/// US-RAUTH-03 / AC tenant-isolation NEGATIVE control on lookup-by-id (the
/// load-bearing half, ADR-0053): a valid `globex-staging` token looking up
/// `acme-prod`'s trace id `aa..aa` gets an EMPTY result — `acme-prod`'s trace
/// is ABSENT. Proves isolation holds on the lookup-by-id path too.
///
/// FALSIFIABILITY: a non-isolating impl returns acme-prod's span for any
/// token; this assertion FAILS. Green only once `get_trace` is scoped to the
/// token's tenant (globex-staging), under which the trace does not exist.
#[tokio::test(flavor = "multi_thread")]
async fn lookup_by_id_negative_control_tenant_b_cannot_read_tenant_a_trace() {
    let instance = start_with_auth(None, "byid-neg").await;
    let token = valid_token_for(TENANT_B);
    let (status, _www, body) = get_trace_by_id(
        instance.addr,
        SEEDED_TRACE_ID,
        Some(&format!("Bearer {token}")),
    )
    .await;
    assert_eq!(status, 200, "globex-staging lookup 200; body: {body}");
    assert_eq!(
        span_count(&body),
        0,
        "negative control: globex-staging must NOT find acme-prod's trace by id; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — FAIL-CLOSED: no token 401 on BOTH routes, store not read
// (@real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

/// US-RAUTH-03 / AC no-token-is-refused-401-before-the-store (traces window).
#[tokio::test(flavor = "multi_thread")]
async fn no_token_is_refused_401_with_challenge_traces_window() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "no-token-win").await;
    let (status, www, body) = get_traces(instance.addr, None).await;
    assert_eq!(
        status, 401,
        "tokenless auth-on traces request must be 401; body: {body}"
    );
    assert!(
        www.contains("Bearer"),
        "401 must carry WWW-Authenticate: Bearer; got {www:?}"
    );
    assert_eq!(
        span_count(&body),
        0,
        "no spans read on the refusal; body: {body}"
    );
}

/// US-RAUTH-03 / AC no-token-is-refused-401-before-the-store (lookup-by-id).
/// The fail-closed control on the SECOND route.
#[tokio::test(flavor = "multi_thread")]
async fn no_token_is_refused_401_with_challenge_lookup_by_id() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "no-token-byid").await;
    let (status, www, body) = get_trace_by_id(instance.addr, SEEDED_TRACE_ID, None).await;
    assert_eq!(
        status, 401,
        "tokenless auth-on lookup-by-id must be 401; body: {body}"
    );
    assert!(
        www.contains("Bearer"),
        "401 must carry WWW-Authenticate: Bearer; got {www:?}"
    );
    assert_eq!(
        span_count(&body),
        0,
        "no spans read on the refusal; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-03 — reject reason: invalid signature (traces)
// (@real-io @driving_adapter @US-RAUTH-03)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn bad_signature_token_is_refused_401_traces() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "bad-sig").await;
    let token = bad_signature_token();
    let (status, _www, body) = get_traces(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(
        status, 401,
        "a bad-signature token must be refused 401; body: {body}"
    );
    assert_eq!(span_count(&body), 0, "nothing read; body: {body}");
}

// =========================================================================
// US-RAUTH-04 — AUDIENCE FENCE on traces (cross-surface)
// (@real-io @driving_adapter @US-RAUTH-04)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn ingest_audience_token_is_rejected_on_traces() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "ingest-aud").await;
    let token = ingest_audience_token();
    let (status, _www, body) = get_traces(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(
        status, 401,
        "ingest-audience token must reject on traces read; body: {body}"
    );
    assert_eq!(span_count(&body), 0, "nothing read; body: {body}");
}

// =========================================================================
// US-RAUTH-03 — REDACTION on traces (the token never appears in the 401 body)
// (@real-io @driving_adapter — hard guardrail)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn the_token_and_secret_never_appear_in_the_traces_401_body() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "redaction").await;
    let token = bad_signature_token();
    let (status, _www, body) = get_traces(instance.addr, Some(&format!("Bearer {token}"))).await;
    assert_eq!(status, 401, "body: {body}");
    assert!(
        !body.contains(&token),
        "raw token must NOT appear; body: {body}"
    );
    let secret_str = std::str::from_utf8(SECRET).expect("ascii secret");
    assert!(
        !body.contains(secret_str),
        "secret bytes must NOT appear; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-02 — BACKWARD COMPAT on traces (GUARDRAILS — GREEN, not ignored)
// (@real-io @driving_adapter @backward-compat @US-RAUTH-02)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn auth_off_resolves_env_tenant_and_ignores_header_traces() {
    let (store, _dir) = open_durable_store("compat-env");
    seed(
        &store,
        &TenantId(TENANT_A.to_string()),
        vec![rich_span(1_717_000)],
    );
    let router = trace_query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn TraceStore + Send + Sync>,
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
    let (status, _www, body) = get_traces(addr, Some("Bearer stray-token-ignored")).await;
    handle.abort();
    assert_eq!(
        status, 200,
        "auth-off env-tenant traces read 200; body: {body}"
    );
    assert_eq!(
        span_count(&body),
        1,
        "env tenant scopes the query, header ignored; body: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_off_unset_env_tenant_still_refuses_401_traces() {
    let (store, _dir) = open_durable_store("compat-unset");
    let router = trace_query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn TraceStore + Send + Sync>,
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
    let (status, _www, body) = get_traces(addr, None).await;
    handle.abort();
    assert_eq!(status, 401, "auth-off + unset env tenant still refuses 401");
    assert!(
        body.contains("no tenant resolvable"),
        "the existing fail-closed reason is unchanged; body: {body}"
    );
}
