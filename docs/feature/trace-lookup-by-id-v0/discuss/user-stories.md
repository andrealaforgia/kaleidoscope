<!-- markdownlint-disable MD024 -->

# User Stories: trace-lookup-by-id-v0

British English. No em dashes. This feature is a thin slice on top of the
existing `crates/trace-query-api`. The substrate's
`ray::TraceStore::get_trace(&tenant, &trace_id) -> Result<Vec<Span>,
TraceStoreError>` already exists at `crates/ray/src/store.rs:72` with the
right semantics ("Return every span sharing `trace_id` for this tenant.
Empty trace returns `Ok(Vec::new())`"). The DELIVER is parse-and-wire on
the HTTP boundary. No ray change.

The exact HTTP shape of the new way to ask (new path versus extended
parameter) is a DESIGN flag (see `wave-decisions.md` FLAG 1). The
stories phrase the entry point as "GET the trace lookup endpoint with
`trace_id = <hex>`" so they remain robust to either DESIGN choice. The
walking-skeleton acceptance test in slice 01 will use whichever URL
DESIGN settles on; the user value (the OPERATOR's question, the
OPERATOR's answer) is the same either way.

## System Constraints

- The substrate is unchanged. The ray `TraceStore` trait is NOT modified;
  `get_trace` already exists. NO change to ray, NO change to any storage
  trait. The DELIVER touches only `crates/trace-query-api/src/lib.rs` (or
  a sibling module within the same crate) and the acceptance tests.
- The response shape on the lookup arm is the SAME bare JSON array of
  `ray::Span`s the existing window arm returns (ADR-0048 Decision 2), in
  the store's ascending `start_time_unix_nano` order. There is no
  envelope. There is no parent/child topology. Trace assembly is OUT.
- Tenant scoping is mandatory and fail-closed, identical to the existing
  arm: a request with no resolvable tenant is refused (HTTP 401). The
  `(tenant, trace_id)` key on the ray dual index makes cross-tenant
  isolation a property of the substrate; the HTTP boundary preserves it.
- The error envelope is the existing one (`{status:"error",
  error:"<reason>"}`) at the chosen status code, with the existing
  redaction posture extended: the raw `trace_id` value is NEVER echoed,
  the body contains neither "SECRET" nor "Bearer" (ADR-0048 Decision 2).
- `MAX_RESULT_ROWS = 100_000` (ADR-0050 Decision 2) still applies. A
  `get_trace` result whose `Vec<Span>` length strictly exceeds the cap is
  refused with the same NAMED 400 ("result exceeds 100000 rows") the
  existing arm returns. The window cap is inert on the lookup arm (no
  window); confirmed in `wave-decisions.md` FLAG 3.
- An unknown `trace_id` is the calm-empty arm: HTTP 200 with `[]`, NOT
  HTTP 404. A 404 would mean "the endpoint does not exist"; the endpoint
  EXISTS and is responding. "The trace was never seen for this tenant"
  is the same shape of calm empty as "the window has no spans" on the
  existing arm.
- An invalid `trace_id` format (non-hex, wrong length) is HTTP 400 with
  the existing envelope, with the existing redaction (the raw value is
  not echoed).
- OTel pins `trace_id` to a 128-bit value rendered as exactly 32
  lowercase hex characters (W3C trace context). The ray hex codec at
  `crates/ray/src/span.rs:42-60` is case-insensitive on the input. The
  HTTP boundary honours the substrate; FLAG 2 confirms.
- FLAGGED to DESIGN, NOT decided here: (1) the URL shape, new path
  `/api/v1/traces/by_id?trace_id=...` versus extension of the existing
  `/api/v1/traces` with a `trace_id` parameter that overrides
  `service+range`; (2) the exact `trace_id` parsing rule (32-char hex,
  case-insensitive recommended); (3) whether the result cap applies on
  the lookup arm (recommended uniform); (4) ADR-0053 vs in-place edit of
  ADR-0048 (recommended small ADR-0053).
- OUT of scope for slice 01 (deferred, declared): tree assembly (root +
  children topology, parent/child resolution beyond the bare-array
  shape); per-service filter inside a trace; batch lookup of multiple
  trace_ids in one request; complex URL path routing (e.g.
  `GET /api/v1/traces/<trace_id>` REST-style); cap relaxation
  specifically for the lookup arm (cap still applies as for the existing
  endpoint); any prism UI for the lookup arm.

---

## US-01: Look up a trace by id and read its spans over HTTP

### Elevator Pitch

- Before: an operator with a `trace_id` in hand (from prism, a log line,
  telemetry from another system) must use the existing
  `/api/v1/traces?service=<name>&start=&end=` shape, which forces them
  to name a service they already know is implicit in the trace and to
  estimate a time window when they no longer need to estimate one. The
  spans they want are durable in `FileBackedTraceStore` under the
  `(tenant, trace_id)` index already; the question shape just is not on
  the HTTP boundary yet.
- After: the operator GETs the trace lookup endpoint with `trace_id =
  "aabbccddeeff00112233445566778899"` for tenant "acme-prod" and sees
  the HTTP 200 response carry exactly that trace's spans (and only that
  trace's spans) as a bare JSON array, in ascending
  `start_time_unix_nano` order, with every `Span` field intact, e.g.
  `[{"trace_id":"aabbccddeeff00112233445566778899","span_id":"01...","name":"place-order","start_time_unix_nano":1716200005000000000,"kind":"Server", ...}, ...]`,
  exercised in the acceptance suite via tower `oneshot` against the real
  durable store.
- Decision enabled: the operator can finally pivot from "I have this
  trace_id" to "here are the spans for this trace, for my tenant",
  without naming a service or estimating a window.

### Problem

Sara Okafor is on call for tenant "acme-prod". A user-reported error
gives her a `trace_id` (the user's frontend has shipped it via prism's
error overlay, or a partner team's incident ticket pasted it from their
logging stack). She knows EXACTLY which trace she wants. The
trace-query-api today only accepts `(service, start, end)`. To read the
trace she must (a) recall or guess which service emitted the root span,
(b) estimate a window wide enough to catch the trace, and (c) filter the
response client-side to the one trace_id. Step (a) is brittle (a trace
typically crosses services); step (b) is wasted estimation; step (c) is
wasted bandwidth and CPU. She needs to ask for the trace by id.

### Who

- On-call SRE / operator | reading a specific trace by `trace_id` mid-
  incident | wants the spans for that trace, fast, scoped to their
  tenant.
- Future prism trace client | reading a trace by id from a permalink or
  a click-through from an error overlay | needs the trace's spans as
  JSON.

### Solution

An HTTP GET endpoint that, for a resolved tenant and a `trace_id`,
reads the matching spans from the real durable `FileBackedTraceStore`
via the EXISTING `TraceStore::get_trace(&tenant, &trace_id)` and returns
them as a bare JSON array, faithfully carrying every `Span` field, in
ascending `start_time_unix_nano` order. The URL shape (new path versus
extension) is a DESIGN decision (FLAG 1); the substrate call, the
response shape, and the field fidelity are pinned here.

### Domain Examples

### 1: Happy Path - known trace_id, only that trace's spans returned

Tenant "acme-prod" has six spans persisted via the aperture trace path,
TWO trace_ids represented. Four spans share trace_id
`"abc123abc123abc123abc123abc12345"` (a Server "place-order", an
Internal "reserve-stock", an Internal "decrement-inventory", a Client
"charge-card"). Two spans share a different trace_id
`"ffffffffffffffffffffffffffffffff"` (a Server "health-probe", an
Internal "stats-flush"). Sara GETs the trace lookup endpoint with
`trace_id = "abc123abc123abc123abc123abc12345"`. The response carries
exactly the four spans of that trace, in ascending `start_time` order,
each with its `name`, `kind`, and `trace_id` equal to the requested id.
None of the two "ffff..." spans appears.

### 2: Edge Case - unknown trace_id returns the calm empty arm

Sara GETs the trace lookup endpoint with `trace_id =
"00000000000000000000000000000000"` for tenant "acme-prod". No spans
under that trace_id are persisted for "acme-prod". The response is HTTP
200 with the bare empty array `[]`, NOT a 404, NOT an error.

### 3: Boundary - all span fields round-trip faithfully for the looked-up trace

Tenant "acme-prod" has one trace
`"deadbeefdeadbeefdeadbeefdeadbeef"` consisting of one Server span
with `name = "place-order"`, `status.code = Error`, `status.message =
"upstream timeout"`, `attributes = {"http.route":"/orders"}`,
`resource_attributes = {"service.name":"checkout"}`, a populated
`parent_span_id`, one event, one link. Sara GETs the trace lookup
endpoint with that trace_id. The returned JSON carries every field,
including the hex-encoded `trace_id`/`span_id`, the status, the
attribute maps, the event, and the link, with no field dropped or
renamed.

## UAT Scenarios (BDD)

### Scenario: Operator reads only that trace's spans by trace_id

Given tenant "acme-prod" has four spans sharing trace_id "abc123abc123abc123abc123abc12345" and two spans sharing a different trace_id
When the operator GETs the trace lookup endpoint with trace_id "abc123abc123abc123abc123abc12345"
Then the response carries exactly the four spans of that trace
And no span of the other trace_id appears
And the spans are in ascending start_time order

### Scenario: Every Span field round-trips in the lookup response

Given tenant "acme-prod" has a trace carrying a name, kind, status, span attributes, resource attributes, a parent span id, an event, and a link
When the operator GETs the trace lookup endpoint with that trace_id
Then the response carries every field of every span in the trace
And no field is dropped or renamed
And the trace_id field on every returned span equals the requested trace_id

### Scenario: The spans are read from the real durable store

Given the aperture trace path has persisted a trace for tenant "acme-prod" into the durable ray store
When the operator GETs the trace lookup endpoint with that trace_id
Then the same spans the write path wrote are returned
And they are read through TraceStore::get_trace against the real FileBackedTraceStore, not a fixture

## Acceptance Criteria

- [ ] A GET for a resolved tenant with a 32-char hex `trace_id` returns the spans sharing that `trace_id` under that tenant.
- [ ] Spans are returned in ascending `start_time_unix_nano` order (the store's natural order).
- [ ] Every `Span` field (`trace_id`, `span_id`, `parent_span_id`, `name`, `kind`, `start`/`end` times, `status`, `attributes`, `resource_attributes`, `events`, `links`) is carried in the response without loss or rename.
- [ ] The spans are read via `TraceStore::get_trace` against the real `FileBackedTraceStore`.
- [ ] The acceptance test exercises the endpoint via the tower `oneshot` pattern: seed two trace_ids, look up one, assert only its spans come back.

## Outcome KPIs

- **Who**: on-call operator (and the future prism trace client)
- **Does what**: pivots from "I have a trace_id" to "here are the spans
  for this trace" in one HTTP call, instead of estimating a window and
  filtering client-side
- **By how much**: from "no by-id read path exists; operator must
  estimate a window and filter client-side" to "a GET with the trace_id
  returns exactly that trace's spans for the tenant from the durable
  store"; 100% of Span fields round-trip
- **Measured by**: E2E seed -> lookup -> read acceptance test via tower
  oneshot with a two-trace_id fixture; field-fidelity assertion
- **Baseline**: 0% (no by-id read path exists; the only HTTP shape is
  the window+service one shipped in `ray-query-api-v0`)

## Technical Notes (Optional)

- Store surface: `TraceStore::get_trace(&tenant, &trace_id) ->
  Result<Vec<Span>, TraceStoreError>` (`crates/ray/src/store.rs:72`);
  durable adapter `FileBackedTraceStore`. No change to ray.
- `TraceId` is 16 bytes (`crates/ray/src/span.rs:65`), serialised as 32
  lowercase hex characters. The hex codec (`span.rs:42-60`) is
  case-insensitive on the input.
- `Span` already derives `serde::Serialize` (`span.rs`); field fidelity
  needs no hand-written mapping. The existing `success_response`
  helper (`crates/trace-query-api/src/lib.rs:265`) is reused as is.
- URL shape (new path vs extended parameter) is FLAG 1; trace_id parse
  rule is FLAG 2.

---

## US-02: Unknown trace_id returns the calm empty arm, never 404

### Elevator Pitch

- Before: an operator with a stale or wrong `trace_id` could not even
  ask the lookup question; the workflow forced a window-and-service
  query whose calm-empty arm was already a 200 with `[]` (ADR-0048
  Decision 2). The natural expectation for the by-id arm is the same.
- After: the operator GETs the trace lookup endpoint with a `trace_id`
  that the platform has never seen for tenant "acme-prod", and the
  response is HTTP 200 with the bare empty array `[]`. NOT a 404. NOT
  an error. The same calm-empty arm as the existing window arm.
- Decision enabled: the operator distinguishes "this trace was never
  seen for my tenant" (200 `[]`) from "the endpoint does not exist"
  (404, never happens because the endpoint exists) and from "the
  backend failed" (500, US-05's territory).

### Problem

Sara has a `trace_id` she got from a partner team. She queries the
lookup endpoint for "acme-prod". The trace_id was actually emitted by a
different tenant, or it predates the retention window in the ray store,
or it was simply mistyped. The substrate is honest: ray's `get_trace`
returns `Ok(Vec::new())` for an unknown `(tenant, trace_id)` pair
(`InMemoryTraceStore::get_trace` at `store.rs:182-192` makes this
explicit via `state.by_trace.get(&key).cloned().unwrap_or_default()`).
If the HTTP boundary turned that into a 404 ("endpoint not found"),
Sara would believe the endpoint was broken; if it turned it into a 500,
she would believe the backend was broken. Neither is true: the platform
is calmly answering "I have no spans for that trace_id under your
tenant". The 200 with `[]` is the honest answer.

A 404 specifically is the wrong code. A 404 means "the URL does not
resolve to a resource"; the URL resolves to the lookup endpoint, which
is responding. The "not found" is on the trace, not the endpoint.
HTTP's affordance for "endpoint exists, your specific query matched
nothing" is 200 with the empty arm, the same affordance ADR-0048
Decision 2 chose for the window arm.

### Who

- On-call operator | querying with stale or mistyped trace_ids during
  incident triage | needs "no such trace for my tenant" to read as
  calm, not alarming, and unmistakably different from a backend
  failure.

### Solution

When `TraceStore::get_trace` returns `Ok(Vec::new())`, the endpoint
serialises the empty arm of the response (HTTP 200 with the bare empty
array `[]`), never an error status. The contract is identical to the
existing window arm's empty case.

### Domain Examples

### 1: Happy Path - trace_id that was never seen for this tenant

Tenant "acme-prod" has spans persisted under several trace_ids; none of
them is `"00000000000000000000000000000000"`. Sara GETs the lookup
endpoint with that trace_id. The store returns `Ok(Vec::new())`. The
response is HTTP 200 with `[]`.

### 2: Edge Case - a perfectly-formed but stale trace_id (rotated out of retention)

Sara has a `trace_id` `"11112222333344445555666677778888"` from a
six-week-old incident report. The aperture trace path no longer holds
spans for that trace_id (retention rotation). The store returns
`Ok(Vec::new())`. The response is HTTP 200 with `[]`, NOT a 404.

### 3: Boundary - a trace_id present under a different tenant only

A `trace_id` `"deadbeefdeadbeefdeadbeefdeadbeef"` exists under tenant
"globex-prod" but NOT under "acme-prod". The endpoint resolves
"acme-prod" and Sara queries that trace_id. The store returns
`Ok(Vec::new())` for the `(acme-prod, that-trace-id)` key. The
response is HTTP 200 with `[]` (cross-tenant isolation is enforced; see
US-05 for the dedicated isolation scenario).

## UAT Scenarios (BDD)

### Scenario: An unknown trace_id returns 200 with the empty array

Given tenant "acme-prod" has no spans persisted under trace_id "00000000000000000000000000000000"
When the operator GETs the trace lookup endpoint with that trace_id
Then the HTTP status is 200
And the response body is the bare empty array []
And the response is not an error envelope

### Scenario: An unknown trace_id is never a 404

Given tenant "acme-prod" has no spans persisted under trace_id "00000000000000000000000000000000"
When the operator GETs the trace lookup endpoint with that trace_id
Then the HTTP status is 200
And the HTTP status is not 404

## Acceptance Criteria

- [ ] An empty `TraceStore::get_trace` result serialises to HTTP 200 with the bare empty array `[]`.
- [ ] An unknown `trace_id` is NEVER a 404; the endpoint exists and is responding.
- [ ] Empty is never an error envelope (no `{status:"error",...}` body on the empty arm).
- [ ] The acceptance test seeds spans for one trace_id, queries a different (unknown) trace_id, and asserts the calm-empty arm.

## Outcome KPIs

- **Who**: on-call operator
- **Does what**: distinguishes "no spans for this trace_id under my
  tenant" from "the backend failed" and from "the URL does not exist"
- **By how much**: 100% of unknown-trace_id queries return 200 `[]`; 0
  false 404s; 0 false 5xxs on the empty path
- **Measured by**: acceptance test asserting 200 `[]` for an unknown
  trace_id with a populated store
- **Baseline**: n/a (no lookup arm exists today)

## Technical Notes (Optional)

- `TraceStore::get_trace` semantics are pinned in the doc comment
  (`crates/ray/src/store.rs:70-71`): "Return every span sharing
  `trace_id` for this tenant. Empty trace returns `Ok(Vec::new())`."
  Confirmed in the `InMemoryTraceStore` adapter
  (`store.rs:182-192`).
- The existing `success_response` helper (`lib.rs:265`) already
  serialises an empty `Vec<Span>` as `[]` with HTTP 200; no special
  casing needed beyond calling it on the empty arm.

---

## US-03: Refuse a lookup when no tenant resolves (fail-closed)

### Elevator Pitch

- Before: tenant fail-closed is the existing route's contract (ADR-0048
  Decision 4): unresolved tenant means HTTP 401, no store query run.
  Without an explicit story, a new arm could quietly slip past the
  seam.
- After: the operator (or anyone) GETs the trace lookup endpoint with a
  perfectly-formed `trace_id` but no resolvable tenant, and the
  response is HTTP 401 with the existing error envelope. No `get_trace`
  call is made; no span is returned even if a matching trace exists
  under some tenant. Exercised via tower `oneshot` with a no-tenant
  fixture.
- Decision enabled: the platform owner and the security reviewer can
  trust that the lookup arm honours the same fail-closed posture as
  the window arm, so a misconfigured deploy refuses to serve rather
  than fabricating a tenant.

### Problem

The platform is multi-tenant; ray keys every span bucket by
`(TenantId, ...)`. A lookup endpoint that ignored the tenant seam, or
defaulted to "first tenant found", would leak one customer's spans to
an unauthenticated caller, which for span attributes (request routes,
identifiers, sometimes payload fragments) is a serious breach. The
existing window arm refuses unscoped requests (`lib.rs:134-142`, HTTP
401); the lookup arm must match.

### Who

- Platform owner / security reviewer | needs guaranteed tenant
  isolation on trace lookups, including refusal when no tenant
  resolves.
- On-call operator | scoped to their own tenant | must never accidentally
  fetch a span outside their tenant by id.

### Solution

The lookup arm reuses the existing tenant-resolve seam at the router.
If `Option<TenantId>` is `None`, the endpoint returns HTTP 401 with the
existing error envelope before any input parsing and before any
`get_trace` call. The mechanism (configured single tenant today,
header / Bearer later) is identical to the existing arm; the seam is
shared.

### Domain Examples

### 1: Happy Path - configured tenant, lookup runs

The endpoint is configured with tenant "acme-prod". A request with a
valid trace_id under "acme-prod" returns 200 with that trace's spans.

### 2: Edge Case - no tenant configured, lookup refused

The endpoint starts with no configured tenant
(`KALEIDOSCOPE_TRACE_QUERY_TENANT` unset or empty) and the request
carries no tenant signal. Sara GETs the lookup endpoint with a
perfectly-formed `trace_id` (e.g. `"abc123abc123abc123abc123abc12345"`).
The response is HTTP 401 with `{"status":"error",
"error":"no tenant resolvable: ..."}`. No `get_trace` call is made.

### 3: Boundary - matching spans exist under SOME tenant, still refused

A trace_id `"abc123abc123abc123abc123abc12345"` is persisted under
tenant "acme-prod". The endpoint is started with no configured tenant
and a request for that trace_id arrives. The response is HTTP 401; the
spans under "acme-prod" are NOT returned (the unscoped caller has not
earned the right to see them).

## UAT Scenarios (BDD)

### Scenario: A lookup with no resolvable tenant is refused

Given the endpoint has no configured tenant and the request carries no tenant signal
When the operator GETs the trace lookup endpoint with a valid trace_id
Then the HTTP status is 401
And the response body is the error envelope
And no get_trace call is made against the store

### Scenario: Spans existing under a tenant are not returned to an unscoped caller

Given the durable store holds spans for trace_id "abc123abc123abc123abc123abc12345" under tenant "acme-prod"
And the endpoint has no configured tenant
When an unscoped request for that trace_id arrives
Then the HTTP status is 401
And the response body does not contain any span field from that trace

## Acceptance Criteria

- [ ] A lookup with no resolvable tenant returns HTTP 401 with the existing error envelope.
- [ ] No `get_trace` call is made against the store on the fail-closed path (proven by a `FailingTraceStore` double that would lift the response to a 500 if wrongly queried).
- [ ] Spans existing under some tenant are not returned to an unscoped caller.
- [ ] The acceptance test seeds spans, runs the router with `tenant = None`, and asserts 401 with no leak.

## Outcome KPIs

- **Who**: platform owner / security reviewer
- **Does what**: trusts that the lookup arm refuses unscoped requests
- **By how much**: 100% of no-tenant lookups return 401; 0 span leaks
  to unscoped callers; 0 `get_trace` calls on the fail-closed path
- **Measured by**: acceptance test running the router with `tenant =
  None` and a populated store, asserting 401 with no span leak;
  `FailingTraceStore` double assertion confirms the store is not
  touched
- **Baseline**: n/a (no lookup arm exists today; the existing window
  arm already enforces fail-closed, established by ADR-0048 Decision 4)

## Technical Notes (Optional)

- The existing tenant-resolve seam at `lib.rs:134-142` is reused; no
  change. The mechanism (`KALEIDOSCOPE_TRACE_QUERY_TENANT` today,
  header / Bearer later) is identical to the existing arm.
- Test posture: reuse `tenant("acme-prod")` and `None` patterns from
  `crates/trace-query-api/tests/common/mod.rs`; reuse
  `FailingTraceStore` to prove no store call is made.

---

## US-04: Reject a malformed trace_id without echoing the raw value

### Elevator Pitch

- Before: without an explicit rejection arm, a malformed `trace_id` (a
  non-hex character, a 16-character string mistaken for a span_id, a
  31-character truncation) could fall through to the store or be
  silently coerced. The redaction posture (ADR-0048 Decision 2) could
  be quietly broken by an error message that echoed the raw value.
- After: the operator GETs the lookup endpoint with `trace_id =
  "not-a-trace-id"` and sees HTTP 400 with the existing error envelope
  `{"status":"error","error":"invalid trace_id format"}`. The response
  text does NOT echo the raw `trace_id` value, does NOT contain
  "SECRET" or "Bearer", and the store is NOT touched (proven by a
  `FailingTraceStore` double).
- Decision enabled: the operator (and the team auditing logs) can
  trust that a 400 means "fix your trace_id" without the platform
  having spread the user's input across the response envelope and
  the request logs.

### Problem

Sara pastes a `trace_id` that turns out to be a 16-character W3C
span_id by mistake. Or her telemetry tool generated a 31-character
truncation. Or the value contains the letter `g` (not a hex digit). The
endpoint must:

1. Reject the request with HTTP 400 BEFORE touching the store.
2. Name the class of the fault ("invalid trace_id format") without
   echoing the raw value (the value may contain a credential the
   operator has not realised they are forwarding).
3. Honour the existing redaction posture: the body MUST contain
   neither "SECRET" nor "Bearer", and must NEVER carry the raw
   `trace_id` value (extending the redaction the existing arm
   enforces for `service`/`start`/`end`).

The substrate's `TraceId` deserialiser at `span.rs:73-87` already
rejects strings whose length is not exactly 32, and the hex codec at
`span.rs:42-60` rejects non-hex characters. The HTTP boundary must
turn those rejections into the named 400 envelope and stop the
request there.

### Who

- On-call operator | fat-fingers or pastes a malformed identifier |
  needs an honest 400, not a silent empty.
- Security reviewer | auditing the response logs | needs the
  redaction posture (no raw values, no credentials) honoured on this
  arm too.

### Solution

The lookup handler parses the `trace_id` parameter as a 32-character
hex string. On any failure (missing, empty, wrong length, non-hex
character) it returns HTTP 400 with `{"status":"error", "error":"<class
label>"}`. The class label is one of a small fixed set ("trace_id is
required", "invalid trace_id format"); the raw value is NEVER echoed.
The store is NEVER touched on this path; the proof is a
`FailingTraceStore` double whose every call returns
`PersistenceFailed` (i.e. a 500 if reached). A clean 400 here proves
the bad input was caught before the store. The exact parsing rule
(case-insensitive hex, accept only 32 characters) is FLAG 2.

### Domain Examples

### 1: Happy Path - non-hex character rejected

Sara sends `trace_id = "ggggggggggggggggggggggggggggggg1"` (one
non-hex character mixed with valid hex; total length 32). The
endpoint returns HTTP 400 with `{"status":"error","error":"invalid
trace_id format"}`. The body does NOT contain `"ggggggg"`. No
`get_trace` call is made.

### 2: Edge Case - wrong length (16-char span_id mistakenly pasted)

Sara pastes a 16-character W3C span_id by mistake, `trace_id =
"0102030405060708"`. The endpoint returns HTTP 400; the body does
NOT contain `"0102030405060708"`. No `get_trace` call is made. The
reason is a literal class label ("invalid trace_id format"), NOT a
hint that mentions span_ids (per `wave-decisions.md` FLAG 2
recommendation, the redaction wins over the helpful diagnostic).

### 3: Boundary - missing trace_id parameter entirely

The request URL contains no `trace_id` parameter at all. The endpoint
returns HTTP 400 with `{"status":"error","error":"trace_id is
required"}` (or the equivalent literal class label DESIGN settles
on). No `get_trace` call is made.

## UAT Scenarios (BDD)

### Scenario: A non-hex trace_id is rejected with no store call

Given the operator submits a trace_id containing a non-hex character
When the endpoint validates the trace_id
Then the HTTP status is 400
And the response body is the error envelope
And no get_trace call is made against the store

### Scenario: A wrong-length trace_id is rejected with no store call

Given the operator submits a trace_id whose length is not 32 characters
When the endpoint validates the trace_id
Then the HTTP status is 400
And the response body is the error envelope
And no get_trace call is made against the store

### Scenario: A missing trace_id parameter is rejected

Given the operator submits a request with no trace_id parameter at all
When the endpoint validates the request
Then the HTTP status is 400
And the response body is the error envelope
And no get_trace call is made against the store

### Scenario: A malformed trace_id error never echoes the raw value

Given the operator submits a trace_id containing the recognisable string "SECRET"
When the endpoint returns the 400 envelope
Then the error text does not contain "SECRET"
And the error text does not contain "Bearer"
And the error text does not contain the raw trace_id value

## Acceptance Criteria

- [ ] A `trace_id` with a non-hex character returns HTTP 400 with the existing error envelope.
- [ ] A `trace_id` with the wrong length returns HTTP 400 with the existing error envelope.
- [ ] A missing `trace_id` parameter returns HTTP 400 with the existing error envelope.
- [ ] The error text NEVER echoes the raw `trace_id` value.
- [ ] The error text contains neither "SECRET" nor "Bearer".
- [ ] No `get_trace` call is made against the store on any 400 path (proven by a `FailingTraceStore` double).
- [ ] Each rejected form is covered by an executable test via tower oneshot.

## Outcome KPIs

- **Who**: on-call operator and security reviewer
- **Does what**: gets honest 400s without spreading the raw value
  across the response
- **By how much**: 100% of malformed trace_ids return 400; 0 raw
  values echoed; 0 store calls on the 400 path; 0 "SECRET" / "Bearer"
  occurrences in any error body
- **Measured by**: acceptance test per malformed form (non-hex,
  wrong length, missing); redaction assertion on the error body
- **Baseline**: n/a (no lookup arm exists today)

## Technical Notes (Optional)

- `ray::TraceId::deserialize` (`span.rs:73-87`) rejects wrong-length
  strings via `hex::decode::<16>` (`span.rs:42-60`). The HTTP
  boundary catches the failure (e.g. via `hex::decode` directly on
  the raw parameter, or via deserialise of `TraceId` from a String,
  whichever DESIGN picks) and returns the named 400.
- Redaction symmetry: ADR-0048 Decision 2 ("the body must contain
  neither 'SECRET' nor 'Bearer', and never the raw `service`/`start`
  /`end` values"). The lookup arm extends "never the raw value" to
  the new `trace_id` parameter.
- The exact parse rule (case-insensitive hex, accept only 32-char) is
  FLAG 2; the redaction posture is HARD.

---

## US-05: Cross-tenant isolation on the lookup arm

### Elevator Pitch

- Before: the existing window arm enforces cross-tenant isolation by
  the `(tenant, service)` key on the ray dual index, exercised in
  `slice_01_traces_read.rs` US-03 scenario (
  `a_trace_query_returns_only_the_resolved_tenants_spans`). The
  lookup arm rides a DIFFERENT key, `(tenant, trace_id)`. Without an
  explicit isolation story the new arm could pass the substrate's
  isolation invariant without an explicit HTTP-boundary proof.
- After: a `trace_id` that exists under tenant A returns HTTP 200 with
  `[]` (NOT the trace's spans) when the endpoint resolves tenant B.
  Exercised via tower `oneshot` with a two-tenant fixture: seed the
  trace_id under "acme-prod"; resolve the endpoint as "globex-prod";
  assert 200 `[]` with no acme-prod span leaking into the body.
- Decision enabled: the security reviewer and the platform owner can
  see, in an executable acceptance test, that the lookup arm honours
  the same tenant isolation the substrate enforces, with no leak
  across the boundary even when the trace_id is identical across
  tenants.

### Problem

Two tenants could in principle hold spans under the same `trace_id`
(a hash collision, or a deliberate replay attack with a guessed id, or
a shared client library that hard-codes a test id). The ray substrate
keys the dual index on `(tenant, trace_id)`, so `InMemoryTraceStore::
get_trace(tenant=B, trace_id=X)` returns `Ok(Vec::new())` even when
spans exist for `(A, X)` (`store.rs:182-192`). The HTTP boundary must
preserve that property: the response under tenant B for the same
trace_id is the calm empty arm (US-02's territory), not a 404, not a
500, and CRUCIALLY not the spans from tenant A.

This story is the cross-tenant variant of US-02. US-02 covers
"trace_id never written for ANY tenant"; US-05 covers "trace_id
written for ANOTHER tenant but not for the resolved tenant", which is
the riskier case (a leak here would be a serious breach).

### Who

- Platform owner / security reviewer | needs a dedicated acceptance
  test proving zero cross-tenant leak on the lookup arm.
- On-call operator | trusts that their tenant's lookup is honestly
  scoped.

### Solution

The lookup handler resolves the tenant first (US-03's seam), then
calls `TraceStore::get_trace(&resolved_tenant, &trace_id)`. The ray
substrate's `(tenant, trace_id)` index ensures the result is scoped
to the resolved tenant. The HTTP boundary returns the bare result,
which for a tenant-mismatched lookup is `Ok(Vec::new())` and
therefore HTTP 200 `[]`. No additional logic is needed at the HTTP
boundary; the test pins the property end to end.

### Domain Examples

### 1: Happy Path - trace_id present for tenant A only, queried under tenant B

Tenant "acme-prod" has a trace persisted under trace_id
`"abc123abc123abc123abc123abc12345"`. Tenant "globex-prod" has NO
spans under that trace_id. The endpoint resolves "globex-prod". Sara
(masquerading as the globex-prod operator) GETs the lookup endpoint
with that trace_id. The response is HTTP 200 with `[]`; none of
acme-prod's spans appears in the body.

### 2: Edge Case - same trace_id present under BOTH tenants

Both "acme-prod" and "globex-prod" have spans under trace_id
`"deadbeefdeadbeefdeadbeefdeadbeef"` (a hash collision, or a deliberate
replay). The endpoint resolves "acme-prod". The lookup returns ONLY
acme-prod's spans; globex-prod's spans do not appear, even though the
trace_id matches.

### 3: Boundary - response body never contains another tenant's span name

A trace under "globex-prod" has a span named "globex: secret-order".
A lookup against "acme-prod" for the same trace_id returns 200 `[]`;
the response body's rendered string does NOT contain "globex" or
"secret-order".

## UAT Scenarios (BDD)

### Scenario: A trace_id present under tenant A returns the empty arm for tenant B

Given the durable store holds spans for trace_id "abc123abc123abc123abc123abc12345" under tenant "acme-prod"
And tenant "globex-prod" has no spans under that trace_id
When the endpoint resolving "globex-prod" GETs the lookup endpoint with that trace_id
Then the HTTP status is 200
And the response body is the bare empty array []
And the response body does not contain any acme-prod span name or attribute

### Scenario: A trace_id present under both tenants returns only the resolved tenant's spans

Given the durable store holds spans for trace_id "deadbeefdeadbeefdeadbeefdeadbeef" under both "acme-prod" and "globex-prod"
When the endpoint resolving "acme-prod" GETs the lookup endpoint with that trace_id
Then the HTTP status is 200
And the response body carries only acme-prod's spans
And no globex-prod span appears

## Acceptance Criteria

- [ ] A `trace_id` present under tenant A returns HTTP 200 `[]` when the endpoint resolves tenant B.
- [ ] When the same `trace_id` is present under both tenants, the lookup returns ONLY the resolved tenant's spans.
- [ ] The response body for the cross-tenant empty arm does NOT contain any span name, attribute, or identifier from the other tenant.
- [ ] The acceptance test uses a two-tenant fixture seeded into the real durable store and asserts the isolation property end to end.

## Outcome KPIs

- **Who**: platform owner / security reviewer
- **Does what**: trusts that the lookup arm enforces tenant isolation
  on the `(tenant, trace_id)` key as well as on the `(tenant, service)`
  key
- **By how much**: 0 cross-tenant span leaks on the lookup arm; 100% of
  cross-tenant lookups return 200 `[]`
- **Measured by**: two-tenant acceptance test (seeded under "acme-prod",
  endpoint resolves "globex-prod", assert 200 `[]` with no leak);
  same-trace_id-both-tenants acceptance test asserting only the
  resolved tenant's spans are returned
- **Baseline**: n/a (no lookup arm exists today; the existing window
  arm already enforces isolation on the `(tenant, service)` key, US-03
  of `ray-query-api-v0`)

## Technical Notes (Optional)

- `ray::InMemoryTraceStore::get_trace` (`store.rs:182-192`) uses the
  `(tenant, trace_id)` key on `state.by_trace`. An unknown key
  returns `Ok(Vec::new())` via `cloned().unwrap_or_default()`. The
  substrate enforces isolation; the HTTP boundary preserves it.
- Test posture: extend the `common/mod.rs` helpers with a
  `traces_by_id_request(trace_id)` builder (or whatever URL shape
  DESIGN picks for FLAG 1), reusing `open_durable_store`, `tenant`,
  `seed`, `span_with_ids` for the two-tenant fixture.
