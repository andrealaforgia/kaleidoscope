# Outcome KPIs: ray-query-api-v0

## Feature: ray-query-api-v0

### Objective
Close the platform TRACES read loop: an operator GETs the traces endpoint for a tenant
over a time window and sees the real in-window spans, served from the durable ray store,
instead of having spans written and unseen. The THIRD and final observability pillar, the
analogue of what query-range-api-v0 did for metrics and lumen-query-api-v0 did for logs.
With this, an operator can read all three signals.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | On-call operator | Reads the real in-window spans for a tenant over a window, served from the durable ray store | From "cannot read traces at all" to "a GET returns exactly the in-window spans the write path wrote" | 0% (spans written and unseen; no read path exists, the last pillar) | E2E: ingest in-window and out-of-window spans, GET the endpoint, assert only in-window spans return | Leading (Outcome) |
| 2 | Traces read endpoint | Returns in-window `Span`s carrying every field without loss or rename | 100% of `Span` fields (`trace_id`, `span_id`, `parent_span_id`, `name`, `kind`, start/end times, `status`, `attributes`, `resource_attributes`, `events`, `links`) round-trip | n/a (no serialisation exists) | Field-fidelity acceptance test asserting each field survives the round-trip | Leading (Outcome) |
| 3 | Traces read endpoint | Reads the durable store within a latency budget for a representative window | p95 read latency at most 500 ms on GitHub Actions ubuntu-latest for a window over <= 1000 spans | No endpoint exists | Timed acceptance test in CI (ubuntu-latest), cross-checked with the store's own `record_query` recorder | Leading (Secondary) |
| 4 | Traces read endpoint | Refuses to serve when no tenant resolves and never leaks across tenants | 100% of no-tenant requests refused (401); 0 cross-tenant span leaks | n/a | Tenant-isolation acceptance tests (two-tenant fixture; no-tenant fixture) | Guardrail |

### Metric Hierarchy
- **North Star**: KPI 1 - the operator reads real in-window spans served from the durable
  ray store (the traces read loop closes; the third pillar comes online).
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
hardware. KPI 3's 500 ms p95 is a CI-runner budget for a window over up to ~1000 spans
against the file-backed store; revisit if representative span volumes grow. Per project
memory: Kaleidoscope is pure trunk-based, CI is feedback not a gate, so these KPIs are
correctness signals, not merge blockers.

### Hypothesis
We believe that an HTTP endpoint reading `TraceStore::query(&tenant, &service, range)`
over the durable ray store, for the on-call operator, will close the traces read loop. We
will know this is true when the operator reads exactly the in-window spans the write path
wrote (KPI 1) with every field intact (KPI 2 = 100%), within the CI latency budget (KPI
3), with tenant isolation held (KPI 4). The open question is how the required service key
is supplied (FLAG 3); the KPIs hold under whichever option DESIGN chooses.

## Handoff to DEVOPS (platform-architect)
- Instrument: per-query duration (already a ray recorder seam: `record_query`), returned
  span count, tenant-resolution outcome (resolved vs refused).
- Dashboards: trace-query latency p95, empty-vs-non-empty ratio, refused-request rate.
- Alerting thresholds: KPI 3 p95 > 500 ms; any cross-tenant leak (KPI 4) is a hard alert.
- Baseline: none needed; this is a greenfield traces read path (baseline is
  "unreadable"), the last of the three pillars.
