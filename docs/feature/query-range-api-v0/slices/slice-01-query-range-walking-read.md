# Slice 01: query_range walking read

## Elevator Pitch
An operator opens Prism against the query backend, queries a metric by name, and sees
its time series plotted, read out of the durable Pulse store. The read loop
(ingest -> store -> query -> visualise) closes for the first time.

## Stories in this slice
- US-01 Serve a metric time series as a Prometheus matrix (P1, Must)
- US-04 Scope every query to one tenant, fail-closed (P1, Must)
- US-02 Return a calm empty result for an unknown metric or empty range (P1, Must)
- US-03 Reject an unparseable query with an operator-readable error (P2, Should)
- US-05 Hold the v0 scope boundary at the contract edge (P2, Should)

## End-to-end demonstrable behaviour
`GET /api/v1/query_range?query=<bare_metric_name>&start=<sec>&end=<sec>&step=15s`
returns a Prometheus `matrix` that Prism's `isPromSuccess` validator accepts, scoped to
one fail-closed tenant, with empty and error arms that Prism renders distinctly.

## Contract (pinned, do not redesign)
- Request params: `query` (raw PromQL, bare name only), `start`/`end` (epoch seconds), `step=15s`.
- Success: `{status:"success", data:{resultType:"matrix", result:[{metric:{...}, values:[[sec,"val"],...]}]}}`.
- Empty: `result: []`. Error: HTTP 400 `{status:"error", error:"..."}`.
- Source of truth: `apps/prism/src/lib/promql/queryRange.ts`, ADR-0027 §2.

## Integration points (3)
1. Prism HTTP client (the contract consumer).
2. Pulse `query(&TenantId, &MetricName, TimeRange)` (durable read).
3. aegis `TenantId` (tenant scoping vocabulary).

## Open decisions handed to DESIGN
- RED CARD 1: tenant-supply mechanism (slice-01 default: configured single tenant, fail-closed).
- RED CARD 2: raw points vs step-resampling (recommended: raw points, no resample at v0).
- RED CARD 3: matrix series grouping key (recommended: one series per merged label set, include __name__).
- Where the service lives (new crate/binary depending on pulse + aegis).

## Out of scope (deferred)
Single `{label="value"}` matcher; instant `/api/v1/query`; `/config.json` serving;
operators, functions, aggregations, range vectors, full PromQL; logs and traces.

## KPIs targeted
North Star (KPI 2: operator sees a real series), KPI 1 (contract round-trip 100%),
KPI 3 (p95 <= 500 ms on ubuntu-latest), KPI 4 (tenant fail-closed, 0 leaks).
