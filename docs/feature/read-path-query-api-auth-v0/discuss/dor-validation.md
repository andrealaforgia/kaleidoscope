# Definition of Ready Validation — read-path-query-api-auth-v0

9-item hard gate. Each item must PASS with evidence before handoff to
DESIGN. Validated 2026-06-08 by Luna (`nw-product-owner`).

> **MODEL FORK — ANDREA MAY VETO**: these stories proceed on the ADDITIVE
> model (full flag in `wave-decisions.md`). DoR passes under both models;
> only US-RAUTH-02 reshapes on a veto to per-request-only. The veto does
> not block readiness — it is a recorded decision Andrea may overturn.

## US-RAUTH-01 — Walking slice: per-request bearer auth + isolation on the metrics query API

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear, domain language | PASS | "Priya cannot certify per-tenant read isolation; query-api resolves one tenant per process from KALEIDOSCOPE_QUERY_TENANT and applies it to every request, and no caller's identity is checked." Domain language, no solution. |
| 2 | User/persona with specific characteristics | PASS | Priya (platform-security operator, `acme-observability`, audit-accountable); Nadia (SRE, queries her tenant's metrics); Mallory (cross-tenant caller). |
| 3 | 3+ domain examples with real data | PASS | Nadia's `acme-prod` `up`-series read; Mallory's `globex-staging` cross-tenant ABSENT read (negative control); Nadia's missing-token 401-before-store. Real tenants, series, audience. |
| 4 | UAT in Given/When/Then (3-7) | PASS | 5 scenarios: own-tenant read, isolation, no-token-401-before-store, expired reject, redaction. |
| 5 | AC derived from UAT | PASS | 6 AC trace to the 5 scenarios (valid-read, isolation, no-token-before-store, expired-reason, secret/token-redaction, one-event). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | One read API, the per-request boundary + isolation + shared capability, end-to-end. 5 scenarios. Demonstrable in a single session. |
| 7 | Technical notes: constraints/dependencies | PASS | DD1 (config fields), DD2 (HTTP 401 + WWW-Authenticate), DD3 (additive precedence), shared-capability placement in query-http-common, fail-closed seam reuse (F2). |
| 8 | Dependencies resolved or tracked | PASS | aegis v0 (`Validator`/`TenantContext`/`validate_with_subject`/`load_catalogue`) available; `query-http-common` shared seam available (F2); ADR-0068 ingest shape to mirror available. No DIVERGE (tracked as R8). |
| 9 | Outcome KPIs with measurable targets | PASS | KPI-1 (100% authenticated reads scoped to token tenant), KPI-2 (100% invalid refused, store untouched), KPI-4 (isolation negative control). Baseline 0% per-request. |

### DoR Status: PASSED

## US-RAUTH-02 — Backward compatibility (the additive default)

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear | PASS | "Priya cannot risk breaking the fleet of existing single-tenant env-tenant deployments while adding per-request auth; the additive promise must be a tested property, not an assumption." |
| 2 | Persona specific | PASS | Priya (mixed rollout); Omar (legacy single-tenant operator, `KALEIDOSCOPE_LOG_QUERY_TENANT=acme-prod`). |
| 3 | 3+ examples real data | PASS | Omar's unchanged env-tenant read; the unset-env 401 unchanged; the auth-on no-downgrade-to-env-tenant case. |
| 4 | UAT G/W/T (3-7) | PASS | 3 scenarios: env-tenant-unchanged, unset-env-still-401, auth-on-no-downgrade. |
| 5 | AC from UAT | PASS | 4 AC: env-unchanged, unset-still-401, no-bearer-bypass, existing-slice-tests-green. |
| 6 | Right-sized | PASS | The additive precedence + the opt-in fence. 3 scenarios. Smaller than the WS. |
| 7 | Technical notes | PASS | DD3 precedence; the VETO PIVOT recorded (on per-request-only this story becomes "auth always required, no-config refuses to start"); composition.rs resolve_tenant/probe unchanged. |
| 8 | Dependencies tracked | PASS | Depends on US-RAUTH-01 (the per-request capability it gates as opt-in). Tracked. |
| 9 | Outcome KPIs | PASS | KPI-5 (100% env-tenant deployments unchanged; 100% no-bearer-bypass). Baseline 100% env-tenant (the only mode today). |

### DoR Status: PASSED

## US-RAUTH-03 — Parity: log + trace query APIs

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear | PASS | "An isolated metrics path beside per-instance log and trace paths is still a partially-open read surface; a caller still reads whatever tenant the log/trace process is pinned to." |
| 2 | Persona specific | PASS | Priya (full read surface), Nadia (logs + traces), Mallory (cross-tenant on both). |
| 3 | 3+ examples real data | PASS | Nadia's `acme-prod` logs read; Mallory's `globex-staging` cross-tenant trace lookup ABSENT; Nadia's invalid-signature 401-before-store on logs. |
| 4 | UAT G/W/T | PASS | 4 scenarios: own-tenant logs, trace isolation (incl. lookup-by-id), no-token-401 on both, invalid-signature reject. |
| 5 | AC from UAT | PASS | 6 AC incl. isolation on both, 401-before-store on both, shared-capability-reused. |
| 6 | Right-sized | PASS | Reuses the WS capability across 2 read APIs; the three handlers already share the tenant seam, so this is wiring. 4 scenarios. |
| 7 | Technical notes | PASS | Reuses the query-http-common capability; trace lookup-by-id (ADR-0053) must also be isolated; depends on US-RAUTH-01. |
| 8 | Dependencies tracked | PASS | Depends on US-RAUTH-01. Tracked. May collapse into WS at DESIGN's call (placement note). |
| 9 | Outcome KPIs | PASS | KPI-1 + KPI-2 + KPI-4 extended to logs + traces (full read surface). Baseline 0% per-request. |

### DoR Status: PASSED

## US-RAUTH-04 — Legible denials + the cross-surface audience fence

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear | PASS | "A fail-closed read boundary is necessary but not sufficient; Priya must triage by cause and be certain an ingest-minted token cannot be replayed to read." |
| 2 | Persona specific | PASS | Priya (triage), Riley (audit query), Trent (ingest-audience token holder). |
| 3 | 3+ examples real data | PASS | 318 expired vs 2 forged-signature triage; Trent's `kaleidoscope-ingest` token rejected wrong_audience; `auditor` unknown-role. |
| 4 | UAT G/W/T | PASS | 3 scenarios: ingest-audience-rejected, unknown-role-rejected, all-8-reasons-distinct. |
| 5 | AC from UAT | PASS | 5 AC incl. matching-reason, ingest-audience-fence, one-event-per-reject, reasons-distinct, DD6-role-resolved. |
| 6 | Right-sized | PASS | Reason-surfacing across the existing reject paths + the audience-fence + one decision (DD6). 3 scenarios. |
| 7 | Technical notes | PASS | `ValidationError::reason()` reused verbatim; the audience fence is the SAME exact-audience check configured with `kaleidoscope-query`; DD5+DD6. |
| 8 | Dependencies tracked | PASS | Depends on US-RAUTH-01 + US-RAUTH-03. Tracked. |
| 9 | Outcome KPIs | PASS | KPI-3 (100% denials carry one of 8 distinct reasons), KPI-6 (100% ingest-audience tokens rejected wrong_audience). |

### DoR Status: PASSED

## Anti-Pattern Scan

| Anti-Pattern | Found? | Note |
|--------------|--------|------|
| Implement-X | No | Every story starts from a user pain (Priya can't certify read isolation; Mallory reads a victim tenant; Trent replays an ingest token). |
| Generic data | No | Real tenants (`acme-prod`, `globex-staging`), personas (Priya/Nadia/Mallory/Omar/Riley/Trent), real endpoints (`GET /api/v1/query_range`, `/api/v1/logs`, trace lookup-by-id), real audiences (`kaleidoscope-query` vs `kaleidoscope-ingest`), real series (`up`). |
| Technical AC | No | AC are observable outcomes (status, store-untouched, isolation present/absent, audit reason, redaction) not mechanisms; tech choices live in DD1-DD6 for DESIGN. |
| Technical scenario title | No | Titles are business outcomes ("An authenticated client reads its own tenant's metrics", "A token for one tenant cannot read another tenant's traces", "A token minted for the ingest audience is rejected on the read path"). |
| Oversized story | No | Largest is the WS at 5 scenarios; feature sliced WS + 3 releases by outcome. |
| Abstract requirements | No | 3+ concrete examples per story; positive + negative isolation control concrete. |

## Overall: PASSED (4/4 stories, all 9 items each)

Solution-neutrality verified: requirements state WHAT must be observable
(scoped-to-token-tenant / store-untouched-on-refusal / wrong-tenant-absent
/ reason / wrong_audience-fence / secret-and-token-never-logged /
env-tenant-unchanged); the six DESIGN decisions (DD1-DD6) carry the HOW.

Elevator Pitch check (Dimension 0): every story has an Elevator Pitch with
Before / After / Decision enabled; every "After" references a real
user-invocable entry point (a real HTTP GET to `/api/v1/query_range`,
`/api/v1/logs`, or the trace lookup with an `Authorization: Bearer`
header); every "sees" clause is concrete observable output (the right
tenant's matrix/logs/spans returned, the wrong tenant's data absent, a 401
with `WWW-Authenticate: Bearer`); every "Decision enabled" names a real
decision (Priya certifies read isolation; operators adopt with no
regression; Priya triages by reason and proves ingest tokens cannot read).
No `@infrastructure`-only slice.
</content>
