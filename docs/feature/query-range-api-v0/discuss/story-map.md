# Story Map: query-range-api-v0

## User: Prism's query client (machine consumer, pinned contract) + the on-call operator behind it
## Goal: Read metrics out of durable Pulse via a Prometheus-compatible /api/v1/query_range, closing the ingest -> store -> query -> visualise loop

## Backbone

| Receive request | Resolve tenant | Parse selector | Query Pulse | Render matrix |
|-----------------|----------------|----------------|-------------|---------------|
| Parse query/start/end/step | Configured single tenant, fail-closed | Bare metric name | pulse.query over [start,end) ns | Group into series + emit matrix JSON |
| Convert seconds -> nanos | Header-based tenant (X-Scope-OrgID / aegis Bearer) | Single {label="value"} matcher | predicate query_with for matcher | status:error 400 for bad selector |
| | | richer PromQL (operators, fns, rate) | instant /api/v1/query | /config.json serving |

---

### Walking Skeleton

Thinnest end-to-end read slice (one task per backbone activity):

1. Receive `GET /query_range?query=<name>&start=<sec>&end=<sec>&step=15s`, convert seconds to nanoseconds.
2. Resolve tenant via a single configured tenant, fail-closed (mirrors gateway `KALEIDOSCOPE_DEFAULT_TENANT`).
3. Parse a bare metric-name selector.
4. `pulse.query(&tenant, &MetricName, TimeRange[start_ns..end_ns])`.
5. Group rows into matrix series and emit `{status:success, data:{resultType:matrix, result:[...]}}` that Prism's `isPromSuccess` accepts.

This is slice 01 (US-01 + US-02 + US-04 happy path) and is independently demonstrable:
an operator opens Prism against the query backend, queries a metric by name, and sees
its time series plotted, read out of durable Pulse.

### Release 1 (Walking Skeleton): the read loop closes
Stories: US-01 (matrix happy path), US-02 (empty result calm arm), US-03 (bad selector
status:error), US-04 (tenant scoping, fail-closed slice-01 default).
Target outcome: Prism renders a real metric series served from Pulse; the response
round-trips through Prism's own validator. KPI: North Star + Correctness (see outcome-kpis.md).
Rationale: validates the riskiest assumption (does the contract round-trip end to end at all?).

### Release 2 (later, NOT in this feature scope): narrowing and instant
Stories (deferred): single `{label="value"}` matcher via `query_with` predicate;
instant `/api/v1/query`; `/config.json` serving from the backend; richer PromQL.
Target outcome: operators narrow noisy series and get point-in-time reads.

## Priority Rationale

Priority by outcome impact and dependency, not by technical layer:

1. **US-01 matrix happy path** (P1, Must) - the walking skeleton's spine. Without a valid
   matrix that Prism accepts, nothing else has value. Highest value, derisks the fatal
   assumption (contract round-trip).
2. **US-04 tenant scoping** (P1, Must) - Pulse is per-tenant; a query with no tenant cannot
   run safely. Fail-closed is a correctness AND safety requirement, not an enhancement.
   Co-equal with US-01 in the skeleton.
3. **US-02 empty result arm** (P1, Must) - unknown metric / empty range is the single most
   common operator path and Prism renders it as a distinct calm state. Cheap, high
   frequency, prevents a false "error" feel. Belongs in the skeleton.
4. **US-03 bad-selector error** (P2, Should) - protects the operator from silent wrong answers
   when they paste real PromQL the service cannot honour. Needed before the slice is
   trustworthy, slightly lower urgency than the success path.
5. **US-05 scope-boundary guard** (P2, Should) - an explicit, testable rejection of
   out-of-scope queries (logs/traces/full-PromQL) so scope creep is caught at the contract
   edge rather than half-implemented.

Deferred (Won't-Have this feature): label matcher, instant endpoint, /config.json, full PromQL.

## Scope Assessment: PASS - 5 stories, ~2 modules (new query crate/binary + pulse + aegis reuse), estimated 4-6 days

Oversized signals checked (none tripped at the 2+ threshold):
- User stories: 5 (<= 10). PASS.
- Bounded contexts/modules touched: a new query service crate, reusing pulse and aegis. 1 new + 2 reused (<= 3). PASS.
- Walking skeleton integration points: Prism client (HTTP), Pulse query, aegis TenantId. 3 (<= 5). PASS.
- Estimated effort: 4-6 days (slightly above the 1-3 day single-story size but split into 5 right-sized stories). PASS at feature level.
- Independent shippable outcomes: one (the read loop). The deferred items (matcher, instant, config) are genuinely separate later slices, already excluded. PASS.

Right-sized. No split required. Each story is 3-7 scenarios and 1-2 days.
