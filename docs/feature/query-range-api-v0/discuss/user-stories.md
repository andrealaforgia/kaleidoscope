<!-- markdownlint-disable MD024 -->

# User Stories: query-range-api-v0

British English. No em dashes. The response shape is a PINNED external contract
(`apps/prism/src/lib/promql/queryRange.ts` + ADR-0027); these stories specify the
provider against that contract verbatim.

## System Constraints

- Cross-cutting: the response MUST satisfy Prism's `isPromSuccess` (status==='success'
  AND `Array.isArray(data.result)`) on every success/empty path, and `isPromError`
  (status==='error' AND typeof error==='string') on every parse-error path.
- `query` arrives as a RAW PromQL string. Slice 01 supports only a bare metric-name
  selector. Operators, functions, aggregations, `rate()` etc. are out of scope and
  must be rejected with HTTP 400 status:error, never silently mis-answered.
- `start`/`end` arrive as float epoch SECONDS; pulse `TimeRange` is u64 NANOSECONDS.
  Convert exactly. Range is half-open `[start, end)`.
- `values` pairs are `[seconds:number, value:string]`. NaN encodes as the string `"NaN"`.
- Tenant scoping is mandatory and fail-closed (RED CARD 1; mechanism owned by DESIGN).
- Metrics only. Logs (lumen) and traces (ray) are different Prism panels and endpoints;
  `query_range` returns a matrix (time series = metrics) and serves Pulse alone.
- Where the service lives (new crate/binary) is a DESIGN decision, not pinned here.
- status:error bodies must never echo a forwarded header/credential value (ADR-0027 §6).

---

## US-01: Serve a metric time series as a Prometheus matrix

### Elevator Pitch
- Before: Prism's QueryPanel is unmounted; metrics persist into Pulse but nothing can read them.
- After: an operator hits `GET /api/v1/query_range?query=http_server_requests&start=...&end=...&step=15s` and sees `{ "status": "success", "data": { "resultType": "matrix", "result": [ { "metric": {...}, "values": [[1716200000, "0.42"], ...] } ] } }`, which Prism renders as a plotted line.
- Decision enabled: the operator can read a metric's recent trend and decide whether the service is healthy.

### Problem
Sara Okafor is an on-call SRE for the "checkout" service. Telemetry flows into the
platform and persists into Pulse, but Prism cannot show it: the QueryPanel is
deliberately unmounted because the `/api/v1/query_range` backend does not exist. Sara
is blind to her own metrics despite the data being durably stored a few millimetres away.

### Who
- On-call SRE / operator | querying a known metric by name over a recent range | wants to see the trend, fast, during or before an incident.
- Prism's HTTP query client | the immediate machine consumer | needs a response its pinned validator accepts.

### Solution
A `GET /api/v1/query_range` endpoint that parses a bare metric-name selector, queries
Pulse for the tenant over `[start, end)`, groups the returned points into matrix
series by label set, and serialises a Prometheus `matrix` response that passes Prism's
`isPromSuccess` validator.

### Domain Examples
### 1: Happy Path - known metric, multiple points
Sara queries `query=process_cpu_utilization&start=1716200000&end=1716200060&step=15s`.
Pulse holds 4 points for tenant "acme-prod" at 1716200000s, ...015s, ...030s, ...045s
with values 0.40, 0.55, 0.61, 0.58 and resource attribute `service.name=checkout`.
The service returns one matrix series `metric: {"__name__":"process_cpu_utilization","service.name":"checkout"}`,
`values: [[1716200000,"0.4"],[1716200015,"0.55"],[1716200030,"0.61"],[1716200045,"0.58"]]`.

### 2: Edge Case - two series under one metric name
Tenant "acme-prod" has `http_server_active_requests` with points carrying
`route=/cart` and `route=/pay`. The service returns TWO matrix series, one per distinct
label set, each with its own `values` array.

### 3: Boundary - single point at exactly start
A metric has one point at `time_unix_nano` equal to `start * 1e9`. Because the range is
half-open `[start, end)`, the point at exactly `start` is INCLUDED; a point at exactly
`end` is EXCLUDED. The series contains the single included point.

## UAT Scenarios (BDD)
### Scenario: Operator sees a metric plotted over a range
Given tenant "acme-prod" has metric "process_cpu_utilization" with points at 1716200000s, 1716200015s, 1716200030s valued 0.40, 0.55, 0.61
When the operator requests query_range for "process_cpu_utilization" over start 1716200000 to end 1716200045
Then the response status is "success" and resultType is "matrix"
And the result contains one series with values [[1716200000,"0.4"],[1716200015,"0.55"],[1716200030,"0.61"]]
And Prism's success validator accepts the response

### Scenario: Points split into one series per label set
Given tenant "acme-prod" has metric "http_server_active_requests" with points labelled route="/cart" and route="/pay"
When the operator requests query_range for "http_server_active_requests" over the range covering both points
Then the result contains two series, one per distinct label set
And each series carries its own ascending values array

### Scenario: Half-open range includes start and excludes end
Given tenant "acme-prod" has metric "queue_depth" with a point at exactly the range start and a point at exactly the range end
When the operator requests query_range over [start, end)
Then the series includes the point at start
And the series excludes the point at end

### Scenario: Values are encoded as Prometheus strings
Given tenant "acme-prod" has metric "gc_pause_seconds" with a point value 0.0
When the operator requests query_range over a covering range
Then each values pair is [seconds_number, value_string]
And the value 0.0 is encoded as the JSON string "0"

### Scenario: Malformed or inverted time bounds are rejected
Given the operator requests query_range with a non-numeric start, or with start later than end
When the service parses the time bounds
Then the HTTP status is 400
And the response is {status:"error", error:<a message naming the invalid time bounds>}
And Prism's error validator accepts the response

### Scenario: A persistence failure surfaces as a server error
Given the Pulse store fails to read (a persistence error) for tenant "acme-prod"
When the operator requests query_range for "process_cpu_utilization"
Then the HTTP status is a 5xx server error
And Prism renders this as a transport error naming the backend, not a fabricated empty result

## Acceptance Criteria
- [ ] Response satisfies Prism's isPromSuccess (status==='success' AND Array.isArray(data.result)) on the success path.
- [ ] resultType is "matrix".
- [ ] Points are grouped into one series per distinct merged label set, including __name__.
- [ ] Each values pair is [seconds:number, value:string]; NaN encodes as "NaN".
- [ ] start/end seconds are converted to nanoseconds; range is half-open [start, end).
- [ ] Series values are in ascending time order.
- [ ] Non-numeric or inverted (start > end) time bounds return HTTP 400 status:error.
- [ ] A Pulse persistence failure returns HTTP 5xx (never a fabricated empty success).

## Outcome KPIs
- **Who**: Prism's query client and the on-call operator
- **Does what**: receives a matrix response its own validator accepts and sees a series rendered
- **By how much**: 100% of contract shapes round-trip; a known metric renders a non-empty series
- **Measured by**: contract test through Prism's validators + E2E ingest->query->render
- **Baseline**: 0% (QueryPanel unmounted, no backend)

## Technical Notes (Optional)
- Pulse surface: `query(&TenantId, &MetricName, TimeRange) -> Vec<(Metric, MetricPoint)>` (crates/pulse/src/store.rs).
- TimeRange is half-open and in nanoseconds (crates/pulse/src/metric.rs).
- Where the service lives is a DESIGN decision (likely a new crate depending on pulse + aegis).

---

## US-02: Return a calm empty result for an unknown metric or empty range

### Elevator Pitch
- Before: an operator querying a metric with no data in range would not know if the platform is broken or simply empty.
- After: the operator hits `GET /api/v1/query_range?query=does_not_exist&...` and sees `{ "status": "success", "data": { "resultType": "matrix", "result": [] } }`, which Prism renders as a calm "No data for {range}. Check the metric name or widen the range."
- Decision enabled: the operator distinguishes "no data" from "backend broken" and adjusts the metric name or range.

### Problem
Sara mistypes a metric name, or queries a window before the service started emitting.
If the backend returned an error or a hang, she would think the platform is broken
during an incident. She needs the difference between "nothing matched" and "something
failed" to be unmistakable.

### Who
- On-call operator | exploring metric names and ranges | needs "empty" to feel calm, not alarming.

### Solution
When Pulse returns an empty `Vec`, serialise `{status:success, data:{resultType:matrix, result:[]}}`.
This is the dedicated `empty` arm in Prism's client; it is NOT an error.

### Domain Examples
### 1: Happy Path - unknown metric name
Sara queries `query=htp_server_requests` (typo). Pulse has no series under that name for
tenant "acme-prod" and returns an empty Vec. The service returns `result: []` with status success.

### 2: Edge Case - known metric, range before first point
`disk_io_bytes` exists but its earliest point is at 1716300000s; Sara queries
start 1716200000 to end 1716200060. The half-open range matches nothing; `result: []`.

### 3: Boundary - point exactly at end only
A metric has a single point at `time_unix_nano == end * 1e9`. The half-open range
excludes `end`, so nothing matches; `result: []`.

## UAT Scenarios (BDD)
### Scenario: Unknown metric returns a calm empty result
Given tenant "acme-prod" has no metric named "htp_server_requests"
When the operator requests query_range for "htp_server_requests" over any range
Then the response status is "success" and resultType is "matrix"
And data.result is an empty array
And Prism renders the calm empty state, not an error banner

### Scenario: Known metric with no points in range returns empty
Given tenant "acme-prod" has metric "disk_io_bytes" whose earliest point is later than the requested range
When the operator requests query_range over the earlier range
Then data.result is an empty array
And the response status is "success"

### Scenario: A point exactly at end is excluded
Given tenant "acme-prod" has metric "queue_depth" with a single point at exactly the range end
When the operator requests query_range over [start, end)
Then data.result is an empty array because the range is half-open

## Acceptance Criteria
- [ ] Empty Pulse result serialises to {status:success, data:{resultType:matrix, result:[]}}.
- [ ] Empty is never an error status; isPromSuccess accepts it (empty array is valid).
- [ ] Half-open range excludes a point at exactly end.

## Outcome KPIs
- **Who**: on-call operator
- **Does what**: distinguishes "no data" from "backend failure"
- **By how much**: 100% of no-match queries return the success+empty shape, 0 false errors
- **Measured by**: acceptance test asserting the empty arm; Prism empty-state E2E
- **Baseline**: n/a (no backend today)

## Technical Notes (Optional)
- Pulse `query` returns `Ok(Vec::new())` for unknown series; no special-casing needed beyond serialising empty.

---

## US-03: Reject an unparseable query with an operator-readable error

### Elevator Pitch
- Before: a query the service cannot honour might be silently mis-answered or crash, misleading the operator during an incident.
- After: the operator hits `GET /api/v1/query_range?query=rate(http_requests[5m])&...` and sees HTTP 400 with `{ "status": "error", "error": "unsupported query: this endpoint accepts a bare metric name only at v0" }`, which Prism renders verbatim above the chart with the query input keeping focus.
- Decision enabled: the operator immediately understands the query was rejected and why, and can correct it to a bare metric name.

### Problem
Sara, used to full Prometheus, pastes `rate(http_requests_total[5m])`. The v0 service
supports only a bare metric name. If it guessed, hung, or 500'd, Sara would be misled
at the worst possible moment. She needs an honest, readable rejection.

### Who
- On-call operator (PromQL-literate) | pasting richer PromQL than v0 supports | needs an honest, specific rejection she can act on.

### Solution
Parse the `query` string. If it is anything other than a bare metric name (an operator,
function call, aggregation, range vector, or empty), return HTTP 400 with
`{status:error, error:"<clear reason>"}` that Prism's `isPromError` accepts. The message
names what is unsupported and what is accepted. It never echoes a forwarded header value.

### Domain Examples
### 1: Happy Path - rate() function rejected
Sara queries `query=rate(http_requests_total[5m])`. The service returns HTTP 400
`{status:error, error:"unsupported query: functions are not supported at v0; use a bare metric name"}`.

### 2: Edge Case - binary operator rejected
Sara queries `query=cpu_seconds_total / node_count`. Returns HTTP 400
`{status:error, error:"unsupported query: operators are not supported at v0; use a bare metric name"}`.

### 3: Boundary - empty query string
Sara submits an empty `query=`. Returns HTTP 400
`{status:error, error:"empty query: provide a metric name"}`.

## UAT Scenarios (BDD)
### Scenario: A function call is rejected with a readable reason
Given the operator submits query_range with query "rate(http_requests_total[5m])"
When the service parses the selector
Then the HTTP status is 400
And the response is {status:"error", error:<a message naming functions as unsupported>}
And Prism's error validator accepts the response and shows the verbatim text

### Scenario: A binary operator is rejected
Given the operator submits query_range with query "cpu_seconds_total / node_count"
When the service parses the selector
Then the HTTP status is 400
And the response status is "error" with a message naming operators as unsupported

### Scenario: An empty query is rejected
Given the operator submits query_range with an empty query string
When the service parses the selector
Then the HTTP status is 400
And the response status is "error" with a message asking for a metric name

### Scenario: A rejection never leaks a forwarded header value
Given the operator's request carries a forwarded Authorization header "Bearer SECRET"
When the service returns a status:error response for an unsupported query
Then the error text does not contain "SECRET" or the header value

## Acceptance Criteria
- [ ] Unsupported query forms (functions, operators, aggregations, range vectors, empty) return HTTP 400.
- [ ] The 400 body satisfies Prism's isPromError (status==='error' AND typeof error==='string').
- [ ] The error message names what is unsupported and what is accepted (bare metric name).
- [ ] The error message never contains a forwarded header/credential value.

## Outcome KPIs
- **Who**: on-call operator
- **Does what**: receives an honest, readable rejection instead of a silent wrong answer
- **By how much**: 100% of unsupported queries return status:error 400, 0 silent mis-answers
- **Measured by**: acceptance test per unsupported form; redaction test for header leak
- **Baseline**: n/a

## Technical Notes (Optional)
- Mirrors ADR-0027's parse-error arm (400 + status:error). Redaction posture mirrors ADR-0027 §6.

---

## US-04: Scope every query to one tenant, fail-closed

### Elevator Pitch
- Before: Pulse is per-tenant but no query path exists, so there is no tenant-scoping behaviour to trust.
- After: a request that resolves tenant "acme-prod" hits `GET /api/v1/query_range?...` and sees only acme-prod's series; a request with no resolvable tenant is refused (no data returned).
- Decision enabled: an operator (and the platform owner) can trust that a query returns this tenant's data and only this tenant's data.

### Problem
The platform is multi-tenant; Pulse keys every series by `(TenantId, MetricName)`. A
query service that ignored tenancy, or defaulted to "all tenants", would leak one
customer's metrics to another. The write path already fails closed when no tenant is
resolvable; the read path must match that posture.

### Who
- Platform owner / security reviewer | needs guaranteed tenant isolation on reads.
- On-call operator | scoped to their own tenant | must never see another tenant's data.

### Solution
Resolve the tenant for each request (slice-01 default: a single configured tenant,
fail-closed if unset, mirroring the gateway's `KALEIDOSCOPE_DEFAULT_TENANT`). Pass the
resolved `aegis::TenantId` to `pulse.query`. If no tenant resolves, refuse to serve.
The header-based mechanism (`X-Scope-OrgID` or aegis Bearer token) is deferred to a
later slice; the resolution SEAM is designed so swapping it in is non-breaking.

> RED CARD 1: the production tenant-supply mechanism is a DESIGN decision. This story
> pins the BEHAVIOUR (scoped, fail-closed) not the mechanism.

### Domain Examples
### 1: Happy Path - configured tenant returns its own data
The service is configured with tenant "acme-prod". A query for "process_cpu_utilization"
returns only acme-prod's series for that metric.

### 2: Edge Case - other tenant's identically-named metric is not returned
Tenant "globex-prod" also has "process_cpu_utilization". With the service scoped to
"acme-prod", the query returns acme-prod's points only; globex-prod's are absent.

### 3: Boundary - no tenant resolvable
The service starts with no configured tenant and the request carries no tenant signal.
The request is refused (fail-closed); no metric data is returned.

## UAT Scenarios (BDD)
### Scenario: A query returns only the resolved tenant's data
Given the service resolves tenant "acme-prod" and Pulse holds "process_cpu_utilization" for both "acme-prod" and "globex-prod"
When the operator requests query_range for "process_cpu_utilization"
Then the result contains only acme-prod's series
And no globex-prod points appear in any series

### Scenario: A request with no resolvable tenant is refused
Given the service has no configured tenant and the request carries no tenant signal
When the operator requests query_range for any metric
Then the service refuses to serve the request
And no metric data is returned

### Scenario: Tenant scoping uses the platform tenant identity
Given the write path persisted "queue_depth" under aegis tenant "acme-prod"
When the read path resolves tenant "acme-prod" and queries "queue_depth"
Then the same points written by the gateway are returned
And the tenant identity is the same aegis TenantId vocabulary used on write

## Acceptance Criteria
- [ ] Every query is scoped to exactly one resolved aegis TenantId.
- [ ] A query never returns another tenant's points (zero cross-tenant leak).
- [ ] No resolvable tenant -> request refused (fail-closed), no data returned.
- [ ] The tenant-resolution seam is swappable (config today; header/Bearer later) without changing the query path.

## Outcome KPIs
- **Who**: query service / platform owner
- **Does what**: scopes reads to one tenant and refuses when none resolves
- **By how much**: 0 cross-tenant leaks; 100% of no-tenant requests refused
- **Measured by**: tenant-isolation acceptance tests (two-tenant fixture; no-tenant fixture)
- **Baseline**: n/a (no read path today)

## Technical Notes (Optional)
- aegis::TenantId newtype (crates/aegis). Gateway resolves default tenant via KALEIDOSCOPE_DEFAULT_TENANT, fail-closed (crates/kaleidoscope-gateway/src/main.rs).
- Mechanism choice is RED CARD 1, owned by DESIGN.

---

## US-05: Hold the v0 scope boundary at the contract edge

### Elevator Pitch
- Before: without an explicit boundary, the service risks half-implementing logs/traces/full-PromQL and shipping confusing partial behaviour.
- After: a query that asks for out-of-scope behaviour (e.g. a range-vector selector, or a non-metric intent) hits `GET /api/v1/query_range?...` and sees HTTP 400 `{ "status":"error", "error":"unsupported query: ..." }`, never a partial or fabricated result.
- Decision enabled: the operator and the team can trust that what the endpoint accepts is exactly what it correctly serves; scope creep is caught at the edge.

### Problem
`query_range` returns a matrix, which is metrics-only by definition. Logs and traces
are separate Prism panels and endpoints. Full PromQL is a large language. Without a
testable boundary, a well-meaning change could start half-answering out-of-scope
queries, which is worse than refusing them.

### Who
- The team / future maintainer | needs the scope boundary expressed as executable rejections.
- On-call operator | needs honest refusals rather than partial answers.

### Solution
Express the v0 boundary as explicit, tested rejections: anything beyond a bare
metric-name selector over a time range returns status:error 400. This story is the
guard rail that makes the scope boundary in wave-decisions.md executable.

### Domain Examples
### 1: Happy Path - range-vector selector rejected
`query=http_requests_total[5m]` (a range vector, the building block of rate()) returns
HTTP 400 status:error naming range vectors as unsupported at v0.

### 2: Edge Case - aggregation rejected
`query=sum(process_cpu_utilization)` returns HTTP 400 status:error naming aggregations
as unsupported at v0.

### 3: Boundary - bare name with surrounding whitespace accepted
`query=  process_cpu_utilization  ` (leading/trailing whitespace) is trimmed and
accepted as the bare metric name; it is NOT treated as unsupported.

## UAT Scenarios (BDD)
### Scenario: A range-vector selector is rejected
Given the operator submits query_range with query "http_requests_total[5m]"
When the service parses the selector
Then the HTTP status is 400 with status "error"
And the message names range vectors as unsupported at v0

### Scenario: An aggregation is rejected
Given the operator submits query_range with query "sum(process_cpu_utilization)"
When the service parses the selector
Then the HTTP status is 400 with status "error"
And the message names aggregations as unsupported at v0

### Scenario: A bare metric name with surrounding whitespace is accepted
Given the operator submits query_range with query "  process_cpu_utilization  "
When the service parses the selector
Then the whitespace is trimmed and the query is accepted as a bare metric name
And the service queries Pulse for "process_cpu_utilization"

## Acceptance Criteria
- [ ] Range-vector selectors, aggregations, functions, and operators all return status:error 400.
- [ ] A bare metric name (after trimming surrounding whitespace) is accepted.
- [ ] The boundary is covered by executable tests, one per rejected form.
- [ ] No out-of-scope query ever returns a partial or fabricated matrix.

## Outcome KPIs
- **Who**: the team / on-call operator
- **Does what**: relies on an executable scope boundary instead of partial behaviour
- **By how much**: 100% of out-of-scope forms rejected; 0 partial answers
- **Measured by**: one acceptance test per rejected form
- **Baseline**: n/a

## Technical Notes (Optional)
- This story shares the parser with US-03; it is the scope-boundary half (US-03 is the operator-error half). Kept separate so the boundary is independently demonstrable.
