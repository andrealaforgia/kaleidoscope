// Kaleidoscope query-api — slice 07 read-path per-request bearer auth
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

//! Slice 07 — read-path per-request bearer auth on the metrics query API.
//!
//! Feature: `read-path-query-api-auth-v0` (ADR-0074, DD1-DD6). This slice
//! is the WALKING SKELETON (DESIGN slice 1): it lands the riskiest
//! assumption — `aegis::Validator` wired onto the shared read seam,
//! fail-closed, isolated, with no env-path regression — on `query-api`
//! (`GET /api/v1/query_range`, metrics / Pulse). It also pins, on the
//! metrics surface, the two load-bearing security controls DESIGN said the
//! WS slice may carry: the NO-BEARER-BYPASS (US-RAUTH-02 arm 2) and the
//! cross-surface AUDIENCE FENCE (US-RAUTH-04). The log + trace parity and
//! the full 8-reason matrix live in their own crates' slices.
//!
//! ## Driving port (black-box, brief.md "For Acceptance Designer")
//!
//! The RUNNING `query-api` HTTP transport: `GET /api/v1/query_range` with
//! an `Authorization: Bearer <jwt>` header. Every scenario in this file
//! binds the REAL `query_api::router_with_auth(...)` on an EPHEMERAL port
//! (`127.0.0.1:0`, read back from the listener — the fixed 9090 default is
//! never bound, so the suite cannot flake on a port clash) and drives a
//! REAL `reqwest` HTTP GET over loopback. This is the @real-io
//! @driving_adapter leg: it exercises the real axum transport and the real
//! `Authorization`-header extraction the auth feature adds, which an
//! in-process `oneshot` over a hand-built `Request` would still cover but
//! the wire path makes unambiguous. No internal auth type is reached:
//! `resolve_request_tenant_or_refuse` and the `Validator` wiring are
//! crate-internal; the tests assert observable HTTP + audit outcomes only.
//!
//! ## The token-minting seam (MANDATORY for falsifiability)
//!
//! Tokens are minted IN-SUITE with `jsonwebtoken::encode` (the same engine
//! aegis validates with), signed with the SAME `SECRET` bytes the test
//! config's `secret_file` points at, audience `kaleidoscope-query`
//! (DD6 — the read audience, NOT the ingest audience), for a tenant in the
//! test catalogue, future `exp`. Negative-control mints perturb one axis
//! each. This mirrors aperture's `slice_10_ingest_auth.rs` token seam and
//! aligns with the Verifier's A19/A20 harness so the same tokens drive
//! both doors.
//!
//! ## DELIVERED (read-path-query-api-auth-v0, slice 1)
//!
//! The auth wiring now exists: `router_with_auth` threads the validator
//! into `ApiState`, and the handler resolves the per-request tenant through
//! `query_http_common::resolve_request_tenant_or_refuse` (the 3-arm
//! precedence). Every scenario below is GREEN and un-ignored. The
//! falsifiability the DISTILL scaffold proved (each reject FAILS against an
//! env-tenant fall-through; the no-bearer-bypass returns 401 not the env
//! tenant's data) is preserved by the assertions themselves: the reject
//! scenarios deliberately set the env tenant to `acme-prod` and seed its
//! data, so an `else env_tenant` fall-through would return 200 and fail the
//! `status == 401` assertion. The mutation suite (gate 5) kills the
//! precedence / audience / exp / tenant-resolution mutants through these
//! scenarios.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::{load_catalogue, TenantId, Validator, ValidatorConfig};
use jsonwebtoken::{encode, EncodingKey, Header};
use pulse::{FileBackedMetricStore, MetricStore};
use serde::Serialize;
use serde_json::Value;

use common::{gauge, open_durable_store, point, secs_to_nanos};

const RED: &str = "RED until DELIVER: read-path-query-api-auth-v0";

// =========================================================================
// Test fixtures — issuer / audience / secret / catalogued tenants
// =========================================================================

const ISSUER: &str = "acme-observability";
/// The READ audience (DD6). The cross-surface fence: an ingest token
/// (`kaleidoscope-ingest`) must reject `wrong_audience` here.
const AUDIENCE: &str = "kaleidoscope-query";
const INGEST_AUDIENCE: &str = "kaleidoscope-ingest";
const SECRET: &[u8] = b"slice-07-read-auth-test-secret-not-for-production";
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
    let key = EncodingKey::from_secret(secret);
    encode(&header, claims, &key).expect("encode HS256 jwt")
}

/// A valid read token for a catalogued tenant: correct
/// iss/aud(`kaleidoscope-query`)/signature/role and a future `exp`.
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

fn expired_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() - 300,
            tenant_id: TENANT_A,
            kaleidoscope_role: ROLE_VIEWER,
        },
        SECRET,
    )
}

/// A correctly-signed token minted for the INGEST audience — the
/// cross-surface fence target (must reject `wrong_audience` on read).
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

// =========================================================================
// The auth-configured validator + the ephemeral-bind running instance
// =========================================================================

/// Build the read-auth `Validator` from the test issuer/audience/secret +
/// an in-memory catalogue holding both tenants. The validator is the same
/// `aegis::Validator` production builds at composition (DD1); here it is
/// constructed directly from a `ValidatorConfig` so the suite needs no
/// secret/catalogue temp files for the validator itself (the redaction
/// guardrail still proves the SECRET bytes never reach any wire surface).
fn read_auth_validator() -> Arc<Validator> {
    // A catalogue file written to a temp path, loaded via the production
    // `load_catalogue` loader (real TOML I/O), so the catalogue surface is
    // exercised the way the binary loads it (DD1).
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let cat_path = std::env::temp_dir().join(format!("query-api-read-auth-cat-{stamp}.toml"));
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

/// A running query-api instance bound on an EPHEMERAL loopback port. Holds
/// the real durable store (so isolation reads hit real filesystem I/O) and
/// the actual bound address read back from the listener.
struct Instance {
    addr: SocketAddr,
    /// Kept alive so the spawned server keeps a readable store for the
    /// duration of the request (the router holds its own clone; this is the
    /// suite-side handle that pins the durable backing).
    _store: Arc<FileBackedMetricStore>,
    _dir: std::path::PathBuf,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Seed `up=1` for tenant A and bind the auth-configured router on an
/// EPHEMERAL port. `env_tenant` models a possibly-leftover env tenant (the
/// no-bearer-bypass control sets it to `Some(TENANT_A)` to prove a missing
/// bearer does NOT downgrade to it).
async fn start_with_auth(env_tenant: Option<TenantId>, label: &str) -> Instance {
    let (store, dir) = open_durable_store(label);
    // Seed tenant A with an `up` series at value 1, tenant B with nothing.
    store
        .ingest(
            &TenantId(TENANT_A.to_string()),
            pulse::MetricBatch::with_metrics(vec![gauge(
                "up",
                "payments-api",
                vec![point(secs_to_nanos(1_717_000_100), 1.0, &[])],
            )]),
        )
        .expect("seed tenant A");

    let router = query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn MetricStore + Send + Sync>,
        env_tenant,
        Some(read_auth_validator()),
        None,
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

/// GET `/api/v1/query_range?query=up&...` over the real wire, with an
/// optional `Authorization` header. Returns (status, body-text).
async fn get_query_range(addr: SocketAddr, authorization: Option<&str>) -> (u16, String) {
    let url = format!(
        "http://{addr}/api/v1/query_range?query=up&start=1717000000&end=1717003600&step=60"
    );
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(value) = authorization {
        req = req.header("Authorization", value);
    }
    let resp = req.send().await.expect("send GET /api/v1/query_range");
    let status = resp.status().as_u16();
    let www = resp
        .headers()
        .get("WWW-Authenticate")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = resp.text().await.expect("read body text");
    // Stash the challenge inside the returned body string by convention is
    // ugly; instead callers that need the header use `get_with_challenge`.
    let _ = www;
    (status, body)
}

/// As `get_query_range` but also returns the `WWW-Authenticate` challenge.
async fn get_with_challenge(
    addr: SocketAddr,
    authorization: Option<&str>,
) -> (u16, String, String) {
    let url = format!(
        "http://{addr}/api/v1/query_range?query=up&start=1717000000&end=1717003600&step=60"
    );
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

/// The number of `up` result series in a Prometheus success body. `acme-prod`
/// has one (value 1); a tenant with no `up` series has zero.
fn up_series_count(body: &str) -> usize {
    let v: Value = serde_json::from_str(body).expect("body is JSON");
    v.get("data")
        .and_then(|d| d.get("result"))
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0)
}

// =========================================================================
// US-RAUTH-01 — WALKING SKELETON: a valid token reads its OWN tenant
// (@walking_skeleton @driving_port @real-io @driving_adapter @US-RAUTH-01)
// =========================================================================

/// WS / US-RAUTH-01 / AC a-valid-token-reads-its-own-tenant. Nadia presents
/// a valid `acme-prod` bearer token; query-api returns `acme-prod`'s `up`
/// series with the existing Prometheus response shape, scoped to her tenant.
///
/// FALSIFIABILITY: against the scaffold the auth-configured router resolves
/// the ENV tenant (here unset -> the existing 401) rather than the token's
/// tenant, so the "200 + acme-prod's up series present" assertion FAILS. It
/// passes only once DELIVER scopes the query to the validated token tenant.
#[tokio::test(flavor = "multi_thread")]
async fn ws_valid_token_reads_its_own_tenant_metrics() {
    let instance = start_with_auth(None, "ws-valid-own").await;
    let token = valid_token_for(TENANT_A);
    let (status, body) = get_query_range(instance.addr, Some(&format!("Bearer {token}"))).await;

    assert_eq!(
        status, 200,
        "a valid acme-prod token must read 200; body: {body}"
    );
    assert_eq!(
        up_series_count(&body),
        1,
        "acme-prod's up series must be present for its own token; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-01 — TENANT ISOLATION (positive + negative control; the north star)
// (@US-RAUTH-01 @real-io @driving_adapter)
// =========================================================================

/// US-RAUTH-01 / AC tenant-isolation-positive-and-negative-control. The
/// POSITIVE half: `acme-prod`'s token sees `acme-prod`'s `up` series.
#[tokio::test(flavor = "multi_thread")]
async fn isolation_positive_control_tenant_a_sees_its_up_series() {
    let instance = start_with_auth(None, "iso-pos").await;
    let token = valid_token_for(TENANT_A);
    let (status, body) = get_query_range(instance.addr, Some(&format!("Bearer {token}"))).await;

    assert_eq!(status, 200, "acme-prod token reads 200; body: {body}");
    assert_eq!(
        up_series_count(&body),
        1,
        "positive control: acme-prod sees its own up series"
    );
}

/// US-RAUTH-01 / AC tenant-isolation ... the NEGATIVE control (the load-
/// bearing half): a valid `globex-staging` token on the SAME query returns
/// `globex-staging`'s (empty) result — `acme-prod`'s `up` series is ABSENT.
/// Together with the positive control this proves no cross-tenant read.
///
/// FALSIFIABILITY: against the scaffold the env tenant (here unset -> 401)
/// or a non-isolating impl would either refuse or return acme-prod's data
/// regardless of the token, so this assertion FAILS. It passes only once
/// the query is scoped to the TOKEN's tenant (globex-staging) and acme-prod's
/// data is therefore absent.
#[tokio::test(flavor = "multi_thread")]
async fn isolation_negative_control_tenant_b_cannot_read_tenant_a_metrics() {
    let instance = start_with_auth(None, "iso-neg").await;
    let token = valid_token_for(TENANT_B);
    let (status, body) = get_query_range(instance.addr, Some(&format!("Bearer {token}"))).await;

    assert_eq!(status, 200, "globex-staging token reads 200; body: {body}");
    assert_eq!(
        up_series_count(&body),
        0,
        "negative control: globex-staging must NOT see acme-prod's up series; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-01 — FAIL-CLOSED: no token is refused 401 BEFORE the store
// (@US-RAUTH-01 @real-io @driving_adapter)
// =========================================================================

/// US-RAUTH-01 / AC no-token-is-refused-401-before-the-store. With auth
/// configured, a request with NO `Authorization` header returns 401 +
/// `WWW-Authenticate: Bearer`, and the Pulse store is never queried.
///
/// FALSIFIABILITY: the env tenant is set to `acme-prod` so a fall-through
/// impl would return 200 with acme-prod's data (catching a bypass), AND the
/// scaffold's env path (which DOES return acme-prod's data here) makes the
/// `status==401` assertion FAIL — proving the test cannot pass on the
/// no-auth code. It passes only once the bearer gate returns the RFC-6750
/// 401 with the `WWW-Authenticate: Bearer` challenge instead of the env
/// tenant's data.
#[tokio::test(flavor = "multi_thread")]
async fn no_token_is_refused_401_with_www_authenticate_challenge() {
    // env tenant set on purpose: a tokenless request must NOT return its
    // data, and must NOT use the bare "no tenant resolvable" 401 (no
    // challenge) — it must be the bearer-gate 401.
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "no-token").await;
    let (status, www, body) = get_with_challenge(instance.addr, None).await;

    assert_eq!(
        status, 401,
        "a tokenless auth-on request must be refused 401, not served the env tenant; body: {body}"
    );
    assert!(
        www.contains("Bearer"),
        "the 401 must carry a WWW-Authenticate: Bearer challenge (RFC 6750); got {www:?}"
    );
}

// =========================================================================
// US-RAUTH-02 — NO-BEARER-BYPASS (the load-bearing negative control, R3)
// (@US-RAUTH-02 @real-io @driving_adapter)
// =========================================================================

/// US-RAUTH-02 / AC auth-on-missing-token-does-NOT-downgrade-to-env-tenant.
/// THE no-bearer-bypass control: auth is configured AND an env tenant is
/// ALSO set (`acme-prod`), and a request arrives with NO bearer. The result
/// MUST be 401 — NOT a silent downgrade that scopes the query to the env
/// tenant `acme-prod` and returns its `up` series. The env tenant is never
/// consulted on the missing-bearer arm.
///
/// FALSIFIABILITY: this is the test that catches an `else env_tenant`
/// fall-through. Against the scaffold (whose handler resolves the env tenant
/// for a tokenless request), the request is scoped to `acme-prod` and
/// returns 200 with its `up` series — so the "401 AND no acme-prod data"
/// assertion FAILS. It passes only once arm 2 returns the 401 directly with
/// no fall-through. A DELIVER impl that downgrades to env on a missing bearer
/// CANNOT make this pass.
#[tokio::test(flavor = "multi_thread")]
async fn no_bearer_does_not_downgrade_to_env_tenant() {
    // The env tenant is set to acme-prod (a leftover); auth is on.
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "no-bypass").await;
    let (status, body) = get_query_range(instance.addr, None).await;

    assert_eq!(
        status, 401,
        "auth-on + no bearer must be 401, NOT downgraded to the env tenant; body: {body}"
    );
    assert_eq!(
        up_series_count(&body),
        0,
        "the no-bearer request must NOT be scoped to the env tenant acme-prod (no up series); \
         body: {body}"
    );
}

// =========================================================================
// US-RAUTH-01 — reject reason: expired token (the WS-named reject control)
// (@US-RAUTH-01 @real-io @driving_adapter)
// =========================================================================

/// US-RAUTH-01 / AC expired-token-refused-with-the-matching-reason. An
/// expired token returns 401; nothing is read.
///
/// FALSIFIABILITY: the env tenant is set to `acme-prod`, so against the
/// scaffold (whose env path ignores the header) the expired token is served
/// acme-prod's data 200 — the `status==401` assertion FAILS, proving the
/// test cannot pass on a fall-through. It passes only once the validator
/// rejects the past-`exp` token before the store.
#[tokio::test(flavor = "multi_thread")]
async fn expired_token_is_refused_401() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "expired").await;
    let token = expired_token();
    let (status, body) = get_query_range(instance.addr, Some(&format!("Bearer {token}"))).await;

    assert_eq!(
        status, 401,
        "an expired token must be refused 401; body: {body}"
    );
    assert_eq!(
        up_series_count(&body),
        0,
        "an expired token must read nothing, not the env tenant's data; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-04 — AUDIENCE FENCE: an ingest token cannot read
// (@US-RAUTH-04 @real-io @driving_adapter)
// =========================================================================

/// US-RAUTH-04 / AC ingest-audience-token-rejected-wrong-audience. Trent
/// presents a correctly-signed token whose `aud` is `kaleidoscope-ingest`
/// (minted to WRITE). On the read path it MUST reject 401 — an ingest token
/// cannot be replayed to read.
///
/// FALSIFIABILITY: the env tenant is set to `acme-prod`, so against the
/// scaffold (whose env path ignores the audience) the ingest token is served
/// acme-prod's data 200 — the `status==401` assertion FAILS, proving the
/// test cannot pass on a fall-through that never checks the audience. It
/// passes only once the read validator (configured `kaleidoscope-query`)
/// rejects the `kaleidoscope-ingest` audience.
#[tokio::test(flavor = "multi_thread")]
async fn ingest_audience_token_is_rejected_on_the_read_path() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "ingest-aud").await;
    let token = ingest_audience_token();
    let (status, body) = get_query_range(instance.addr, Some(&format!("Bearer {token}"))).await;

    assert_eq!(
        status, 401,
        "an ingest-audience token must reject wrong_audience on the read path; body: {body}"
    );
    assert_eq!(
        up_series_count(&body),
        0,
        "the ingest-audience token must read nothing; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-01 — REDACTION: the secret + token never appear on any surface
// (@US-RAUTH-01 @real-io @driving_adapter — a hard guardrail)
// =========================================================================

/// US-RAUTH-01 / AC the-secret-and-token-are-never-logged. A request with a
/// bad-signature token is rejected; the 401 BODY carries the aegis reason
/// but NEVER the raw token value, and NEVER the secret bytes.
///
/// FALSIFIABILITY: the env tenant is set to `acme-prod`, so the `status==401`
/// assertion FAILS against the scaffold's fall-through (which serves the env
/// tenant 200) — the test cannot pass on the no-auth code. Once the
/// bad-signature token is rejected 401, the substring-absence assertions
/// catch any mutation that echoes the token into the reason/body or logs the
/// secret. The secret + token are ASCII so a substring scan is faithful.
/// (The stderr audit-line redaction is additionally pinned at the shared-
/// crate layer in `query-http-common/tests/slice_07_read_auth_shared.rs`;
/// here we pin the wire 401 body.)
#[tokio::test(flavor = "multi_thread")]
async fn the_secret_and_token_never_appear_in_the_401_body() {
    let instance = start_with_auth(Some(TenantId(TENANT_A.to_string())), "redaction").await;
    let token = bad_signature_token();
    let (status, body) = get_query_range(instance.addr, Some(&format!("Bearer {token}"))).await;

    assert_eq!(
        status, 401,
        "a bad-signature token must be refused 401; body: {body}"
    );
    assert!(
        !body.contains(&token),
        "the raw bearer token must NEVER appear in the 401 body; body: {body}"
    );
    let secret_str = std::str::from_utf8(SECRET).expect("ascii secret");
    assert!(
        !body.contains(secret_str),
        "the HS256 secret bytes must NEVER appear in the 401 body; body: {body}"
    );
}

// =========================================================================
// US-RAUTH-02 — BACKWARD COMPAT: auth-off resolves the env tenant, header
// ignored. This is a GUARDRAIL — it must stay GREEN at every commit, so it
// is NOT #[ignore]'d: it drives `router_with_auth(.., None, ..)` (auth off),
// which the scaffold already honours via the existing env path.
// (@US-RAUTH-02 @real-io @driving_adapter @backward-compat)
// =========================================================================

/// US-RAUTH-02 / AC env-tenant-unchanged-when-auth-absent. With NO auth
/// configured (`auth = None`) and the env tenant set to `acme-prod`, a
/// request — even one carrying a stray `Authorization` header — returns
/// `acme-prod`'s `up` series exactly as before this feature: the header is
/// ignored, the env tenant scopes the query.
///
/// GUARDRAIL (GREEN now and after DELIVER): proves the additive opt-out. A
/// DELIVER change that read the header when auth is off (a regression) would
/// turn this RED.
#[tokio::test(flavor = "multi_thread")]
async fn auth_off_resolves_env_tenant_and_ignores_the_header() {
    let (store, _dir) = open_durable_store("compat-env");
    store
        .ingest(
            &TenantId(TENANT_A.to_string()),
            pulse::MetricBatch::with_metrics(vec![gauge(
                "up",
                "payments-api",
                vec![point(secs_to_nanos(1_717_000_100), 1.0, &[])],
            )]),
        )
        .expect("seed");
    // auth = None (env-tenant mode); env tenant = acme-prod.
    let router = query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn MetricStore + Send + Sync>,
        Some(TenantId(TENANT_A.to_string())),
        None,
        None,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    // A stray Authorization header that must be IGNORED in env mode.
    let (status, body) =
        get_query_range(addr, Some("Bearer some-stray-token-that-must-be-ignored")).await;
    handle.abort();

    assert_eq!(
        status, 200,
        "auth-off env-tenant request reads 200; body: {body}"
    );
    assert_eq!(
        up_series_count(&body),
        1,
        "auth-off must resolve the env tenant acme-prod and ignore the header; body: {body}"
    );
}

/// US-RAUTH-02 / AC unset-env-tenant-still-refuses-401. With NO auth and an
/// UNSET env tenant, the existing 401 "no tenant resolvable" refusal is
/// byte-for-byte unchanged.
///
/// GUARDRAIL (GREEN now and after DELIVER).
#[tokio::test(flavor = "multi_thread")]
async fn auth_off_unset_env_tenant_still_refuses_401() {
    let (store, _dir) = open_durable_store("compat-unset");
    let router = query_api::router_with_auth(
        Arc::clone(&store) as Arc<dyn MetricStore + Send + Sync>,
        None, // unset env tenant
        None, // auth off
        None,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    let (status, body) = get_query_range(addr, None).await;
    handle.abort();

    assert_eq!(
        status, 401,
        "auth-off + unset env tenant must still refuse 401"
    );
    assert!(
        body.contains("no tenant resolvable"),
        "the existing fail-closed reason must be unchanged; body: {body}"
    );
}

/// Keep the shared RED-reason constant referenced so it documents intent for
/// the whole suite even though each `#[ignore = "..."]` writes the literal.
#[test]
fn red_reason_is_documented() {
    assert_eq!(RED, "RED until DELIVER: read-path-query-api-auth-v0");
}
