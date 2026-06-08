# DESIGN Decisions — read-path-query-api-auth-v0

> **Wave**: DESIGN (nWave). **Architect**: Morgan (`nw-solution-architect`).
> **Date**: 2026-06-08. **Mode**: PROPOSE (autonomous overnight). **Decision 0 scope**: APPLICATION.
> **Primary artefacts**: `docs/product/architecture/adr-0074-read-path-query-api-auth.md` (the decision record) and the `## Application Architecture — read-path-query-api-auth-v0` section of `docs/product/architecture/brief.md` (C4 Container L2 + Component L3, the For-Acceptance-Designer handoff).
> **Grounding**: read on 2026-06-08 — `query-http-common/src/lib.rs:240-252` (the fail-closed seam), the three `composition.rs` env-tenant + probe functions, `query-api/src/lib.rs:81-222` (router state + handler), `trace-query-api/src/lib.rs` (the two handlers incl. lookup-by-id), `aegis/src/validator.rs:74-258` (the Validator), `aegis/src/catalogue.rs`, ADR-0068 (the ingest auth mirrored).

## ANDREA-VETO FLAG (carried forward, still load-bearing — KEPT VISIBLE)

```
+--------------------------------------------------------------------------+
|  MODEL FORK — ANDREA MAY VETO                                           |
|                                                                          |
|  DESIGNED TO: the ADDITIVE model (DECIDED by Luna in DISCUSS, Morgan    |
|    proceeds per decide-don't-ask).                                      |
|    - PRESERVE today's per-instance env-tenant default                  |
|      (KALEIDOSCOPE_*_QUERY_TENANT), unchanged when no auth config.      |
|    - ADD an OPTIONAL per-request bearer path: auth-on + valid bearer    |
|      scopes the query to THAT token's tenant.                          |
|                                                                          |
|  VETO TARGET: per-request-ONLY (mandatory bearer, no env fallback).    |
|                                                                          |
|  THE DESIGN FORECLOSES NOTHING: every fail-closed + isolation decision |
|    (DD2, DD3 arms 1+2, DD4, DD5, DD6) holds IDENTICALLY under          |
|    per-request-only. Only DD3 arm (3) + US-RAUTH-02 reshape on a veto, |
|    and NO bearer-validation work is wasted.                            |
|                                                                          |
|  TO VETO: switch to per-request-only. Localised change: the           |
|    config-validation rule (require auth config; refuse a request with |
|    no resolvable per-request tenant even when an env tenant is set).   |
|    DD3 arm (3) is removed; auth is always Some; no-auth-config refuses |
|    to start (mirror ADR-0068 DD4). US-RAUTH-01/-03/-04 UNCHANGED.      |
+--------------------------------------------------------------------------+
```

## DD1–DD6 resolutions (Morgan owns the mechanism; full rationale in ADR-0074)

### DD1 — read-API auth config wiring; secret never logged

Each read binary's composition root resolves an **OPTIONAL** read-auth config from env vars keyed off its existing prefix: `KALEIDOSCOPE_<API>_QUERY_AUTH_{ISSUER, AUDIENCE, SECRET_FILE, CATALOGUE}` where `<API>` ∈ {`` (metrics), `LOG_`, `TRACE_`}. The **secret is by file reference** (`secret_file` = a `PathBuf`, never the bytes inline): bytes are read into a local `Vec<u8>`, moved into `aegis::ValidatorConfig { hs256_key }`, and live only inside the `aegis::Validator` (opaque-Debugged key). Config-validation errors name the file **by path only**. `audience` is `kaleidoscope-query` (DD6). **Auth is "on" iff the config is present AND complete**; a **partial** config (some-but-not-all required fields) is a **refuse-to-start** error (the half-configured silent-downgrade trap, mirroring ADR-0061/ADR-0068 DD4); a **wholly absent** config is the additive opt-out (DD3 arm 3). Validator built once → `Option<Arc<Validator>>`.

### DD2 — bearer extraction + reject mapping

`Bearer <jwt>` from the `Authorization` HTTP header. Absence / empty value / non-`Bearer`-prefix / empty token → reason **`missing_claim`**, decided at the extraction boundary BEFORE `validate` (so the cheap path stays cheap and the reason is stable; no `validate("")` call). A well-formed `Bearer <jwt>` → `validate_with_subject(jwt, SystemTime::now(), subject)`. Reject = **HTTP 401 + `WWW-Authenticate: Bearer`** (RFC 6750), aegis `reason()` carried in the **existing `query_http_common::error_response`/`ErrorBody`** envelope (`{"status":"error","error":"<reason>"}`) plus the `WWW-Authenticate` header. No secret, no raw token in any field.

### DD3 — per-request tenant resolution precedence (the additive core; the no-bearer-bypass)

The capability lands ONCE in `query-http-common`. Recommended signature (contract pinned; crafter owns the body):

```
resolve_request_tenant_or_refuse(
    auth: Option<&Arc<aegis::Validator>>,
    headers: &HeaderMap,
    env_tenant: &Option<TenantId>,
    service_label: &'static str,
    subject: &'static str,
    now: SystemTime,
) -> Result<TenantId, Response>
```

Precedence:
1. **auth `Some` + valid bearer** → `Ok(ctx.tenant_id)`.
2. **auth `Some` + missing/malformed/invalid bearer** → **`Err(401)` BEFORE the store; `env_tenant` is NEVER consulted in this arm.** *(The no-bearer-bypass, R3: the function returns the 401 directly from the validation-failure branch; there is NO `else env_tenant` fall-through. A code path that downgrades to the env tenant on a failed/absent bearer is forbidden by construction.)*
3. **auth `None`** → `resolve_tenant_or_refuse(env_tenant, service_label)` (the EXISTING seam); `Some(t)` → `Ok(t)`; `None` → the existing 401. The `Authorization` header is **ignored**.

The resolved `TenantId` is consumed by the EXISTING tenant-scoped store query identically whether env- or bearer-derived (the store already scopes by `&TenantId`; the feature authenticates the provenance). **Veto**: collapse to arms 1+2; arm 3 removed; auth always `Some`; no-config refuses to start.

### DD4 — fail-closed before the store + the Earned-Trust auth probe

Unresolved tenant (invalid/missing bearer auth-on; unset env auth-off) → 401 BEFORE any store access, reusing the existing seam discipline. The negative control is mandatory (right tenant present, wrong tenant ABSENT) on metrics, logs, traces, AND the trace lookup-by-id path (ADR-0053). **Earned-Trust (principle 12)**: the env-store `probe()` (wire→probe→use) is preserved. When auth is configured, the composition root additionally runs a **negative startup probe** — a deliberately-invalid token (e.g. wrong signature) MUST be **rejected** by the constructed validator before any socket binds; a validator that cannot reject is refused with `event=health.startup.refused`. This proves the auth lock actually rejects in the real environment, not merely that it was constructed.

### DD5 — audit / observability of denials

aegis already emits exactly one structured event per `validate_with_subject` call (`tenant_id`/`role`/`decision`/`subject`/`reason`; `info!` allow, `warn!` deny). The read API relies on it for every validate-reached request. The ONE pre-validate `missing_claim` case (decided before `validate`) → one event emitted by the shared capability in the same field shape. **Exactly one decision event per request across all paths; never zero, never duplicated.** `subject` = `query_range`/`log_query`/`trace_query`. Rides `query_http_common::init_tracing` (JSON-stderr).

### DD6 — scope fence + cross-surface audience fence + role question

- **Scope**: the three READ APIs, HTTP only. Ingest (ADR-0068) OUT. SPIFFE/RS256/JWKS/OPA = aegis v1, OUT.
- **Audience fence**: read `aud=kaleidoscope-query` (configured into each read `Validator`); ingest `aud=kaleidoscope-ingest`. An ingest-audience token → `wrong_audience` on read (the SAME `aegis` exact-aud check `validator.rs:228-233`, a config value, not new code). The boundary that stops an ingest token reading and a read token writing.
- **Role question — RESOLVED, deferred** (mirror ADR-0068 DD6): v0 = authentication + tenant-scoping only; any catalogued `viewer`/`operator` token reads; `unknown_role` rejected free; role-gated read authorization deferred with the decision recorded. `TenantContext.role` is already available to the handler, so a future role gate needs no re-plumbing.

## Reuse Analysis (MANDATORY — every CREATE-NEW justified)

| Capability | Verdict | Where / How | Justification |
|---|---|---|---|
| HS256 JWT validation (sig/exp/iss/aud/tenant/role) + one audit event/call | **REUSE verbatim** | `aegis::Validator::validate_with_subject` (`validator.rs:180-210`) | The SAME lock the ingest door uses (ADR-0068). No crypto change. |
| Typed success/failure types + 8 stable reasons + exact-aud check | **REUSE verbatim** | `aegis::TenantContext`/`TenantId`/`Role`/`ValidationError`+`reason()` (`validator.rs:33,43,66,74-108,228-233`) | The audience fence is this exact-aud check configured `kaleidoscope-query`. |
| Validator construction + catalogue load + opaque-Debug key | **REUSE verbatim** | `Validator::new` (`:162`), `load_catalogue`/`TenantCatalogue` (`catalogue.rs:111`), Debug (`:149-158`) | Built once at composition; key never loggable. |
| Fail-closed tenant seam (401 before store) | **REUSE verbatim** | `query_http_common::resolve_tenant_or_refuse` (`lib.rs:240-252`) | IS the env-tenant arm (DD3 arm 3) AND the discipline the new capability mirrors. |
| JSON error envelope + reason-redaction + audit subscriber | **REUSE verbatim** | `error_response`/`ErrorBody` (`lib.rs:269-275`), reason-redaction tests (`:403-416`), `init_tracing` (`:318`) | The 401 body, the no-credential-marker discipline, the stderr sink — all already pinned. |
| Existing tenant-scoped store queries | **REUSE verbatim** | `MetricStore::query(&tenant,…)`, `LogStore::query(&tenant,…)`, `TraceStore::query`+`get_trace(&tenant,…)` | Consumed identically whether env- or bearer-derived. **pulse/lumen/ray UNTOUCHED.** |
| Composition `resolve_tenant` + Earned-Trust `probe` (env path) | **REUSE verbatim** | the three `composition.rs:54/54/58` + their `probe` | Backward compat (US-RAUTH-02); arm 3 byte-for-byte today. |
| `query-http-common` (the shared crate) | **EXTEND** | add `resolve_request_tenant_or_refuse` + bearer extraction + the pre-validate `missing_claim` event | Auth logic lands HERE ONCE (ADR-0054 rationale, R7). aegis dep edge already present (for `TenantId`) — extend, not new edge. |
| The three read APIs | **EXTEND** | router `ApiState` gains `Option<Arc<Validator>>`; handler swaps one resolution call; composition resolves optional auth config + the auth startup probe; main reads four env vars | Thin wiring over the shared capability; no per-crate auth logic. |
| Read-auth config fields (issuer/audience/secret_file/catalogue) | **CREATE** | the four env-backed fields per binary | **Justified by F3**: verified zero — no read composition carries an HS256/issuer/audience/catalogue field today (they read only tenant/addr/pillar_root/static_dir). Genuinely new config surface; mirrors ADR-0068 DD1 shape. |
| Bearer extraction + the pre-validate `missing_claim` reason | **CREATE** | inside `query-http-common`, part of the shared capability | **Justified by F3**: verified zero `Authorization`/`authorization` reads across the three read crates today. The new auth-extraction boundary. Created ONCE in the shared crate, not per API. |

**Net**: REUSE the entire aegis core + the `query-http-common` seam/envelope/redaction/subscriber + the existing tenant-scoped store queries verbatim; EXTEND `query-http-common` (capability once) + the three read APIs (thin wiring); CREATE only the four read-auth config fields and the bearer-extraction boundary, both justified by the verified zero-`Authorization` fact. **No new crate, no new dependency edge, no store change, no duplicated validator.**

## For Acceptance Designer

> Full per-AC observable spec is in `brief.md` (`### For Acceptance Designer — read-path-query-api-auth-v0`). Summary below for the DISTILL handoff.

**Three driving ports** (running binaries, each with an `Authorization: Bearer <jwt>` header):
1. `GET /api/v1/query_range` — `query-api` (metrics / Pulse, `:9090`).
2. `GET /api/v1/logs` — `log-query-api` (logs / Lumen, `:9091`).
3. `GET /api/v1/traces` **and the trace lookup-by-id path** — `trace-query-api` (traces / Ray, `:9092`). The lookup-by-id path (ADR-0053) MUST also be isolated.

Mint tokens in-suite (HS256, same secret the test config's `secret_file` points at, `aud=kaleidoscope-query`, catalogued tenant, future `exp`); negative-control variants: no token, empty `Bearer `, malformed, expired, bad signature, wrong issuer, `aud=kaleidoscope-ingest`, unknown tenant, `auditor` role.

**Each AC asserts:**
- **valid-token-reads-its-own-tenant** — response shape byte-identical to today; one `allow` audit line.
- **tenant-isolation positive+negative control** — `acme-prod` token sees `acme-prod` data; `globex-staging` token sees it **ABSENT** (metrics, logs, traces, lookup-by-id). Both halves mandatory; no AC asserts isolation with only one half.
- **no-token-401-before-store** — 401 + `WWW-Authenticate: Bearer`; store never queried.
- **auth-on-missing-token-does-NOT-downgrade-to-env-tenant** — the no-bearer-bypass: auth-on + env tenant ALSO set + no bearer → 401, NOT env-scoped, store never queried. *(load-bearing negative control for DD3 arm 2.)*
- **8 reasons, mutually distinct** — each `ValidationError` variant surfaces its matching `reason`.
- **ingest-audience-token rejected wrong_audience** — the cross-surface fence.
- **secret-and-token never logged** — none in any 401 body, error, log, or audit event (hard guardrail).
- **one-audit-event-per-request** — exactly one (allow/deny), never zero/duplicated, incl. the pre-validate `missing_claim` case.
- **backward compat** — auth-off → today's env-tenant behaviour, header ignored; unset env → existing 401; existing slice tests stay green.
- **DD6 role resolved** — recorded deferral (v0 = authn + tenant-scoping only).

**Falsifiability / Earned-Trust**: every reject AC must FAIL against an env-tenant fall-through (the no-bypass test) AND a non-validating impl (the reason matrix). The auth-on startup negative probe (known-bad token rejects before bind) is the Earned-Trust probe for the auth dependency.

## Slice impact (the collapse question DISCUSS deferred to DESIGN)

Because the capability lands ONCE in `query-http-common` and all three handlers already route tenant resolution through it (F2), wiring all three APIs is one small change after the WS. **Recommendation: 4 stories → 3 delivery slices.**

1. **Slice 1 — WS (US-RAUTH-01)**: per-request bearer auth + isolation on `query-api` (metrics). Lands the shared capability; proves the riskiest assumption (validator wired into the shared read seam, fail-closed, isolated, no env-path regression).
2. **Slice 2 — log + trace parity (US-RAUTH-03 COLLAPSED)**: wire `log-query-api` + `trace-query-api` (incl. trace lookup-by-id isolation) over the shared capability in ONE thin slice. The per-crate work is mechanical (swap one handler call; resolve the same optional config), not new auth logic — a separate log slice and a separate trace slice are NOT warranted.
3. **Slices 3+4 remain distinct as the two thin closing slices**: **US-RAUTH-02** (backward compat + the no-bearer-bypass) and **US-RAUTH-04** (the 8-reason matrix + the cross-surface audience fence + DD6 role decision). Each asserts a property the WS does not, so each is its own verifiable slice — but both are thin and could even land adjacent to their natural partners (US-RAUTH-02 alongside the WS, US-RAUTH-04 after parity) at DELIVER's discretion.

DELIVER may sequence US-RAUTH-02's no-bypass assertion into the WS slice (it exercises the SAME arm-2 branch the WS introduces); the recommendation is that the **capability lands once and all three APIs are covered**, with the log+trace parity genuinely collapsed.

## Back-propagation to DISCUSS

**None required.** The design honours every DISCUSS assumption without contradiction: the additive model (kept, veto-flagged), the shared-capability-once placement (DD3 in `query-http-common`), the fail-closed-before-store seam reuse, the redaction discipline, the audience fence, the 8-reason taxonomy, and the backward-compat guarantee are all designed exactly as the requirements state. The one DESIGN-owned latitude DISCUSS explicitly granted — "DESIGN may collapse US-RAUTH-03 into the WS if wiring all three is a single small change" — is exercised (the parity slice is collapsed into one, not three), which is a permitted DESIGN call, not a changed DISCUSS assumption. No `design/upstream-changes.md` is created.

## Self-Review (solution-architect critique dimensions)

> The `solution-architect-reviewer` was not nested-invocable in this autonomous run; Morgan performed a structured self-review against the `nw-sa-critique-dimensions` skill. Verdict and evidence below.

| Dimension | Check | Verdict |
|---|---|---|
| **D1 — Bias detection** | Resume-driven / latest-tech / preference bias? | **PASS.** No new tech: aegis (existing, tested), axum (existing), the existing shared crate. The chosen handler-side shared function is justified against a middleware alternative (Option C) on type-safety + single-resolution-point grounds, not preference. No trendy pattern introduced. |
| **D2 — ADR quality** | Context, ≥2 alternatives w/ rejection, consequences? | **PASS.** ADR-0074 has full Context (verified code facts), **four** alternatives (per-request-only/the veto target, per-API auth, tower middleware, enabled-flag) each with pros/cons/rejection, positive+negative consequences, and an ATAM trade-off section. |
| **D3 — Completeness** | Security, isolation, observability, testability, perf addressed? | **PASS.** Security posture section (fail-closed, no-bypass, isolation, audience fence, secret-never-logged); DD5 observability; the For-Acceptance-Designer testability spec; perf (I/O-free validate, O(1) catalogue, auth-off adds nothing). |
| **D4 — Feasibility** | Team capability, testability via ports, budget? | **PASS.** Reuses a validator the team already wired on ingest; the three driving ports are existing HTTP endpoints; isolated testing via the in-suite token minting + the recording stores. No infra cost (local secret + local catalogue, no IdP). |
| **D5 — Priority** | Largest bottleneck, simpler alts, data-justified? | **PASS.** Q1: the read side is 0% per-request authenticated (the whole read surface) — the largest gap. Q2: simpler alternatives (per-request-only, per-API, middleware, flag) considered + rejected with rationale. Q3: not inverted — the capability lands once over the shared seam, the minimal ripple. Q4: grounded in verified code facts F1–F7, not assumption. |
| **No-bearer-bypass precedence explicit** | Is arm 2's no-fall-through unmissable? | **PASS.** Stated in ADR-0074 DD3 + the security posture, the brief's L3 diagram (explicit "NO fall-through to env_tenant" edges), and this doc's DD3. Pinned as a mutation-killed branch + a dedicated negative-control AC. |
| **Fail-closed-before-store** | Refusal before any store access? | **PASS.** Both refusal arms reuse the existing pre-store seam; DD4; the store-never-queried AC; the Earned-Trust startup negative probe. |
| **Redaction** | Secret + raw token never logged? | **PASS.** Structural (PathBuf not bytes; opaque-Debug; path-only errors; reason = aegis class name); inherits the existing `query-http-common` reason-redaction tests; a dedicated AC + a hard guardrail. |
| **C4 diagrams present** | L1+L2 minimum; L3 for the complex subsystem? | **PASS.** Container L2 (the shared seam wired into the three APIs + the env fallback + the secret-by-reference) and Component L3 (the 3-arm precedence + the no-bypass + the audience fence). L1 not re-produced (no new external actor/system beyond the existing read tier — justified). |
| **No overstated claims** | Every claim grounded; honest residuals? | **PASS.** Claims tied to read line/file refs; the one honest residual (per-request-only is the stricter posture the additive model trades away, reversible via the veto) is disclosed, not hidden; no "before any byte"-style overstatement; Earned-Trust probe named for the auth dependency. |

**Self-review verdict: APPROVED.** All critique dimensions PASS; no critical or high issues. The load-bearing security property (no-bearer-bypass) is explicit and triply-pinned (ADR + L3 diagram + dedicated AC/mutation target). The Andrea-veto flag is preserved across ADR, brief, and this document. Recommendation: proceed to DISTILL with the 4-stories-to-3-slices guidance.

## Inherited gates / constraints honoured

ADR-0005's five gates; per-feature mutation testing at 100% kill rate on the modified files (`query-http-common` + the three read crates); Rust idiomatic (data + free functions + traits where polymorphism is genuine — the `Arc<dyn Store>` + `Arc<Validator>` are genuine polymorphism, no inheritance); NEVER 1.0.0 (Andrea's call); pure trunk-based (CI is feedback, not a gate); no production code written in this wave (crafter owns `crates/<name>/src/`); no commit made.
