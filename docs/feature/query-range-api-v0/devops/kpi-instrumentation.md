# query-range-api-v0 - KPI instrumentation

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Source KPIs**: `discuss/outcome-kpis.md` (KPI-1..KPI-4)

## Instrumentation posture

The `query-api` crate is exercised in CI as a library plus a thin binary
booted under integration tests; there is no Kaleidoscope-hosted
dashboard, no Prometheus scrape, no alerting webhook (the query-api
runtime is operator-owned, fronted by the operator's reverse proxy).
The "instrumentation" for every KPI is the acceptance-test suite running
under CI Gate 1, with the NEW `gate-5-mutants-query-api` job (A1) as the
test-quality probe that proves those tests actually measure what they
claim. The CI pass/fail IS the measurement; a red gate IS the alert.
KPI-3's latency budget is asserted on GitHub Actions `ubuntu-latest`,
the runner class the budget was calibrated against (outcome-kpis.md "CI
realism"), so the budget is honest from the first commit.

## Per-KPI -> gate mapping

| KPI | Type | Budget / target | Measured by (test) | Gate | Verified against outcome-kpis.md |
|-----|------|-----------------|--------------------|------|----------------------------------|
| KPI-1 - contract round-trip | leading (Outcome) | 100% of the four pinned shapes (success, empty, parse-error, http-status) accepted by Prism's own `isPromSuccess`/`isPromError` validators, none falling into `transport-error:shape` | consumer-driven contract test feeding the service's real responses through Prism's validators (or equivalents) | Gate 1 (correctness) + Gate 5 (probe) | YES - method present in measurement plan |
| KPI-2 - operator sees a real series (NORTH STAR) | leading (Outcome) | from "cannot read at all" to "renders a series for a known metric over a range", served from durable Pulse | E2E: ingest a metric via the gateway, query it via query-api, assert a non-empty matrix renders | Gate 1 (correctness) + Gate 5 (probe) | YES - North Star, KPI 2 |
| KPI-3 - query latency | leading (Secondary) | p95 query latency <= 500 ms on ubuntu-latest for a single-metric range over <= 1000 points | timed acceptance test in CI (ubuntu-latest); service-emitted query duration cross-checked with Prism footer `queryMs` | Gate 1 | YES - pinned to ubuntu-latest |
| KPI-4 - tenant fail-closed (GUARDRAIL) | guardrail | 100% of no-tenant requests refused; 0 cross-tenant leaks (tenant A never returns tenant B data) | acceptance test: no-resolvable-tenant returns refusal; A-scoped request never returns B data | Gate 1 (correctness) + Gate 5 (probe) | YES - guardrail, KPI 4 |

## Metric hierarchy (from outcome-kpis.md)

- **North Star**: KPI-2 - the on-call operator sees a real metric series
  plotted in Prism, served from the durable Pulse store (the read loop
  closes). Enforced by the E2E ingest-to-query-to-render Gate 1 test;
  its mutation coverage is enforced by the new `gate-5-mutants-query-api`
  job, the only mechanism that proves a behaviourally-mutated translator
  (a flipped label-merge precedence, a dropped point, a wrong
  time-conversion branch) is killed rather than silently passing.
- **Leading indicators**: KPI-1 (contract round-trips at 100% against
  Prism's own validators) and KPI-3 (p95 query latency within the
  500 ms CI budget). Each is a Gate 1 test.
- **Guardrails**: KPI-4 (tenant fail-closed, zero cross-tenant leak),
  and the standing constraint that response-shape correctness must NOT
  regress (KPI-1 stays at 100%). KPI-4 is the security guardrail - a
  cross-tenant leak is the worst failure this service can have - and is
  the DD7 fail-closed invariant Gate 5 must prove the tests can detect.

## CI-realism calibration

KPI-3's p95 <= 500 ms is set against GitHub Actions `ubuntu-latest` from
DISCUSS, not a fast workstation - per the project CI-realism discipline
(locally-calibrated budgets fail on CI). The budget covers a
single-metric range query of up to ~1000 points; revisit if
representative series grow (outcome-kpis.md). Per project memory,
Kaleidoscope is pure trunk-based and CI is feedback not a gate, so these
KPIs are correctness signals, not hard merge blockers - but a red Gate 1
is still the alert the team acts on before merge.

## Guardrail alerting model

| KPI | Alert condition | Channel | Response |
|-----|-----------------|---------|----------|
| KPI-1 | a pinned response shape is rejected by Prism's validator (falls into `transport-error:shape`) | red Gate 1 in CI | block-or-escalate before merge - contract drift breached; the round-trip is the contract |
| KPI-2 | the E2E test renders no series for a known ingested metric | red Gate 1 in CI | block-or-escalate - North Star read loop broken |
| KPI-3 | p95 query latency > 500 ms on ubuntu-latest | red Gate 1 in CI | investigate before merge; do not raise the budget without measured justification |
| KPI-4 | a no-tenant request is served, OR tenant A receives tenant B data | red Gate 1 in CI - HARD alert | block - cross-tenant leak is the highest-severity failure; DD7 fail-closed invariant breached |

No external dashboards or pagers on the Kaleidoscope side. For a
read-only HTTP service whose only driven dependency is the local Pulse
store, the test suite under CI Gate 1 plus the Gate 5 mutation probe is
the complete and correct instrumentation surface. The DESIGN handoff in
outcome-kpis.md notes operator-side instrumentation seams for when the
running service is monitored (per-query duration via the Pulse
`record_query` recorder seam, result series/point counts, tenant-
resolution outcome resolved-vs-refused; dashboards for query-latency
p95, contract-shape pass rate, empty-vs-success ratio; a hard alert on
any cross-tenant leak). Those are operator-owned and out of scope for
this wave, recorded so the seam is not lost.
