# Story Map: trace-lookup-by-id-v0

## User: the on-call operator with a trace_id in hand (and, later, prism's trace client behind them)

## Goal

Pivot from "I have this trace_id" to "here are the spans for this trace,
for my tenant" in one HTTP call, without naming a service or estimating a
time window. Reuse the substrate seam
`ray::TraceStore::get_trace(&tenant, &trace_id)` that already exists at
`crates/ray/src/store.rs:72`; do NOT change ray; do NOT change any storage
trait. This is the read half of "the operator has a trace_id" the platform
should already have served the day `ray-query-api-v0` shipped.

## Backbone

| Receive request | Resolve tenant | Resolve query inputs | Query ray | Render spans |
|-----------------|----------------|----------------------|-----------|--------------|
| Parse the URL (existing route extended OR new path; FLAG 1) | Configured single tenant, fail-closed (existing seam) | Parse and validate `trace_id` (32-char hex, case-insensitive; FLAG 2) | `TraceStore::get_trace(&tenant, &trace_id)` over the durable store | Serialise the bare `Vec<Span>` JSON array (existing `success_response`) |
| (existing window+service shape on `/api/v1/traces` keeps working unchanged) | Header-based tenant (X-Scope-OrgID / aegis Bearer) - later | (later) batch lookup of multiple trace_ids | (later) `query_with(predicate)` on the lookup arm (filter inside a trace) | empty arm (HTTP 200 with `[]`) / 400 invalid trace_id / 401 no tenant / 500 store failure / 400 result-cap (FLAG 3) |
| | | (later) trace assembly: parent/child topology in the response | (later) assemble complete traces from the bare array | (later) per-service projection inside a trace |

---

### Walking Skeleton

Thinnest end-to-end trace-by-id lookup slice (one task per backbone activity):

1. Receive a GET for the trace lookup endpoint with a `trace_id`
   parameter (URL shape per FLAG 1).
2. Resolve tenant via the existing fail-closed seam (reuse
   `KALEIDOSCOPE_TRACE_QUERY_TENANT` and the `Option<TenantId>` router
   parameter; no new mechanism).
3. Parse and validate `trace_id` as a 32-character hex string,
   case-insensitive on the hex digits (FLAG 2 recommendation; OTel
   W3C trace context shape).
4. `TraceStore::get_trace(&tenant, &trace_id)` against the real durable
   `FileBackedTraceStore` (no ray change; the substrate method already
   exists).
5. Serialise the returned `Vec<Span>` as a bare JSON array (HTTP 200,
   `[]` when empty), using the existing `success_response` helper at
   `crates/trace-query-api/src/lib.rs:265`. Errors: 400 invalid
   trace_id, 401 no tenant, 500 store failure, 400 cap breach (FLAG 3).

This is slice 01 (US-01 walking skeleton + US-02 empty + US-03 fail-closed
+ US-04 malformed + US-05 cross-tenant) and is independently demonstrable:
an operator with a `trace_id` GETs the lookup endpoint and sees that
trace's spans, scoped to their tenant, served from the durable ray store,
for the first time.

### Release 1 (Walking Skeleton): the by-id lookup arm ships

Stories: US-01 (known trace_id, happy path), US-02 (unknown trace_id is
200 `[]` not 404), US-03 (tenant fail-closed, 401), US-04 (invalid
trace_id format, 400), US-05 (cross-tenant isolation, 200 `[]`).
Target outcome: an operator pivots from a trace_id to that trace's
spans in one HTTP call, with honest fail-closed tenant scoping and
honest 400 / 200-empty / 500 distinctions. KPI: North Star +
correctness (see `outcome-kpis.md`).
Rationale: validates the riskiest assumption: that the substrate seam
`get_trace`, surfaced on the HTTP boundary with the right URL shape
and the right redaction, gives the operator the lookup arm the
platform should already have shipped, with zero ray change.

### Release 2 (later, NOT in this feature scope): trace assembly and narrowing inside a trace

Stories (deferred): assembling complete traces from spans (parent/child
stitching, root identification); per-service filter inside a trace
(narrowing a multi-service trace to one service's spans); batch lookup
of multiple trace_ids in one request; predicate matchers on a
trace_id-keyed query (likely needs a `get_trace_with(predicate)` seam,
ray-side); REST-style URL paths
(`GET /api/v1/traces/<trace_id>`); a prism trace UI consuming this
arm.

Target outcome: operators get a topology-aware view of a trace and can
narrow inside it; prism gains a trace-detail panel.

## Priority Rationale

Priority by outcome impact and dependency, not by technical layer:

1. **US-01 known trace_id happy path** (P1, Must) - the walking skeleton's
   spine. Without returning the looked-up trace's spans, nothing else has
   value. Highest value; derisks the fatal assumption (does the
   `get_trace` substrate seam, served on the HTTP boundary with the right
   URL shape, give the operator the answer?).
2. **US-03 tenant fail-closed** (P1, Must) - ray is per-tenant; a lookup
   with no tenant cannot run safely. Spans leaking across tenants on the
   lookup arm is a serious breach. Fail-closed is correctness AND
   safety, co-equal with US-01 in the skeleton.
3. **US-05 cross-tenant isolation** (P1, Must) - the riskier sibling of
   US-02. A trace_id matching across tenants must NOT leak. The `(tenant,
   trace_id)` substrate key enforces it; the HTTP boundary preserves it.
   Cheap (one extra fixture in the existing two-tenant pattern), high
   stakes.
4. **US-02 unknown trace_id is 200 `[]`** (P1, Must) - the calm-empty arm
   is the EXISTING contract on the window route (ADR-0048 Decision 2);
   the lookup arm must match. The "never 404" decision is HARD; a 404 on
   an unknown trace_id would mean "the endpoint does not exist", which
   it does. Cheap, high frequency, prevents a false-broken feel.
5. **US-04 invalid trace_id format, 400** (P2, Should) - protects the
   operator from a silent empty when the input is malformed and honours
   the redaction posture (no raw value in error text). Distinct
   outcomes (200 vs 200-empty vs 400 vs 401 vs 500) are what make the
   slice trustworthy.

Deferred (Won't-Have this feature): trace assembly, per-service filter
inside a trace, batch lookup, REST-style URL paths, predicate matchers
on the lookup arm, prism UI for the lookup.

## Scope Assessment: PASS - 5 stories, 1 module (`crates/trace-query-api`), estimated 2-3 days

Oversized signals checked (none tripped at the 2+ threshold):

- User stories: 5 (<= 10). PASS.
- Bounded contexts/modules touched: `crates/trace-query-api` only.
  Reuses `ray` (no change) and `aegis` (no change). 1 modified + 2
  reused (<= 3). PASS.
- Walking skeleton integration points: HTTP client (tower oneshot in
  tests), `ray::TraceStore::get_trace`, `aegis::TenantId`. 3 (<= 5).
  PASS.
- Estimated effort: 2-3 days end-to-end. Parse one parameter and
  validate it as a 32-char hex (~30 lines), branch one handler arm
  (or wire one new handler under a new route, FLAG 1) to call
  `get_trace` (~10 lines), five acceptance scenarios in one new test
  file (~150 lines, mostly fixture seeding via existing helpers),
  one mutation-test pass on the modified file (existing
  `gate-5-mutants-trace-query-api` workflow, no new CI job). PASS at
  feature level.
- Independent shippable outcomes: one (the trace lookup arm closes).
  The deferred items (trace assembly, per-service filter inside a
  trace, batch lookup, REST-style paths, predicate matchers, prism
  UI) are genuinely separate later slices, already excluded. PASS.

Right-sized. No split required. Each story is 2-4 scenarios; the
crafter will enable them one at a time behind the established outer
loop convention (walking skeleton enabled first, others `#[ignore]`d
until enabled).

> NOTE: the only elevated risk is the URL shape choice (FLAG 1: new
> path vs extended parameter). DESIGN picks; either choice keeps the
> slice's size unchanged, only redistributes ~20 lines of routing
> across one or two handlers.
