# Story Map: lumen-query-api-v0

## User: the on-call operator reading logs (and, later, prism's log client behind them)
## Goal: Read logs out of the durable lumen store via an HTTP endpoint, by tenant and time window, closing the logs read loop (ingest -> store -> query -> see) the way query-range-api-v0 closed it for metrics

## Backbone

| Receive request | Resolve tenant | Parse window | Query lumen | Render records |
|-----------------|----------------|--------------|-------------|----------------|
| Parse start/end window | Configured single tenant, fail-closed | Validate [start, end), convert to ns | `LogStore::query(&tenant, range)` over the durable store | Serialise in-window LogRecords as JSON |
| Map on-wire bounds (RED CARD 4) | Header-based tenant (X-Scope-OrgID / aegis Bearer) | (later) severity / level filter | (later) `query_with(predicate)` for attribute matchers | empty arm (HTTP 200) / 400 bad window / 5xx store failure |
| | | (later) full-text body search | (later) pagination / limits | (later) Loki vs plain-array envelope choice (FLAG 1) |

---

### Walking Skeleton

Thinnest end-to-end logs read slice (one task per backbone activity):

1. Receive a GET for the logs endpoint with a window `[start, end]`.
2. Resolve tenant via a single configured tenant, fail-closed (mirrors gateway
   `KALEIDOSCOPE_DEFAULT_TENANT` and the metrics read path `KALEIDOSCOPE_QUERY_TENANT`).
3. Validate the window and convert to the half-open `[start, end)` u64-nanosecond
   `TimeRange`.
4. `LogStore::query(&tenant, range)` against the real durable `FileBackedLogStore`.
5. Serialise the in-window `LogRecord`s as JSON (envelope shape is FLAG 1 for DESIGN);
   empty arm for no match, 400 for a bad window, 5xx for a store failure.

This is slice 01 (US-01 + US-03 happy path + US-02 + US-04) and is independently
demonstrable: an operator GETs the logs endpoint for a tenant over a window and sees the
in-window records that the gateway wrote, read out of the durable lumen store, for the
first time.

### Release 1 (Walking Skeleton): the logs read loop closes
Stories: US-01 (in-window records happy path), US-02 (empty arm calm), US-03 (tenant
scoping, fail-closed slice-01 default), US-04 (bad window 400 + store failure 5xx).
Target outcome: an operator reads real in-window logs served from the durable lumen
store, exercised end to end via tower oneshot. KPI: North Star + correctness (see
outcome-kpis.md).
Rationale: validates the riskiest assumption (can we read the durable log store back
out over HTTP, scoped and honest, at all?).

### Release 2 (later, NOT in this feature scope): narrowing and search
Stories (deferred): severity/level filtering; attribute/resource matchers via
`query_with(predicate)`; full-text body search; pagination/limits/ordering; a prism log
panel and same-origin static serving (FLAG 3).
Target outcome: operators narrow noisy log volumes by level, service, and text.

## Priority Rationale

Priority by outcome impact and dependency, not by technical layer:

1. **US-01 in-window records happy path** (P1, Must) - the walking skeleton's spine.
   Without returning the in-window records from the durable store, nothing else has
   value. Highest value; derisks the fatal assumption (can we read logs back at all?).
2. **US-03 tenant scoping** (P1, Must) - lumen is per-tenant; a log query with no tenant
   cannot run safely, and log bodies leaking across tenants is a serious breach.
   Fail-closed is a correctness AND safety requirement, co-equal with US-01 in the
   skeleton.
3. **US-02 empty arm** (P1, Must) - an empty window / unknown tenant is a common
   operator path and must read as calm, not as an error. Cheap, high frequency, prevents
   a false "broken" feel. Belongs in the skeleton.
4. **US-04 bad window 400 + store failure 5xx** (P2, Should) - protects the operator
   from a misleading empty result when the window is malformed or the backend fails.
   The three-way distinction (empty vs 400 vs 5xx) is what makes the slice trustworthy;
   slightly lower urgency than the success path.

Deferred (Won't-Have this feature): severity filter, attribute matchers, body search,
pagination, prism UI, same-origin serving.

## Scope Assessment: PASS - 4 stories, ~2 modules (new-or-extended query crate + lumen + aegis reuse), estimated 3-5 days

Oversized signals checked (none tripped at the 2+ threshold):
- User stories: 4 (<= 10). PASS.
- Bounded contexts/modules touched: the query service (new crate OR extend `query-api`,
  FLAG 2), reusing lumen and aegis. 1 new-or-extended + 2 reused (<= 3). PASS.
- Walking skeleton integration points: HTTP client (tower oneshot in tests), lumen
  `LogStore::query`, aegis `TenantId`. 3 (<= 5). PASS.
- Estimated effort: 3-5 days (split into 4 right-sized stories, each 1-2 days,
  3-6 scenarios). PASS at feature level.
- Independent shippable outcomes: one (the logs read loop). The deferred items (severity,
  matchers, search, UI) are genuinely separate later slices, already excluded. PASS.

Right-sized. No split required. Each story is 3-4 scenarios and 1-2 days.
