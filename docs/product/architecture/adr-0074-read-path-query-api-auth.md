# ADR-0074 — Read-path query-API authentication: wire the aegis JWT validator onto the three live read APIs as an optional, fail-closed per-request bearer path

- **Status**: Accepted
- **Date**: 2026-06-08
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `read-path-query-api-auth-v0`
- **Mode of operation**: PROPOSE (Decision 0 scope = APPLICATION; Decision 1 = PROPOSE)
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0068 (`aegis-ingest-auth-v0`, the ingest auth this read path mirrors on every axis except audience; DD6 carved out read-path auth as this follow-up feature), ADR-0054 (the `query-http-common` shared seam: the rule-of-three that earned `resolve_tenant_or_refuse`/`error_response` into one crate), ADR-0061 (`tls-config-reject-v0`, fail-closed refuse-to-start precedent), ADR-0008 (config schema + `deny_unknown_fields`), ADR-0053 (trace lookup-by-id, which must also be isolated), aegis-v0 ADRs (the HS256 `Validator`).

> **MODEL FORK — ANDREA MAY VETO (flag carried forward from DISCUSS, verbatim intent).**
> This ADR is designed to the **ADDITIVE** model, DECIDED by Luna in DISCUSS and proceeded on per decide-don't-ask:
> **preserve** today's per-instance env-tenant default (`KALEIDOSCOPE_*_QUERY_TENANT`), **add** an OPTIONAL per-request bearer path that, when an auth config is present, scopes the query to the validated token's tenant.
> The veto target is **per-request-only** (mandatory bearer, no env fallback). The additive choice **forecloses nothing**: every fail-closed and isolation decision here holds identically under per-request-only; only DD3's precedence arm (3) and the backward-compat story (US-RAUTH-02) reshape on a veto, and the bearer-validation work is not wasted. **To veto**: tell Luna/Morgan to switch to per-request-only; the change is localised to the config-validation rule (require auth config; refuse a request with no resolvable per-request tenant even when an env tenant is set). Recorded here so the fork stays visible at the architecture layer, not just in the DISCUSS notes.

## Context

The three live READ query APIs resolve tenant **per-instance** today, not per-caller. Verified by reading the source on 2026-06-08:

- `query-api` (metrics / Pulse, `:9090`, `GET /api/v1/query_range`), `log-query-api` (logs / Lumen, `:9091`, `GET /api/v1/logs`), and `trace-query-api` (traces / Ray, `:9092`, `GET /api/v1/traces` + trace lookup-by-id) each have a `composition.rs::resolve_tenant(env_value: Option<String>) -> Option<TenantId>` (`crates/query-api/src/composition.rs:54`, `crates/log-query-api/src/composition.rs:54`, `crates/trace-query-api/src/composition.rs:58`) that maps one env var to `Some(t)` when set/non-empty, else `None`. That single `Option<TenantId>` is baked into the router state (`ApiState { store, tenant }`, `query-api/src/lib.rs:81-85`) for **every** request the process serves.
- The shared fail-closed tenant seam already exists and already returns 401 **before the store is touched**: `query_http_common::resolve_tenant_or_refuse(tenant: &Option<TenantId>, service_label) -> Result<&TenantId, Response>` (`crates/query-http-common/src/lib.rs:240-252`). Each handler calls it first (`query-api/src/lib.rs:158-161`; the trace handlers at `trace-query-api/src/lib.rs:131,241`). `Some` → `Ok(t)`; `None` → `Err(401)` with the per-pillar `"no tenant resolvable: <label> service refuses unscoped requests"` reason. **This is the seam the per-request bearer path plugs into.**
- The three read crates do **not** depend on aegis's `Validator` and read **no** `Authorization` header today (grep across the three crates: zero `Authorization`/`authorization` matches; they import only `aegis::TenantId`). The bearer extraction + validation is genuinely new code. `query-http-common` already depends on aegis for `TenantId`, so adding the validator wire is an extend of an existing dependency edge, not a new one.
- The correct lock to fit is **already built and tested**: `aegis::Validator::validate_with_subject(token, now, subject) -> Result<TenantContext, ValidationError>` (`crates/aegis/src/validator.rs:180-210`) is real HS256 (jsonwebtoken), alg-confusion-safe (`Algorithm::HS256` pinned, `:220`), fail-closed on `exp` (`:234-241`), exact issuer + audience equality (`:228-233`), unknown-tenant rejected against a TOML catalogue (`:246-248`), 8 typed `ValidationError` variants each with a stable `reason()` (`:96-107`), and emits **exactly one** structured `tracing` audit event per call (`:186-209`). The signing key is opaque-Debugged (`:149-158`).
- The ingest door (ADR-0068) is the shape to mirror. The **one deliberate divergence** is the audience: ingest tokens carry `aud=kaleidoscope-ingest`; read tokens carry `aud=kaleidoscope-query` (ADR-0068 already named `kaleidoscope-query` as the read audience). The two audiences are the cross-surface fence.

The job: close the read-side asymmetry. After the ingest auth landed per-request, the read APIs are still authenticated per-DEPLOYMENT (one tenant per process), not per-CALLER. Priya (platform-security operator) cannot certify per-tenant read isolation at the request boundary. This feature wires `aegis::Validator` onto the read path as an OPTIONAL per-request bearer step, fail-closed, while preserving the env-tenant default unchanged.

### What DESIGN must lock (this ADR resolves DD1–DD6)

1. **DD1** — read-API auth config wiring for the HS256 validator, secret never logged.
2. **DD2** — bearer extraction on the HTTP read transport + exact 401 reject mapping.
3. **DD3** — the additive per-request tenant resolution precedence (the no-bearer-bypass property).
4. **DD4** — fail-closed posture: unresolved tenant refused before the store.
5. **DD5** — audit / observability of read-path denials.
6. **DD6** — scope fence + the cross-surface audience fence + the role question.

## Decision

Add the per-request bearer-validation + tenant-resolution capability **ONCE, in `query-http-common`**, as the per-request analogue of the existing `resolve_tenant_or_refuse` seam, reusing `aegis::Validator` verbatim. Wire an optional `Arc<aegis::Validator>` (plus the env tenant) into each of the three read routers; thread it to each handler. On each read request, when auth is configured, extract the bearer from `Authorization`, validate it (audience `kaleidoscope-query`), and resolve the tenant to the token's `TenantContext.tenant_id`; on any failure return 401 with the aegis reason, **before the store**; on success scope the existing tenant-scoped store query to that tenant. When auth is **not** configured, the existing env-tenant path is byte-for-byte unchanged. A missing/invalid bearer when auth is configured **never** downgrades to the env tenant.

### DD1 — read-API auth config wiring; the secret is never logged

Each read binary's composition root gains an **optional** read-auth config, resolved at startup, mirroring ADR-0068 DD1's secret-by-reference and never-logged invariant. The config shape (the three APIs share one shape, each keyed off its own env prefix so they remain independently deployable):

| Field | Source | Meaning |
|---|---|---|
| `issuer` | `KALEIDOSCOPE_<API>_QUERY_AUTH_ISSUER` | required when auth is on; exact-match issuer |
| `audience` | `KALEIDOSCOPE_<API>_QUERY_AUTH_AUDIENCE` | required when auth is on; **`kaleidoscope-query`** is the read audience (DD6) |
| `secret_file` | `KALEIDOSCOPE_<API>_QUERY_AUTH_SECRET_FILE` | required when auth is on; **PATH** to the HS256 secret bytes, never the bytes inline |
| `catalogue_path` | `KALEIDOSCOPE_<API>_QUERY_AUTH_CATALOGUE` | required when auth is on; path to the aegis tenant catalogue |

`<API>` is `` (metrics, e.g. `KALEIDOSCOPE_QUERY_AUTH_*`), `LOG_` (`KALEIDOSCOPE_LOG_QUERY_AUTH_*`), `TRACE_` (`KALEIDOSCOPE_TRACE_QUERY_AUTH_*`), matching each binary's existing env prefix (`KALEIDOSCOPE_QUERY_*` / `_LOG_QUERY_*` / `_TRACE_QUERY_*`). **Auth is OPTIONAL (additive model):** auth is "configured" iff the read-auth config is present and complete. A *partial* auth config (some but not all required fields set) is a **refuse-to-start config error** (mirroring ADR-0061/ADR-0068 DD4): half-configured auth is the silent-downgrade trap, so it fails closed at startup with an `event=config_validation_failed`-style refusal that names the missing field, never a secret. A *wholly absent* auth config is the additive opt-out: env-tenant mode, unchanged (DD3 arm 3).

**Never-logged invariant — structural, not by discipline (verbatim from ADR-0068 DD1):**
- The composition layer holds `secret_file: PathBuf`, never the secret bytes. The bytes are read into a local `Vec<u8>`, moved into `aegis::ValidatorConfig { hs256_key }`, and live only inside the `aegis::Validator` (whose `Debug` renders the key `"<opaque>"`). Nothing secret is stored on the router state, so nothing secret can leak through a derived `Debug`.
- Config-validation errors name the `secret_file` **by path only** (e.g. `secret_file "/etc/…/hs256.key" is unreadable: <io-error-kind>`).
- The deny/allow audit events (DD5) carry only `tenant_id`/`role`/`decision`/`subject`/`reason` — never the token, never the secret. The 401 reason carries the aegis `reason()` class name, never the raw token (the `query-http-common` reason-redaction discipline, `lib.rs:403-416`, extends to the bearer-derived 401s).

The validator is constructed **once** at composition: `aegis::load_catalogue(catalogue_path)` → `aegis::Validator::new(ValidatorConfig { issuer, audience, hs256_key, catalogue })`, wrapped in an `Arc<Validator>`, and threaded into the router as `Option<Arc<Validator>>` (`Some` when auth configured, `None` when not).

### DD2 — bearer extraction on the read transport + exact reject mapping

The read APIs are HTTP-only (axum). The JWT arrives in the `Authorization` request header as `Bearer <jwt>`. The shared capability in `query-http-common` (DD3) extracts and maps. Extraction rules mirror ADR-0068 DD2:

- no `Authorization` header, or empty value, or a value not matching `Bearer <token>` with a non-empty token → treated as **reason `missing_claim`**, decided at the extraction boundary before `validate` is called (so the cheap path stays cheap and the reason is stable). No call into aegis with an empty string.
- a well-formed `Bearer <jwt>` → `validator.validate_with_subject(jwt, SystemTime::now(), subject)` where `subject` is `query_range` / `log_query` / `trace_query`.

On reject, return **HTTP 401 Unauthorized** with a `WWW-Authenticate: Bearer` challenge (RFC 6750), the aegis `reason()` string carried in the response, **reusing the existing `query-http-common` `error_response`/`ErrorBody` envelope** (`{"status":"error","error":"<aegis reason>"}`) for the body, with the `WWW-Authenticate` header added. The reject leaks neither the secret nor the raw token. (The exact `WWW-Authenticate` parameterisation — bare `Bearer` for the no-token case, `Bearer error="invalid_token", error_description="<reason>"` for a present-but-invalid token, per RFC 6750 §3 — matches ADR-0068's HTTP shape.)

### DD3 — per-request tenant resolution precedence (the additive model's core; the no-bearer-bypass property)

The new capability in `query-http-common` is the per-request analogue of `resolve_tenant_or_refuse`. Recommended signature (HOW is the crafter's, but the contract is pinned here):

```
resolve_request_tenant_or_refuse(
    auth: Option<&Arc<aegis::Validator>>,   // Some => auth configured; None => env-only
    headers: &HeaderMap,                     // to read Authorization
    env_tenant: &Option<TenantId>,           // the existing per-instance env tenant
    service_label: &'static str,             // "the query" / "the log query" / "the trace query"
    subject: &'static str,                   // "query_range" / "log_query" / "trace_query"
    now: SystemTime,
) -> Result<TenantId, Response>              // Ok(resolved tenant) | Err(401 fail-closed Response)
```

**The additive precedence (load-bearing security property — the no-bearer-bypass):**

1. **auth configured (`Some`) AND a valid bearer** → tenant = the token's `TenantContext.tenant_id`. Isolation: this tenant scopes the query and reads only its own data.
2. **auth configured (`Some`) AND a missing / malformed / invalid bearer** → **fail-closed 401** with the aegis reason, **before the store**. **The env tenant is NOT consulted in this arm.** Once auth is configured, the bearer is the sole authority; omitting or forging the header can never silently downgrade to the env tenant. *This is the unmissable property:* arm 2 must not read `env_tenant` at all — a code path that falls through from a failed validation to the env tenant is a bearer-bypass (R3) and is forbidden by construction. The function returns the 401 `Response` directly from the validation-failure branch; there is no `else env_tenant` after it.
3. **auth NOT configured (`None`)** → today's per-instance env tenant via the existing seam: `resolve_tenant_or_refuse(env_tenant, service_label)`. `Some(t)` → `Ok(t)`; `None` (unset/empty) → the existing 401 `"no tenant resolvable"`. The `Authorization` header is **ignored** in this arm (backward compatibility, US-RAUTH-02).

The two refusal arms (2 and 3) both produce a 401 `Response` and both happen **before any store access** — they reuse the same fail-closed seam discipline (DD4). The resolved `TenantId` is consumed by the **existing** tenant-scoped store query **identically** whether it came from the env or from the bearer: the handler already does `state.store.query(&tenant, …)` (`query-api/src/lib.rs:197`) / `get_trace(&tenant, …)` — the only change is *where `tenant` comes from*, not how it scopes. This is why tenant isolation (positive + negative control) holds for free: the store already scopes by `&TenantId`; the feature only authenticates the provenance of that `TenantId`.

> **VETO NOTE**: on a per-request-only veto, precedence collapses to arms (1) and (2) only; arm (3) is removed; `auth` is always `Some`; a process with no auth config refuses to start. DD3 is the one decision the veto reshapes (plus US-RAUTH-02's shape).

### DD4 — fail-closed posture: unresolved tenant refused BEFORE the store

A request whose tenant cannot be resolved — invalid/missing bearer when auth is on (arm 2); unset env tenant when auth is off (arm 3) — is refused with a 401 **before the backing store is queried**, reusing the existing `query-http-common` fail-closed seam. No partial reads, no store touch on an unresolved tenant. The project's standard isolation negative control is mandatory: prove the data IS returned for the right tenant and ABSENT (empty / forbidden) for the wrong one, on each of metrics, logs, traces, and the trace lookup-by-id path (ADR-0053).

**Earned-Trust note (principle 12).** The existing composition `probe()` (wire → probe → use) is preserved unchanged for the env-tenant store-readability check. When auth is configured, the composition root additionally proves it can honour the auth contract before binding the listener: at startup it reads the `secret_file` and `catalogue_path`, constructs the `Validator`, and a *negative* startup probe asserts that a deliberately-invalid token (e.g. wrong signature) is **rejected** — i.e. the lock actually rejects, not merely that it was constructed. A config that builds a validator which cannot reject is refused with `event=health.startup.refused` rather than binding an open-looking-but-broken auth door. This is the "wire then probe then use" invariant applied to the auth dependency, mirroring how the store probe proves readability before serving. (Crafter owns the probe body; the contract is: auth-on startup demonstrates a known-bad token rejects before any socket binds.)

### DD5 — audit / observability of read-path denials

One structured decision event per read request, reusing aegis's `reason()` taxonomy and the locked field contract (`tenant_id`/`role`/`decision`/`subject`/`reason`), with `subject` = `query_range` / `log_query` / `trace_query`. **Aegis already emits exactly one event per `validate_with_subject` call** (`info!` on allow, `warn!` on deny), so the read API relies on aegis's event for every validate-reached request — no double-logging, no zero-logging. The **one gap** mirrors ADR-0068 DD5: the no-token / malformed-bearer case is decided at the extraction boundary *before* `validate`, so aegis sees nothing; for that single case the shared capability emits **one** decision event itself in the same field shape (`decision=deny reason=missing_claim subject=<read action>`). Result: exactly one decision event per request across all paths — validate-reached requests get aegis's event; pre-validate rejects get the one shared-capability event; never both, never neither. No secret, no raw token in any field. Rides the read tier's existing `query_http_common::init_tracing` JSON-stderr subscriber.

### DD6 — scope fence + the cross-surface audience fence + role question

- **Scope**: the three READ query APIs only, HTTP only. The ingest path (ADR-0068) is OUT. SPIFFE / RS256 / JWKS / OPA are aegis v1, OUT.
- **Audience fence**: read tokens carry `aud=kaleidoscope-query`, configured into each read `Validator`. A token minted for `kaleidoscope-ingest` MUST reject `wrong_audience` on the read path (and a read token rejects on ingest, already enforced by ADR-0068's ingest validator configured with `kaleidoscope-ingest`). This is the SAME `aegis::Validator` exact-audience check (`validator.rs:228-233`) configured with `kaleidoscope-query`; no new code, a config value. This is the boundary that stops an ingest token from reading and a read token from writing.
- **Role question — RESOLVED, deferred (mirror ADR-0068 DD6)**: v0 read auth is **authentication + tenant-scoping only**. Any valid token for a catalogued tenant (role `viewer` OR `operator`) may read. aegis still rejects `unknown_role` (a role that is neither) free. Whether a future feature restricts reads to a specific role is **deferred with the decision recorded**: the minimum fail-closed property is authentication + tenant isolation, not role-gated read authorization. The `TenantContext.role` is already available to the handler, so a future role gate needs no re-plumbing (one `if ctx.role != … { reject }`). US-RAUTH-04 AC "DD6 role resolved" is satisfied by this explicit deferral with rationale.

## Reuse Analysis (MANDATORY)

| Capability | Verdict | Where / How |
|---|---|---|
| HS256 JWT validation (sig/exp/iss/aud/tenant/role) | **REUSE verbatim** | `aegis::Validator::validate_with_subject` (`validator.rs:180-210`). No crypto change. |
| Typed success context | **REUSE verbatim** | `aegis::TenantContext { tenant_id, role }`, `aegis::TenantId`, `aegis::Role` (`validator.rs:33,43,66`). |
| Typed failure taxonomy + 8 stable reasons | **REUSE verbatim** | `aegis::ValidationError` + `reason()` (`validator.rs:74-108`); the audience fence is the SAME exact-aud check (`:228-233`). |
| Validator construction | **REUSE verbatim** | `aegis::Validator::new(ValidatorConfig { issuer, audience, hs256_key, catalogue })` (`validator.rs:162`). |
| Tenant catalogue load | **REUSE verbatim** | `aegis::load_catalogue` / `TenantCatalogue` (`catalogue.rs:111`, re-exported `aegis/src/lib.rs:52`). |
| One-audit-event-per-call + field contract | **REUSE verbatim** | aegis emits one `info!`/`warn!` with `tenant_id`/`role`/`decision`/`subject`/`reason` (`validator.rs:186-209`); the read API supplies `subject`. |
| Opaque-Debug for the signing key | **REUSE verbatim** | `aegis::Validator`'s hand-written `Debug` prints `key="<opaque>"` (`validator.rs:149-158`); the read APIs store `secret_file: PathBuf`, never bytes. |
| Fail-closed tenant seam (refuse 401 before store) | **REUSE verbatim** | `query_http_common::resolve_tenant_or_refuse` (`lib.rs:240-252`) is the env-tenant arm (DD3 arm 3) AND the discipline the new capability mirrors. |
| JSON error envelope | **REUSE verbatim** | `query_http_common::error_response` / `ErrorBody` (`lib.rs:269-275`) builds the 401 body; the new path adds only the `WWW-Authenticate` header. |
| Reason-redaction discipline | **REUSE verbatim** | `query-http-common`'s "reason never carries a credential marker" tests (`lib.rs:403-416`); the bearer-derived 401s inherit it (reason = aegis class name, never the token). |
| Read-tier audit subscriber | **REUSE verbatim** | `query_http_common::init_tracing` (`lib.rs:318`) is the stderr JSON sink the aegis events already ride. |
| Existing tenant-scoped store query | **REUSE verbatim** | `MetricStore::query(&tenant, …)` / `LogStore::query(&tenant, …)` / `TraceStore::query`+`get_trace(&tenant, …)` already scope by `&TenantId` (F1) — the resolved tenant is consumed identically whether env- or bearer-derived. **No store change. pulse/lumen/ray UNTOUCHED.** |
| Composition `resolve_tenant` / `probe` (env path) | **REUSE verbatim** | the three `composition.rs` env-tenant + Earned-Trust probe functions stay (US-RAUTH-02 backward compat); arm 3 is byte-for-byte today. |
| `query-http-common` (the shared crate) | **EXTEND** | add the per-request `resolve_request_tenant_or_refuse` capability + the bearer extraction + the pre-validate `missing_claim` event. Already depends on aegis for `TenantId`; the `Validator` import is on the same edge. **The auth logic lands here ONCE.** |
| The three read APIs (`query-api`/`log-query-api`/`trace-query-api`) | **EXTEND** | each router gains `Option<Arc<Validator>>` in its `ApiState`; each handler swaps its `resolve_tenant_or_refuse` call for the new `resolve_request_tenant_or_refuse`; each `composition.rs` resolves the optional read-auth config + the auth startup probe (DD4). Thin wiring over the shared capability — no per-crate auth logic. |
| Read-auth config fields (issuer/audience/secret_file/catalogue) | **CREATE** | the four read-auth env-backed fields do not exist on any read binary (verified: the read composition roots read only tenant/addr/pillar_root/static_dir). **Justified**: F3 — no read handler reads `Authorization` and no read composition carries an HS256/issuer/audience/catalogue field today; this is the genuinely new config surface. Mirrors ADR-0068 DD1 shape. |
| Bearer extraction + the pre-validate `missing_claim` reason | **CREATE** | inside `query-http-common` as part of the shared capability. **Justified**: F3 — zero `Authorization` reads in the read tier today; this is the new auth-extraction boundary. Created ONCE in the shared crate, not per API. |

**Net**: REUSE the entire aegis validation core, the `query-http-common` fail-closed seam + envelope + redaction + subscriber, and the existing tenant-scoped store queries **verbatim**; EXTEND `query-http-common` with the per-request capability (landed once) and the three read APIs with thin wiring; CREATE only the four read-auth config fields and the bearer-extraction boundary, both justified by the verified zero-`Authorization` fact (F3). **No new crate, no new dependency edge, no store change, no duplicated validator. The stores (pulse/lumen/ray) and the env-tenant path are untouched.**

## Security posture (load-bearing)

- **Fail-closed by construction (Fail-Secure)**: auth-on + missing/invalid bearer → 401 before the store, env tenant NOT consulted (DD3 arm 2, the no-bearer-bypass). Auth-on + unreadable secret/catalogue or partial config → refuse-to-start. Auth-off + unset env tenant → existing 401. There is never a silent default-tenant accept when auth is on.
- **No bearer-bypass (the load-bearing property, R3)**: once auth is configured, the env tenant is unreachable from the request path. The shared capability returns the 401 directly from the validation-failure branch with no fall-through to `env_tenant`. A startup negative probe (DD4) proves the configured validator actually rejects a known-bad token.
- **Complete mediation (A01)**: every read request on every API is resolved through the one shared seam; no cached decision; the resolution happens before any store access.
- **Tenant isolation (the north star, R4)**: the resolved `TenantId` scopes the existing tenant-scoped store query; positive control (right tenant's data present) + negative control (wrong tenant's data ABSENT, including trace lookup-by-id). The store already scopes by `&TenantId`; the feature authenticates the provenance of that id.
- **Cross-surface fence (R6)**: read `aud=kaleidoscope-query`; an ingest-audience token rejects `wrong_audience` on read (and vice versa). Same exact-aud check, a config value.
- **Secret never logged (Info-Disclosure, R2)**: secret-by-file-reference; composition holds `PathBuf`, never bytes; aegis opaque-Debugs the key; errors name the file by path; audit/deny lines carry no token and no secret. Structural, not by vigilance.
- **Legible denials / non-repudiation**: 8 distinct stable reasons let Priya triage `expired` (refresh) from `invalid_signature` (forgery) and tally `wrong_audience` (cross-surface replay attempts).
- **Backward compat preserved (R5)**: auth-off is byte-for-byte today; existing read-API slice tests stay green; the `composition.rs` env path and `probe` are untouched.
- **Performance — auth is on the hot path but cheap**: `validate` is I/O-free (the `DecodingKey` is pre-computed at construction; the catalogue lookup is O(1) `HashSet`). A single sub-microsecond HMAC-SHA256 verify + a hash lookup per request, negligible against the existing store query. Auth-off adds nothing (the header is not even read).

## Test seam (for DISTILL)

Driven **end-to-end through the three real read-API binaries** — the driving ports are the `Authorization: Bearer <jwt>` HTTP header on `GET /api/v1/query_range`, `GET /api/v1/logs`, `GET /api/v1/traces` + the trace lookup-by-id path, and each binary's read-auth config. Mirror ADR-0068's test seam:

- **Mint test tokens in-suite**: a helper signs an HS256 JWT with the same secret the test config's `secret_file` points at, audience `kaleidoscope-query`, for a tenant in the test catalogue, future `exp`. Negative-control variants: no token, empty `Bearer `, `Bearer not-a-jwt` (malformed), past `exp` (expired), bad signature, wrong `iss`, **`aud=kaleidoscope-ingest`** (the cross-surface fence → `wrong_audience`), tenant not in catalogue (`unknown_tenant`), role `auditor` (`unknown_role`).
- **Accept + isolation**: a valid `acme-prod` token → response shape byte-identical to today's, scoped to `acme-prod` (positive control); a valid `globex-staging` token on the same query → `acme-prod`'s data ABSENT (negative control), on metrics, logs, traces, and trace lookup-by-id.
- **Fail-closed + no-bypass**: each negative-control token → 401 + `WWW-Authenticate: Bearer` with the matching `reason`, the store never queried, exactly one deny audit line; auth-on + no token with an env tenant ALSO set → 401, NOT scoped to the env tenant (the no-bearer-bypass assertion).
- **Backward compat**: auth-off + env tenant → today's behaviour, `Authorization` ignored; auth-off + unset env tenant → existing 401; existing read-API slice tests stay green.
- **Redaction**: no secret bytes and no raw token in any 401 body, error, log line, or audit event.
- **Fail-closed config**: partial auth config / unreadable secret → refuse-to-start naming the field by path, no secret bytes in the error; the auth startup negative probe rejects a known-bad token before bind.

## Public-API / semver posture

`query-http-common`, the three read APIs, and `aegis` are workspace-internal crates (not the Gate 2/3 public-API set). The new `query-http-common` function is additive `pub` surface; the router signature change (`Option<Arc<Validator>>` in state) is breaking to in-crate callers only (the three binaries, updated in lockstep). `aegis` is **unchanged** (reused verbatim). **Semver**: pre-1.0 for all; minor/patch-level internal evolution under 0.x. **NEVER bump any crate to 1.0.0** (Andrea's call; CLAUDE.md / MEMORY).

## Alternatives Considered

### Option A — Per-request-only (mandatory bearer, no env fallback) — THE VETO TARGET

Drop the env-tenant default; require an auth config; refuse a request with no resolvable per-request tenant even when an env tenant is set. Mirrors the ingest path's "no token, no ingest" exactly.

**Pros**: a stricter, more uniform multi-tenant posture; one resolution model (bearer only); no additive-precedence subtlety to get wrong.
**Cons**: a **breaking change for every existing single-tenant env-tenant deployment** — they would refuse to start until reconfigured, a fleet-wide migration. The additive model delivers the identical fail-closed + isolation security properties (arms 1 and 2 are unchanged) while leaving legacy deployments byte-for-byte intact, and it **forecloses nothing**: Andrea can later flip to mandatory by changing only the config-validation rule, with zero wasted bearer work. **Rejected for v0** as the higher-regret default — but recorded as the explicit veto target (the MODEL FORK flag), localised to DD3 arm (3) + US-RAUTH-02. The choice is Andrea's; the additive model is the low-regret proceed.

### Option B — Per-API auth logic (wire the validator into each read crate independently)

Implement bearer extraction + validation + the precedence inside each of `query-api`, `log-query-api`, `trace-query-api` separately.

**Pros**: each crate is self-contained; no shared-crate change.
**Cons**: triplicates the auth logic across three crates — the exact duplication ADR-0054 earned out of the tenant seam. A reason-text or precedence change becomes three edits; the mutation kill-rate is diluted across three copies; the no-bearer-bypass property would have to be re-proven three times. The three handlers **already** route tenant resolution through `query-http-common` (F2), so the shared placement is the minimal ripple. **Rejected**: the capability lands once in `query-http-common` (R7), wired through three thin handlers.

### Option C — A tower middleware layer that validates and injects the tenant via request extensions

Validate in a `tower::Layer`, stash the `TenantId` in request extensions, read it in the handler.

**Pros**: textbook separation; handlers stay auth-agnostic; reusable.
**Cons**: the validated tenant becomes a runtime extension convention rather than a value threaded through the one fail-closed seam the handlers already call — it splits the resolution across a layer and the handler, makes "every read is tenant-scoped through the same seam" a runtime invariant instead of a single call site, and complicates the env-tenant fallback (the layer would need to know whether auth is configured and replicate arm 3). The handler-side shared function keeps all three precedence arms in ONE place, reuses the existing `resolve_tenant_or_refuse` for arm 3 verbatim, and keeps the no-bearer-bypass property a single auditable branch. **Rejected**: the shared-seam function gives one auditable resolution point with strictly less indirection; the middleware's reusability is premature for three sibling HTTP handlers that already share a seam.

### Option D — Auth gated by an `enabled = false`-by-default flag

An explicit on-by-opt-in flag.

**Pros**: smallest behaviour change.
**Cons**: redundant under the additive model — "auth not configured" *is* the opt-out, with no flag to forget. A separate `enabled=false` flag would let an operator set the auth fields yet leave auth off (the half-configured silent-downgrade trap ADR-0061/ADR-0068 closed). The additive model makes "auth on" mean "auth config present and complete", and a **partial** config refuses to start rather than silently running open. **Rejected**: no flag; presence-of-complete-config is the switch, partial-config fails closed.

## Consequences

### Positive
- The read surface gains per-request tenant authentication + isolation at the boundary; Priya can certify "no caller reads another tenant's data" with a demonstrated positive + negative control across metrics, logs, traces, and trace lookup-by-id.
- Reuses the entire aegis core, the `query-http-common` seam/envelope/redaction/subscriber, and the existing tenant-scoped store queries verbatim — the capability lands once, wired through three thin handlers; no new crate, no new dependency edge, no store change.
- The no-bearer-bypass property is a single auditable branch in one shared function; the cross-surface audience fence is a config value on the same validator.
- Backward compatible: auth-off deployments are byte-for-byte today; existing slice tests stay green; the env path and probe are untouched.
- Denials are legible (8 distinct reasons), one audit event per request, no secret/token ever logged.

### Negative
- **Behaviour change for callers querying an auth-configured read API without a token** — that is the intended security change (release notes). The negative control (a valid token reads exactly as before) bounds the regression to unauthenticated callers, and only when auth is configured.
- The router signature change (`Option<Arc<Validator>>` in state) touches the three binaries' composition + router + one handler call each; the non-regression guard is the unchanged response shape + the green existing slice tests.
- A partial auth config now refuses to start (a stricter posture than today, where there was no auth config at all) — deliberate, to close the half-configured silent-downgrade trap.

### Trade-off ATAM
- **Sensitivity point — Security/Authenticity**: the read boundary converts an unauthenticated, per-deployment tenant into an HS256-verified, catalogue-checked per-request tenant.
- **Trade-off point — Security vs Backward-compatibility**: the additive model trades the stricter per-request-only posture for zero-regression legacy deployments; the trade is reversible (the veto path) and forecloses nothing.
- **Sensitivity point — Security/Confidentiality (the secret)**: secret-by-file-reference + opaque-Debug + path-only errors keep the HS256 key off every loggable surface.

## Enforcement
- The behaviour is covered by integration tests across the accept + isolation path, the 8-reason reject matrix (incl. the `wrong_audience` cross-surface fence), the no-bearer-bypass assertion, the backward-compat suite, and the fail-closed config refusal — supplying the per-feature 100% mutation kill coverage (CLAUDE.md / ADR-0005 Gate 5) on the new shared capability + the read-auth config validation (modified files: `query-http-common` + the three read crates).
- `cargo deny` enforces the non-wildcard aegis path dependency (already present on `query-http-common`).
- No new architectural-style rule: the auth resolution is one more invariant in the existing `query-http-common` fail-closed seam, and the secret-never-stored discipline is structural (composition holds `PathBuf`, not bytes). The auth startup negative probe (DD4) is the Earned-Trust enforcement that the configured validator actually rejects before any socket binds.
