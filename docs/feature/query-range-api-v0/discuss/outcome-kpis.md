# Outcome KPIs: query-range-api-v0

## Feature: query-range-api-v0

### Objective
Close the platform read loop: an operator opens Prism, queries a metric by name, and
sees its real time series plotted, served from the durable Pulse store, in the exact
Prometheus matrix shape Prism's pinned client accepts.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Prism's query client | Receives a response that its own `isPromSuccess`/`isPromError` validator accepts, for the four known shapes (success, empty, parse-error, http-status) | 100% of contract-test shapes round-trip without falling into `transport-error:shape` | 0% (no backend exists; QueryPanel unmounted) | Contract test that feeds the service's real responses through Prism's validators (or equivalents) | Leading (Outcome) |
| 2 | On-call operator | Sees a real metric series plotted in Prism (not a disabled/unmounted panel), served from durable Pulse | From "cannot read at all" to "renders a series for a known metric over a range" | Read loop open; nothing readable | E2E: ingest a metric via the gateway, query it via the service, assert a non-empty matrix renders | Leading (Outcome) |
| 3 | Query service | Returns a `query_range` matrix within the latency budget on a representative series | p95 query latency at most 500 ms on GitHub Actions ubuntu-latest for a single-metric range over <= 1000 points | No service exists | Timed acceptance test in CI (ubuntu-latest); the service emits its own query duration; cross-checked with Prism footer `queryMs` | Leading (Secondary) |
| 4 | Query service | Refuses to serve when no tenant resolves (fail-closed) | 100% of no-tenant requests refused, 0 cross-tenant leaks | n/a | Acceptance test: request with no resolvable tenant returns refusal; request scoped to tenant A never returns tenant B data | Guardrail |

### Metric Hierarchy
- **North Star**: KPI 2 - the operator sees a real metric series served from Pulse (the read loop closes).
- **Leading Indicators**: KPI 1 (contract round-trips), KPI 3 (latency within budget).
- **Guardrail Metrics**: KPI 4 (tenant fail-closed, zero cross-tenant leak); response-shape correctness must NOT regress (KPI 1 stays at 100%).

### Measurement Plan
| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | Contract test fixtures + Prism validators | CI acceptance stage | Per commit | DELIVER |
| 2 | E2E ingest->query->render | CI E2E (ubuntu-latest) | Per commit | DELIVER |
| 3 | Service-emitted query duration + CI timing | Timed test on ubuntu-latest | Per commit | DEVOPS/DELIVER |
| 4 | Tenant-scoping acceptance tests | CI acceptance stage | Per commit | DELIVER |

### CI realism
All latency budgets are stated against GitHub Actions `ubuntu-latest`, not developer
hardware. KPI 3's 500 ms p95 is a CI-runner budget for a single-metric range query of
up to ~1000 points; revisit if representative series grow. Per project memory:
Kaleidoscope is pure trunk-based, CI is feedback not a gate, so these KPIs are
correctness signals, not merge blockers.

### Hypothesis
We believe that a Prometheus-compatible `/api/v1/query_range` matrix endpoint served
from durable Pulse, for the on-call operator using Prism, will close the read loop.
We will know this is true when Prism's client receives a response its own validator
accepts (KPI 1 = 100%) and the operator sees a real series rendered (KPI 2), within
the CI latency budget (KPI 3), with tenant isolation held (KPI 4).

## Handoff to DEVOPS (platform-architect)
- Instrument: per-query duration (already a Pulse recorder seam: `record_query`), result series/point counts, tenant-resolution outcome (resolved vs refused).
- Dashboards: query latency p95, contract-shape pass rate, empty-vs-success ratio.
- Alerting thresholds: KPI 3 p95 > 500 ms; any cross-tenant leak (KPI 4) is a hard alert.
- Baseline: none needed; this is a greenfield read path (baseline is "unreadable").
