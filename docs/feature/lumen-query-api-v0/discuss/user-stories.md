<!-- markdownlint-disable MD024 -->

# User Stories: lumen-query-api-v0

British English. No em dashes. This feature is the HTTP READ path for logs: the exact
analogue of `query-range-api-v0` for metrics. Logs are stored durably in `lumen`
(`FileBackedLogStore`) and written by the gateway, but nothing reads them back. Slice
01 is one thin walking skeleton: given a tenant and a window `[start, end]`, return the
`LogRecord`s in the window as JSON, read from the real durable store. The "After" line
of each story references the real entry point (the new HTTP endpoint), exercised in the
acceptance suite via the tower `oneshot` pattern. The exact path/shape of the endpoint
and the response envelope are DESIGN decisions (see System Constraints and
wave-decisions.md).

## System Constraints

- Logs are NOT metrics. There is no PromQL, no metric name, and no query language in
  slice 01. The query inputs are a tenant and a half-open time window `[start, end)`
  only. This is the deliberate divergence from the metrics endpoint.
- The store surface is verified (`crates/lumen/src/store.rs`):
  `LogStore::query(&self, tenant: &TenantId, range: TimeRange) -> Result<Vec<LogRecord>, LogStoreError>`.
  Per-tenant isolation; ascending `observed_time_unix_nano` order; half-open
  `[start, end)`. `FileBackedLogStore` is the durable adapter (`file_backed.rs:211`).
- A `LogRecord` (`crates/lumen/src/record.rs:44`) carries `observed_time_unix_nano`,
  `severity_number`, `severity_text`, `body`, `attributes`, `resource_attributes`,
  optional `trace_id` / `span_id`. The response carries these fields faithfully.
- `TimeRange` is half-open `[start_unix_nano, end_unix_nano)` in u64 nanoseconds
  (`record.rs:97`). A record matches iff `start <= observed_time_unix_nano < end`.
- Tenant scoping is mandatory and fail-closed: every query is scoped to exactly one
  resolved `aegis::TenantId`; no resolvable tenant means the request is refused. The
  resolution MECHANISM is a DESIGN decision (RED CARD 3); the BEHAVIOUR is pinned here.
- `LogStoreError::PersistenceFailed` surfaces as an HTTP 5xx, never a fabricated empty
  success.
- FLAGGED to DESIGN, NOT decided here: (1) the response contract, Loki-shaped (Grafana)
  versus a plain JSON array of `LogRecord`s; (2) whether this is a NEW crate
  (`log-query-api` / `lumen-query-api`) or extends the existing `query-api` crate;
  (3) same-origin/static serving for a future prism log UI (out of slice 01).
- OUT of scope for slice 01 (deferred, declared): severity/level filtering; full-text
  body search; attribute/resource matchers (even though `query_with(predicate)`
  exists); pagination/limits/ordering beyond the store's natural order; any prism UI.
- The endpoint path and the on-the-wire shape of `start`/`end` (RED CARD 4) are DESIGN
  decisions. Stories phrase the entry point as "GET the logs endpoint for tenant X over
  [start,end]".

---

## US-01: Read the in-window logs for a tenant over HTTP

### Elevator Pitch
- Before: the gateway writes logs durably into lumen, but there is no read path at all; the logs are written and invisible, sitting millimetres away in `FileBackedLogStore` with nothing able to surface them.
- After: an operator GETs the logs endpoint for tenant "acme-prod" over `[start, end]` and sees the in-window `LogRecord`s returned as JSON, e.g. `[{"observed_time_unix_nano":1716200005000000000,"severity_text":"ERROR","body":"checkout: payment timeout","resource_attributes":{"service.name":"checkout"}, ...}, ...]` (envelope shape is a DESIGN decision), exercised in the acceptance suite via a tower `oneshot` against the real durable store.
- Decision enabled: the operator can finally READ what the platform recorded for this tenant in this window, and decide whether the service was healthy during it.

### Problem
Sara Okafor is an on-call SRE for the "checkout" service, tenant "acme-prod". The
gateway has been writing her service's logs into lumen for weeks; they are durable in
`FileBackedLogStore`. But there is no way to read them back: no HTTP query API for logs
exists anywhere in the platform. When checkout misbehaves she has metrics (the
`query_range` read path shipped) but is blind to her own logs, even though they are
stored a few millimetres away. She needs to pull the logs for her tenant over a window
and read them.

### Who
- On-call SRE / operator | reading the logs for a known tenant over a recent window |
  wants to see the records, fast, during or after an incident.
- Later, prism's log client | the future machine consumer | needs the in-window records
  as JSON in whatever envelope DESIGN settles on.

### Solution
An HTTP GET endpoint that, for a resolved tenant and a half-open window `[start, end)`,
calls `LogStore::query(&tenant, range)` on the real durable `FileBackedLogStore` and
returns the matching `LogRecord`s as JSON, faithfully carrying the record fields
(`observed_time_unix_nano`, `severity_*`, `body`, `attributes`, `resource_attributes`,
optional `trace_id`/`span_id`), in the store's ascending `observed_time` order. The
response envelope (Loki-shaped vs plain array) is a DESIGN decision; the records and
their fields are pinned here.

### Domain Examples
### 1: Happy Path - in-window records returned, out-of-window excluded
Tenant "acme-prod" has six log records ingested via the gateway: three with
`observed_time` inside `[1716200000s, 1716200060s)` (an INFO "checkout: started", a WARN
"checkout: slow upstream", an ERROR "checkout: payment timeout") and three outside it
(two earlier, one later). Sara GETs the logs endpoint for "acme-prod" over
`[1716200000, 1716200060]`. The response carries exactly the three in-window records, in
ascending `observed_time` order, each with its `severity_text`, `body`, and
`resource_attributes.service.name = "checkout"`.

### 2: Edge Case - half-open window includes start, excludes end
"acme-prod" has one record at exactly `start` (`observed_time == start_ns`) and one at
exactly `end` (`observed_time == end_ns`). Because the range is half-open `[start, end)`,
the record at `start` is INCLUDED and the record at `end` is EXCLUDED. The response
carries the single record at `start`.

### 3: Boundary - all fields round-trip faithfully
"acme-prod" has one record with `body = "db pool exhausted"`, `severity_text = "ERROR"`,
`severity_number = 17`, `attributes = {"http.status_code":"503"}`,
`resource_attributes = {"service.name":"checkout"}`, and a populated `trace_id`. Sara
queries a window covering it. The returned JSON carries every field, including the
attribute maps and the trace id, with no field dropped or renamed.

## UAT Scenarios (BDD)
### Scenario: Operator reads the in-window logs for a tenant
Given tenant "acme-prod" has log records inside the window [1716200000s, 1716200060s) and others before and after it
When the operator GETs the logs endpoint for tenant "acme-prod" over [1716200000, 1716200060]
Then the response carries exactly the in-window records
And no out-of-window record appears
And the records are in ascending observed_time order

### Scenario: The half-open window includes start and excludes end
Given tenant "acme-prod" has one record at exactly the window start and one at exactly the window end
When the operator GETs the logs endpoint over [start, end)
Then the record at start is included
And the record at end is excluded

### Scenario: Every LogRecord field round-trips in the response
Given tenant "acme-prod" has a record carrying a body, severity text and number, record attributes, resource attributes, and a trace id
When the operator GETs the logs endpoint over a covering window
Then the response carries every field of the record
And no field is dropped or renamed

### Scenario: The logs are read from the real durable store
Given the gateway has persisted log records for tenant "acme-prod" into the durable lumen store
When the operator GETs the logs endpoint over a window covering them
Then the same records the gateway wrote are returned
And they are read through LogStore::query against the real FileBackedLogStore, not a fixture

## Acceptance Criteria
- [ ] A GET for a resolved tenant over `[start, end)` returns the records whose `observed_time_unix_nano` falls in the half-open window.
- [ ] Records are returned in ascending `observed_time_unix_nano` order (the store's natural order).
- [ ] A record at exactly `start` is included; a record at exactly `end` is excluded.
- [ ] Every `LogRecord` field (`observed_time_unix_nano`, `severity_number`, `severity_text`, `body`, `attributes`, `resource_attributes`, `trace_id`, `span_id`) is carried in the response without loss or rename.
- [ ] The records are read via `LogStore::query(&tenant, range)` against the real `FileBackedLogStore`.
- [ ] The acceptance test exercises the endpoint via the tower `oneshot` pattern, ingest in-window and out-of-window records, query, assert only in-window come back.

## Outcome KPIs
- **Who**: on-call operator (and the future prism log client)
- **Does what**: reads the stored logs for a tenant over a window, instead of having them written and unseen
- **By how much**: from "cannot read logs at all" (0 readable) to "a GET returns exactly the in-window records for the tenant from the durable store"; 100% of LogRecord fields round-trip
- **Measured by**: E2E ingest -> query -> read acceptance test via tower oneshot; field-fidelity assertion
- **Baseline**: 0% (no logs read path exists anywhere in the platform)

## Technical Notes (Optional)
- Store surface: `LogStore::query(&TenantId, TimeRange) -> Result<Vec<LogRecord>, LogStoreError>` (`crates/lumen/src/store.rs`); durable adapter `FileBackedLogStore` (`crates/lumen/src/file_backed.rs:211`).
- `TimeRange` is half-open and in u64 nanoseconds (`crates/lumen/src/record.rs:97`); on-wire window shape is RED CARD 4 for DESIGN.
- Response envelope (Loki-shaped vs plain array) is FLAG 1 for DESIGN; placement (new crate vs extend `query-api`) is FLAG 2.
- Test fixtures: see `crates/lumen/tests/v1_slice_02_snapshot.rs` for how `LogRecord`s are built, seeded and queried.

---

## US-02: Return a calm empty result when nothing is in the window

### Elevator Pitch
- Before: an operator querying a tenant or window with no logs would not know whether the platform is broken or simply has nothing to show.
- After: the operator GETs the logs endpoint for tenant "acme-prod" over a window with no records and sees a calm, well-formed empty result (an empty JSON list, or the empty arm of whatever envelope DESIGN chooses), HTTP 200, not an error.
- Decision enabled: the operator distinguishes "no logs in this window" from "backend failure" and adjusts the tenant or window.

### Problem
Sara queries a window before checkout started emitting, or a quiet period with no logs.
The store correctly returns an empty `Vec` (it returns `Ok(Vec::new())`, not an error,
for an unknown tenant or an empty match, verified in `store.rs`). If the endpoint
turned that into an error or a hang, Sara would think the platform is broken during an
incident. She needs "nothing matched" to feel calm and unmistakably different from
"something failed".

### Who
- On-call operator | exploring tenants and windows | needs "empty" to read as calm, not
  alarming.

### Solution
When `LogStore::query` returns an empty `Vec`, the endpoint serialises the empty arm of
the response (an empty list / empty result) with HTTP 200, never an error status. This
holds for both an unknown tenant (store returns empty) and a known tenant with no
records in the window.

### Domain Examples
### 1: Happy Path - known tenant, window before first log
"acme-prod" has logs but its earliest record is at `1716300000s`; Sara queries
`[1716200000, 1716200060]`. The half-open window matches nothing; the store returns an
empty `Vec`; the response is the empty arm with HTTP 200.

### 2: Edge Case - unknown tenant
The endpoint resolves tenant "globex-prod", which has never had any logs written. The
store returns `Ok(Vec::new())`. The response is the empty arm with HTTP 200, not an
error.

### 3: Boundary - a single record exactly at end only
"acme-prod" has exactly one record at `observed_time == end_ns`. Because the window is
half-open `[start, end)`, the record at `end` is excluded; the response is the empty arm.

## UAT Scenarios (BDD)
### Scenario: A window with no logs returns a calm empty result
Given tenant "acme-prod" has no log records whose observed_time falls in the requested window
When the operator GETs the logs endpoint over that window
Then the HTTP status is 200
And the result is the empty arm (an empty list of records)
And the response is not an error

### Scenario: An unknown tenant returns the empty arm, not an error
Given the endpoint resolves tenant "globex-prod" which has never had logs written
When the operator GETs the logs endpoint over any window
Then the HTTP status is 200
And the result is the empty arm

### Scenario: A record exactly at the window end is excluded
Given tenant "acme-prod" has a single record at exactly the window end
When the operator GETs the logs endpoint over [start, end)
Then the result is the empty arm because the window is half-open

## Acceptance Criteria
- [ ] An empty `LogStore::query` result serialises to the empty arm of the response with HTTP 200.
- [ ] Empty is never an error status (an empty list is a valid success).
- [ ] An unknown tenant (store returns `Ok(Vec::new())`) yields the empty arm, not an error.
- [ ] The half-open window excludes a record at exactly `end`.

## Outcome KPIs
- **Who**: on-call operator
- **Does what**: distinguishes "no logs in this window" from "backend failure"
- **By how much**: 100% of no-match queries return the success+empty arm; 0 false errors
- **Measured by**: acceptance test asserting the empty arm for an empty window and for an unknown tenant
- **Baseline**: n/a (no read path today)

## Technical Notes (Optional)
- `LogStore::query` returns `Ok(Vec::new())` for an unknown tenant or an empty match (`store.rs:141`, `file_backed.rs:211`); no special-casing needed beyond serialising empty.

---

## US-03: Scope every log query to one tenant, fail-closed

### Elevator Pitch
- Before: lumen is per-tenant but no read path exists, so there is no tenant-scoping behaviour on logs to trust.
- After: a request that resolves tenant "acme-prod" GETs the logs endpoint and sees only acme-prod's records; a request with no resolvable tenant is refused with no records returned, exercised via tower oneshot with a two-tenant and a no-tenant fixture.
- Decision enabled: an operator (and the platform owner) can trust that a log query returns this tenant's logs and only this tenant's logs.

### Problem
The platform is multi-tenant; lumen keys every record bucket by `TenantId`
(`HashMap<TenantId, Vec<LogRecord>>`, verified in `file_backed.rs`). A read path that
ignored tenancy, or defaulted to "all tenants", would leak one customer's logs to
another, which for log bodies (often carrying request detail) is a serious breach. The
write path and the metrics read path both fail closed when no tenant resolves; the logs
read path must match that posture.

### Who
- Platform owner / security reviewer | needs guaranteed tenant isolation on log reads.
- On-call operator | scoped to their own tenant | must never see another tenant's logs.

### Solution
Resolve the tenant for each request (slice-01 default: a single configured tenant,
fail-closed if unset, mirroring the gateway's `KALEIDOSCOPE_DEFAULT_TENANT` and the
metrics read path's `KALEIDOSCOPE_QUERY_TENANT`). Pass the resolved `aegis::TenantId` to
`LogStore::query`. If no tenant resolves, refuse to serve. The header/Bearer mechanism
is deferred (RED CARD 3); the resolution SEAM is designed so swapping it in is
non-breaking.

> RED CARD 3: the production tenant-supply mechanism is a DESIGN decision. This story
> pins the BEHAVIOUR (scoped, fail-closed), not the mechanism.

### Domain Examples
### 1: Happy Path - configured tenant returns its own logs
The endpoint is configured with tenant "acme-prod". A query over a window returns only
acme-prod's records for that window.

### 2: Edge Case - another tenant's logs are not returned
Tenant "globex-prod" also has logs in the same window. With the endpoint scoped to
"acme-prod", the query returns acme-prod's records only; globex-prod's are absent.

### 3: Boundary - no tenant resolvable
The endpoint starts with no configured tenant and the request carries no tenant signal.
The request is refused (fail-closed); no log records are returned.

## UAT Scenarios (BDD)
### Scenario: A log query returns only the resolved tenant's records
Given the endpoint resolves tenant "acme-prod" and lumen holds in-window logs for both "acme-prod" and "globex-prod"
When the operator GETs the logs endpoint over the window
Then the result contains only acme-prod's records
And no globex-prod record appears

### Scenario: A request with no resolvable tenant is refused
Given the endpoint has no configured tenant and the request carries no tenant signal
When the operator GETs the logs endpoint over any window
Then the endpoint refuses to serve the request
And no log records are returned

### Scenario: Tenant scoping uses the platform tenant identity
Given the write path persisted logs under aegis tenant "acme-prod"
When the read path resolves tenant "acme-prod" and queries the window
Then the same records the gateway wrote are returned
And the tenant identity is the same aegis TenantId vocabulary used on write

## Acceptance Criteria
- [ ] Every log query is scoped to exactly one resolved `aegis::TenantId`.
- [ ] A query never returns another tenant's records (zero cross-tenant leak).
- [ ] No resolvable tenant means the request is refused (fail-closed); no records returned.
- [ ] The tenant-resolution seam is swappable (config today; header/Bearer later) without changing the query path.

## Outcome KPIs
- **Who**: the logs read endpoint / platform owner
- **Does what**: scopes log reads to one tenant and refuses when none resolves
- **By how much**: 0 cross-tenant log leaks; 100% of no-tenant requests refused
- **Measured by**: tenant-isolation acceptance tests via tower oneshot (two-tenant fixture; no-tenant fixture)
- **Baseline**: n/a (no logs read path today)

## Technical Notes (Optional)
- `aegis::TenantId` newtype. Lumen buckets are keyed by `TenantId` (`file_backed.rs` `per_tenant: HashMap<TenantId, Vec<LogRecord>>`). The gateway resolves a fail-closed default via `KALEIDOSCOPE_DEFAULT_TENANT`; the metrics read path mirrors it with `KALEIDOSCOPE_QUERY_TENANT` (ADR-0042 Decision 7).
- Mechanism choice is RED CARD 3, owned by DESIGN.

---

## US-04: Reject a malformed window and surface a store failure honestly

### Elevator Pitch
- Before: a malformed time window or a store read failure might be silently mis-answered (an empty list that looks like "no logs") or crash the endpoint, misleading the operator during an incident.
- After: the operator GETs the logs endpoint with a non-numeric or inverted window and sees an HTTP 400 with a readable error naming the bad window; and when the durable store fails to read, sees an HTTP 5xx, never a fabricated empty success, exercised via tower oneshot with a store double that fails.
- Decision enabled: the operator can trust that an empty result means "no logs", a 400 means "fix your window", and a 5xx means "the backend failed", three unmistakably different outcomes.

### Problem
Sara fat-fingers the window (a non-numeric bound, or `start` later than `end`), or the
durable store hits an I/O error and returns `LogStoreError::PersistenceFailed`. If the
endpoint turned a bad window into an empty list, or a store failure into an empty
success, Sara would read "no logs" and conclude her service was silent during an
incident, when in fact she asked the wrong question or the backend broke. She needs
these three outcomes (empty, bad-request, server-error) to be distinct and honest.

### Who
- On-call operator | submitting a window or hitting a backend failure | needs honest,
  distinct outcomes rather than a misleading empty result.
- The team / future maintainer | needs the failure boundary expressed as executable
  rejections.

### Solution
Validate the window: a non-numeric bound, or `start > end`, returns HTTP 400 with a
readable error naming the invalid window. When `LogStore::query` returns
`LogStoreError::PersistenceFailed`, return HTTP 5xx, never a fabricated empty success.
The error text never echoes a forwarded header/credential value (symmetry with the
metrics read path and ADR-0027 section 6).

### Domain Examples
### 1: Happy Path - inverted window rejected
Sara submits a window with `start` later than `end`. The endpoint returns HTTP 400 with
an error naming the inverted window. No store query is run.

### 2: Edge Case - non-numeric bound rejected
Sara submits a window with a non-numeric `start`. The endpoint returns HTTP 400 with an
error naming the malformed bound.

### 3: Boundary - store read failure surfaces as 5xx
The durable store fails to read for tenant "acme-prod" (a `PersistenceFailed`). Sara
GETs the logs endpoint over a valid window. The endpoint returns HTTP 5xx naming a
backend failure, NOT an empty list. A forwarded `Authorization: Bearer SECRET` header is
not echoed in the error text.

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

### Scenario: A store read failure surfaces as a server error, not an empty result
Given the durable lumen store fails to read for tenant "acme-prod" with a persistence error
When the operator GETs the logs endpoint over a valid window
Then the HTTP status is a 5xx server error
And the response names a backend failure
And the response is not a fabricated empty success

### Scenario: A failure response never leaks a forwarded header value
Given the operator's request carries a forwarded Authorization header "Bearer SECRET"
When the endpoint returns an error response
Then the error text does not contain "SECRET" or the header value

## Acceptance Criteria
- [ ] A non-numeric window bound, or `start > end`, returns HTTP 400 with a readable error naming the invalid window.
- [ ] A malformed window never silently becomes an empty result.
- [ ] A `LogStoreError::PersistenceFailed` from the store returns HTTP 5xx, never a fabricated empty success.
- [ ] The error text never contains a forwarded header/credential value.
- [ ] Each rejected form and the store-failure arm are covered by an executable test via tower oneshot (a store double that returns `PersistenceFailed`).

## Outcome KPIs
- **Who**: on-call operator and the team
- **Does what**: receives honest, distinct outcomes (empty vs 400 vs 5xx) instead of a misleading empty result
- **By how much**: 100% of malformed windows return 400 and 100% of store failures return 5xx; 0 fabricated empties; 0 leaked header values
- **Measured by**: acceptance test per rejected window form; a store-double test for the 5xx arm; a redaction test for the header leak
- **Baseline**: n/a (no read path today)

## Technical Notes (Optional)
- `LogStoreError::PersistenceFailed { reason }` is the only typed store failure (`store.rs:44`); map it to 5xx. The in-memory adapter never returns it, so the 5xx arm is tested with a store double / failing `FileBackedLogStore`.
- On-wire window shape and exact 400 mapping mirror the metrics read path's bounds handling (ADR-0042 Decision 6); RED CARD 4 for DESIGN.
- Redaction posture mirrors the metrics read path and ADR-0027 section 6.
