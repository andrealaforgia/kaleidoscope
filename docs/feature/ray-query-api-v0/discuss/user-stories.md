<!-- markdownlint-disable MD024 -->

# User Stories: ray-query-api-v0

British English. No em dashes. This feature is the HTTP READ path for TRACES, the third
observability pillar: the exact analogue of `lumen-query-api-v0` (logs, ADR-0047) and
`query-range-api-v0` (metrics, ADR-0042). Spans are stored durably in `ray`
(`FileBackedTraceStore`) and written by the aperture trace path, but nothing reads them
back over HTTP. Slice 01 is one thin walking skeleton: given a tenant and a window
`[start, end]`, return the in-window `Span`s as JSON, read from the real durable store.
The "After" line of each story references the real entry point (the new HTTP endpoint),
exercised in the acceptance suite via the tower `oneshot` pattern. The exact path/shape
of the endpoint, the response envelope, and how the service key is supplied are DESIGN
decisions (see System Constraints, wave-decisions.md FLAG 1-5).

## System Constraints

- Traces are NOT metrics and NOT logs. There is no PromQL, no metric name, and no query
  language in slice 01. The query inputs are a tenant and a half-open time window
  `[start, end)` (plus the service-key reality below). This is the deliberate divergence
  from the metrics endpoint.
- The store surface is verified (`crates/ray/src/store.rs`). Its read methods are
  `get_trace(&tenant, &trace_id)`, `query(&tenant, &service, range)`, and
  `query_with(&tenant, &service, range, &predicate)`. Per-tenant isolation; spans in
  ascending `start_time_unix_nano` order; half-open `[start, end)`. `FileBackedTraceStore`
  is the durable adapter (`file_backed.rs:203`); its `query` is at `file_backed.rs:243`.
- CONTRADICTION (carried to DESIGN as FLAG 3 / RED CARD 5): unlike the logs store, the
  trace store has NO tenant+range-only `query`. Its range query REQUIRES a
  `&ServiceName`. DISCUSS does NOT invent a new trait method and does NOT silently drop
  the service. The stories pin "the in-window spans for the tenant" as the observable
  outcome and treat HOW the service key is supplied or eliminated as the DESIGN decision.
  Recommended slice-01 default: service supplied as an explicit request parameter (no
  store change); see wave-decisions.md FLAG 3.
- A `Span` (`crates/ray/src/span.rs:184`) carries `trace_id`, `span_id`,
  `parent_span_id` (optional), `name`, `kind`, `start_time_unix_nano`,
  `end_time_unix_nano`, `status` (code + message), `attributes`, `resource_attributes`,
  `events`, `links`. It ALREADY derives `serde::Serialize`. `trace_id`/`span_id`
  serialise as lowercase hex strings. The response carries these fields faithfully.
- `Span::service_name()` pulls `service.name` from `resource_attributes`, defaulting to
  `""`; a span with an empty service name is indexed by trace only, not by service
  (`file_backed.rs:325`). Such spans are not reachable by a service query under the
  recommended FLAG 3 options; declared, not silently lost.
- `TimeRange` is half-open `[start_unix_nano, end_unix_nano)` in u64 nanoseconds
  (`span.rs:247`). A span matches iff `start <= start_time_unix_nano < end`.
- Tenant scoping is mandatory and fail-closed: every query is scoped to exactly one
  resolved `aegis::TenantId`; no resolvable tenant means the request is refused. The
  resolution MECHANISM is a DESIGN decision (RED CARD 3); the BEHAVIOUR is pinned here.
- `TraceStoreError::PersistenceFailed` surfaces as an HTTP 5xx, never a fabricated empty
  success.
- FLAGGED to DESIGN, NOT decided here: (1) the response contract, plain `Span` JSON array
  versus a richer/assembled-trace/Tempo-shaped envelope; (2) NEW crate
  (`trace-query-api`) versus extend; (3) the service-key question; (4) raw spans versus
  assembled traces (likely raw for v0); (5) same-origin/static serving for a future prism
  trace UI (out of slice 01).
- OUT of scope for slice 01 (deferred, declared): filters by service / operation /
  duration; lookup by `trace_id`; assembling complete traces from spans;
  predicate matchers (even though `query_with` exists); pagination/limits; any prism UI.
- The endpoint path and the on-the-wire shape of `start`/`end` (RED CARD 4) are DESIGN
  decisions. Stories phrase the entry point as "GET the traces endpoint for tenant X over
  [start, end]".

---

## US-01: Read the in-window spans for a tenant over HTTP

### Elevator Pitch
- Before: the aperture write path persists spans durably into ray, but there is no read path at all; the spans are written and invisible, sitting millimetres away in `FileBackedTraceStore` with nothing able to surface them. Metrics and logs can be read; traces, the third pillar, cannot.
- After: an operator GETs the traces endpoint for tenant "acme-prod" over `[start, end]` and sees the in-window `Span`s returned as JSON, e.g. `[{"trace_id":"aaaa...","span_id":"01...","name":"place-order","start_time_unix_nano":1716200005000000000,"kind":"Server","resource_attributes":{"service.name":"checkout"}, ...}, ...]` (envelope shape and service-key supply are DESIGN decisions), exercised in the acceptance suite via a tower `oneshot` against the real durable store.
- Decision enabled: the operator can finally READ the spans the platform recorded for this tenant in this window, and decide whether the traced requests were healthy during it.

### Problem
Sara Okafor is an on-call SRE for the "checkout" service, tenant "acme-prod". The
aperture trace path has been writing her service's spans into ray for weeks; they are
durable in `FileBackedTraceStore`. But there is no way to read them back: no HTTP query
API for traces exists anywhere in the platform. When checkout misbehaves she now has
metrics (the `query_range` read path shipped) and logs (the `lumen` read path shipped),
but she is blind to her own traces, even though they are stored a few millimetres away.
She needs to pull the spans for her tenant over a window and read them.

### Who
- On-call SRE / operator | reading the spans for a known tenant over a recent window |
  wants to see the spans, fast, during or after an incident.
- Later, prism's trace client | the future machine consumer | needs the in-window spans
  as JSON in whatever envelope DESIGN settles on.

### Solution
An HTTP GET endpoint that, for a resolved tenant and a half-open window `[start, end)`,
reads the matching in-window `Span`s from the real durable `FileBackedTraceStore` via the
existing `TraceStore::query` and returns them as JSON, faithfully carrying the span
fields (`trace_id`, `span_id`, `parent_span_id`, `name`, `kind`, `start`/`end` times,
`status`, `attributes`, `resource_attributes`, `events`, `links`), in the store's
ascending `start_time_unix_nano` order. The response envelope, and how the required
service key is supplied (FLAG 3), are DESIGN decisions; the spans and their fields are
pinned here.

### Domain Examples
### 1: Happy Path - in-window spans returned, out-of-window excluded
Tenant "acme-prod", service "checkout", has six spans persisted via the aperture trace
path: three with `start_time` inside `[1716200000s, 1716200060s)` (a Server span
"place-order", an Internal span "reserve-stock", a Client span "charge-card") and three
outside it (two earlier, one later). Sara GETs the traces endpoint for "acme-prod" over
`[1716200000, 1716200060]`. The response carries exactly the three in-window spans, in
ascending `start_time` order, each with its `name`, `kind`, and
`resource_attributes.service.name = "checkout"`.

### 2: Edge Case - half-open window includes start, excludes end
"acme-prod"/"checkout" has one span with `start_time == start_ns` and one with
`start_time == end_ns`. Because the range is half-open `[start, end)`, the span at
`start` is INCLUDED and the span at `end` is EXCLUDED. The response carries the single
span at `start`.

### 3: Boundary - all span fields round-trip faithfully
"acme-prod"/"checkout" has one Server span with `name = "place-order"`,
`status.code = Error`, `status.message = "upstream timeout"`,
`attributes = {"http.route":"/orders"}`, `resource_attributes = {"service.name":"checkout"}`,
a populated `parent_span_id`, one event, and one link. Sara queries a window covering it.
The returned JSON carries every field, including the hex-encoded `trace_id`/`span_id`,
the status, the attribute maps, the event, and the link, with no field dropped or
renamed.

## UAT Scenarios (BDD)
### Scenario: Operator reads the in-window spans for a tenant
Given tenant "acme-prod" has spans inside the window [1716200000s, 1716200060s) and others before and after it
When the operator GETs the traces endpoint for tenant "acme-prod" over [1716200000, 1716200060]
Then the response carries exactly the in-window spans
And no out-of-window span appears
And the spans are in ascending start_time order

### Scenario: The half-open window includes start and excludes end
Given tenant "acme-prod" has one span at exactly the window start and one at exactly the window end
When the operator GETs the traces endpoint over [start, end)
Then the span at start is included
And the span at end is excluded

### Scenario: Every Span field round-trips in the response
Given tenant "acme-prod" has a span carrying a name, kind, status, span attributes, resource attributes, a parent span id, an event, and a link
When the operator GETs the traces endpoint over a covering window
Then the response carries every field of the span
And no field is dropped or renamed

### Scenario: The spans are read from the real durable store
Given the aperture trace path has persisted spans for tenant "acme-prod" into the durable ray store
When the operator GETs the traces endpoint over a window covering them
Then the same spans the write path wrote are returned
And they are read through TraceStore::query against the real FileBackedTraceStore, not a fixture

## Acceptance Criteria
- [ ] A GET for a resolved tenant over `[start, end)` returns the spans whose `start_time_unix_nano` falls in the half-open window.
- [ ] Spans are returned in ascending `start_time_unix_nano` order (the store's natural order).
- [ ] A span at exactly `start` is included; a span at exactly `end` is excluded.
- [ ] Every `Span` field (`trace_id`, `span_id`, `parent_span_id`, `name`, `kind`, `start`/`end` times, `status`, `attributes`, `resource_attributes`, `events`, `links`) is carried in the response without loss or rename.
- [ ] The spans are read via `TraceStore::query` against the real `FileBackedTraceStore`.
- [ ] The acceptance test exercises the endpoint via the tower `oneshot` pattern: ingest in-window and out-of-window spans, query, assert only the in-window spans come back.

## Outcome KPIs
- **Who**: on-call operator (and the future prism trace client)
- **Does what**: reads the stored spans for a tenant over a window, instead of having them written and unseen
- **By how much**: from "cannot read traces at all" (0 readable) to "a GET returns exactly the in-window spans for the tenant from the durable store"; 100% of Span fields round-trip
- **Measured by**: E2E ingest -> query -> read acceptance test via tower oneshot; field-fidelity assertion
- **Baseline**: 0% (no traces read path exists anywhere in the platform; the last of the three pillars)

## Technical Notes (Optional)
- Store surface: `TraceStore::query(&tenant, &service, range) -> Result<Vec<Span>, TraceStoreError>` (`crates/ray/src/store.rs:80`); durable adapter `FileBackedTraceStore` (`file_backed.rs:203`, query at `:243`). NOTE the REQUIRED `&service` (CONTRADICTION / FLAG 3).
- `TimeRange` is half-open and in u64 nanoseconds (`span.rs:247`); on-wire window shape is RED CARD 4 for DESIGN.
- `Span` already derives `serde::Serialize` (`span.rs:183`); field fidelity needs no hand-written mapping.
- Response envelope (plain array vs richer) is FLAG 1; placement (new crate vs extend) is FLAG 2; raw spans vs assembled traces is FLAG 4.
- Test fixtures: see `crates/ray/tests/v1_slice_02_snapshot.rs` for the `span(...)` helper and how spans are built, seeded, and queried.

---

## US-02: Return a calm empty result when nothing is in the window

### Elevator Pitch
- Before: an operator querying a tenant or window with no spans would not know whether the platform is broken or simply has nothing to show.
- After: the operator GETs the traces endpoint for tenant "acme-prod" over a window with no spans and sees a calm, well-formed empty result (an empty JSON array, or the empty arm of whatever envelope DESIGN chooses), HTTP 200, not an error.
- Decision enabled: the operator distinguishes "no spans in this window" from "backend failure" and adjusts the tenant, service, or window.

### Problem
Sara queries a window before checkout started emitting spans, or a quiet period with no
traffic. The store correctly returns an empty `Vec` (it returns `Ok(Vec::new())`, not an
error, for an unknown `(tenant, service)` key or an empty match, verified in
`file_backed.rs:253`). If the endpoint turned that into an error or a hang, Sara would
think the platform is broken during an incident. She needs "nothing matched" to feel calm
and unmistakably different from "something failed".

### Who
- On-call operator | exploring tenants, services, and windows | needs "empty" to read as
  calm, not alarming.

### Solution
When `TraceStore::query` returns an empty `Vec`, the endpoint serialises the empty arm of
the response (an empty array / empty result) with HTTP 200, never an error status. This
holds for an unknown `(tenant, service)` key (store returns empty) and a known key with
no spans in the window.

### Domain Examples
### 1: Happy Path - known tenant/service, window before first span
"acme-prod"/"checkout" has spans but its earliest is at `1716300000s`; Sara queries
`[1716200000, 1716200060]`. The half-open window matches nothing; the store returns an
empty `Vec`; the response is the empty arm with HTTP 200.

### 2: Edge Case - unknown service for the tenant
The endpoint resolves tenant "acme-prod" and a service "ghost-service" that has never had
spans written. The store returns `Ok(Vec::new())` for that `(tenant, service)` key. The
response is the empty arm with HTTP 200, not an error.

### 3: Boundary - a single span exactly at end only
"acme-prod"/"checkout" has exactly one span at `start_time == end_ns`. Because the window
is half-open `[start, end)`, the span at `end` is excluded; the response is the empty arm.

## UAT Scenarios (BDD)
### Scenario: A window with no spans returns a calm empty result
Given tenant "acme-prod" has no spans whose start_time falls in the requested window
When the operator GETs the traces endpoint over that window
Then the HTTP status is 200
And the result is the empty arm (an empty array of spans)
And the response is not an error

### Scenario: An unknown tenant/service returns the empty arm, not an error
Given the endpoint resolves a tenant/service key which has never had spans written
When the operator GETs the traces endpoint over any window
Then the HTTP status is 200
And the result is the empty arm

### Scenario: A span exactly at the window end is excluded
Given tenant "acme-prod" has a single span at exactly the window end
When the operator GETs the traces endpoint over [start, end)
Then the result is the empty arm because the window is half-open

## Acceptance Criteria
- [ ] An empty `TraceStore::query` result serialises to the empty arm of the response with HTTP 200.
- [ ] Empty is never an error status (an empty array is a valid success).
- [ ] An unknown `(tenant, service)` key (store returns `Ok(Vec::new())`) yields the empty arm, not an error.
- [ ] The half-open window excludes a span at exactly `end`.

## Outcome KPIs
- **Who**: on-call operator
- **Does what**: distinguishes "no spans in this window" from "backend failure"
- **By how much**: 100% of no-match queries return the success+empty arm; 0 false errors
- **Measured by**: acceptance test asserting the empty arm for an empty window and for an unknown key
- **Baseline**: n/a (no read path today)

## Technical Notes (Optional)
- `TraceStore::query` returns `Ok(Vec::new())` for an unknown `(tenant, service)` key or an empty match (`file_backed.rs:253`); no special-casing needed beyond serialising empty.

---

## US-03: Scope every trace query to one tenant, fail-closed

### Elevator Pitch
- Before: ray is per-tenant but no read path exists, so there is no tenant-scoping behaviour on traces to trust.
- After: a request that resolves tenant "acme-prod" GETs the traces endpoint and sees only acme-prod's spans; a request with no resolvable tenant is refused (HTTP 401) with no spans returned, exercised via tower oneshot with a two-tenant and a no-tenant fixture.
- Decision enabled: an operator (and the platform owner) can trust that a trace query returns this tenant's spans and only this tenant's spans.

### Problem
The platform is multi-tenant; ray keys every span bucket by `TenantId`
(`HashMap<(TenantId, TraceId), ...>` and `HashMap<(TenantId, ServiceName), ...>`, verified
in `file_backed.rs:83`). A read path that ignored tenancy, or defaulted to "all tenants",
would leak one customer's spans to another, which for span attributes (often carrying
request detail such as routes and identifiers) is a serious breach. The write path, the
metrics read path, and the logs read path all fail closed when no tenant resolves; the
traces read path must match that posture.

### Who
- Platform owner / security reviewer | needs guaranteed tenant isolation on trace reads.
- On-call operator | scoped to their own tenant | must never see another tenant's spans.

### Solution
Resolve the tenant for each request (slice-01 default: a single configured tenant,
fail-closed if unset, mirroring the gateway's `KALEIDOSCOPE_DEFAULT_TENANT`, the metrics
read path's `KALEIDOSCOPE_QUERY_TENANT`, and the logs read path's
`KALEIDOSCOPE_LOG_QUERY_TENANT`). Pass the resolved `aegis::TenantId` to
`TraceStore::query`. If no tenant resolves, refuse to serve (HTTP 401). The header/Bearer
mechanism is deferred (RED CARD 3); the resolution SEAM is designed so swapping it in is
non-breaking.

> RED CARD 3: the production tenant-supply mechanism is a DESIGN decision. This story
> pins the BEHAVIOUR (scoped, fail-closed), not the mechanism.

### Domain Examples
### 1: Happy Path - configured tenant returns its own spans
The endpoint is configured with tenant "acme-prod". A query over a window returns only
acme-prod's spans for that window.

### 2: Edge Case - another tenant's spans are not returned
Tenant "globex-prod" also has spans for the same service in the same window. With the
endpoint scoped to "acme-prod", the query returns acme-prod's spans only; globex-prod's
are absent.

### 3: Boundary - no tenant resolvable
The endpoint starts with no configured tenant and the request carries no tenant signal.
The request is refused (fail-closed, HTTP 401); no spans are returned.

## UAT Scenarios (BDD)
### Scenario: A trace query returns only the resolved tenant's spans
Given the endpoint resolves tenant "acme-prod" and ray holds in-window spans for both "acme-prod" and "globex-prod"
When the operator GETs the traces endpoint over the window
Then the result contains only acme-prod's spans
And no globex-prod span appears

### Scenario: A request with no resolvable tenant is refused
Given the endpoint has no configured tenant and the request carries no tenant signal
When the operator GETs the traces endpoint over any window
Then the endpoint refuses to serve the request
And no spans are returned

### Scenario: Tenant scoping uses the platform tenant identity
Given the write path persisted spans under aegis tenant "acme-prod"
When the read path resolves tenant "acme-prod" and queries the window
Then the same spans the write path wrote are returned
And the tenant identity is the same aegis TenantId vocabulary used on write

## Acceptance Criteria
- [ ] Every trace query is scoped to exactly one resolved `aegis::TenantId`.
- [ ] A query never returns another tenant's spans (zero cross-tenant leak).
- [ ] No resolvable tenant means the request is refused (fail-closed, HTTP 401); no spans returned.
- [ ] The tenant-resolution seam is swappable (config today; header/Bearer later) without changing the query path.

## Outcome KPIs
- **Who**: the traces read endpoint / platform owner
- **Does what**: scopes trace reads to one tenant and refuses when none resolves
- **By how much**: 0 cross-tenant span leaks; 100% of no-tenant requests refused
- **Measured by**: tenant-isolation acceptance tests via tower oneshot (two-tenant fixture; no-tenant fixture)
- **Baseline**: n/a (no traces read path today)

## Technical Notes (Optional)
- `aegis::TenantId` newtype. Ray buckets are keyed by `(TenantId, ...)` (`file_backed.rs:83`). The gateway resolves a fail-closed default via `KALEIDOSCOPE_DEFAULT_TENANT`; the metrics read path mirrors it (`KALEIDOSCOPE_QUERY_TENANT`, ADR-0042 Decision 7) and the logs read path mirrors it (`KALEIDOSCOPE_LOG_QUERY_TENANT`, ADR-0047 Decision 4).
- Mechanism choice is RED CARD 3, owned by DESIGN.

---

## US-04: Reject a malformed window and surface a store failure honestly

### Elevator Pitch
- Before: a malformed time window or a store read failure might be silently mis-answered (an empty array that looks like "no spans") or crash the endpoint, misleading the operator during an incident.
- After: the operator GETs the traces endpoint with a non-numeric or inverted window and sees an HTTP 400 with a readable error naming the bad window; and when the durable store fails to read, sees an HTTP 500, never a fabricated empty success, exercised via tower oneshot with a store double that fails.
- Decision enabled: the operator can trust that an empty result means "no spans", a 400 means "fix your window", and a 500 means "the backend failed", three unmistakably different outcomes.

### Problem
Sara fat-fingers the window (a non-numeric bound, or `start` later than `end`), or the
durable store hits an I/O error and returns `TraceStoreError::PersistenceFailed`. If the
endpoint turned a bad window into an empty array, or a store failure into an empty
success, Sara would read "no spans" and conclude her service was silent during an
incident, when in fact she asked the wrong question or the backend broke. She needs these
three outcomes (empty, bad-request, server-error) to be distinct and honest.

### Who
- On-call operator | submitting a window or hitting a backend failure | needs honest,
  distinct outcomes rather than a misleading empty result.
- The team / future maintainer | needs the failure boundary expressed as executable
  rejections.

### Solution
Validate the window: a non-numeric bound, or `start > end`, returns HTTP 400 with a
readable error naming the invalid window, before any store query is run. When
`TraceStore::query` returns `TraceStoreError::PersistenceFailed`, return HTTP 500, never a
fabricated empty success. The error text never echoes a forwarded header/credential value
(symmetry with the metrics and logs read paths, ADR-0042 Decision 6 and ADR-0027 section
6).

### Domain Examples
### 1: Happy Path - inverted window rejected
Sara submits a window with `start` later than `end`. The endpoint returns HTTP 400 with
an error naming the inverted window. No store query is run.

### 2: Edge Case - non-numeric bound rejected
Sara submits a window with a non-numeric `start`. The endpoint returns HTTP 400 with an
error naming the malformed bound. No store query is run.

### 3: Boundary - store read failure surfaces as 500
The durable store fails to read for tenant "acme-prod" (a `PersistenceFailed`). Sara GETs
the traces endpoint over a valid window. The endpoint returns HTTP 500 naming a backend
failure, NOT an empty array. A forwarded `Authorization: Bearer SECRET` header is not
echoed in the error text.

## UAT Scenarios (BDD)
### Scenario: An inverted window is rejected
Given the operator submits a window where start is later than end
When the endpoint validates the window
Then the HTTP status is 400
And the error message names the inverted or invalid window
And no store query is run

### Scenario: A non-numeric window bound is rejected
Given the operator submits a window with a non-numeric bound
When the endpoint validates the window
Then the HTTP status is 400
And the error message names the malformed bound
And no store query is run

### Scenario: A store read failure surfaces as a server error, not an empty result
Given the durable ray store fails to read for tenant "acme-prod" with a persistence error
When the operator GETs the traces endpoint over a valid window
Then the HTTP status is a 500 server error
And the response names a backend failure
And the response is not a fabricated empty success

### Scenario: A failure response never leaks a forwarded header value
Given the operator's request carries a forwarded Authorization header "Bearer SECRET"
When the endpoint returns an error response
Then the error text does not contain "SECRET" or the header value

## Acceptance Criteria
- [ ] A non-numeric window bound, or `start > end`, returns HTTP 400 with a readable error naming the invalid window.
- [ ] A malformed window never silently becomes an empty result, and no store query is run for it.
- [ ] A `TraceStoreError::PersistenceFailed` from the store returns HTTP 500, never a fabricated empty success.
- [ ] The error text never contains a forwarded header/credential value.
- [ ] Each rejected form and the store-failure arm are covered by an executable test via tower oneshot (a store double that returns `PersistenceFailed`).

## Outcome KPIs
- **Who**: on-call operator and the team
- **Does what**: receives honest, distinct outcomes (empty vs 400 vs 500) instead of a misleading empty result
- **By how much**: 100% of malformed windows return 400 and 100% of store failures return 500; 0 fabricated empties; 0 leaked header values
- **Measured by**: acceptance test per rejected window form; a store-double test for the 500 arm; a redaction test for the header leak
- **Baseline**: n/a (no read path today)

## Technical Notes (Optional)
- `TraceStoreError::PersistenceFailed { reason }` is the only typed store failure (`store.rs:35`); map it to 500. The in-memory adapter never returns it, so the 500 arm is tested with a store double / failing `FileBackedTraceStore`.
- On-wire window shape and exact 400 mapping mirror the metrics and logs read paths' bounds handling (ADR-0042 Decision 6, ADR-0047 Decision 3); RED CARD 4 for DESIGN.
- Redaction posture mirrors the sibling read paths and ADR-0027 section 6.
