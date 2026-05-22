# ADR-0048 — ray trace-query-api response contract, service-key mechanism, and crate layout

- **Status**: Accepted
- **Date**: 2026-05-22
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `ray-query-api-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0047 (the lumen log-query-api contract and crate layout; the
  DIRECTLY SYMMETRIC precedent this slice mirrors in shape and diverges from on
  the one structural fact below; cited as the framing precedent, NOT modified).
  ADR-0042 (the metrics query-api contract, fail-closed tenancy, and
  Earned-Trust probe; the grandparent precedent; cited, NOT modified). ADR-0043
  (the Prism same-origin / `/api/v1` reconciliation; the static-serving posture
  deferred here for a future prism trace UI; cited, NOT modified).

## Context

Kaleidoscope stores traces durably in the `ray` crate (`FileBackedTraceStore`,
`crates/ray/src/file_backed.rs:203`); the aperture trace path writes spans, but
nothing reads them back over HTTP. This feature is the read half of the TRACES
pillar, the THIRD and final observability pillar, the exact analogue of what
`query-range-api-v0` (ADR-0042) did for metrics and `lumen-query-api-v0`
(ADR-0047) did for logs. Slice 01 is one thin walking skeleton: given a resolved
tenant, a service, and a half-open window `[start, end)`, return the in-window
`Span`s that fall in the window as JSON, read from the real durable ray store.

The store surface is verified and FIXED for this slice
(`crates/ray/src/store.rs:80`):

```text
TraceStore::query(&self, tenant: &TenantId, service: &ServiceName, range: TimeRange)
    -> Result<Vec<Span>, TraceStoreError>
```

Per-tenant isolation; ascending `start_time_unix_nano` order; half-open
`[start, end)`. `Span` (`span.rs:184`) carries `trace_id`, `span_id`,
`parent_span_id`, `name`, `kind`, `start_time_unix_nano`, `end_time_unix_nano`,
`status` (`code` + `message`), `attributes`, `resource_attributes`, `events`,
`links`, and ALREADY derives `serde::Serialize`; `trace_id`/`span_id` serialise
as lowercase hex strings (`span.rs:67`, `span.rs:93`).
`TraceStoreError::PersistenceFailed { reason }` is the only typed failure
(`store.rs:35`); the in-memory adapter never returns it, so the 5xx arm is
exercised with a failing store double.

**The one structural divergence from logs (CONTRADICTION 1 / RED CARD 5).**
Unlike `lumen::LogStore::query(&tenant, range)`, the ray range query REQUIRES a
`&ServiceName`. There is NO tenant+range-only trace query: the dual index is
keyed `(tenant, trace_id)` (for `get_trace`) and `(tenant, service)` (for
`query`). DISCUSS surfaced this as the dominant red card, did NOT invent a trait
method, and did NOT silently drop the service; it pinned the BEHAVIOUR (the
in-window spans for the tenant returned as JSON, fail-closed tenancy, calm empty
200, bad-window 400, store-failure 5xx, full field fidelity) and FLAGGED the
service-key mechanism, the response contract, and the placement to DESIGN, which
this ADR resolves.

ADRs in this repository are immutable (superseded, never edited). ADR-0047,
ADR-0042, and ADR-0043 are Accepted and referenced as precedents, not modified.
ADR-0048 is the next free number (the highest existing was 0047, verified).

## Decision

### 1. Service key: an EXPLICIT required request parameter, ray trait UNCHANGED (RED CARD 5 / FLAG 3)

The window arrives alongside a required `service` query-string parameter:
`GET /api/v1/traces?service=<name>&start=<epoch_seconds>&end=<epoch_seconds>`.
The handler resolves the tenant (fail-closed), reads `service` from the query
string, validates the window, and calls the EXISTING
`TraceStore::query(&tenant, &ServiceName::new(service), range)`. The endpoint is
honestly "the in-window spans for tenant X, service Y, over `[start, end)`",
which is exactly the shape the verified store offers.

A **missing or empty `service` parameter is a 400** (a required parameter, named
in the error, no store query run), NOT an empty result. Rationale: an empty
result would be indistinguishable from "this service has no spans in the
window", collapsing the honest three-way distinction US-04 requires; a request
that omits a parameter the store mandates is malformed, the same class of fault
as a malformed window, and belongs in the 400 arm before the store is touched.
The `service` value is NOT echoed in the error text (redaction symmetry below).

The ray `TraceStore` trait is UNCHANGED: no method is added, removed, or
re-signed. The read path rides the existing `query`; zero blast radius on the
store and its other callers.

### 2. Response contract: a plain, explicit JSON array of `Span`s (FLAG 1 / RED CARD 1)

The success arm is a plain JSON array of the in-window `Span`s, in the store's
ascending `start_time_unix_nano` order, each span serialised faithfully via the
field set `Span` already derives with `serde::Serialize` (`span.rs:183`):
`trace_id`/`span_id` (lowercase hex strings), `parent_span_id` (hex or null),
`name`, `kind`, `start_time_unix_nano`/`end_time_unix_nano` (numbers), `status`
(`code` + `message`), `attributes` (object), `resource_attributes` (object),
`events`, `links`. The empty arm is the empty array `[]` with HTTP 200. No
envelope wraps the array at v0.

```text
GET .../traces?service=checkout&start=<epoch_seconds>&end=<epoch_seconds>  ->  200
[
  {
    "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "span_id": "0102030405060708",
    "parent_span_id": null,
    "name": "place-order",
    "kind": "Server",
    "start_time_unix_nano": 1716200005000000000,
    "end_time_unix_nano": 1716200005120000000,
    "status": { "code": "Error", "message": "upstream timeout" },
    "attributes": { "http.route": "/orders" },
    "resource_attributes": { "service.name": "checkout" },
    "events": [ ... ],
    "links": [ ... ]
  }
]

empty match / unknown (tenant, service)  ->  200  []
```

A richer envelope (a trace-assembled `parent_span_id`-stitched tree, or a
Grafana Tempo-shaped response) is REJECTED for v0 for the same reason ADR-0047
rejected Loki-shaping for logs: there is NO prism trace consumer pinning a
contract yet (no prism trace panel exists), and any assembled/Tempo projection
is lossy or speculative against the OTLP `Span` field set, the exact fidelity
US-01 example 3 requires the response to preserve. The most honest, simplest
contract is to return the spans the store actually holds, in their native
OTLP-shaped field set, raw (FLAG 4: raw spans, no trace assembly at v0, the
store's natural `Vec<Span>` unit). When a consumer needs assembly or Tempo
shaping, it arrives behind the same route, additively, as a later slice.

The error arm reuses the sibling endpoints' shape EXACTLY for cross-pillar
symmetry: `{status:"error", error:"<reason>"}` at the relevant status code. The
reason names the fault (the missing service, the invalid window, a backend read
failure) and NEVER echoes a forwarded header or credential value, nor the
`service`/`start`/`end` raw values (DD redaction symmetry with ADR-0047
Decision 1, ADR-0042 Decision 6, and ADR-0027 §6). The success arm is the bare
array (no `{status:"success"}` wrapper) for the same reason as logs: traces have
no Prometheus validator to satisfy and a bare array is the simplest honest shape.

### 3. Route and request shape: `GET /api/v1/traces?service=&start=&end=`, epoch seconds (RED CARD 4)

The route is `GET /api/v1/traces`, sibling to `/api/v1/query_range` (metrics) and
`/api/v1/logs` (logs) under the same `/api/v1` prefix (ADR-0043), so an operator
and any future same-origin prism trace panel reach it with no extra mapping. The
window arrives as query-string `start` and `end` in epoch SECONDS (float-tolerant,
mirroring the metrics and logs endpoints for operator muscle memory), converted
exactly to the half-open `[start, end)` u64-nanosecond `ray::TimeRange`.
Query-string is chosen over path segments because the window and the service are
filters over a collection, not a resource identity, and query-string mirrors the
sibling endpoints the operator already knows.

The status mapping:

| Outcome | Condition | HTTP | Body |
|---|---|---|---|
| Success | `TraceStore::query` returns a non-empty `Vec` | 200 | JSON array of `Span`s, ascending `start_time_unix_nano` |
| Calm empty | `TraceStore::query` returns `Ok(Vec::new())` (empty window OR unknown `(tenant, service)`) | 200 | `[]` |
| Bad request | missing/empty `service`, OR non-numeric bound, OR `start > end` | 400 | `{status:"error", error:"<names the missing service or invalid window>"}`; no store query run |
| Fail-closed | no tenant resolves | 401 | `{status:"error", error:"no tenant resolvable: ..."}`; refused before the store |
| Store failure | `TraceStore::query` returns `PersistenceFailed` | 500 | `{status:"error", error:"the backing trace store could not be read"}`; never a fabricated empty |

The half-open rule is the store's: a span at exactly `start` is included, a span
at exactly `end` is excluded. A span with an empty `service.name` is indexed by
trace only, not by service (`file_backed.rs:325`), so it is simply not reachable
by a service query under this decision; declared, not silently lost.

### 4. Tenancy: configured single tenant, fail-closed, behind the router seam (RED CARD 3)

The slice-01 adapter resolves exactly one `aegis::TenantId` from
`KALEIDOSCOPE_TRACE_QUERY_TENANT` (fail-closed when unset or empty), mirroring
the logs read path's `KALEIDOSCOPE_LOG_QUERY_TENANT` (ADR-0047 Decision 4), the
metrics read path's `KALEIDOSCOPE_QUERY_TENANT` (ADR-0042 Decision 7), and the
gateway's `KALEIDOSCOPE_DEFAULT_TENANT`. The router takes an `Option<TenantId>`:
`None` is "no tenant resolvable" and every request is refused (401). Header
tenancy (`X-Scope-OrgID`) or an aegis Bearer token is deferred and lands behind
the same seam without touching the query path.

### 5. Placement: a NEW crate `crates/trace-query-api`, lib + thin binary (FLAG 2 / RED CARD 2)

A new workspace crate `crates/trace-query-api`, mirroring the `log-query-api`
lib+binary split (itself mirroring `query-api`): a `[lib]` exposing one driving
port `router(store, tenant)` over the `ray::TraceStore` trait and an
`Option<TenantId>` (fail-closed seam), plus a thin `[[bin]]` composition root
that opens the durable `FileBackedTraceStore`, resolves the tenant, runs the
Earned-Trust probe, and binds the axum listener. The existing `query-api` and
`log-query-api` crates are NOT extended.

The MANDATORY reuse analysis (full table in the feature wave-decisions.md) found
that the HTTP SCAFFOLDING is reusable in SHAPE but not in CODE: each existing
read-API crate is domain-specific (its store port, its record type, its
contract). Traces are a third domain with a third store trait (`TraceStore`), a
third record type (`Span`), and the service-key shape unique to it (Decision 1).
Folding traces into either existing crate would mix domains and contracts in one
crate for the sake of sharing ~40 lines of axum boilerplate. The reuse is of
PATTERN (the lib+binary split, the fail-closed `Option<TenantId>` router seam,
the `error_response` shape, the epoch-seconds bounds parser, the tower `oneshot`
test posture, the wire-then-probe-then-use composition root), reproduced in the
new crate, not of any other domain's types.

**Extract-vs-duplicate call: DUPLICATE the minimum, extract NOTHING in this
slice, with a recorded forward-looking recommendation.** This IS the third clone
of the HTTP scaffolding, so the question of extracting a shared
`query-http-common` crate (the fail-closed seam, `error_response`, the
epoch-seconds bounds parser) is now genuinely live and is recorded as a
forward-looking note rather than dismissed. It is deliberately DEFERRED off this
slice for three reasons: (i) the bounds parser is NOT type-identical across the
three (it produces a `pulse::TimeRange`, a `lumen::TimeRange`, and a
`ray::TimeRange` respectively), so a shared parser would need a generic or a
conversion seam that does not exist yet; (ii) the three contracts differ in body
shape (a Prometheus matrix envelope, a plain `LogRecord` array, a plain `Span`
array with a mandatory service parameter), so only the error envelope and the
~5-line fail-closed seam are truly identical; (iii) extracting a shared crate
NOW would couple three crates through a fourth as a side effect of a thin read
slice, which is a refactor with its own blast radius, its own ADR, and its own
mutation gate, not a rider on slice 01. The recommendation: once this third
crate ships and the shared surface has proven its exact shape across three real
call sites, raise a dedicated `query-http-common` extraction feature that touches
`query-api`, `log-query-api`, and `trace-query-api` together under its own ADR.
This slice re-implements the ~30 shared lines and mutation-tests them in place,
matching the deliberate, recorded choice ADR-0047 made for the second clone.

### 6. The ray `TraceStore` trait is UNCHANGED

The endpoint reads through the EXISTING `TraceStore::query(&tenant, &service, range)`
(`store.rs:80`) against the real `FileBackedTraceStore`. No method is added,
removed, or re-signed on the trait. `get_trace` and `query_with(predicate)`
exist but are NOT used in slice 01 (trace-id lookup, trace assembly, and
predicate matching are declared out of scope). Option (c) of FLAG 3 (fan-out
across a tenant's services) is rejected for slice 01 precisely because it WOULD
need a trait change (a "list services" capability the trait does not expose).

### 7. Earned-Trust probe (wire-then-probe-then-use)

The composition root, before binding the listener, runs `probe()`: a trivial
`query` over an empty range for the resolved tenant and a probe service name,
asserting `Ok`. A `None` tenant is the fail-closed refusal; a store error is a
read refusal. A failure emits `event=health.startup.refused` and exits non-zero.
The three-orthogonal-layer enforcement (subtype at the composition-root boundary,
AST pre-commit that the binary probes before binding, behavioural gold-test with
a lying store double) mirrors ADR-0042 Decision 8 and ADR-0047 Decision 6,
reproduced for the new crate.

## Alternatives considered

### Service key A (rejected): a configured/known single service

Resolve the service from a configured default (e.g. `KALEIDOSCOPE_TRACE_QUERY_SERVICE`)
the way the tenant is configured, and call `query(&tenant, &service, range)` with
it. For: ships on the existing trait unchanged; the operator passes only a window,
matching the "tenant + window" framing most closely. Against: a single configured
service makes the endpoint serve exactly one service per process, which is far less
useful to an on-call SRE who pivots between services during an incident, and it
hides a load-bearing input in deploy-time configuration where it cannot be varied
per request. Rejected: less honest about what the store needs and less useful;
the explicit parameter (Decision 1) exposes the real input where the operator can
set it.

### Service key B (CHOSEN): service as an explicit required request parameter

See Decision 1. Most honest to the verified trait, no store change, smallest
slice, lets the operator name the service per request, and keeps the fault
distinction crisp (missing service is a 400, not a misleading empty).

### Service key C (rejected): fan-out across the tenant's services

Enumerate the tenant's services and union the per-service `query` results to
honour a literal tenant+range query across services. For: matches a naive
"all spans for the tenant in the window" reading. Against: the `TraceStore` trait
exposes NO way to list a tenant's services, so this REQUIRES a trait change (a new
`list_services` or a tenant+range method), which is out of a read-API slice's
remit and contradicts Decision 6. It is also the heaviest option (N queries,
result merge and re-sort). Rejected for slice 01; it is a larger, separate, later
slice that would carry its own store-change ADR.

### Response contract A (rejected): an assembled-trace / parent-child-stitched tree

Stitch spans into trace trees via `parent_span_id` and return assembled traces.
For: a richer, more directly useful unit for an operator reading a single trace.
Against: assembly is lossy to express as a flat array, there is no consumer
pinning the tree shape yet, and FLAG 4 explicitly defers assembly to a later
slice (slice 01 is raw spans, the store's natural `Vec<Span>` unit). Rejected for
v0; arrives behind the same route additively when a consumer needs it.

### Response contract B (rejected): a Grafana Tempo-shaped envelope

Shape the response as Tempo's search/trace JSON for instant Grafana Tempo
datasource compatibility. For: Grafana interoperability. Against: there is NO
prism trace consumer pinning this contract yet (unlike the metrics endpoint, whose
shape Prism pinned), and a Tempo projection is lossy against the OTLP `Span` field
set US-01 example 3 requires preserved. Rejected for v0; a later, additive
projection behind the same route.

### Placement A (rejected): extend `query-api` or `log-query-api`

Add the `/api/v1/traces` route to an existing read-API crate. For: reuses the
axum boilerplate, the bounds parser, and `error_response` in place; one fewer
crate, gate, and tag. Against: each existing crate is domain-specific end to end
(its store port, record type, and contract); folding traces in mixes a third
domain and a third contract into a crate built for another, muddying both. The
reuse worth having is of PATTERN, reproduced cheaply in a new crate. Rejected;
the domain separation outweighs the boilerplate saving, consistent with
ADR-0047 Placement A.

### Placement B (rejected for THIS slice, recorded as a forward-looking recommendation): extract a shared `query-http-common` crate now

Create a shared crate holding the fail-closed seam, `error_response`, and the
epoch-seconds bounds parser, depended on by all three read-API crates. For: this
is the THIRD clone, so the duplication is now real (~30 lines x3) and the
extraction is the textbook moment by the rule of three. Against: the bounds
parser is not type-identical across the three (`pulse`/`lumen`/`ray` `TimeRange`),
the three contracts differ in body shape so only the error envelope and the
~5-line seam are truly identical, and extracting now would couple three crates
through a fourth as a side effect of a thin read slice, with its own blast radius
and its own mutation gate. Rejected for THIS slice; RECORDED as the next clean
refactor: a dedicated `query-http-common` extraction feature, after this crate
ships, touching all three crates together under its own ADR. See Decision 5 and
the Consequences forward-looking note.

### Request shape A (rejected): the window in the path

Carry `start`/`end` (and `service`) as path segments. For: a "RESTful"
resource-shaped URL. Against: the window and service are filters over a
collection, not a resource identity, and path segments diverge from the sibling
endpoints the operator already knows. Rejected; query-string mirrors the sibling
read paths.

## Consequences

### Positive

- **The most honest, lowest-ceremony contract.** A plain array of the raw spans
  the store holds, in their native OTLP field set, with no lossy assembly or
  speculative consumer envelope. Field fidelity (US-01 example 3) is satisfied by
  `Span`'s existing `Serialize`, with no hand-written mapping to drift.
- **Honest about the store's shape.** The required `service` parameter exposes
  the real input the store mandates rather than hiding it; the ray trait is
  untouched; missing `service` is a crisp 400, not a misleading empty.
- **Clean domain boundaries.** Traces, logs, and metrics stay separate crates;
  no envelope or store trait leaks across them. `query-api` and `log-query-api`
  are untouched.
- **Pattern reuse without coupling.** The lib+binary split, the fail-closed
  seam, the `error_response` shape, the epoch-seconds parser, the tower `oneshot`
  test posture, and the wire-then-probe-then-use root are reproduced from the
  proven logs and metrics precedents, cheaply, in the new crate.
- **Fail-closed tenancy and honest outcomes.** Empty is a calm 200 `[]`, a bad
  request (missing service or bad window) is a 400 that names the fault, a store
  failure is a 500 that never fabricates an empty; the error text never leaks a
  forwarded header or parameter value.

### Negative

- **A new crate to maintain, with a new CI gate and a new graduation tag.** A new
  `gate-5-mutants-trace-query-api` job and a new per-crate tag at graduation (a
  DEVOPS / graduation matter; see the wave-decisions DEVOPS handoff). Mitigated:
  the lib+binary split is the established platform shape and the crate is thin.
- **~30 duplicated lines for the THIRD time** (the fail-closed seam,
  `error_response`, the bounds parser) across the three read-API crates.
  Mitigated AND now actively planned: this is the rule-of-three trigger, so the
  duplication is recorded as the trigger for a dedicated `query-http-common`
  extraction feature once this crate ships (Decision 5, Placement B). The lines
  are mutation-tested in place until then.
- **One service per request, no cross-service view.** An operator wanting all of
  a tenant's spans in a window must query per service. Mitigated: this is the
  honest shape of the verified store; a tenant+range fan-out is a recorded later
  slice that needs a store change (Service key C).
- **No trace assembly or Tempo shaping yet.** A Grafana Tempo datasource cannot
  point at this endpoint today, and the operator sees raw spans not assembled
  traces. Mitigated: there is no such consumer yet, the spans are carried
  losslessly so a later projection is additive, and both deferrals are recorded
  (FLAG 4, Response contract A/B).

### Trade-off summary

The slice trades a richer assembled/Tempo envelope, a configured-or-fan-out
service key, and a premature shared crate for an honest raw-`Span` plain-array
contract, an explicit `service` parameter on the unchanged ray trait, and a clean
per-domain crate, buying lossless field fidelity, a crisp fault distinction, and
clean boundaries at the cost of one new CI gate, one new tag, one-service-per-
request, and ~30 deliberately duplicated lines whose extraction is now recorded
as the next refactor. Every cost is recorded so a future reader understands it was
chosen, not overlooked.

## External-integration handoff

No NEW external network integration. The endpoint reads the in-process durable
`ray::FileBackedTraceStore` through the `TraceStore` trait; ray is a first-party
library, not a network service. There is no pinned external consumer contract for
the traces response yet (no prism trace panel exists), which is precisely why
Decision 2 chose the plain array. When a prism trace panel or a Grafana Tempo
datasource becomes a real consumer, that boundary acquires a consumer-driven
contract at that time; this slice introduces none.

## Verification

- Acceptance tests via the tower `oneshot` pattern: ingest in-window and
  out-of-window spans for a tenant/service into a real `FileBackedTraceStore`,
  query, assert only the in-window spans return in ascending `start_time_unix_nano`
  order (US-01); the half-open boundary (span at `start` included, at `end`
  excluded); full field fidelity (every `Span` field round-trips, none dropped or
  renamed, hex `trace_id`/`span_id`, status, attribute maps, event, link).
- The calm empty arm: empty window and unknown `(tenant, service)` both yield
  200 `[]` (US-02).
- Tenant isolation: a two-tenant fixture asserts zero cross-tenant leak; a
  no-tenant fixture asserts the fail-closed 401 (US-03).
- The honest outcomes: a missing/empty `service`, a non-numeric bound, and an
  inverted window each yield 400 naming the fault with no store query run; a
  `PersistenceFailed` store double yields 500, never a fabricated empty; a
  redaction test asserts the error text never contains a forwarded
  `Authorization` value nor the raw `service`/`start`/`end` values (US-04).
- **Earned-Trust probe enforcement (three orthogonal layers)**: (a) subtype check
  at the composition-root boundary (the store is used through the `TraceStore`
  port; the probe consumes that port); (b) an AST pre-commit check that the binary
  calls `probe()` before binding the listener; (c) a behavioural gold-test with a
  store double that lies (open succeeds, query returns `PersistenceFailed`)
  asserting startup refuses with `event=health.startup.refused`.
- Mutation testing: `cargo mutants` scoped to `crates/trace-query-api/src/` via
  `--in-diff` at the project 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md).
  Flagged to Apex as a NEW `gate-5-mutants-trace-query-api`. Primary targets: the
  half-open boundary, the empty-vs-error distinction, the missing-service 400, the
  bounds parser, and the fail-closed refusal.
