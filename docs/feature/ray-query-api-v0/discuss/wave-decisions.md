# Wave Decisions: ray-query-api-v0 (DISCUSS)

British English throughout. No em dashes.

## Configuration (decided, not asked)

| # | Decision | Value | Implication |
|---|----------|-------|-------------|
| 1 | Feature type | Backend | HTTP read service; no TUI/web mockups; contract-first |
| 2 | Research depth | Lightweight | Primary "user" is an operator (and later prism) reading traces over HTTP; emotional arc kept brief |
| 3 | JTBD | No | No diverge artifacts; job grounded informally in the traces read-loop closure |
| 4 | Walking skeleton | Yes (read half of an existing write loop) | Ray store, aegis tenancy, and the aperture trace write path all exist; this adds the missing read half |

## What this feature opens

Kaleidoscope stores traces durably in the `ray` crate (`FileBackedTraceStore`,
`crates/ray/src/file_backed.rs`); the aperture write path persists spans via ray. But
there is NO way to read them back over HTTP: spans are written and unseen. This feature
is the HTTP read path for TRACES, the THIRD and final observability pillar, the exact
analogue of what `query-range-api-v0` (ADR-0042) did for metrics and what
`lumen-query-api-v0` (ADR-0047) just did for logs. For metrics and logs an operator can
now query; for traces they can do nothing yet. This feature lets an operator (and,
later, prism) READ the stored spans, querying them by tenant and time window over HTTP.

## DIVERGE artifacts

None present at `docs/feature/ray-query-api-v0/diverge/`. No prior JTBD run. Risk noted:
the job statement is grounded informally in the platform read-loop narrative and the
verified `TraceStore` surface rather than a validated ODI job analysis. Low risk: the
store surface is concrete and verified, and TWO directly analogous precedents exist (the
metrics read path, ADR-0042, and the logs read path, ADR-0047), removing most
requirement ambiguity. The logs feature is the symmetric sibling and was used for voice
and shape.

## Verified facts (stories are grounded in these, read 2026-05-22)

1. `crates/ray/src/store.rs` defines `pub trait TraceStore` with four methods:
   `ingest(&tenant, batch)` (`store.rs:67`), `get_trace(&tenant, &trace_id)`
   (`store.rs:72`), `query(&tenant, &service, range)` (`store.rs:80`), and
   `query_with(&tenant, &service, range, &predicate)` (`store.rs:88`). Per-tenant
   isolation; spans returned in ascending `start_time_unix_nano` order; half-open
   `[start, end)` time range.
2. `FileBackedTraceStore` implements `TraceStore` (`crates/ray/src/file_backed.rs:203`),
   the durable WAL+snapshot adapter; `query` is at `file_backed.rs:243`. An
   in-memory adapter `InMemoryTraceStore` also exists (`store.rs:131`).
3. The span/record type is `ray::Span` (`crates/ray/src/span.rs:184`). Field set mirrors
   `opentelemetry-proto::trace::v1::Span` and ALREADY derives `serde::Serialize` /
   `Deserialize`. Fields: `trace_id: TraceId([u8;16])`, `span_id: SpanId([u8;8])`,
   `parent_span_id: Option<SpanId>`, `name: String`, `kind: SpanKind`,
   `start_time_unix_nano: u64`, `end_time_unix_nano: u64`, `status: SpanStatus`
   (`code: StatusCode`, `message: String`), `attributes: BTreeMap<String,String>`,
   `resource_attributes: BTreeMap<String,String>`, `events: Vec<SpanEvent>`,
   `links: Vec<SpanLink>`. `TraceId` / `SpanId` serialise as lowercase hex strings
   (`span.rs:67`, `span.rs:93`); the hand-rolled hex codec is in-crate.
4. `Span::service_name()` (`span.rs:208`) pulls `service.name` from
   `resource_attributes`, returning `""` if absent. A span with an empty service name is
   indexed by trace only, not by service (`file_backed.rs:325`).
5. `TimeRange` (`span.rs:247`) is half-open `[start_unix_nano, end_unix_nano)` in u64
   nanoseconds; `TimeRange::all()` is `[0, u64::MAX)`. A span matches iff
   `start_unix_nano <= start_time_unix_nano < end_unix_nano`.
6. `TraceStoreError::PersistenceFailed { reason }` is the only typed failure
   (`store.rs:35`). The in-memory adapter never returns it.
7. There is NO HTTP query API for traces today. Only the `ray` crate exists; no
   trace-query crate. (Symmetric with how logs stood before ADR-0047.)
8. Test-build reference: `crates/ray/tests/v1_slice_02_snapshot.rs` shows how `Span`s are
   built (a `span(trace, span, service, name, start, end)` helper seeding
   `resource_attributes["service.name"]`), seeded into the durable store, and queried
   via `query(&tenant, &ServiceName, TimeRange)` and `get_trace(&tenant, &TraceId)`.

## Key decisions taken in DISCUSS

1. **Traces are not metrics and not logs. No PromQL, no query language.** This feature
   reads `Span`s by tenant and time window. There is no metric name, no selector grammar,
   and no full-text query in slice 01. Query inputs are tenant + `[start, end)` (plus the
   service-key reality of CONTRADICTION 1 below).

2. **Read against the real durable store.** The endpoint reads from the real
   `FileBackedTraceStore` via the existing `TraceStore::query`, not a fixture. Same
   posture the metrics and logs read paths took.

3. **Tenant resolved fail-closed.** Ray is per-tenant. The endpoint resolves exactly one
   tenant per request and refuses to serve when none resolves (401), mirroring the
   metrics read path (ADR-0042 Decision 7), the logs read path (ADR-0047 Decision 4),
   and the gateway write path (`KALEIDOSCOPE_DEFAULT_TENANT`, fail-closed). The mechanism
   is a DESIGN decision; DISCUSS pins the BEHAVIOUR (scoped, fail-closed), not the
   mechanism.

4. **Slice 01 is one thin walking skeleton.** An HTTP endpoint that, given a tenant and a
   window `[start, end]`, returns the in-window spans as JSON, read from the real durable
   ray `TraceStore`. That is the whole of slice 01.

## CONTRADICTION found and surfaced (NOT silently resolved)

**CONTRADICTION 1 (the store has no tenant+range-only query).** The brief asks for "query
by tenant + range". But the verified `TraceStore` surface has NO such method. Its read
methods are:

- `get_trace(&tenant, &trace_id)` (lookup by trace id), and
- `query(&tenant, &service, range)` / `query_with(&tenant, &service, range, &predicate)`
  (both REQUIRE a `&ServiceName`).

There is no `query(&tenant, range)` that returns all spans for a tenant over a window
across services. The logs store DID have a tenant+range-only `query`; the trace store
does not (its dual index is keyed `(tenant, trace_id)` and `(tenant, service)`).

This is the central open question of the feature and is FLAGGED to DESIGN, not decided
here. DISCUSS does not invent a new trait method (that would be a store change, out of a
read-API slice's remit) and does not silently drop the service argument. The honest
options, all handed to DESIGN, are recorded as FLAG 3 below. The stories are written so
they hold under whichever option DESIGN chooses: they pin "the in-window spans for the
tenant" as the observable outcome and treat HOW the service key is supplied or eliminated
as the DESIGN decision.

## Flagged to DESIGN (NOT decided in DISCUSS)

- **FLAG 1 (response contract)**: a plain JSON array of `Span`s (the shape ADR-0047 chose
  for logs) versus something richer (a trace-assembled envelope, or a Grafana
  Tempo-shaped response). Both are viable. There is NO pinned prism trace consumer yet
  (no prism trace panel exists), exactly as for logs before ADR-0047. DESIGN owns the
  choice. DISCUSS pins only that the in-window spans are returned as JSON, faithfully
  carrying the `Span` fields (verified fact 3).
- **FLAG 2 (new crate vs extend)**: whether this needs a NEW crate (e.g.
  `trace-query-api`) or extends an existing query crate. ADR-0047 chose a NEW crate
  (`log-query-api`) for the logs domain after a reuse analysis, on the grounds that the
  metrics `query-api` was metrics-domain-specific end to end. The same reasoning likely
  repeats for traces (a third domain, a third store trait, a third record type), but the
  call is DESIGN's. DESIGN owns the choice.
- **FLAG 3 (the service-key question, from CONTRADICTION 1)**: how slice 01 reconciles
  "query by tenant + range" with a store whose range query REQUIRES a service. Options
  for DESIGN, none chosen here:
  - (a) **Configured/known single service**, mirroring the fail-closed configured tenant
    (e.g. resolve a `service` request parameter or a configured default, and call
    `query(&tenant, &service, range)`); narrowest, ships on the existing trait unchanged.
  - (b) **Service as a required request parameter** (the operator names the service),
    making the endpoint "spans for tenant X, service Y, over [start, end]"; honest about
    the store's shape, still no trait change.
  - (c) **Fan-out across services** (enumerate the tenant's services and union the
    per-service results) to honour a literal tenant+range query; needs a way to list a
    tenant's services, which the trait does NOT currently expose, so it implies a store
    change and is the heaviest option.
  Recommended slice-01 default for DESIGN's consideration: (b) service as an explicit
  request parameter (most honest to the verified trait, no store change, smallest slice).
  The stories phrase the entry point so they read correctly under (a) or (b); option (c)
  would be a larger, separate slice.
- **FLAG 4 (granularity: raw spans vs assembled traces)**: slice 01 returns RAW spans
  (the store's natural unit; `query` returns `Vec<Span>`). Assembling complete traces
  from spans (parent/child stitching via `parent_span_id`, or `get_trace` by id) is
  richer and deferred. Likely raw spans for v0. DESIGN owns the choice; DISCUSS records
  the deferral.
- **FLAG 5 (same-origin / static serving for a future prism trace UI)**: out of slice 01
  entirely, exactly as for logs (ADR-0047 Decision defers it). Not decided here.

## Red cards (open questions for DESIGN / clarification)

- RED CARD 1 (response contract): plain `Span` JSON array vs richer envelope vs
  Tempo-shaped. See FLAG 1. Owner: DESIGN.
- RED CARD 2 (placement): new `trace-query-api` crate vs extend an existing query crate.
  See FLAG 2. Owner: DESIGN.
- RED CARD 3 (tenant supply): which mechanism resolves the tenant (configured single
  tenant vs `X-Scope-OrgID` header vs aegis Bearer token)? Recommended slice-01 default:
  configured single tenant, fail-closed (mirrors metrics and logs read paths). Owner:
  DESIGN.
- RED CARD 4 (window on the wire): how `start`/`end` arrive (epoch seconds like the
  metrics and logs endpoints, RFC3339, or nanoseconds) and how they map to the u64-ns
  `TimeRange`. Recommended: mirror the sibling endpoints' epoch seconds for operator
  muscle memory, converting exactly. Owner: DESIGN.
- RED CARD 5 (the service key): how the required `&ServiceName` is supplied or
  eliminated. THE dominant red card for this feature. See CONTRADICTION 1 and FLAG 3.
  Owner: DESIGN.

## Out of scope (deferred to later slices, DECLARED)

- Filters by service / operation (span `name`) / duration.
- Lookup by `trace_id` (the `get_trace` path), even though the trait exposes it.
- Assembling complete traces from spans (parent/child stitching); slice 01 is raw spans.
- Predicate matchers on `name` / `kind` / `status` / attributes, even though
  `query_with(predicate)` exists in the store.
- Pagination / limits / ordering beyond the store's natural ascending
  `start_time_unix_nano` order.
- Any prism UI; same-origin static serving for a prism trace panel (FLAG 5).
- PromQL: traces are not metrics.

## Risk register

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| The store has no tenant+range-only query (the service-key gap) | High | High | CONTRADICTION 1 surfaced; FLAG 3 / RED CARD 5; stories phrased to hold under the per-service options; recommend service as an explicit request parameter, no store change |
| Response contract churn once a prism trace panel arrives | Med | Med | FLAG 1; defer the choice to DESIGN; faithfully carry Span fields whatever the envelope |
| Tenant mechanism mismatch with platform | Med | High | RED CARD 3; mirror gateway/metrics/logs fail-closed default; DESIGN decides |
| Scope creep into trace_id lookup / trace assembly / filters | Med | High | Explicit OUT-of-scope list; slice 01 frozen at tenant + window (+ service per FLAG 3) |
| seconds/nanoseconds (or unit) error on the window | Med | Med | RED CARD 4; explicit conversion AC + half-open boundary example |
| Persistence failure mis-surfaced as empty | Low | High | Dedicated scenario: PersistenceFailed -> 5xx, never a fabricated empty success |
| Spans with empty `service.name` are not in the by-service index | Low | Med | Noted (verified fact 4); under FLAG 3 options (a)/(b) such spans are simply not reachable by a service query; declared, not silently lost |
