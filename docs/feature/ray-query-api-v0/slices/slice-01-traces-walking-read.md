# Slice 01: traces walking read

## Elevator Pitch
An operator GETs the traces endpoint for a tenant over a time window `[start, end]` and
sees the in-window `Span`s returned as JSON, read out of the real durable ray
`FileBackedTraceStore`. The traces read loop (ingest -> store -> query -> see) closes for
the first time, the THIRD and final observability pillar, the analogue of what
lumen-query-api-v0 did for logs and query-range-api-v0 did for metrics.

## Learning hypothesis
We believe an HTTP endpoint calling `TraceStore::query(&tenant, &service, range)` over the
durable ray store will let an operator read real in-window spans, scoped fail-closed and
with honest empty/400/401/500 arms. We will know this is true when an end-to-end test
(ingest in-window and out-of-window spans, GET the endpoint via tower oneshot, assert only
the in-window spans come back, with every field intact) passes against the real
`FileBackedTraceStore`. Size: <= 1 day for the walking-skeleton happy path (US-01 + US-03
happy path); the empty and failure arms (US-02, US-04) follow within the slice. The one
open risk is the service-key question (FLAG 3 / RED CARD 5), resolved in DESIGN, not here.

## Stories in this slice
- US-01 Read the in-window spans for a tenant over HTTP (P1, Must)
- US-03 Scope every trace query to one tenant, fail-closed (P1, Must)
- US-02 Return a calm empty result when nothing is in the window (P1, Must)
- US-04 Reject a malformed window and surface a store failure honestly (P2, Should)

## Walking-skeleton scenario
Ingest some `Span`s in-window and out-of-window for tenant "acme-prod" / service
"checkout", GET the traces endpoint for "acme-prod" over `[start, end]`, and only the
in-window spans come back, in ascending `start_time` order, with every field intact.
Exercised in the acceptance suite via the tower `oneshot` pattern against the real
`FileBackedTraceStore`.

## End-to-end demonstrable behaviour
A GET for the traces endpoint with a tenant and a window `[start, end]` returns the
in-window `Span`s as JSON, scoped to one fail-closed tenant, with a calm empty arm (HTTP
200), a 400 for a malformed window, a 401 for no resolvable tenant, and a 500 for a store
failure, the arms all distinct.

## Carpaccio taste tests
- Vertical (end to end): YES. Request -> tenant resolve -> service/window resolve -> store
  query -> JSON response. A real, observable HTTP read, not a layer.
- Demonstrable in one session: YES. Ingest, GET, see the in-window spans.
- Delivers user value alone: YES. The operator can read spans that were previously written
  and unseen; the third pillar comes online.
- Thin (no fat): YES. No trace-id lookup, no trace assembly, no operation/duration filter,
  no predicate matchers, no pagination. Tenant + window (+ service key per FLAG 3) only.
- Independently shippable: YES. Does not depend on any deferred slice.

## Store surface (verified, do not redesign)
- `TraceStore::query(&self, tenant: &TenantId, service: &ServiceName, range: TimeRange) -> Result<Vec<Span>, TraceStoreError>`
  (`crates/ray/src/store.rs:80`); durable adapter `FileBackedTraceStore`
  (`crates/ray/src/file_backed.rs:203`, query at `:243`). NOTE the REQUIRED `&service`
  (CONTRADICTION 1 / FLAG 3 / RED CARD 5): the store has NO tenant+range-only query.
- `get_trace(&tenant, &trace_id)` and `query_with(&tenant, &service, range, &predicate)`
  also exist but are OUT of scope for slice 01.
- `TimeRange` half-open `[start_unix_nano, end_unix_nano)`, u64 nanoseconds
  (`crates/ray/src/span.rs:247`).
- `Span` fields: `trace_id`, `span_id`, `parent_span_id`, `name`, `kind`,
  `start_time_unix_nano`, `end_time_unix_nano`, `status`, `attributes`,
  `resource_attributes`, `events`, `links` (`crates/ray/src/span.rs:184`). Already derives
  `serde::Serialize`; `trace_id`/`span_id` serialise as lowercase hex.
- `Span::service_name()` reads `resource_attributes["service.name"]`, defaulting to `""`;
  empty-service spans are indexed by trace only, not by service (`file_backed.rs:325`).
- `TraceStoreError::PersistenceFailed { reason }` is the only typed failure
  (`crates/ray/src/store.rs:35`).
- Test-build reference: `crates/ray/tests/v1_slice_02_snapshot.rs` shows the
  `span(trace, span, service, name, start, end)` helper and how spans are built, seeded,
  and queried.

## Integration points (3)
1. HTTP client (the contract consumer; exercised via tower `oneshot` in the acceptance
   suite).
2. ray `TraceStore::query(&TenantId, &ServiceName, TimeRange)` against the durable
   `FileBackedTraceStore`.
3. aegis `TenantId` (tenant scoping vocabulary).

## Flagged to DESIGN (do NOT decide in this slice)
- FLAG 1 (response contract): plain JSON array of `Span`s (the shape ADR-0047 chose for
  logs) versus a richer / assembled-trace / Tempo-shaped envelope. Flagged, not chosen.
- FLAG 2 (placement): a NEW crate (`trace-query-api`) versus extending an existing query
  crate. ADR-0047 chose a new crate for logs. Flagged, not chosen.
- FLAG 3 / RED CARD 5 (the service key): how slice 01 reconciles "query by tenant + range"
  with a store whose range query REQUIRES a `&ServiceName`. Options: (a) configured/known
  single service; (b) service as an explicit request parameter (recommended slice-01
  default, no store change); (c) fan-out across a tenant's services (needs a store change
  to enumerate services; heaviest, a later slice). Flagged, not chosen.
- FLAG 4 (granularity): raw spans (slice 01) versus assembled complete traces (deferred).
  Likely raw spans for v0. Flagged, not chosen.
- FLAG 5 (same-origin / static serving for a future prism trace UI): out of slice 01.
- RED CARD 3 (tenant supply): configured single tenant (recommended slice-01 default,
  fail-closed) versus `X-Scope-OrgID` header versus aegis Bearer token.
- RED CARD 4 (window on the wire): epoch seconds (recommended, mirrors the metrics and
  logs endpoints) versus RFC3339 versus nanoseconds; convert exactly to the u64-ns
  `TimeRange`.

## Out of scope (deferred, declared)
Lookup by `trace_id`; assembling complete traces from spans; filters by service /
operation (`name`) / duration; predicate matchers on `kind` / `status` / attributes (even
though `query_with(predicate)` exists); a tenant+range fan-out across services;
pagination / limits / ordering beyond the store's natural ascending `start_time` order;
any prism UI; same-origin static serving for prism; PromQL (traces are not metrics).

## KPIs targeted
North Star (KPI 1: operator reads real in-window spans), KPI 2 (every Span field
round-trips, 100%), KPI 3 (p95 <= 500 ms on ubuntu-latest), KPI 4 (tenant fail-closed, 0
cross-tenant leaks).
