# Outcome KPIs — read-path-query-api-auth-v0

## Feature: read-path-query-api-auth-v0

### Objective

Within this feature's delivery, make the three live READ query APIs
(`query-api`, `log-query-api`, `trace-query-api`) enforce per-request
tenant authentication + isolation, so every authenticated read is scoped
to the tenant proven by its bearer token, no caller reads another tenant's
data, and an unresolved tenant is refused before the store — while
preserving today's per-instance env-tenant default unchanged (additive
model). Turn the read side from per-DEPLOYMENT tenancy (one tenant per
process) into per-REQUEST tenancy at the boundary, mirroring the ingest
auth (ADR-0068) on every axis except audience.

> **MODEL FORK — ANDREA MAY VETO** (full flag in `wave-decisions.md`):
> the additive model is DECIDED, not final. KPI-5 (backward compatibility)
> is the only KPI that reshapes on a veto to per-request-only.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Clients querying the read APIs with auth configured | Authenticated reads are scoped to the token's tenant | 100% of authenticated read requests (metrics + logs + traces) scoped to the token's tenant; positive + negative isolation control passes | 0% per-request (one tenant per process via env today) | read audit `allow` events (`subject`, `tenant_id`) + isolation tests | Leading (Outcome) |
| 2 | Unauthenticated / invalid-token callers (auth on) | Are refused 401 with nothing read | 100% of tokenless/invalid read requests refused (`401` + `WWW-Authenticate: Bearer`) with the store never touched | 0% (no auth on the read path; one tenant per process) | reject status + absence of a store-read per rejected request | Leading (Outcome) |
| 3 | Operators triaging read-auth denials | Distinguish denial cause by reason | 100% of read deny events carry exactly one of the 8 distinct aegis reasons; zero "unknown/other" bucket | n/a (no read denials today) | distribution of the `reason` field across read deny events | Leading (Secondary) |
| 4 | Adversaries / cross-tenant callers (Mallory) | Cannot read a victim tenant's data | 100% of cross-tenant reads (tenant-A token, tenant-B data) return the wrong tenant's (absent) data, never the victim's | n/a (no per-request scoping today) | the negative-control isolation test per read API (incl. trace lookup-by-id) | Leading (Outcome) |
| 5 | Operators of existing env-tenant read-API deployments | Upgrade with zero behaviour change when no auth is configured | 100% of env-tenant-only deployments behave byte-for-byte as before; 100% of auth-on deployments refuse a missing token rather than downgrading to the env tenant | 100% env-tenant today (the only mode) | existing read-API slice-test suite (unchanged, green) + no-bearer-bypass assertion | Leading (Outcome) |
| 6 | Callers holding an ingest-audience token (Trent) | Cannot replay an ingest token to read | 100% of `kaleidoscope-ingest`-audience tokens rejected `wrong_audience` on the read APIs | n/a (no audience fence on read today) | the wrong-audience reject rate for ingest-audience tokens on the read path | Leading (Secondary) |

### Metric Hierarchy

- **North Star**: % of authenticated read requests scoped to the token's
  tenant, with the isolation negative control passing (KPI-1 + KPI-4). The
  single number that says "the read boundary enforces per-tenant
  isolation". Target 100%; baseline 0%.
- **Leading Indicators**: reject-coverage of invalid read requests
  (KPI-2); reason-coverage of denials (KPI-3); audience-fence coverage
  (KPI-6). Each predicts the north star — if any drops, a cross-tenant or
  cross-surface read can slip through.
- **Guardrail Metrics** (must NOT degrade):
  - The existing env-tenant read happy path and response shape
    (non-regression — System Constraint 6 / KPI-5). An env-tenant-only
    deployment must read exactly as fast and in the same shape as today,
    and an authenticated read must return the same response shape as the
    pre-auth read.
  - The existing fail-closed 401 ("no tenant resolvable") behaviour for
    the unset env tenant — unchanged.
  - Zero secret bytes AND zero raw-token bytes in any log/event/error/body
    line (System Constraint 4) — a hard guardrail: any occurrence is a
    Critical defect.
  - No store read on a refused request (the fail-closed-before-store
    invariant) — a hard guardrail.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | read-API stderr JSON audit events | correlate `decision=allow` events with the scoped store read per request | per request; rolled up per deploy | platform-architect (DEVOPS) |
| 2 | read-API audit + store-read events | per rejected request, assert 401 AND no store read | per request | platform-architect |
| 3 | read-API deny events | tally `reason` field distribution | continuous; reviewed per incident | platform-security operator (Priya) |
| 4 | isolation test suite | per read API, assert the right tenant's data present + the wrong tenant's absent (incl. trace lookup-by-id) | per delivery / CI | acceptance-designer (DISTILL) |
| 5 | existing read-API slice-test suite + no-bearer-bypass test | run unchanged; assert green + assert auth-on missing-token → 401 not env-tenant | per delivery / CI | acceptance-designer |
| 6 | read-API deny events | tally `reason=wrong_audience` for ingest-audience tokens | continuous | platform-security operator |

### Hypothesis

We believe that wiring the correct `aegis::Validator` onto the three read
APIs' request paths as an optional per-request bearer path, fail-closed,
for the platform's query clients will achieve read APIs that return only
the data of the tenant proven by a validated token. We will know this is
true when 100% of authenticated read requests are scoped to the token's
tenant with the isolation negative control passing (KPI-1 + KPI-4), 100%
of invalid/missing-token (auth-on) requests are refused 401 with nothing
read (KPI-2), every read denial reports a distinct aegis reason (KPI-3),
an ingest-audience token cannot read (KPI-6), and existing env-tenant
deployments are byte-for-byte unchanged (KPI-5) — with the env-tenant
happy path, the unset-tenant 401, and the no-secret/token-leak guardrails
holding.

### Handoff to DEVOPS

- **Data collection**: read-API audit events are structured via the
  existing `query_http_common::init_tracing` JSON-stderr subscriber (the
  aegis field contract `tenant_id`/`role`/`decision`/`subject`/`reason`,
  `subject` = `query_range`/`log_query`/`trace_query`). DEVOPS instruments
  the allow↔store-read and deny↔no-store-read correlation per read API.
- **Dashboards**: north-star per-request authenticated + isolated read
  coverage (KPI-1 + KPI-4); reason-distribution panel for read denials
  (KPI-3); audience-fence panel (KPI-6).
- **Alerting thresholds**: KPI-1/KPI-4 < 100% (any cross-tenant read, or
  any authenticated read not scoped to the token's tenant) is a Critical
  alert. Any secret-bytes- or raw-token-in-logs occurrence is a Critical
  alert (guardrail). Any store read on a refused request is a Critical
  alert (guardrail).
- **Baseline measurement**: none needed — baselines are 0% per-request by
  construction (one tenant per process today); KPI-5's baseline is the
  existing env-tenant behaviour captured by the current slice tests.
</content>
