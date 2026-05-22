# Outcome KPIs: lumen-query-api-v0

## Feature: lumen-query-api-v0

### Objective
Close the platform LOGS read loop: an operator GETs the logs endpoint for a tenant over
a time window and sees the real in-window log records, served from the durable lumen
store, instead of having logs written and unseen. The second observability pillar, the
analogue of what query-range-api-v0 did for metrics.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | On-call operator | Reads the real in-window log records for a tenant over a window, served from the durable lumen store | From "cannot read logs at all" to "a GET returns exactly the in-window records the gateway wrote" | 0% (logs written and unseen; no read path exists) | E2E: ingest in-window and out-of-window records, GET the endpoint, assert only in-window records return | Leading (Outcome) |
| 2 | Logs read endpoint | Returns in-window `LogRecord`s carrying every field without loss or rename | 100% of `LogRecord` fields (`observed_time`, severity, body, attributes, resource attributes, trace/span ids) round-trip | n/a (no serialisation exists) | Field-fidelity acceptance test asserting each field survives the round-trip | Leading (Outcome) |
| 3 | Logs read endpoint | Reads the durable store within a latency budget for a representative window | p95 read latency at most 500 ms on GitHub Actions ubuntu-latest for a window over <= 1000 records | No endpoint exists | Timed acceptance test in CI (ubuntu-latest), cross-checked with the store's own `record_query` recorder | Leading (Secondary) |
| 4 | Logs read endpoint | Refuses to serve when no tenant resolves and never leaks across tenants | 100% of no-tenant requests refused; 0 cross-tenant log leaks | n/a | Tenant-isolation acceptance tests (two-tenant fixture; no-tenant fixture) | Guardrail |

### Metric Hierarchy
- **North Star**: KPI 1 - the operator reads real in-window logs served from the durable
  lumen store (the logs read loop closes).
- **Leading Indicators**: KPI 2 (fields round-trip), KPI 3 (latency within budget).
- **Guardrail Metrics**: KPI 4 (tenant fail-closed, zero cross-tenant leak); field
  fidelity (KPI 2) must NOT regress below 100%.

### Measurement Plan
| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | E2E ingest -> query -> read (tower oneshot) | CI acceptance stage | Per commit | DELIVER |
| 2 | Field-fidelity acceptance test | CI acceptance stage | Per commit | DELIVER |
| 3 | Store-emitted query duration + CI timing | Timed test on ubuntu-latest | Per commit | DEVOPS/DELIVER |
| 4 | Tenant-scoping acceptance tests | CI acceptance stage | Per commit | DELIVER |

### CI realism
All latency budgets are stated against GitHub Actions `ubuntu-latest`, not developer
hardware. KPI 3's 500 ms p95 is a CI-runner budget for a window over up to ~1000
records against the file-backed store; revisit if representative log volumes grow. Per
project memory: Kaleidoscope is pure trunk-based, CI is feedback not a gate, so these
KPIs are correctness signals, not merge blockers.

### Hypothesis
We believe that an HTTP endpoint reading `LogStore::query(&tenant, range)` over the
durable lumen store, for the on-call operator, will close the logs read loop. We will
know this is true when the operator reads exactly the in-window records the gateway
wrote (KPI 1) with every field intact (KPI 2 = 100%), within the CI latency budget
(KPI 3), with tenant isolation held (KPI 4).

## Handoff to DEVOPS (platform-architect)
- Instrument: per-query duration (already a lumen recorder seam: `record_query`),
  returned record count, tenant-resolution outcome (resolved vs refused).
- Dashboards: log-query latency p95, empty-vs-non-empty ratio, refused-request rate.
- Alerting thresholds: KPI 3 p95 > 500 ms; any cross-tenant leak (KPI 4) is a hard
  alert.
- Baseline: none needed; this is a greenfield logs read path (baseline is "unreadable").
