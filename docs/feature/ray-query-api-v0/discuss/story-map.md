# Story Map: ray-query-api-v0

## User: the on-call operator reading traces (and, later, prism's trace client behind them)
## Goal: Read spans out of the durable ray store via an HTTP endpoint, by tenant and time window, closing the traces read loop (ingest -> store -> query -> see) the way lumen-query-api-v0 closed it for logs and query-range-api-v0 closed it for metrics. This is the THIRD and last of the three observability pillars.

## Backbone

| Receive request | Resolve tenant | Resolve service + window | Query ray | Render spans |
|-----------------|----------------|--------------------------|-----------|--------------|
| Parse start/end window | Configured single tenant, fail-closed | Resolve service key (FLAG 3) + validate `[start, end)`, convert to ns | `TraceStore::query(&tenant, &service, range)` over the durable store | Serialise in-window `Span`s as JSON |
| Map on-wire bounds (RED CARD 4) | Header-based tenant (X-Scope-OrgID / aegis Bearer) | (later) filter by operation/duration | (later) `query_with(predicate)` for matchers; `get_trace` by id | empty arm (HTTP 200) / 400 bad window / 401 no tenant / 500 store failure |
| | | (later) tenant+range-only fan-out across services (needs store change) | (later) assemble complete traces from spans | (later) plain-array vs richer/Tempo envelope (FLAG 1) |

---

### Walking Skeleton

Thinnest end-to-end traces read slice (one task per backbone activity):

1. Receive a GET for the traces endpoint with a window `[start, end]`.
2. Resolve tenant via a single configured tenant, fail-closed (mirrors gateway
   `KALEIDOSCOPE_DEFAULT_TENANT`, the metrics read path `KALEIDOSCOPE_QUERY_TENANT`, and
   the logs read path `KALEIDOSCOPE_LOG_QUERY_TENANT`).
3. Resolve the service key (FLAG 3; recommended slice-01 default: an explicit request
   parameter) and validate the window, converting to the half-open `[start, end)`
   u64-nanosecond `TimeRange`.
4. `TraceStore::query(&tenant, &service, range)` against the real durable
   `FileBackedTraceStore`.
5. Serialise the in-window `Span`s as JSON (envelope shape is FLAG 1 for DESIGN); empty
   arm for no match, 400 for a bad window, 401 for no tenant, 500 for a store failure.

This is slice 01 (US-01 + US-03 happy path + US-02 + US-04) and is independently
demonstrable: an operator GETs the traces endpoint for a tenant over a window and sees
the in-window spans that the write path wrote, read out of the durable ray store, for the
first time.

### Release 1 (Walking Skeleton): the traces read loop closes
Stories: US-01 (in-window spans happy path), US-02 (empty arm calm), US-03 (tenant
scoping, fail-closed slice-01 default), US-04 (bad window 400 + no tenant 401 + store
failure 500).
Target outcome: an operator reads real in-window spans served from the durable ray store,
exercised end to end via tower oneshot. KPI: North Star + correctness (see
outcome-kpis.md).
Rationale: validates the riskiest assumption (can we read the durable trace store back
out over HTTP, scoped and honest, at all, given the store has no tenant+range-only
query?).

### Release 2 (later, NOT in this feature scope): trace-id lookup, assembly, and narrowing
Stories (deferred): lookup by `trace_id` via `get_trace`; assembling complete traces from
spans (parent/child stitching); filters by operation (`name`) / duration; predicate
matchers on `kind` / `status` / attributes via `query_with(predicate)`; a tenant+range
fan-out across services (which needs a store change to enumerate a tenant's services);
pagination/limits; a prism trace panel and same-origin static serving (FLAG 5).
Target outcome: operators pivot from a window to a specific trace, see assembled traces,
and narrow noisy span volumes.

## Priority Rationale

Priority by outcome impact and dependency, not by technical layer:

1. **US-01 in-window spans happy path** (P1, Must) - the walking skeleton's spine.
   Without returning the in-window spans from the durable store, nothing else has value.
   Highest value; derisks the fatal assumption (can we read traces back at all, over a
   store whose range query needs a service key?).
2. **US-03 tenant scoping** (P1, Must) - ray is per-tenant; a trace query with no tenant
   cannot run safely, and span attributes leaking across tenants is a serious breach.
   Fail-closed is a correctness AND safety requirement, co-equal with US-01 in the
   skeleton.
3. **US-02 empty arm** (P1, Must) - an empty window / unknown service is a common
   operator path and must read as calm, not as an error. Cheap, high frequency, prevents
   a false "broken" feel. Belongs in the skeleton.
4. **US-04 bad window 400 + no tenant 401 + store failure 500** (P2, Should) - protects
   the operator from a misleading empty result when the window is malformed or the
   backend fails. The distinct outcomes (empty vs 400 vs 401 vs 500) are what make the
   slice trustworthy; slightly lower urgency than the success path.

Deferred (Won't-Have this feature): trace-id lookup, trace assembly, operation/duration
filters, predicate matchers, tenant+range fan-out, pagination, prism UI, same-origin
serving.

## Scope Assessment: PASS - 4 stories, ~2 modules (new-or-extended trace-query crate + ray + aegis reuse), estimated 3-5 days

Oversized signals checked (none tripped at the 2+ threshold):
- User stories: 4 (<= 10). PASS.
- Bounded contexts/modules touched: the trace-query service (new crate OR extend, FLAG
  2), reusing ray and aegis. 1 new-or-extended + 2 reused (<= 3). PASS.
- Walking skeleton integration points: HTTP client (tower oneshot in tests), ray
  `TraceStore::query`, aegis `TenantId`. 3 (<= 5). PASS.
- Estimated effort: 3-5 days (split into 4 right-sized stories, each 1-2 days, 3-4
  scenarios). PASS at feature level.
- Independent shippable outcomes: one (the traces read loop). The deferred items (trace
  lookup, assembly, filters, matchers, fan-out, UI) are genuinely separate later slices,
  already excluded. PASS.

Right-sized. No split required. Each story is 3-4 scenarios and 1-2 days.

> NOTE: the one elevated risk is NOT scope size but the store contradiction (no
> tenant+range-only query; the range query requires a service). That is surfaced as
> CONTRADICTION 1 / FLAG 3 / RED CARD 5 in wave-decisions.md and handed to DESIGN; it
> does not change the slice's size, only how the service key is supplied.
