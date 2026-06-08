# DISCUSS Decisions — read-path-query-api-auth-v0

> **Wave**: DISCUSS (nWave). **Analyst**: Luna (`nw-product-owner`).
> **Date**: 2026-06-08. **Feature type**: Backend / Cross-cutting —
> a per-request authentication boundary on the three live READ query
> APIs (`query-api`, `log-query-api`, `trace-query-api`).
> **Origin**: the read-path-auth follow-up explicitly carved out by
> `aegis-ingest-auth-v0` DD6 / ADR-0068 (DD6 scope fence: "Read-path auth
> (query-api / log-query-api / trace-query-api) is a separate future
> feature"). This feature IS that deferred slice — the highest-value
> remaining security item now the ingest door is closed.

## The job (fail-closed framing)

When a client queries the read APIs (a metrics `query_range`, a log
search, a trace lookup), the API resolves WHICH tenant's data the request
may read. Today that resolution is **per-instance**: one process-wide env
var (`KALEIDOSCOPE_QUERY_TENANT` / `_LOG_QUERY_TENANT` /
`_TRACE_QUERY_TENANT`) fixes a single tenant for every request the
process serves. There is **no per-request authentication** — the read
side is authenticated per-deployment, not per-caller, while the ingest
side (after `aegis-ingest-auth-v0`) is authenticated per-request. This
feature closes that asymmetry: it adds an OPTIONAL per-request bearer
path so a request carrying a valid bearer token is scoped to THAT token's
tenant, while preserving the env-tenant default unchanged when no auth is
configured. **A request whose tenant cannot be resolved is refused with
a 401 BEFORE the store is touched** (the existing `query-http-common`
fail-closed seam). A token for tenant A can never read tenant B's data.

## THE MODEL DECISION — ADDITIVE (Andrea-veto FLAGGED)

> **This is a load-bearing model fork that Luna has DECIDED and is
> proceeding on. It is recorded here prominently for Andrea to VETO.
> Both the Verifier and a prior note judged this fork to be Andrea's
> call. Luna proceeds on the additive superset per the standing
> decide-don't-ask + continua instruction because the substantive
> backlog is otherwise exhausted, and the additive choice FORECLOSES
> NOTHING.**

### ANDREA-VETO FLAG

```
+--------------------------------------------------------------------------+
|  MODEL FORK — ANDREA MAY VETO                                            |
|                                                                          |
|  DECIDED: the ADDITIVE model.                                           |
|    - PRESERVE today's per-instance env-tenant default                  |
|      (KALEIDOSCOPE_*_QUERY_TENANT), behaviour unchanged when no auth    |
|      config is present.                                                 |
|    - ADD an OPTIONAL per-request bearer path: when an auth config IS    |
|      present, a request carrying a valid bearer token is scoped to     |
|      THAT token's tenant.                                               |
|                                                                          |
|  ALTERNATIVE (the veto target): per-request-ONLY (mandatory bearer,    |
|    no env fallback) — a stricter multi-tenant posture that mirrors the |
|    INGEST path's "no token means no ingest" exactly.                    |
|                                                                          |
|  WHY ADDITIVE IS LOW-REGRET:                                           |
|    (a) the bearer-validation + per-request-tenant surface is needed    |
|        under EITHER model — none of it is wasted if Andrea later makes |
|        auth mandatory;                                                 |
|    (b) backward-compatible — existing single-tenant deployments        |
|        (env-tenant only) keep working unchanged;                       |
|    (c) it MIRRORS the existing ingest auth for token shape / redaction |
|        / fail-closed-on-invalid, so the platform's auth posture is     |
|        consistent;                                                     |
|    (d) it does NOT foreclose the stricter option — Andrea can later    |
|        flip auth to mandatory and DROP the env default without wasting |
|        the bearer work (one config-validation change, no re-plumbing). |
|                                                                          |
|  TO VETO: tell Luna to switch to per-request-ONLY. The change is       |
|    localized to the config-validation rule (require auth config;       |
|    reject a request with no resolvable per-request tenant even when an |
|    env tenant is set) — US-RAUTH-02's backward-compat scenario inverts |
|    to a refuse-to-start, and the env-default examples drop.            |
+--------------------------------------------------------------------------+
```

The requirement set below is written so the additive model is the default
narrative, BUT every story's fail-closed and isolation AC hold IDENTICALLY
under per-request-only — only US-RAUTH-02 (backward compatibility) changes
shape on a veto. This keeps the veto cheap.

## Verified facts (grounded in code, not the brief)

Confirmed by reading the source on 2026-06-08. Load-bearing premises for
every story and slice.

- **F1 — the three read APIs resolve tenant PER-INSTANCE today, from an
  env var, once at composition.** Each binary's `composition.rs` has a
  `resolve_tenant(env_value: Option<String>) -> Option<TenantId>` that
  maps the env var (`KALEIDOSCOPE_QUERY_TENANT` at
  `crates/query-api/src/composition.rs:54`; `_LOG_QUERY_TENANT` at
  `crates/log-query-api/src/composition.rs:54`; `_TRACE_QUERY_TENANT` at
  `crates/trace-query-api/src/composition.rs:58`) to `Some(t)` when
  set/non-empty, else `None`. That single `Option<TenantId>` is then
  baked into the router state for ALL requests. There is no per-request
  tenant resolution; every request the process serves reads the same
  tenant.

- **F2 — the shared fail-closed tenant seam already exists in
  `query-http-common` and already returns 401 on an unresolved tenant,
  BEFORE the store is touched.** `resolve_tenant_or_refuse(tenant:
  &Option<TenantId>, service_label: &'static str) -> Result<&TenantId,
  Response>` (`crates/query-http-common/src/lib.rs:240-252`) returns
  `Ok(t)` for `Some` and `Err(error_response(UNAUTHORIZED, ...))` for
  `None`, with the per-pillar reason `"no tenant resolvable: <label>
  service refuses unscoped requests"`. The three handlers each call it
  per request before querying the store. **This is the seam the
  per-request bearer path plugs into**: today the `Option<TenantId>` is
  the constant env tenant; this feature makes it the per-request
  resolution (bearer-derived tenant when present and valid, env tenant as
  the additive fallback, `None`-refuse when neither resolves).

- **F3 — the read APIs do NOT depend on aegis's `Validator` and read no
  `Authorization` header today.** A grep for `Authorization` /
  `authorization` across the three read crates returns ZERO matches; they
  import only `aegis::TenantId` (the newtype), never `Validator`,
  `TenantContext`, `ValidationError`, or `load_catalogue`. The bearer
  extraction + validation is genuinely new code. (`query-http-common`
  already depends on `aegis` for `TenantId`, so adding the validator wire
  is an extend, not a new dependency edge.)

- **F4 — the ingest auth (aegis-ingest-auth-v0 / ADR-0068) is the shape
  to MIRROR.** The ingest path extracts `Bearer <jwt>` from the
  transport, calls `aegis::Validator::validate_with_subject(token, now,
  subject)`, rejects fail-closed on any of the 8 `ValidationError`
  variants with the stable `reason()` taxonomy, emits exactly one audit
  event per request, and NEVER logs the secret or the raw token. The
  read path must be CONSISTENT: same HS256 token shape, same redaction,
  same fail-closed-on-invalid, same one-event-per-request audit. The
  ONE deliberate divergence is the **audience**: ingest tokens carry
  `aud=kaleidoscope-ingest`; read tokens carry `aud=kaleidoscope-query`
  (ADR-0068 US-AUTH-04's wrong-audience example already names
  `kaleidoscope-query` as "the read-path audience"). A read API
  presented an ingest-audience token MUST reject `wrong_audience`, and
  vice versa — the two audiences are the cross-surface fence.

- **F5 — aegis is reused verbatim; the validator is the same correct
  lock the ingest door now uses.** `aegis::Validator::validate` is real
  HS256 (jsonwebtoken), alg-confusion-safe, fail-closed on `exp`, exact
  issuer + audience equality, catalogue-checked tenant, 8 typed
  `ValidationError` variants each with a stable `reason()`, exactly one
  `tracing` audit event per call (`crates/aegis/src/validator.rs:174-209`).
  `validate_with_subject` lets the read handler stamp the audit `subject`
  with the read action (e.g. `query_range`, `log_query`, `trace_query`).
  NO crypto change. This feature WIRES the validator onto the read
  request path.

- **F6 — `TenantId` redaction symmetry is already a pinned invariant in
  the shared seam.** `query-http-common`'s inline tests already assert
  the reason constants "never contain a credential marker" (`SECRET`,
  `Bearer`) — `crates/query-http-common/src/lib.rs:403-416`. The
  bearer-derived 401 reasons this feature adds inherit that discipline:
  the reason names the failure class from the aegis taxonomy, never the
  token.

- **F7 — the per-instance env path stays the default under the additive
  model.** Backward compatibility is a hard requirement: a deployment
  that sets only the env tenant and configures no auth must behave
  byte-for-byte as today. The three `composition.rs` `resolve_tenant`
  functions and their fail-closed `probe` stay; the per-request path is
  layered ON TOP, only active when an auth config is present.

## Shared-capability placement (this is the carpaccio hinge)

**The per-request bearer-validation + tenant-resolution capability belongs
in `query-http-common`, shipped once, wired through three APIs.** This
mirrors ADR-0054's whole rationale: the three read APIs duplicate the
tenant seam, so the seam lives in the common crate (where mutation kill
rate is meaningful and a reason-text change is one edit, not three). The
new capability is the per-request analogue of the existing
`resolve_tenant_or_refuse`: a function (DESIGN names the exact signature)
that, given the request's `Authorization` header, the configured
`Arc<Validator>` (or `None` when no auth is configured), and the
per-instance env tenant, returns the resolved `&TenantId` or the
fail-closed 401 `Response`. Because the three handlers ALREADY route
their tenant resolution through `query-http-common`, wiring the validator
in ONE place and threading an `Option<Arc<Validator>>` through the three
routers is the minimal ripple.

This is why the **walking slice proves the whole path on ONE API**
(metrics / `query-api`) — extract bearer, validate, resolve tenant,
scoped query, isolation (positive + negative control), 401 fail-closed
before store, redaction — and the other two APIs are thin parity slices
that REUSE the shared capability. If, at DESIGN, the shared capability
fully centralises the resolution such that wiring all three at once is one
small change, DESIGN may collapse the two parity slices into one; the
requirement does not mandate three separate slices, it mandates the
capability lands once and all three APIs are covered.

## Six decisions flagged for DESIGN (solution-architect owns the mechanism)

> Requirements stay solution-neutral. These are the seams DESIGN must
> resolve; the requirement says WHAT must be observable, DESIGN says HOW.

### DD1 — read-API auth config wiring for the HS256 validator (mirror ADR-0068 DD1)

The three read binaries today read only env vars at composition. DESIGN
must decide the config shape for the read-path validator: the HS256
**secret** (by reference — file or env, NEVER inline, NEVER logged), the
**issuer**, the **read audience** (`kaleidoscope-query`, distinct from the
ingest `kaleidoscope-ingest`), and the **tenant catalogue path**. Mirror
ADR-0068's `[...security.auth.jwt]` table shape and its structural
never-logged invariant (config stores a `PathBuf`, not the secret bytes;
aegis opaque-Debugs the key; config-validation errors name the secret by
path only). Whether the three read APIs share one config block shape or
each has its own is DESIGN's call; the never-logged invariant and the
read-audience value are the requirements.

### DD2 — bearer extraction on the read transport + exact reject mapping

The read APIs are HTTP-only (axum). The JWT arrives in the
`Authorization` request header as `Bearer <jwt>`. On failure, reject with
**HTTP 401 Unauthorized** carrying a `WWW-Authenticate: Bearer` challenge
(RFC 6750) and the aegis `reason()` taxonomy in the body — REUSING the
existing `query-http-common` `error_response`/`ErrorBody` envelope shape
where it fits, or a sibling 401 shape DESIGN pins. The reject must NOT
leak the secret or the raw token. Confirmed (F3): no read handler reads
`Authorization` today.

### DD3 — per-request tenant resolution precedence (the additive model's core)

DESIGN locks the resolution function in `query-http-common` (DD-placement
above). The additive precedence:

1. **auth configured AND request carries a valid bearer token** → the
   tenant is the token's `TenantContext.tenant_id` (scoped to THAT
   tenant). Isolation: this tenant queries only its own data.
2. **auth configured AND request carries an INVALID/missing bearer
   token** → fail-closed 401 (the aegis reason), BEFORE the store. (Under
   the additive model, "missing token when auth is configured" is a
   reject — the env tenant is NOT a fallback once auth is on, otherwise
   the bearer would be bypassable; DESIGN confirms this precedence so the
   additive path cannot be downgraded to the env tenant by simply omitting
   the header.)
3. **auth NOT configured** → today's per-instance env tenant
   (`KALEIDOSCOPE_*_QUERY_TENANT`), behaviour byte-for-byte unchanged;
   `None` (unset/empty) still refuses 401 via the existing seam.

> **VETO NOTE**: on a per-request-only veto, precedence collapses to "auth
> ALWAYS configured; (1) and (2) only; (3) removed; a process with no
> resolvable per-request tenant refuses to start or refuses every
> request". DD3 is the one decision the veto reshapes.

### DD4 — fail-closed posture: unresolved tenant refused BEFORE the store

A request whose tenant cannot be resolved (invalid/missing bearer when
auth is on; unset env tenant when auth is off) is refused with a 401
BEFORE the backing store is queried — REUSING the existing
`query-http-common` fail-closed seam (F2). The negative control the
project always uses: prove the data IS returned for the right tenant and
ABSENT (empty / forbidden) for the wrong one. No partial reads, no
store touch on an unresolved tenant.

### DD5 — audit / observability of read-path denials (mirror ADR-0068 DD5)

One structured decision event per read request, reusing aegis's
`reason()` taxonomy (8 variants → 8 stable reasons) and the locked field
contract (`tenant_id`/`role`/`decision`/`subject`/`reason`), with the
`subject` naming the read action (`query_range` / `log_query` /
`trace_query`). aegis already emits exactly one event per `validate` call;
DESIGN decides whether the read API relies on aegis's event or emits its
own, without double- or zero-logging. The no-token case (decided at the
extraction boundary before `validate`) emits one `reason=missing_claim`
event. No secret, no raw token in any field. Aligns with the read tier's
existing `init_tracing` JSON-stderr subscriber
(`query-http-common::init_tracing`).

### DD6 — scope fence + the cross-surface audience fence + role question

- **Scope**: the three READ query APIs only, HTTP only. The ingest path
  (already done, ADR-0068) is OUT. SPIFFE / RS256 / JWKS / OPA are aegis
  v1, OUT.
- **Audience fence**: read tokens carry `aud=kaleidoscope-query`; a token
  minted for `kaleidoscope-ingest` MUST reject `wrong_audience` on the
  read path (and vice versa). This is the boundary that stops an ingest
  token from reading data and a read token from writing telemetry.
- **Role question — RESOLVED, deferred (mirror ADR-0068 DD6)**: v0 is
  authentication + tenant-scoping only. Any valid token for a catalogued
  tenant (role `viewer` OR `operator`) may read. aegis still rejects
  `unknown_role` (a role that is neither) free. Whether a future feature
  restricts reads to a specific role (e.g. `viewer` may read but a
  read-only token may not be required to be `operator`) is deferred with
  the decision recorded — the minimum fail-closed property is
  authentication + tenant isolation, not role-gated read authorization.

## Risks

| ID | Risk | Prob | Impact | Mitigation |
|----|------|------|--------|------------|
| R1 | **MODEL FORK**: the additive model may not be what Andrea wants (he may prefer per-request-only / mandatory bearer). | Medium | High | The Andrea-veto FLAG above; the requirement set holds identically under both models except US-RAUTH-02; the additive choice forecloses nothing and the bearer work is not wasted on a veto. DECIDED additive per decide-don't-ask. |
| R2 | The HS256 secret leaks into a log line / error / Debug. | Medium | Critical | DD1 never-logged invariant; AC `the-secret-is-never-logged`; aegis already opaque-Debugs the key; `query-http-common` already pins reason-redaction (F6). |
| R3 | The per-request path silently DOWNGRADES to the env tenant when a token is missing (auth-on but header-absent falls through to env tenant) — a bearer-bypass. | Medium | High | DD3 precedence: once auth is configured, a missing/invalid bearer REJECTS; the env tenant is a fallback ONLY when auth is not configured. AC pins "auth-on + no-token → 401, not env-tenant". |
| R4 | Tenant isolation is not actually enforced — a tenant-A token reads tenant-B data. | Low | Critical | The negative-control scenario (tenant-A token querying tenant-B → empty/forbidden) is in EVERY API's story; the positive+negative control pair is mandatory; the store already takes `&TenantId` and scopes by it (F1 query signatures). |
| R5 | The per-request path regresses the env-tenant default (backward compat) — an existing single-tenant deployment breaks. | Medium | High | DD3 precedence (auth-not-configured → env tenant unchanged); US-RAUTH-02 backward-compat scenario; the three `composition.rs` `resolve_tenant`/`probe` stay; existing read-API slice tests stay green. |
| R6 | Cross-surface audience confusion — an ingest token reads data (or a read token writes). | Low | High | DD6 audience fence: read `aud=kaleidoscope-query`, ingest `aud=kaleidoscope-ingest`; wrong-audience rejects on each surface (mirrors ADR-0068 US-AUTH-04). |
| R7 | The shared-capability placement is wrong (capability duplicated per crate instead of in `query-http-common`). | Low | Medium | DD-placement: the capability lands in `query-http-common` once (ADR-0054 rationale); WS proves it on one API, parity slices reuse it. |
| R8 | No DIVERGE artifacts exist for this feature. | n/a | Low | Job grounded in ADR-0068 DD6 (the explicit carve-out), the verified code facts F1-F7, and the ingest auth shape to mirror. JTBD not re-run (lightweight, brownfield wiring). Noted, not blocking. |

## DIVERGE grounding

No `diverge/recommendation.md` or `diverge/job-analysis.md` exists for
this feature. The job is grounded instead in: ADR-0068 DD6 (the explicit
read-path carve-out naming these three APIs as the follow-up feature),
the `aegis-ingest-auth-v0` discuss artifacts (the shape to mirror), and
the verified code facts F1-F7. This is acceptable for a brownfield wiring
feature with a single, well-understood fail-closed job; noted as R8.
Per the pre-decided interactive choices: Feature Type = Backend; Walking
Skeleton = No (brownfield, the first slice is a thin end-to-end proof on
ONE API, not a greenfield skeleton); UX Research Depth = Lightweight;
JTBD = No.

## The mirror-the-ingest-auth constraint (load-bearing)

The read-path auth MUST be consistent with the ingest auth
(`aegis-ingest-auth-v0` / ADR-0068) on every axis except audience:

- **Same token shape**: HS256 JWT, issuer + audience + `tenant_id` +
  `kaleidoscope_role` claims, validated by the SAME `aegis::Validator`.
- **Same redaction**: the secret and the raw token NEVER appear in a log,
  error body, audit event, or Debug. Structural, not by discipline
  (config holds a `PathBuf`; aegis opaque-Debugs the key; reason texts
  name the failure class, F6).
- **Same fail-closed-on-invalid**: any of the 8 `ValidationError`
  variants → reject; missing/empty token (auth on) → reject; nothing read
  before the store on a refusal.
- **Same one-event-per-request audit**: aegis's locked field contract,
  `subject` naming the read action; never zero, never duplicated.
- **The ONE divergence**: `aud=kaleidoscope-query` (read) vs
  `kaleidoscope-ingest` (ingest), enforced as the cross-surface fence
  (DD6).

## Inherited gates

ADR-0005's five gates apply; per-feature mutation testing at 100% kill
rate on the modified files (CLAUDE.md). Rust idiomatic (data + free
functions + traits where polymorphism is genuinely needed). NEVER bump
any crate to 1.0.0 (Andrea's call). Kaleidoscope is pure trunk-based (CI
is feedback, not a gate).
</content>
</invoke>
