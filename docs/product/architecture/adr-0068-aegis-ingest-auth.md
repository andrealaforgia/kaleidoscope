# ADR-0068 — Aegis ingest authentication: wire the JWT validator onto the live aperture ingest path, fail-closed

- **Status**: Accepted
- **Date**: 2026-06-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aegis-ingest-auth-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0061 (`tls-config-reject-v0`, the fail-closed refuse-to-start precedent — mirrored for DD4), ADR-0008 (config schema + `deny_unknown_fields`), ADR-0006 (aperture transport stack / `Arc<dyn OtlpSink>`), ADR-0007 (OtlpSink trait), ADR-0010 (concurrency caps / permit ordering), ADR-0066 (serve-loop exit-code taxonomy 0/1/2/3), aegis-v0 ADRs (the HS256 `Validator`).

## Context

Aegis is **a correct lock with no door fitted**. `aegis::Validator::validate(token, now) -> Result<TenantContext, ValidationError>` (`crates/aegis/src/validator.rs:174-209`) is real HS256 (jsonwebtoken), alg-confusion-safe (`Algorithm::HS256` pinned, `validator.rs:220`), fail-closed on `exp` (`:234-241`), exact issuer + audience equality (`:228-233`), unknown-tenant rejected against a TOML catalogue (`:246-248`), 8 typed `ValidationError` variants each with a stable `reason()` audit string (`:94-108`), and emits **exactly one** structured `tracing` audit event per call (`:186-209`: `info!` on allow, `warn!` on deny; fields `tenant_id`/`role`/`decision`/`subject`/`reason`). A `validate_with_subject(token, now, subject)` entry point already exists (`:180`) so a caller can stamp the audit `subject` with the ingest action.

But **aperture has zero auth today** and **does not depend on aegis** (verified: zero `aegis` references in `crates/aperture/`, no aegis dep in `crates/aperture/Cargo.toml`). The gRPC handlers (`LogsServiceImpl::export` etc., `transport.rs:638,715,781`) and the HTTP handlers (`handle_logs`/`handle_traces`/`handle_metrics`, `transport.rs:344,436,523`) never read gRPC request metadata nor the HTTP `Authorization` header. Each ingest path (`app::ingest_logs/ingest_traces/ingest_metrics`, `app.rs:64,89,115`) runs `(bytes, transport, sink) -> validate (OTLP-conformance, NOT auth) -> sink.accept(SinkRecord)`. The sink receives a `SinkRecord` (`ports/mod.rs:30`, `#[non_exhaustive]`) carrying **no tenant at all**.

The platform-wide consequence (verified): **zero production authn/z call sites**. Every `.validate(` outside aegis is a *different* validator (codex semconv, spark catalogue). The 15+ crates depending on aegis import only the `TenantId` newtype; none import `Validator`. So tenant id flows through the platform as a typed newtype **whose provenance is unauthenticated at the system boundary** — any caller POSTs OTLP with any `tenant_id` and it is accepted and stored under that tenant. Aegis-v0's own DISCUSS D10 deferred exactly this ("Aperture/Beacon/Prism keep auth-free at v0; integrating Aegis into each component is its own slice in v1"). **This feature is that deferred integration slice.**

The reserved Phase 2 config hooks are **TLS + SPIFFE, not HS256** (`config/mod.rs:160-177`; schema `[aperture.security.tls]` + `[aperture.security.auth.spiffe]`, `:432-468`). There is **no** `[aperture.security.auth.jwt]` table and no HS256 secret / issuer / audience / catalogue-path field. Those fields **do not exist yet and must be added** (DD1).

### What DESIGN must lock (this ADR)

1. **DD1** — the aperture config wiring for the HS256 validator, with a never-logged secret.
2. **DD2** — per-transport token extraction and the exact reject mapping (gRPC `UNAUTHENTICATED`; HTTP `401` + `WWW-Authenticate: Bearer`).
3. **DD3** — the authenticated-tenant ripple from handler → `ingest_*` → `SinkRecord`, without breaking the single-validator-per-signal invariant or the `#[non_exhaustive]` evolution guarantee.
4. **DD4** — the fail-closed default (refuse-to-start without auth config), mirroring ADR-0061.
5. **DD5** — the audit/observability of denials (one deny event per request, aegis `reason()` taxonomy, secret/token never logged).
6. **DD6** — the scope fence (ingest-path only) and the role-gating question.
7. **DD7** — the aegis "JWKS" doc overstatement (fold vs adjacent).

## Decision

Wire `aegis::Validator` onto aperture's ingest request path as a **new authentication step that runs before `ingest_*`**, fail-closed. Aperture gains a path dependency on aegis, constructs a `Validator` once at composition, and **refuses to start** if the ingest auth config is absent or unreadable. Every ingest request on every signal × transport extracts a bearer token, calls `validate_with_subject(token, now, "ingest_<signal>")`, and either rejects (`UNAUTHENTICATED` / `401`, nothing stored, one audit event) or accepts with the validated `tenant_id` riding the `SinkRecord` into the sink.

### DD1 — aperture config wiring; the secret is never logged

Add a new TOML sub-table **`[aperture.security.auth.jwt]`**, sibling to the existing `[aperture.security.auth.spiffe]`, with `#[serde(deny_unknown_fields)]` like every other aperture config struct. Fields:

```toml
[aperture.security.auth.jwt]
issuer         = "acme-observability"          # required (string)
audience       = "kaleidoscope-ingest"         # required (string)
secret_file    = "/etc/kaleidoscope/hs256.key" # required: PATH to the HS256 secret bytes
catalogue_path = "/etc/kaleidoscope/tenants.toml" # required: path to the aegis tenant catalogue
```

**The secret is supplied by reference (a file path), never inline.** Rationale (clig.dev "never accept secrets via flags; prefer files/stdin/env"; OWASP Secrets Management): an inline TOML string is loggable, lands in config dumps, lands in `Debug`, and lands in shell history / process listings. A **file path** keeps the secret bytes out of the config struct entirely — aperture reads the file at composition, hands the bytes straight to `aegis::ValidatorConfig { hs256_key }`, and the bytes live only inside the `aegis::Validator` (whose `Debug` already renders the key as `"<opaque>"`, `validator.rs:149-158`). An `APERTURE__SECURITY__AUTH__JWT__SECRET_FILE` env override also names the file by reference, not the secret. (A future `secret_env` variant — naming an env var that holds the bytes — is a non-breaking additive option; v0 ships `secret_file` only to keep the surface minimal.)

**Never-logged invariant — enforced structurally, not by discipline:**
- The config struct stores `secret_file: PathBuf`, **never the secret bytes**. The bytes are read into a local `Vec<u8>`, moved into `ValidatorConfig`, and never stored on `Config`. There is therefore nothing secret on `Config` to leak through its derived `Debug`.
- The aegis `Validator`'s hand-written `Debug` already prints `key = "<opaque>"`. Aperture holds the validator behind an `Arc<Validator>` and never logs the raw bytes.
- Config-validation errors name the secret **by path only** (e.g. `secret_file "/etc/…/hs256.key" is unreadable: <io-error-kind>`), never the bytes.
- The deny/allow audit events (DD5) carry `tenant_id`/`role`/`decision`/`subject`/`reason` — never the token, never the secret.

Aperture adds an `aegis = { path = "../aegis" }` dependency (non-wildcard, per the workspace `cargo deny` rule), constructs the validator **once** at composition from the loaded config (`aegis::load_catalogue(catalogue_path)` → `aegis::Validator::new(ValidatorConfig { issuer, audience, hs256_key, catalogue })`), and threads an `Arc<Validator>` into the gRPC services and `HttpState` alongside the existing `sink` + `limiter`.

### DD2 — token extraction per transport + exact reject mapping

A new pure-ish free function per transport extracts the bearer token; a single shared mapping translates `Result<TenantContext, ValidationError>` into the transport's reject. The auth step is inserted **after the ADR-0010 concurrency permit is acquired** (so a flood of tokenless requests is still bounded by the cap) and **before any body/content-type work** — fail-closed means an unauthenticated caller learns nothing about the body it sent.

**gRPC** (`LogsServiceImpl::export` and siblings). After `let _permit = ...`, read `request.metadata().get("authorization")`. The bearer extraction rules:
- no `authorization` key, or empty value, or a value not matching `Bearer <token>` with a non-empty token → treat as **`ValidationError::MissingClaim("authorization")`-equivalent**, i.e. reject with reason `missing_claim` (the aegis taxonomy already owns `missing_claim`; a missing bearer is "the token claim is absent"). Aperture does **not** call `validate` with an empty string and rely on aegis to classify it — the absence is decided at the extraction boundary so the reason is stable and the cheap path stays cheap.
- a well-formed `Bearer <jwt>` → call `validator.validate_with_subject(jwt, SystemTime::now(), "ingest_logs")`.

(On the pre-validate reject path aperture emits the reason **string** `"missing_claim"` directly — it never enters aegis, so no `aegis::ValidationError` is constructed; the parameterised `MissingClaim(&'static str)` variant lives only inside aegis for the *claims-missing-inside-a-decoded-token* case.)

On `Err(e)`: `return Err(Status::unauthenticated(e.reason()))`. The status message is **exactly the aegis `reason()` string** (`expired` / `unknown_tenant` / …) — machine-stable, leaks neither secret nor token. On `Ok(ctx)`: proceed to `ingest_logs(&bytes, Transport::Grpc, ctx.tenant_id, &self.sink)`.

**HTTP** (`handle_logs` and siblings). After `let _permit = ...` and **before** the `is_protobuf_content_type` check, read `headers.get(header::AUTHORIZATION)`. Same bearer extraction rules. On reject return:

```
401 Unauthorized
WWW-Authenticate: Bearer error="invalid_token", error_description="<aegis reason()>"
Content-Type: text/plain; charset=utf-8
<body: the aegis reason() string>
```

per RFC 6750 §3 (the `WWW-Authenticate: Bearer` challenge with `error`/`error_description`). For the no-token case RFC 6750 permits a bare `WWW-Authenticate: Bearer` with no `error` param; aperture emits `WWW-Authenticate: Bearer` and reason `missing_claim` in the body. On accept, proceed to `ingest_logs(&body, Transport::HttpProtobuf, tenant_id, &state.sink)`.

**Ordering is preserved and explicit**: gRPC `permit → auth → ingest`; HTTP `permit → auth → content-type(415) → ingest`. Placing auth before the 415 check means a tokenless request never reveals media-type acceptance — the fail-closed boundary is the outermost gate after backpressure. The 503-backpressure path (permit exhaustion) still precedes auth, which is correct: a saturated gateway sheds load before spending a signature verification.

### DD3 — the authenticated tenant ripple

The validated `TenantContext.tenant_id` flows handler → `ingest_*` → `SinkRecord` as follows:

1. **`ingest_*` signature** gains a tenant parameter:
   `ingest_logs(body: &[u8], transport: Transport, tenant: TenantId, sink: &Arc<dyn OtlpSink>) -> IngestOutcome` (and the traces/metrics siblings). This is the natural seam: the handler owns the authenticated identity, the app core threads it onto the record.
2. **`SinkRecord`** carries the tenant. Because `SinkRecord` is `#[non_exhaustive]` **at the enum level** (not the variant level), adding a tenant to each variant is a change to the *variant payloads*, which is a breaking change to in-crate matches but **not** to downstream crates (no downstream crate constructs or exhaustively matches `SinkRecord` — it is `pub` but only aperture builds it; the sink trait is the seam). The chosen shape: wrap the existing per-signal request in a small `struct` that pairs it with the tenant, i.e. each variant becomes `Logs(TenantScoped<ExportLogsServiceRequest>)` where `pub struct TenantScoped<T> { pub tenant: TenantId, pub inner: T }`. This is additive-by-composition, keeps `summarise_record` a one-line change per arm (`req` → `&scoped.inner`), and makes "every accepted record is tenant-tagged" a **type-level guarantee** — there is no way to build a `SinkRecord` without a tenant. The `OtlpSink::accept(record: SinkRecord)` signature is unchanged (the tenant rides inside the record), so no sink implementor signature breaks.
3. **Single-validator-per-signal invariant is preserved.** That invariant (`tests/invariant_single_validator.rs`, `xtask single-validator-per-signal`) counts call sites of the **OTLP-conformance** `validate_logs/traces/metrics`. The auth check is a call to `aegis::Validator::validate_with_subject` — a *different* function — and lives in the transport handler, **not** in `ingest_*`. The number of harness `validate_*` call sites stays exactly one per signal. The runtime corroboration (one `sink_accepted` per accepted request) also holds: auth gates *whether* `ingest_*` runs, it does not add a second hand-off.

The ripple is **brownfield but bounded**: it touches `app.rs` (3 signatures + 3 `SinkRecord::*` constructions + 3 `summarise_record` arms), `ports/mod.rs` (the `SinkRecord` payloads + `TenantScoped`), and `transport.rs` (6 handlers thread the tenant). The non-regression guard: a correctly-authenticated accept is byte-shape identical to today's accept (the tenant rides *inside* the record, the response shape is untouched).

### DD4 — fail-closed default: refuse-to-start without auth config (the ADR-0061 reflex)

**Auth is on whenever the ingest listeners bind. There is no off switch on the ingest path.** Mechanism, mirroring ADR-0061 exactly:

- If `[aperture.security.auth.jwt]` is **absent**, or any required field (`issuer`/`audience`/`secret_file`/`catalogue_path`) is missing, or the `secret_file` / `catalogue_path` is **unreadable**, or the catalogue fails to parse → aperture **refuses to start** at `RawConfig::into_config` (the same post-deserialise validation seam ADR-0061 uses), returns `Err(ConfigError(...))`, which hits the existing `main.rs` exit-2 arm, emits one `event=config_validation_failed` line naming the missing/unreadable auth config **by reference**, and **no listener binds** (structural: `Config` is never constructed, so the bind path is never entered).
- A **complete, readable** `[aperture.security.auth.jwt]` → startup proceeds, the validator is constructed, both listeners bind, the ingest path is authenticated.

Exit code **2** (config error) is reused verbatim — distinct from ADR-0066's exit 3 (serve-failure). No new exit code. The refusal is one more invariant in the same `into_config` validator that already rejects identical bind addresses and the ADR-0061 security knobs.

**No opt-out flag.** A flag defaulting OFF would turn "forgot to configure auth" into "silently shipped an open gateway" — the exact silent downgrade ADR-0061 closed for TLS. Local/dev runs supply a dev `[aperture.security.auth.jwt]` with a throwaway secret file and a one-tenant catalogue; that is one extra config block, not a security-relaxing flag. The secure path is the only path, which is the only honest posture for a multi-tenant ingest gateway.

**Interaction with ADR-0061:** the SPIFFE/TLS knobs still refuse-to-start when `=true` (aegis v0 is HS256, not SPIFFE; SPIFFE is aegis v1). So a config that sets *both* `auth.spiffe.enabled=true` and a `jwt` table still refuses on the SPIFFE knob — there is no contradiction, the two refusals are independent invariants in the same validator. The `jwt` table is the **HS256** scheme; `spiffe` remains the reserved v1 workload-identity scheme.

### DD5 — audit / observability of denials: aegis owns the per-request decision event

**Aperture does NOT emit its own deny event. Aegis's single audit event per `validate` call is the one source of truth** (`validator.rs:186-209`), and aperture supplies the `subject` via `validate_with_subject(_, _, "ingest_logs" | "ingest_traces" | "ingest_metrics")`. This gives exactly one structured decision event per validated request — `decision=deny reason=<one of 8> subject=ingest_<signal>` on reject, `decision=allow tenant_id=<t> role=<r> subject=ingest_<signal>` on accept — with **no double-logging** and **no zero-logging**. The aegis event already aligns to the locked field contract (`tenant_id`/`role`/`decision`/`subject`/`reason`, aegis D5) and rides aperture's existing `tracing` stderr stream.

The **one gap**: the no-token / malformed-bearer case is decided at aperture's extraction boundary *before* `validate` is called (DD2), so aegis does not see it and emits no event. For that single case **aperture emits the one decision event itself**, in the same field shape: `decision=deny reason=missing_claim subject=ingest_<signal> transport=<grpc|http>`. This is the only aperture-owned authz audit line, and it fires **only** on the pre-validate reject path, so the "exactly one event per request" invariant holds across all paths: validate-reached requests get aegis's event; pre-validate rejects get aperture's single event; never both, never neither. No secret, no raw token, in any field. (Aperture's existing `event=request_received` line is a transport-trace event on a different axis, not an authz decision; it is unaffected.)

The `transport=` field is added to the deny axis so Priya can partition denials by front door. The aperture closed event vocabulary (`observability.rs`) gains no new *event name* for the allow/deny axis (aegis owns `"aegis authz decision"`); the pre-validate aperture line reuses the same message and field set for shape-compatibility.

### DD6 — scope fence + role question

**Scope fence**: v0 = the **ingest path only** (gRPC + HTTP, three signals, full reject matrix). Read-path auth (query-api / log-query-api / trace-query-api) is a **separate future feature**, explicitly out. SPIFFE / RS256 / JWKS / OPA are aegis v1, out.

**Role question — RESOLVED: v0 is authentication-only; role-gating is deferred.** Any valid token for a catalogued tenant (role `viewer` **or** `operator`) may ingest. Rationale:
- The minimum fail-closed property the audit demands is "no unauthenticated/forged write" — that is *authentication + tenant tagging*, fully delivered by validating the token and tagging the record. Requiring `operator` adds *authorization* on top, which is a separate concern with its own policy questions (should `viewer` ever write? is "write" even a `viewer` capability?) better answered when the read-path auth lands and the full role matrix is in view.
- Aegis already **rejects** `unknown_role` (a role that is neither `viewer` nor `operator`) inside `validate` — that protection is inherited free and surfaces as reason `unknown_role`. What v0 does *not* do is reject a **valid** `viewer` token on the write path. That is the deferred authorization decision.
- Deferring keeps the WS thin and the blast radius minimal on the live gateway (R1): we change *who can write* from "anyone" to "any authenticated catalogued tenant", not "any authenticated catalogued tenant with role=operator" — the smaller, safer step, with role-gating as a clean follow-up that needs no re-plumbing (the `TenantContext.role` is already threaded to the handler; a future feature adds one `if ctx.role != Operator { reject }` gate).

This is recorded as the DD6 resolution; US-AUTH-05 AC "DD6 resolved" is satisfied by this explicit deferral with rationale.

### DD7 — the aegis "JWKS" doc overstatement: adjacent, NOT folded

`aegis/src/lib.rs:18-23,39-41` says aegis validates "against a configured issuer + JWKS"; the validator is HS256 pre-shared-key only (no JWKS, no network at validation time). **Decision: flag as an adjacent low-priority doc-fix; do NOT fold into this feature.** Rationale: (a) this feature is security-critical and large; a doc edit in *another crate* dilutes the review focus and the mutation scope (the 100% kill-rate gate is scoped to the *modified aperture files* — pulling an aegis source file in would force aegis back into the mutation set for a non-behavioural change); (b) it touches aegis, not aperture, so it is outside this feature's modified-file set; (c) per "decide rather than ask", the call is made — it is a one-line correction worth a trivial fix-forward note or its own micro-wave, not scope-creep on the auth boundary. Logged here and in the feature wave-decisions so it is not lost. **Recommended disposition**: a `docs:` fix-forward on the closed wave or a trivial follow-up; the correct text is "validates against a configured issuer + audience using a pre-shared HS256 key (RS256/JWKS is v1)".

## Reuse Analysis (MANDATORY)

| Capability | Verdict | Where / How |
|---|---|---|
| HS256 JWT validation (sig/exp/iss/aud/tenant/role) | **REUSE verbatim** | `aegis::Validator::validate` / `validate_with_subject` (`validator.rs:174-209`). No crypto change. |
| Typed success context | **REUSE verbatim** | `aegis::TenantContext { tenant_id, role }`, `aegis::TenantId`, `aegis::Role` (`validator.rs:33,43,66`). |
| Typed failure taxonomy + stable reasons | **REUSE verbatim** | `aegis::ValidationError` (8 variants) + `reason()` (`validator.rs:74-108`). |
| Validator construction | **REUSE verbatim** | `aegis::Validator::new(ValidatorConfig { issuer, audience, hs256_key, catalogue })` (`validator.rs:162`). |
| Tenant catalogue load | **REUSE verbatim** | `aegis::load_catalogue` / `TenantCatalogue` (`catalogue.rs`, re-exported `lib.rs:52`). |
| One-audit-event-per-call + field contract | **REUSE verbatim** | aegis emits `info!`/`warn!` with `tenant_id`/`role`/`decision`/`subject`/`reason` (`validator.rs:186-209`); aperture supplies `subject`. |
| Fail-closed refuse-to-start at config validation | **REUSE pattern** | ADR-0061 `RawConfig::into_config` → `ConfigError` → `main.rs` exit-2, no listener binds. Same seam, new invariant. |
| Opaque-Debug for the signing key | **REUSE verbatim** | `aegis::Validator`'s hand-written `Debug` prints `key="<opaque>"` (`validator.rs:149-158`); aperture must not undo it (stores `secret_file: PathBuf`, never the bytes). |
| Concurrency permit ordering | **REUSE/EXTEND** | ADR-0010 permit acquired first; auth slots in after permit, before ingest (`transport.rs:356,649`). |
| `Arc<dyn OtlpSink>` composition wiring | **EXTEND** | services + `HttpState` gain `Arc<aegis::Validator>` alongside `sink`+`limiter` (`transport.rs:154-183,223-252`). |
| aperture config schema | **EXTEND** | add `[aperture.security.auth.jwt]` sibling to `spiffe` (`config/mod.rs:445-468`); add `deny_unknown_fields`; map in `into_config`. |
| aperture ingest path | **EXTEND** | `ingest_*` gains a `tenant: TenantId` parameter; `SinkRecord` variants gain a tenant via `TenantScoped<T>` (`app.rs:64-128`, `ports/mod.rs:30-34`). |
| aperture transport handlers | **EXTEND** | 6 handlers extract bearer, validate, reject/accept (`transport.rs:344,436,523,638,715,781`). |
| Bearer extraction + reject mapping | **CREATE** | new free fns `extract_bearer_grpc(&MetadataMap)` / `extract_bearer_http(&HeaderMap)` and a shared `reject_to_status` / `reject_to_http` mapping. Justified: no existing aperture code reads `authorization`/`Authorization` (verified zero matches); this is the genuinely new auth-extraction boundary. |
| aperture config `jwt` fields | **CREATE** | the HS256 secret-file/issuer/audience/catalogue-path fields do not exist (the reserved hooks are TLS+SPIFFE). Justified by F5: no existing field carries HS256. |

**Net**: REUSE the entire aegis validation core and the ADR-0061 fail-closed pattern verbatim; EXTEND aperture's config/handlers/ingest/sink; CREATE only the thin auth-extraction boundary and the four config fields. No new crate, no new always-running task, no duplicated validator.

## Security posture (this is the load-bearing section)

- **Fail-closed by construction (A04/Fail-Secure)**: no `[aperture.security.auth.jwt]` ⇒ no `Config` ⇒ no listener. Missing/empty/malformed bearer ⇒ reject. Any ambiguity rejects. There is never a silent default-tenant accept.
- **Complete mediation (A01)**: every ingest request on every signal × transport is validated; no cached auth decision; the permit→auth→ingest ordering guarantees no body is processed for an unauthenticated caller.
- **Secret never logged (Info-Disclosure)**: secret supplied by file path; config stores `PathBuf` not bytes; aegis opaque-Debugs the key; errors name the file by path; audit/deny lines carry no token and no secret. Enforced structurally (the bytes never reach a loggable field), not by reviewer vigilance.
- **Authentication at the trust boundary (Spoofing)**: tenant id provenance changes from "unauthenticated caller payload" to "claim inside an HS256-verified, catalogue-checked token". Mallory can no longer write under a victim tenant.
- **Legible denials / non-repudiation (Repudiation, A04 abuse-case)**: 8 distinct stable reasons let Priya distinguish `expired` (tell the fleet to refresh) from `invalid_signature` (escalate: forgery) — the abuse-case ("As Mallory, I forge a tenant id") is closed and observable.
- **DoS bound preserved (Availability)**: the ADR-0010 concurrency permit is acquired *before* auth, so a flood of tokenless requests is shed by backpressure before spending signature verifications.
- **Performance — auth is on the hot path but cheap**: `validate` is **I/O-free** (the `DecodingKey` is pre-computed at construction, `validator.rs:128-129,162-169`; the catalogue lookup is O(1) `HashSet`, `catalogue.rs:58`). The per-request cost is a single sub-microsecond HMAC-SHA256 verify plus a hash lookup — negligible against the existing protobuf decode + OTLP-conformance validate already on the path. A perf KPI on authenticated throughput can be added at DISTILL/DELIVER if Priya's fleet volume warrants; it is not load-bearing for this DESIGN.
- **STRIDE residual**: token replay within `exp` window and secret-rotation are aegis-v0-level accepted risks (HS256 pre-shared key; rotation is operational, not in this feature). Read-path auth and role-gating are explicitly deferred (DD6). No new attack surface is created on aperture beyond the (now-closed) ingest door.

## Test seam (for DISTILL)

The auth boundary is driven **end-to-end through the real aperture binary** — the driving ports are the gRPC `authorization` metadata, the HTTP `Authorization` header, and the running binary's config. Mirror the existing slice harness (`slice_02` HTTP, `slice_07` config, `tests/common`):

- **Mint test tokens in-suite**: a tiny test helper signs an HS256 JWT with the *same* secret the test config's `secret_file` points at, for a tenant present in the test `catalogue_path`, with `iss`/`aud` matching the test config and a future `exp`. (jsonwebtoken `encode` is already a workspace dep via aegis; the test fixture re-uses it.) Variants for the negative controls: no token, empty `Bearer `, `Bearer not-a-jwt` (malformed), past `exp` (expired), bad signature (sign with a different key), wrong `iss`, wrong `aud` (`kaleidoscope-query`), `tenant_id` not in catalogue (unknown_tenant), role `auditor` (unknown_role).
- **Accept assertion**: present a valid token over gRPC metadata / HTTP header → assert the accept response shape is byte-identical to the pre-auth accept AND the recording sink's drained record carries the expected `tenant_id` (the `TenantScoped` tenant).
- **Reject assertion**: present each negative-control token → assert the gRPC `Status::unauthenticated` / HTTP `401 + WWW-Authenticate: Bearer` with the matching `reason`, AND the sink is empty (nothing stored), AND exactly one deny audit line with that `reason` (captured via the existing stderr-capture seam) — one event, never zero, never duplicated.
- **Fail-closed config assertion** (mirrors `slice_07`): a config with no `jwt` table → `Config::from_toml_str` returns `Err`, exit-2 path, `event=config_validation_failed` names the missing auth config, no listener binds; an unreadable `secret_file` → same refusal naming the path, **no secret bytes** in the error.
- **Non-regression**: the existing `invariant_single_validator` test stays green (auth adds no harness `validate_*` call site); existing `slice_0*` tests stay green once they supply a valid token + auth config.

## Public-API / semver posture

`aperture` and `aegis` are **not** in the Gate 2/3 public-API-surface set (they are the gateway binary + an internal library; the Gate 1 public-API crates are the harness and the SDK-class crates). The aperture changes are crate-internal: `Config` fields are `pub(crate)`, `ingest_*` and `SinkRecord` are `pub` but only aperture constructs them (no external consumer), the transport handlers are private. The `ingest_*` signature change and the `SinkRecord` payload change are **breaking to in-crate callers only** and **additive-in-spirit** (every record gains a guaranteed tenant). `aegis` is **unchanged** by this feature (reused verbatim; the DD7 doc-fix is adjacent). **Semver**: pre-1.0 for both crates; the aperture change is a minor/patch-level internal evolution under 0.x. **NEVER bump any crate to 1.0.0** (Andrea's call; CLAUDE.md / MEMORY).

## Alternatives Considered

### Option A — Auth as a separate tower/tonic middleware layer, off the handler path

Implement auth as a `tower::Layer` (HTTP) and a tonic interceptor (gRPC) that validates the token before the request reaches the handler.

**Pros**: textbook separation; handlers stay auth-agnostic; the layer is reusable for the read-path later.
**Cons**: the validated `tenant_id` must then be smuggled from the layer into the handler via request extensions, which is *less* type-safe than threading it as a parameter and makes "every record is tenant-tagged" a runtime convention rather than a type guarantee (DD3's whole point). The tonic interceptor sees the *decoded* request only after metadata, but plumbing the tenant back out to `ingest_*` still needs an extension hop. It also splits the audit `subject` decision (the layer doesn't know the signal cleanly for gRPC without inspecting the path). **Rejected**: the handler-path step gives a type-level tenant guarantee and a clean per-signal `subject` with strictly less indirection; the middleware's reusability win is real but premature (read-path auth is a separate feature with its own shape).

### Option B — Trust an upstream proxy / sidecar to authenticate; aperture reads a trusted `X-Tenant-Id` header

Terminate auth at an Envoy/mesh sidecar and have aperture trust a header it injects.

**Pros**: zero crypto in aperture; offloads token handling.
**Cons**: this is *exactly the current vulnerability* re-introduced — aperture would trust a caller-supplied (or proxy-supplied) tenant header whose provenance it cannot verify. It requires a hard network guarantee (no path to aperture except through the trusted proxy) that the deployment does not promise, and it contradicts the fail-closed / Earned-Trust posture (aperture would be trusting an unprobed upstream). It also strands aegis — the correct, tested validator — unused. **Rejected**: re-creates the spoofing hole the feature exists to close; violates "never trust an unprobed dependency".

### Option C — Per-signal independent auth config (separate enable + secret per logs/traces/metrics)

Let each signal have its own auth toggle/secret.

**Pros**: theoretical granularity.
**Cons**: no user story wants per-signal auth; it multiplies the secret-handling surface (more files, more leak vectors), and "logs authenticated but metrics open" is an open gateway by another name. The whole-surface scope (3×2) is the requirement. **Rejected**: needless complexity, more attack surface, contradicts the "no open door" certification goal.

### Option D — Auth gated by an `enabled = false`-by-default flag (opt-in)

A `[aperture.security.auth.jwt] enabled = false` default, on-by-opt-in.

**Pros**: smallest behaviour change; existing configs keep starting.
**Cons**: this is the ADR-0061 trap verbatim — an operator who forgets `enabled = true` silently ships an open multi-tenant gateway, discoverable only by a packet capture / a tokenless curl after every deploy. The entire reason ADR-0061 exists is to refuse this class of silent downgrade. **Rejected**: defaults must be secure; refuse-to-start (DD4) is the honest posture.

### Option E (DD3 sub-alternative) — add a sibling `tenant` parameter to `OtlpSink::accept` instead of putting it on `SinkRecord`

`accept(record, tenant)` rather than tenant-inside-record.

**Pros**: leaves `SinkRecord` untouched.
**Cons**: breaks every `OtlpSink` implementor's signature (a public-trait break), and de-couples the tenant from the record so a future code path could accept a record without a tenant — losing the type-level "every record is tenant-tagged" guarantee. **Rejected**: tenant-inside-record keeps the trait signature stable and makes the tagging guarantee structural.

## Consequences

### Positive
- The ingest gateway's tenant identity is now authenticated at the boundary; Mallory cannot write under a victim tenant; Priya can certify "no unauthenticated write" with a demonstrated tokenless reject + empty sink.
- Reuses the entire aegis validation core and the ADR-0061 fail-closed pattern verbatim — minimal new surface, no new crate, no new task.
- Every accepted record is tenant-tagged by construction (type-level guarantee via `TenantScoped`).
- Denials are legible (8 distinct stable reasons), one audit event per request, no secret/token ever logged.
- Fail-closed default: a misconfigured gateway refuses to start (exit 2) rather than shipping an open door.

### Negative
- **Behaviour break for any caller currently ingesting without a token** — that is the intended security change, called out for release notes. The negative-control (a valid token still ingests exactly as before) bounds the regression to unauthenticated callers.
- **Existing slice/integration tests must now supply a valid token + auth config** to reach the accept path; DELIVER updates the `tests/common` fixture once. Flagged in the feature wave-decisions risk table.
- The brownfield `ingest_*` / `SinkRecord` ripple touches three modules; the non-regression AC (byte-shape-identical accept, existing tests green) is the guard.

### Trade-off ATAM
- **Sensitivity point — Security/Authenticity**: the auth step converts an unauthenticated, spoofable tenant boundary into an HS256-verified, catalogue-checked one.
- **Trade-off point — Security vs Availability**: turning on auth changes *who* can write (max blast radius on a live gateway). Mitigated by thin slicing (WS = one transport/signal), the negative control in every slice, and backpressure-before-auth so the DoS bound is unaffected. The trade is deliberate: a gateway that refuses unauthenticated writes is preferable to one that accepts them silently.
- **Sensitivity point — Security/Confidentiality (the secret)**: secret-by-file-reference + opaque-Debug + path-only errors keep the HS256 key out of every loggable surface.

## Enforcement
- The behaviour is covered by integration tests across the accept path + the 8-reason reject matrix + the fail-closed config refusal, supplying the per-feature 100% mutation kill coverage (CLAUDE.md / ADR-0005 Gate 5) on the new auth-extraction and config-validation branches (modified aperture files only).
- The `single-validator-per-signal` `xtask` AST walk continues to enforce one harness `validate_*` per signal; the aegis `validate` call (a different symbol, in the transport layer) does not trip it.
- `cargo deny` enforces the non-wildcard aegis path dependency.
- No new architectural-style rule is introduced; the refusal is one more invariant in the existing `RawConfig::into_config` validator (alongside identical-bind-address and the ADR-0061 knobs), and the secret-never-stored discipline is structural (the config struct holds `PathBuf`, not bytes).
