# Story Map: read-path-query-api-auth-v0

## User: Priya, a platform-security operator running a multi-tenant Kaleidoscope deployment

## Goal: Enforce per-request tenant authentication + isolation at the READ query boundary (metrics, logs, traces), so a request is scoped to the tenant proven by its bearer token, no caller reads another tenant's data, and an unresolved tenant is refused before the store — while preserving today's per-instance env-tenant default (additive model).

The driving surface is the three running read-API binaries: `query-api`
(metrics, `:9090`, `GET /api/v1/query_range`), `log-query-api` (logs,
`:9091`, `GET /api/v1/logs`), `trace-query-api` (traces, `:9092`,
`GET /api/v1/traces` + trace lookup-by-id). Before this feature: each
process resolves ONE tenant from a process-wide env var and applies it to
every request; no per-caller scoping, no identity check. After: with read
auth configured, a request carrying a valid `kaleidoscope-query`-audience
bearer token is scoped to THAT token's tenant (positive + negative
control); a missing/invalid token gets 401 before the store; the
env-tenant default is preserved when no auth is configured.

> **MODEL FORK — ANDREA MAY VETO** (full flag in `wave-decisions.md`):
> proceeding on the ADDITIVE model (preserve env-tenant default, add
> optional per-request bearer). The alternative is per-request-only
> (mandatory bearer). Only Release 1 (backward compatibility) reshapes on
> a veto.

## Backbone (user activities, chronological)

| A. Present a token | B. Authenticate at the read boundary | C. Resolve the tenant | D. Scope + isolate the read | E. Audit the decision |
|--------------------|--------------------------------------|------------------------|------------------------------|------------------------|
| Client sends `GET /api/v1/...` with `Authorization: Bearer <jwt>` | Read API extracts + validates the token before any store read | Tenant = token's tenant (auth on) OR env tenant (auth off) OR refuse | Query scoped to the resolved tenant; cross-tenant read returns ABSENT | One structured decision event per request |
| A.1 `Authorization` header (HTTP) | B.1 Extract bearer in the shared `query-http-common` seam | C.1 Per-request token tenant (auth configured) | D.1 Metrics query scoped to the token's tenant | E.1 One deny event per rejected request (reason taxonomy) |
| A.2 Missing/empty token | B.2 Validate via aegis (sig/exp/iss/aud=query/tenant/role) | C.2 Env tenant fallback (auth NOT configured) — backward compat | D.2 Logs + traces scoped to the token's tenant | E.2 Secret + raw token never logged |
| A.3 Ingest-audience token (cross-surface) | B.3 Refuse 401 before the store (existing fail-closed seam) | C.3 Unresolved tenant → 401 before store | D.3 Isolation: positive + negative control (right tenant present, wrong tenant absent) | E.3 One allow event per accepted request; reason fence (wrong_audience) |

---

## Walking Skeleton

The thinnest end-to-end slice that connects ALL activities, on ONE read
API (metrics / `query-api`):

- **A.1** client presents `Bearer <jwt>` (audience `kaleidoscope-query`)
  in the `Authorization` header of `GET /api/v1/query_range`
- **B.1 + B.2** the shared `query-http-common` capability extracts the
  bearer and validates it via `aegis::Validator`
- **B.3 + C.3** an invalid OR missing token (auth on) → 401, **the Pulse
  store is never touched** (the existing fail-closed seam)
- **C.1 + D.1 + D.3** a valid token → the query is scoped to the token's
  tenant; isolation proven by the positive control (`acme-prod` token sees
  `acme-prod` data) AND the negative control (`globex-staging` token sees
  it ABSENT)
- **E.1 + E.3** exactly one decision event per request (allow on accept,
  deny on reject), reason from the aegis taxonomy, secret/token redacted

The security boundary (refuse-before-store) AND the isolation negative
control are IN the walking skeleton. A slice that adds only the happy
path is not shippable.

### Priority Rationale

Priority is by outcome impact and dependency, not feature grouping.

1. **Walking Skeleton (P1) — US-RAUTH-01**: establishes the per-request
   auth + isolation boundary end-to-end on the metrics API, and lands the
   shared capability in `query-http-common`. Validates the riskiest
   assumption: that `aegis::Validator` can be wired into the shared read
   seam, fail-closed and isolated, without regressing the existing
   env-tenant read path. Highest value (moves the north-star "% of
   authenticated read requests scoped to the token's tenant" from 0 toward
   1) and de-risks the fatal assumptions (R3 bearer-bypass, R4 isolation,
   R7 shared-placement).
2. **Release 1 (P2) — US-RAUTH-02, backward compatibility**: guarantees
   the additive promise is a tested property — an unconfigured deployment
   is byte-for-byte unchanged, and an auth-on deployment never downgrades a
   missing token to the env tenant. Must land alongside/just after the WS
   because the WS introduces the per-request path that this story fences
   off as opt-in. **This is the story that reshapes on an Andrea veto to
   per-request-only.** Depends on WS.
3. **Release 2 (P3) — US-RAUTH-03, log + trace parity**: extends the
   per-request auth + isolation to the other two read APIs by REUSING the
   shared `query-http-common` capability. Lower per-slice risk (the three
   handlers already share the tenant seam; this is wiring, not new auth
   logic). The trace lookup-by-id path (ADR-0053) must also be isolated.
   Depends on WS. (DESIGN may collapse into the WS if the shared
   capability makes wiring all three a single small change — see
   `wave-decisions.md` placement note.)
4. **Release 3 (P4) — US-RAUTH-04, legible denials + audience fence**: the
   8 aegis reasons each surface on the read path, AND the cross-surface
   audience fence (`kaleidoscope-query` vs `kaleidoscope-ingest`) is
   enforced so an ingest token cannot read; the DD6 role question is
   resolved/deferred. Lowest urgency: the fail-closed boundary already
   rejects every invalid token in the WS; this slice makes each rejection
   LEGIBLE by reason and closes the cross-surface replay gap. Depends on
   all prior.

### Release 1 — Backward compatibility (outcome: env-tenant deployments unchanged; no bearer-bypass)

- A.2 missing/empty token handling under the additive precedence
- C.2 env tenant fallback when auth NOT configured (unchanged)
- C.3 the unset env tenant still refuses 401 (unchanged)
- the auth-on missing-token does NOT downgrade to the env tenant
- Target KPI: KPI-5 (backward-compat: 100% of env-tenant deployments
  unchanged; 100% no-bearer-bypass).

### Release 2 — Log + trace parity (outcome: all three read APIs isolated per-request)

- D.2 per-request tenant scoping for logs (Lumen) + traces (Ray, incl.
  lookup-by-id)
- B + C + D + E symmetric across logs + traces, reusing the shared
  capability
- Target KPI: KPI-1 + KPI-2 cover all 3 read APIs (the full read surface).

### Release 3 — Legible denials + audience fence (outcome: every read denial is legible; ingest tokens cannot read; role question resolved)

- E.1 each of the 8 aegis `ValidationError` variants surfaces with its
  matching reason in the read deny audit event
- A.3 + E.3 the cross-surface audience fence: an `kaleidoscope-ingest`
  token rejects `wrong_audience` on the read APIs
- DD6 role question: v0 = authentication + tenant-scoping only; role-gated
  read authorization deferred with the decision recorded
- Target KPI: KPI-3 (reason-coverage of read denials) reaches 100% of
  variants; KPI-6 (audience fence) at 100%.

## Scope Assessment: PASS (right-sized as sliced — 4 stories, 2 crates touched, estimated 3-5 days)

Assessed against the Elephant Carpaccio oversized signals:

- **User stories**: 4 (US-RAUTH-01..04) — within the 1-feature band. PASS.
- **Bounded contexts / modules touched**: 2 crates of NEW code
  (`query-http-common` gets the shared capability; the three read-API
  crates get the thin wiring) — aegis is reused verbatim, the stores
  (pulse/lumen/ray) are untouched. The shared-capability-once design keeps
  the auth logic in ONE place. Within 3. PASS.
- **Walking-skeleton integration points**: read-API↔aegis (validate),
  read-API↔config (HS256 + read-audience fields), read-API↔shared seam
  (the per-request resolution in `query-http-common`) = 3. At the
  boundary, not over it. PASS.
- **Independent user outcomes that could ship separately**: YES — this is
  WHY the feature is sliced WS + 3 releases by outcome. The walking
  skeleton (metrics isolation) is independently demonstrable; backward
  compat, log/trace parity, and legible-denials each deliver a verifiable
  behaviour. The split is applied, not deferred.

Verdict: **right-sized AS SLICED**. The walking skeleton is one
demonstrable slice (per-request auth + isolation on metrics); each release
is an independent thin end-to-end slice delivering a verifiable behaviour.
Because all three read handlers already route tenant resolution through the
shared `query-http-common` seam, the auth capability lands ONCE and the
parity slice is wiring — no further split required. Autonomous run
proceeds without a confirmation prompt (decide-don't-ask). DESIGN may
collapse US-RAUTH-03 into the WS if wiring all three at once is a single
small change over the shared capability.
</content>
