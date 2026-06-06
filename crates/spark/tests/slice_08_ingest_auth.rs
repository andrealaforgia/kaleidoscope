//! Slice 08 — Spark ingest authentication (the client-side key).
//!
//! Feature: `spark-ingest-auth-v0` (ADR-0069, DD1-DD5). The gateway
//! sibling `aegis-ingest-auth-v0` (ADR-0068) locked the ingest door:
//! aperture now rejects every tokenless ingest `UNAUTHENTICATED` /
//! `reason=missing_claim`, nothing stored. This slice gives the Spark
//! SDK the key — `SparkConfig::with_bearer_token(token)` (and the
//! conventional `OTEL_EXPORTER_OTLP_HEADERS` env path) attach
//! `authorization: Bearer <token>` to ALL THREE OTLP exporters
//! uniformly, the credential is NEVER logged, and the no-auth path
//! stays byte-unchanged.
//!
//! Companion stories (`discuss/user-stories.md`):
//! US-SP-AUTH-01 (driving E2E: a programmatic bearer authenticates the
//! three signals at the real authenticated aperture);
//! US-SP-AUTH-02 (the conventional `OTEL_EXPORTER_OTLP_HEADERS` path —
//! honoured code-free by upstream — env-as-override precedence on
//! collision, and the empty-as-absent control; the malformed case is
//! upstream's silent-drop, not spark's, per the ADR-0069 amendment);
//! US-SP-AUTH-03 (safe by construction: the token is never logged; the
//! no-token path against an unauthenticated collector still works).
//!
//! ## Driving port (Mandate 1 — black-box)
//!
//! The system is observed ONLY through Spark's public surface:
//! `spark::init(SparkConfig)` — configured via the builder
//! (`with_bearer_token`, `with_endpoint`) and/or the
//! `OTEL_EXPORTER_OTLP_HEADERS` env var — then telemetry emitted
//! through the standard OTel global API, then the guard dropped to
//! force-flush. The observable OUTCOME is what reaches the
//! `RecordingSink` behind a REAL aperture: authenticated aperture +
//! valid token => ACCEPTED (sink non-empty); authenticated aperture +
//! no/absent token => DENIED at the door (sink empty); unauthenticated
//! aperture + no token => ACCEPTED (sink non-empty).
//!
//! No spark-internal type (`BearerToken`, `build_auth_metadata`) is
//! reached from these tests; the never-log assertion observes the
//! `target="spark"` event/`Debug` surfaces only.
//!
//! ## Strategy C — real-local-IO walking skeleton
//!
//! Per `distill/wave-decisions.md > WS strategy`: a REAL aegis-
//! authenticated aperture is spawned on EPHEMERAL loopback ports
//! (`127.0.0.1:0`) with a `RecordingSink`, its HS256 secret + tenant
//! catalogue written to real temp files (reaped on every exit path),
//! and the bearer token minted IN-SUITE with `jsonwebtoken::encode`
//! signed with the same secret bytes (the ADR-0068 F5 mint seam, reused
//! verbatim from `aperture/tests/slice_10_ingest_auth.rs`). No
//! in-memory exporter, no synthetic transport — the metadata must reach
//! a real validator over a real gRPC channel for the accept/deny to be
//! meaningful.
//!
//! ## Ephemeral-port hygiene
//!
//! EVERY aperture spawned here binds `grpc_bind_addr` / `http_bind_addr`
//! to `127.0.0.1:0` and Spark connects to the OS-assigned bound address
//! (`fixture.grpc_endpoint()`). The fixed defaults 4317/4318 are NEVER
//! used — they collide with aperture's slice_09/slice_10 refusal-probe
//! tests under parallel runs (the known flake). The aperture child and
//! every temp file are reaped on drop.
//!
//! ## RECONCILED to the ADR-0069 amendment (DISTILL back-propagation)
//!
//! Per `adr-0069 § Amendment` + `design/wave-decisions.md § Changed
//! Assumptions` (2026-06-06): `opentelemetry-otlp =0.27` ALREADY honours
//! `OTEL_EXPORTER_OTLP_HEADERS` unconditionally on spark's construction
//! path, so spark builds NO env parser; on a key collision the ENV value
//! WINS (`HeaderMap::extend` overwrites); a malformed env header is
//! handled upstream by SILENT-DROP. DELIVER builds ONLY the programmatic
//! knob (`build_auth_metadata` + apply-shim + three `.with_metadata`).
//! This file is reconciled to that contract: env-happy-path ADDED,
//! precedence test INVERTED (env-wins), malformed-fail-fast test REMOVED.
//!
//! ## RED-not-BROKEN classification (Mandate 7)
//!
//! `spark` exists; the DISTILL scaffold (`config.rs`) adds
//! `with_bearer_token` + the private `bearer_token: Option<BearerToken>`
//! field + the redacting `BearerToken` newtype, so these tests COMPILE
//! today. But the scaffold only STORES the token — DELIVER lands
//! `build_auth_metadata` + the per-signal apply-shim in `init.rs`, which
//! actually attach the programmatic metadata. So against today's scaffold
//! the behaviourally-RED tests are: the PROGRAMMATIC ACCEPT tests (the
//! token is stored but no `authorization` metadata is attached, so the
//! authenticated aperture DENIES the export and the sink stays empty —
//! the `sink non-empty` assertion FAILS, RED not a missing symbol); and
//! the PRECEDENCE test (env-over-programmatic override is only provable
//! once the programmatic knob actually attaches, so it can't yet be
//! distinguished from "the knob isn't wired"). The control/guard tests
//! already pass and are left UN-ignored: the env-happy-path GUARD (the
//! upstream env honour works code-free — classified GREEN by RUNNING, the
//! amendment's disambiguation probe / reconciliation of Bea msg-038), the
//! never-log GUARDRAIL (the redacting `Debug` is in the scaffold), the
//! no-token DENY negative control, the expired-honest-send, the empty-env
//! control, and the no-auth non-regression.
//!
//! Each behaviourally-RED test is
//! `#[ignore = "RED until DELIVER: spark-ingest-auth-v0"]` so
//! `cargo test -p spark` stays GREEN at the DISTILL commit; DELIVER
//! removes the ignores one at a time. Falsifiability is proven by
//! running with `--ignored`: each ignored test panics on its OUTCOME
//! assertion (sink empty for the programmatic accepts), NOT on a missing
//! symbol.

mod common;

use std::path::PathBuf;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serial_test::serial;
use spark::{init, SparkConfig};

use crate::common::{
    capture_spark_events, expect_spark_event_with_message, wait_for, CANONICAL_SERVICE_NAME,
};

const RED: &str = "RED until DELIVER: spark-ingest-auth-v0";

// The OTel-canonical headers env var Spark must honour (DD4 / US-SP-AUTH-02).
const ENV_OTLP_HEADERS: &str = "OTEL_EXPORTER_OTLP_HEADERS";

// =========================================================================
// Token-minting seam (ADR-0068 F5 — reused verbatim from aperture slice_10)
// =========================================================================
//
// The "valid" token is signed with SECRET for ISSUER / AUDIENCE / TENANT
// (catalogued) with a future exp — exactly what the authenticated aperture
// fixture below pins. The expired-token negative control perturbs only `exp`.

const ISSUER: &str = "acme-observability";
const AUDIENCE: &str = "kaleidoscope-ingest";
const SECRET: &[u8] = b"slice-08-spark-ingest-auth-test-secret-not-for-production";
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

/// An expired token: every axis valid except `exp` is 5 minutes in the
/// past. Spark must SEND it honestly (DD5); the gateway rejects.
fn expired_token() -> String {
    sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() - 300,
            tenant_id: TENANT,
            kaleidoscope_role: ROLE_OPERATOR,
        },
        SECRET,
    )
}

// =========================================================================
// Authenticated-aperture fixture (real aegis validator, ephemeral ports)
// =========================================================================
//
// Spawns a REAL aperture whose ingest path is authenticated against
// ISSUER/AUDIENCE/SECRET/TENANT, fronted by Spark's RecordingSink. The
// `jwt_auth(...)` builder seam is aperture's (ADR-0068 DD1) and is LIVE
// (aegis-ingest-auth-v0 is delivered), so a tokenless export is genuinely
// denied and a valid-bearer export is genuinely accepted — that live
// validator is what makes today's no-knob Spark fail RED.
//
// The secret + catalogue are written to real temp files (Strategy C);
// `TempAuthFiles::Drop` reaps them on every exit path.

struct TempAuthFiles {
    secret_path: PathBuf,
    catalogue_path: PathBuf,
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
    let secret_path = std::env::temp_dir().join(format!("spark-auth-secret-{label}-{stamp}.key"));
    let catalogue_path =
        std::env::temp_dir().join(format!("spark-auth-catalogue-{label}-{stamp}.toml"));
    std::fs::write(&secret_path, SECRET).expect("write test secret file");
    std::fs::write(&catalogue_path, format!("[[tenants]]\nid = \"{TENANT}\"\n"))
        .expect("write test catalogue file");
    TempAuthFiles {
        secret_path,
        catalogue_path,
    }
}

/// Spawn a REAL authenticated aperture on ephemeral loopback ports with
/// Spark's RecordingSink. Mirrors `common::spawn_aperture_with_recording_sink`
/// but adds the `jwt_auth(...)` ingest-auth config so the validator is
/// live. Returns the same `ApertureFixture` shape the other spark slices
/// use (its Drop reaps the aperture + resets Spark's single-init flag).
async fn spawn_authenticated_aperture(files: &TempAuthFiles) -> common::ApertureFixture {
    common::spawn_aperture_with_jwt_auth(
        ISSUER,
        AUDIENCE,
        files.secret_path.clone(),
        files.catalogue_path.clone(),
    )
    .await
}

/// Emit one span + one log + one metric through the standard OTel global
/// API so all three exporters are exercised (US-SP-AUTH-01: the token
/// must ride ALL THREE signals). The guard is dropped to force-flush.
fn emit_all_three_signals_then_flush(guard: spark::SparkGuard) {
    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("payments-api");
        let _span = tracer.start("checkout");
    }
    {
        let meter = opentelemetry::global::meter("payments-api");
        let counter = meter.u64_counter("checkouts").build();
        counter.add(1, &[]);
    }
    // Logs flow through the tracing bridge the fixture wired post-init;
    // a non-"spark" target event becomes an OTel LogRecord.
    tracing::info!(target: "payments-api", "checkout completed");
    drop(guard);
}

// =========================================================================
// US-SP-AUTH-01 — WALKING SKELETON (@walking_skeleton @driving_port @real-io)
// =========================================================================
//
// The thinnest end-to-end user journey delivering observable value: Marco
// configures Spark with a valid bearer token against a REAL authenticated
// aperture, his service exports telemetry, and the telemetry is ACCEPTED
// (reaches the sink) — where without the token it would be DENIED. This
// answers "can Marco get his service's telemetry through the secured
// gateway?" with a demonstrated yes.

/// WS / US-SP-AUTH-01 / AC a-bearer-configured-export-is-accepted-by-the-
/// authenticated-gateway. Marco configures `with_bearer_token(<valid jwt>)`
/// against the authenticated aperture; his export is ACCEPTED (the record
/// reaches the sink).
///
/// FALSIFIABILITY: against today's scaffold the token is stored but not
/// attached to any exporter, so the authenticated aperture denies the
/// export `missing_claim` and the sink stays EMPTY — this `non-empty`
/// assertion FAILS. It passes only once DELIVER attaches the metadata.
#[tokio::test(flavor = "multi_thread")]
#[serial]
#[ignore = "RED until DELIVER: spark-ingest-auth-v0"]
async fn marco_with_a_valid_bearer_token_has_his_export_accepted_by_the_authenticated_gateway() {
    let files = write_auth_files("ws-accept");
    let fixture = spawn_authenticated_aperture(&files).await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_tenant_id(TENANT)
            .with_endpoint(fixture.grpc_endpoint())
            .with_bearer_token(valid_token()),
    )
    .expect("init succeeds with a bearer token configured");
    emit_all_three_signals_then_flush(guard);

    wait_for(|| !fixture.sink.is_empty(), Duration::from_secs(3)).await;
    assert!(
        !fixture.sink.is_empty(),
        "a valid-bearer export must be ACCEPTED by the authenticated gateway \
         (the record must reach the sink)"
    );
}

/// WS / US-SP-AUTH-01 / AC ... the same export with no token is DENIED.
/// The negative control half of the walking skeleton: the SAME service
/// against the SAME authenticated aperture, but WITHOUT a token, is
/// DENIED at the door — nothing reaches the sink.
///
/// FALSIFIABILITY: this is a GUARDRAIL that already holds today (no token
/// -> no metadata -> denied), so it is left UN-ignored as the negative
/// control that makes the accept test above meaningful. If a future
/// mutation attached metadata unconditionally, this control would break.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn marco_without_a_token_is_denied_by_the_authenticated_gateway_nothing_stored() {
    let files = write_auth_files("ws-deny");
    let fixture = spawn_authenticated_aperture(&files).await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_tenant_id(TENANT)
            .with_endpoint(fixture.grpc_endpoint()),
    )
    .expect("init succeeds even with no token (Spark sends none)");
    emit_all_three_signals_then_flush(guard);

    // Give the (rejected) exports time to NOT arrive.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        fixture.sink.is_empty(),
        "a tokenless export to an authenticated gateway must be DENIED \
         (nothing reaches the sink)"
    );
}

/// US-SP-AUTH-01 / AC the-token-reaches-all-three-signals. The token must
/// ride traces AND logs AND metrics. The observable all-three property:
/// when ONLY a metric is emitted (no spans, no logs — Marco's batch-job
/// edge case), the metric export is still authenticated and ACCEPTED.
/// This is the falsifiable witness that the metric exporter is not left
/// un-authenticated by omission (the partial-wire non-goal).
///
/// FALSIFIABILITY: today no metadata is attached to ANY exporter, so the
/// metric-only export is denied and the sink stays empty -> FAILS. A
/// DELIVER partial-wire that authenticated only traces+logs would ALSO
/// fail this test (metric denied), which is exactly its purpose.
#[tokio::test(flavor = "multi_thread")]
#[serial]
#[ignore = "RED until DELIVER: spark-ingest-auth-v0"]
async fn a_metric_only_export_is_authenticated_proving_the_token_reaches_the_metric_signal() {
    let files = write_auth_files("ws-metric-only");
    let fixture = spawn_authenticated_aperture(&files).await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_tenant_id(TENANT)
            .with_endpoint(fixture.grpc_endpoint())
            .with_bearer_token(valid_token()),
    )
    .expect("init succeeds");
    {
        let meter = opentelemetry::global::meter("payments-api");
        let counter = meter.u64_counter("batch_records").build();
        counter.add(42, &[]);
    }
    drop(guard);

    wait_for(|| !fixture.sink.is_empty(), Duration::from_secs(3)).await;
    assert!(
        !fixture.sink.is_empty(),
        "a metric-only authenticated export must be ACCEPTED — the token \
         must reach the metric exporter, not only traces+logs"
    );
}

/// US-SP-AUTH-01 Example 3 / AC Spark-sends-the-token-honestly. Marco's
/// token is EXPIRED. Spark attaches it verbatim and SENDS it (DD5 — its
/// job is correct transmission); the gateway rejects it, so nothing
/// reaches the sink. Spark does not pre-validate or silently drop.
///
/// FALSIFIABILITY: this is a guardrail on the gateway-side reject — today
/// (no metadata) the sink is empty for the wrong reason (missing_claim,
/// not expired). It is left UN-ignored because the OBSERVABLE spark-side
/// outcome (nothing stored, init still succeeded, no panic) holds at both
/// DISTILL and DELIVER; the reject-reason axis is aperture's concern,
/// asserted in aperture's own slice_10. Here we pin Spark's contract:
/// init succeeds with an expired token (Spark does not judge it).
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn marco_with_an_expired_token_still_initialises_spark_sends_it_honestly() {
    let files = write_auth_files("ws-expired");
    let fixture = spawn_authenticated_aperture(&files).await;

    // Spark's contract: it accepts and stores the token without judging
    // its `exp`; init succeeds. (The gateway rejection is aperture's
    // surfacing, covered by aperture/slice_10.)
    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_tenant_id(TENANT)
            .with_endpoint(fixture.grpc_endpoint())
            .with_bearer_token(expired_token()),
    )
    .expect("init must succeed even with an expired token — Spark does not pre-validate it");
    emit_all_three_signals_then_flush(guard);

    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        fixture.sink.is_empty(),
        "an expired token is rejected by the gateway — nothing is stored \
         (Spark sent it honestly; the gateway judged it)"
    );
}

// =========================================================================
// US-SP-AUTH-02 — OTEL_EXPORTER_OTLP_HEADERS conventional path (@driving_port)
// =========================================================================
//
// The credential set the conventional, code-free way in a deployment
// manifest. serial_test + a clean-env helper mirror slice_04's env-var
// pattern; each env-mutating test clears the var on exit.

/// Clear the headers env var, run the body. The body sets the value when
/// the scenario needs it; each test removes it again on exit.
fn with_clean_headers_env<F: FnOnce() -> R, R>(f: F) -> R {
    std::env::remove_var(ENV_OTLP_HEADERS);
    f()
}

// ---------------------------------------------------------------------
// US-SP-AUTH-02 / AC OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer.
//
// RECONCILED to the ADR-0069 § Amendment (DISTILL back-propagation),
// 2026-06-06. The amendment establishes (from a locked-source read) that
// `opentelemetry-otlp =0.27` ALREADY honours `OTEL_EXPORTER_OTLP_HEADERS`
// UNCONDITIONALLY on spark's exact `.with_tonic()...build()` construction
// path — `parse_headers_from_env` is called regardless of `.with_metadata`
// (tonic/mod.rs:156), and the spec percent-decode is applied upstream
// (`url_decode`, mod.rs:233). Two reconciled consequences:
//
//   1. ENV HAPPY-PATH is the amendment's "env-before-init disambiguation
//      probe": setting the env var BEFORE `spark::init` and asserting the
//      real authenticated aperture ACCEPTS proves the env path works on
//      spark's construction with NO spark code. It is the empirical
//      reconciliation of Bea Verifier msg-038 (the black-box observation
//      that no bearer arrived via env). Classified by RUNNING — see the
//      test's doc + distill/wave-decisions.md.
//   2. PRECEDENCE inverts. Because upstream merges via
//      `HeaderMap::extend` (tonic/mod.rs:320-321), which OVERWRITES on key
//      collision, a concurrently-set env `authorization` is the FINAL
//      writer — the ENV WINS, not the programmatic knob. The precedence
//      test below is revised to assert env-as-override (the honest,
//      locked-upstream behaviour), superseding the original
//      "programmatic-wins" assertion (which would have been GREEN against
//      a falsehood).
//
// The original spark-owned MALFORMED-fail-fast test is REMOVED: env
// parsing is upstream's concern and upstream's malformed behaviour is
// SILENT-DROP (`HeaderValue::from_str(..).ok()?`, tonic/mod.rs:335), not
// fail-fast; the programmatic token is a plain String with no parse, so
// there is no spark-owned malformed case. See distill/acceptance-test-
// scenarios.md > Removed test (amendment reconciliation).
// ---------------------------------------------------------------------

/// US-SP-AUTH-02 / AC OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer.
/// The amendment's ENV-BEFORE-INIT DISAMBIGUATION PROBE: set
/// `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<valid jwt>` BEFORE
/// `spark::init`, export to the REAL aegis-authenticated aperture, assert
/// the export is ACCEPTED (the record reaches the sink). This proves the
/// conventional code-free env path delivers an authenticated export on
/// spark's construction — the reconciliation of Bea Verifier msg-038's
/// black-box "no bearer arrived via env" observation.
///
/// CLASSIFICATION (by RUNNING, the load-bearing reconciliation): per the
/// amendment, upstream `opentelemetry-otlp =0.27` honours the env var
/// UNCONDITIONALLY on spark's path, so this is expected GREEN TODAY with
/// NO spark change — which is the whole point of the amendment (the
/// env-honouring half of the feature already works code-free). It is
/// therefore left UN-ignored as a NON-REGRESSION GUARD documenting that
/// the env path needs no spark code: a future spark change that broke or
/// double-attached on the env path would fail this guard. (If running had
/// instead shown RED — spark somehow bypassing the upstream env read — it
/// would have stayed `#[ignore]`d; the empirical run governs.)
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn an_env_authorization_header_set_before_init_is_accepted_by_the_authenticated_gateway() {
    let files = write_auth_files("env-happy-path");
    let fixture = spawn_authenticated_aperture(&files).await;
    let token = valid_token();
    // Set the conventional OTLP headers env var BEFORE init (the read is
    // at exporter-BUILD time, inside `spark::init`). Percent-encoded space
    // per the OTLP spec; upstream percent-decodes it.
    with_clean_headers_env(|| {
        std::env::set_var(ENV_OTLP_HEADERS, format!("authorization=Bearer%20{token}"));
    });

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_tenant_id(TENANT)
            .with_endpoint(fixture.grpc_endpoint()),
    )
    .expect("init succeeds with the headers env var set (no programmatic knob)");
    emit_all_three_signals_then_flush(guard);

    let accepted = {
        let f = &fixture;
        let mut ok = false;
        for _ in 0..120 {
            if !f.sink.is_empty() {
                ok = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        ok
    };
    std::env::remove_var(ENV_OTLP_HEADERS);
    assert!(
        accepted,
        "an env-set authorization bearer (before init) must be honoured by \
         the upstream exporter and ACCEPTED by the authenticated gateway — \
         the code-free env path works on spark's construction"
    );
}

/// US-SP-AUTH-02 / AC precedence — REVISED to env-as-override per the
/// ADR-0069 amendment. When BOTH the programmatic `with_bearer_token`
/// (a VALID token for the catalogued tenant `acme-prod`) AND the env
/// `authorization` (a token for a DIFFERENT, UNKNOWN tenant
/// `acme-prod-evil`) are set, the ENV value is the one that reaches the
/// wire (upstream `HeaderMap::extend` overwrites on key collision). So
/// the gateway sees the UNKNOWN-tenant token and DENIES the export —
/// nothing reaches the sink. This is the honest locked-upstream
/// behaviour, superseding the original "programmatic wins" assertion.
///
/// CLASSIFICATION (by RUNNING — recorded honestly): against today's
/// scaffold the programmatic knob is a no-op (it stores but does not
/// `.with_metadata`-attach), so ONLY the env token is attached (by
/// upstream). The deny outcome (sink empty) is therefore satisfied
/// TRIVIALLY — for the wrong reason ("the knob isn't wired"), NOT because
/// "the knob attached and the env overwrote it". Under `--ignored` this
/// test consequently PASSES today; it is NOT behaviourally RED against the
/// scaffold and is NOT a falsifiable scaffold-RED. It is kept `#[ignore]`d
/// (out of the default suite) precisely so its trivial green is NOT
/// counted as a real control (Critical Rule 7 — Fixture/Upstream Theater).
/// It becomes the MEANINGFUL env-over-programmatic precedence assertion
/// only once DELIVER lands the programmatic `.with_metadata` attach: then
/// "both attached, env overwrites on collision" is the real, observable
/// contention this test pins. DELIVER un-ignores it together with the
/// programmatic-attach landing and verifies it still asserts the deny —
/// the proof that the knob attaches AND the env is final on collision.
#[tokio::test(flavor = "multi_thread")]
#[serial]
#[ignore = "DELIVER-completion: env-over-programmatic precedence is only meaningful once the knob attaches (spark-ingest-auth-v0)"]
async fn the_env_authorization_overrides_the_programmatic_bearer_token_on_collision() {
    let files = write_auth_files("env-precedence");
    let fixture = spawn_authenticated_aperture(&files).await;
    // The env var carries a token for an UNKNOWN tenant. Per the amendment
    // (upstream extend-overwrite), this env value is FINAL on key
    // collision — so even with a valid programmatic token set, the gateway
    // sees the unknown-tenant token and DENIES.
    let env_token = sign(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: "acme-prod-evil",
            kaleidoscope_role: ROLE_OPERATOR,
        },
        SECRET,
    );
    with_clean_headers_env(|| {
        std::env::set_var(
            ENV_OTLP_HEADERS,
            format!("authorization=Bearer%20{env_token}"),
        );
    });

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_tenant_id(TENANT)
            .with_endpoint(fixture.grpc_endpoint())
            .with_bearer_token(valid_token()),
    )
    .expect("init succeeds with both paths set");
    emit_all_three_signals_then_flush(guard);

    // Give the (env-token, unknown-tenant) export time to be DENIED.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let sink_empty = fixture.sink.is_empty();
    std::env::remove_var(ENV_OTLP_HEADERS);
    assert!(
        sink_empty,
        "the env-set authorization (unknown tenant) must OVERRIDE the \
         programmatic bearer (valid tenant) on key collision — the gateway \
         sees the unknown-tenant token and DENIES (sink empty)"
    );
}

/// US-SP-AUTH-02 / AC empty-env-var-is-no-credential. An empty
/// OTEL_EXPORTER_OTLP_HEADERS is treated as absent: no header attached.
/// Observed against an UNAUTHENTICATED aperture, which still ACCEPTS
/// (the no-credential path is preserved).
///
/// FALSIFIABILITY: this is a guardrail (empty == absent). It already
/// holds today (no parser) AND must keep holding after DELIVER, so it is
/// UN-ignored. A DELIVER bug that treated an empty value as a malformed
/// credential (fail-fast) would break this control.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn an_empty_headers_env_var_is_treated_as_no_credential_and_unauth_collector_accepts() {
    let fixture = common::spawn_aperture_with_recording_sink().await;
    with_clean_headers_env(|| {
        std::env::set_var(ENV_OTLP_HEADERS, "");
    });

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(fixture.grpc_endpoint()),
    )
    .expect("init succeeds with an empty headers env var");
    emit_all_three_signals_then_flush(guard);

    wait_for(|| !fixture.sink.is_empty(), Duration::from_secs(3)).await;
    let non_empty = !fixture.sink.is_empty();
    std::env::remove_var(ENV_OTLP_HEADERS);
    assert!(
        non_empty,
        "an empty headers env var must attach no credential — an \
         unauthenticated collector still accepts the export"
    );
}

// NOTE — the original malformed-header-fails-init-fast test (#7) is
// REMOVED here per the ADR-0069 amendment. Env parsing is upstream's
// concern (upstream's malformed behaviour is SILENT-DROP via
// `HeaderValue::from_str(..).ok()?`, tonic/mod.rs:335, NOT fail-fast), and
// the programmatic token is a plain String with no percent-decode/parse —
// so there is no spark-owned malformed case to assert. See
// distill/acceptance-test-scenarios.md > Removed test.

// =========================================================================
// US-SP-AUTH-03 — safe by construction: never logged + no-auth preserved
// =========================================================================

/// US-SP-AUTH-03 / AC the-token-is-never-logged. With a recognisable
/// token configured, the token value must appear ZERO times across every
/// captured `target="spark"` event AND in the `Debug` of the config; the
/// config Debug must render the redacted placeholder where the field
/// shows.
///
/// FALSIFIABILITY: this is the load-bearing SECURITY guardrail. It is
/// implemented by the DISTILL scaffold's redacting `BearerToken` Debug,
/// so it PASSES today and is left UN-ignored as a control — a mutation
/// that un-redacts the token (or a DELIVER regression that logs it on the
/// init event) must be caught here. The recognisable token is ASCII so a
/// substring scan is faithful.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn the_configured_token_never_appears_in_any_spark_log_event_or_config_debug() {
    let recognisable = "eyJTESTtokenRECOGNISABLE-do-not-leak-me-abcdef0123456789";
    let fixture = common::spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .with_endpoint(fixture.grpc_endpoint())
        .with_bearer_token(recognisable);

    // The Debug surface (dbg!/panic!("{config:?}")) must NOT carry the token.
    let config_debug = format!("{config:?}");
    assert!(
        !config_debug.contains(recognisable),
        "the token must NEVER appear in SparkConfig's Debug; got: {config_debug}"
    );
    assert!(
        config_debug.contains("<redacted>"),
        "the config Debug must render a redacted placeholder where the token field shows; \
         got: {config_debug}"
    );

    let guard = init(config).expect("init succeeds with a recognisable token");
    // Confirm the init-succeeded event still lands (closed vocabulary
    // unchanged) and then scan EVERY captured spark event for the token.
    let events = capture.events();
    let _ = expect_spark_event_with_message(&events, "spark::init succeeded");
    for e in &events {
        let rendered = format!("{} {} {}", e.level, e.message, e.fields);
        assert!(
            !rendered.contains(recognisable),
            "the token must NEVER appear in any target=\"spark\" event; leaked in: {rendered}"
        );
    }
    drop(guard);
}

/// US-SP-AUTH-03 / AC no-token-no-header-against-an-unauthenticated-
/// endpoint-still-works. With NO token (no knob, no env var), Spark
/// attaches no authorization header and an unauthenticated collector
/// accepts exactly as before this feature existed. The non-regression
/// guardrail (System Constraint 4).
///
/// FALSIFIABILITY: a guardrail that holds today and must keep holding —
/// UN-ignored. A DELIVER bug that attached an (empty/garbage) header
/// unconditionally could change the unauthenticated collector's
/// behaviour and break this control.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn no_token_no_header_against_an_unauthenticated_collector_still_exports() {
    let fixture = common::spawn_aperture_with_recording_sink().await;
    with_clean_headers_env(|| { /* ensure no env credential */ });

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(fixture.grpc_endpoint()),
    )
    .expect("init succeeds with no token against an unauthenticated collector");
    emit_all_three_signals_then_flush(guard);

    wait_for(|| !fixture.sink.is_empty(), Duration::from_secs(3)).await;
    assert!(
        !fixture.sink.is_empty(),
        "with no token, an unauthenticated collector must accept the export \
         exactly as before this feature (the no-auth path is byte-unchanged)"
    );
}

/// Keep the shared ignore-reason constant referenced so it documents the
/// suite's intent even though each `#[ignore = "..."]` writes the literal.
#[test]
fn red_reason_is_documented() {
    assert_eq!(RED, "RED until DELIVER: spark-ingest-auth-v0");
}
