<!-- markdownlint-disable MD024 -->

# User Stories: query-api-label-matchers-v0

British English. No em dashes. The response shape is a PINNED external contract
(`apps/prism/src/lib/promql/queryRange.ts` + ADR-0027); these stories extend the
`query-range-api-v0` provider against that same contract. Story IDs continue from the
predecessor feature (US-01..US-05); this feature adds US-06..US-08.

## System Constraints

- The response envelope is UNCHANGED by this feature: every success/empty path still
  satisfies Prism's `isPromSuccess` (status==='success' AND `Array.isArray(data.result)`)
  and every error path satisfies `isPromError` (status==='error' AND
  typeof error==='string'). The matcher feature changes only WHICH series the success arm
  carries, never the envelope.
- `query` arrives as a RAW PromQL string and is forwarded verbatim by Prism (confirmed in
  `queryRange.ts buildUrl`: `URLSearchParams({ query: request.q, ... })`). The backend owns
  all parsing of the `{...}` matcher section.
- The selector grammar extends the existing bare-name production
  (`[a-zA-Z_:][a-zA-Z0-9_:]*`, `crates/query-api/src/selector.rs`) to
  `name{ matcher_list }`, where `matcher_list` is zero or more comma-separated matchers
  `label_name OP "value"` with `OP` in `{=, !=}`. The bare-name-only form remains valid.
- The metric name still selects the metric via `pulse.query(&tenant, &MetricName, range)`.
  The OTHER matchers FILTER the translated result by the DERIVED label set of each row:
  `metric.resource_attributes UNION point.attributes UNION {__name__: name}`
  (`crates/query-api/src/matrix.rs merge_labels`), point attributes winning over resource
  attributes, `__name__` authoritative.
- Matcher semantics (Prometheus), applied to the derived label set:
  - `label="value"`: keep iff label PRESENT and equal. If `value` is `""`, `label=""` also
    keeps a series where the label is ABSENT (or present and empty).
  - `label!="value"`: keep iff label ABSENT, OR present with a different value. `label!=""`
    keeps only series where the label is PRESENT and non-empty.
  - Multiple matchers are ANDed: keep a series iff it satisfies EVERY matcher.
- OUT of scope (return honest 400, never a plausible wrong answer; the ADR-0042 discipline):
  regex matchers `=~` `!~`, functions, aggregations, operators, the instant
  `/api/v1/query` endpoint, range vectors. Unchanged unsupported forms keep their existing
  400 behaviour.
- Tenant scoping (US-04, shipped) is unchanged: every query is scoped to one resolved
  aegis TenantId, fail-closed. Matchers filter WITHIN the tenant's data only.
- status:error bodies must never echo the raw query or a forwarded header/credential value
  (DD6 redaction symmetry; ADR-0027 section 6).

---

## US-06: Narrow a metric to the series matching an equality label matcher

### Elevator Pitch
- Before: an operator querying `http_requests_total` during an incident gets a dozen
  overlapping series for every service and cannot read the one she cares about.
- After: Sara hits `GET /api/v1/query_range?query=http_requests_total{service.name="checkout"}&start=...&end=...&step=15s` and sees `{ "status":"success", "data":{ "resultType":"matrix", "result":[ { "metric":{"__name__":"http_requests_total","service.name":"checkout"}, "values":[...] } ] } }`, which Prism plots as a single clean line.
- Decision enabled: Sara reads the checkout series' trend in isolation and decides whether checkout is the source of the incident.

### Problem
Sara Okafor is an on-call SRE for the "checkout" service, tenant "acme-prod". Mid-incident
she queries `http_requests_total` and Prism plots a dozen overlapping lines across every
service in the platform. She knows the metric; she needs only checkout. Her muscle memory
from Prometheus is to add `{service.name="checkout"}`, but slice 01 rejects any `{` with a
400, so she is stuck eyeballing a noisy plot at the worst possible moment.

### Who
- On-call SRE / operator | filtering a known noisy metric to one service by an exact label
  value | wants to read the matching series fast, server-side, during an incident.
- Prism's HTTP query client | forwards the raw `name{...}` query and needs a response its
  pinned validator still accepts.

### Solution
Extend the selector parser to accept `name{ label="value", ... }` for the equality
operator `=`. The metric name selects the metric via `pulse.query`; each `=` matcher then
filters the returned rows by their derived label set, keeping only rows where the label is
present and equal (or, when the matcher value is `""`, where the label is absent or empty).
Multiple matchers are ANDed. The kept rows are grouped into the matrix exactly as today;
the response envelope is unchanged.

### Domain Examples
### 1: Happy Path - single equality matcher narrows to one service
Tenant "acme-prod" has `http_requests_total` with series for `service.name` "checkout",
"cart", and "search". Sara queries
`query=http_requests_total{service.name="checkout"}`. The service returns ONE matrix
series, `metric:{"__name__":"http_requests_total","service.name":"checkout"}`, with its
ascending `values`. Cart and search are absent.

### 2: Edge Case - two ANDed matchers narrow further
`http_requests_total` for "acme-prod" has checkout series with `code="200"` and
`code="500"`. Sara queries
`query=http_requests_total{service.name="checkout", code="200"}`. Only the checkout series
carrying `code="200"` is returned; the `code="500"` checkout series and all other services
are excluded (both matchers must hold).

### 3: Boundary - empty-string value matches an absent label
`http_requests_total` has series A with `code="200"` and series B with no `code` label at
all. Sara queries `query=http_requests_total{code=""}`. Series B (absent `code`) is kept;
series A (`code` present and non-empty) is excluded. This is the Prometheus empty-string
rule: `label=""` matches the ABSENT label.

## UAT Scenarios (BDD)
### Scenario: A single equality matcher narrows to the matching series
Given tenant "acme-prod" has metric "http_requests_total" with series for service.name "checkout", "cart", and "search"
When the operator requests query_range for 'http_requests_total{service.name="checkout"}' over a covering range
Then the response status is "success" and resultType is "matrix"
And the result contains only the checkout series
And no cart or search series appears
And Prism's success validator accepts the response

### Scenario: Multiple matchers are ANDed
Given tenant "acme-prod" has metric "http_requests_total" with checkout series labelled code "200" and code "500"
When the operator requests query_range for 'http_requests_total{service.name="checkout", code="200"}'
Then the result contains only the checkout series carrying code "200"
And the checkout series carrying code "500" is excluded

### Scenario: An equality matcher with an empty-string value matches an absent label
Given tenant "acme-prod" has metric "http_requests_total" with one series carrying code "200" and one series with no code label
When the operator requests query_range for 'http_requests_total{code=""}'
Then the result contains only the series with no code label
And the series carrying code "200" is excluded

### Scenario: A bare metric name with no brace section still works
Given tenant "acme-prod" has metric "http_requests_total" with several series
When the operator requests query_range for 'http_requests_total' with no matcher section
Then all series under the metric name are returned
And the slice-01 behaviour is unchanged

### Scenario: An equality matcher that matches nothing returns the calm empty arm
Given tenant "acme-prod" has no series under "http_requests_total" where service.name equals "chekout"
When the operator requests query_range for 'http_requests_total{service.name="chekout"}'
Then the response status is "success" and resultType is "matrix"
And data.result is an empty array
And Prism renders the calm empty state, not an error banner

## Acceptance Criteria
- [ ] The selector parser accepts `name{ label="value" }` and `name{ l1="v1", l2="v2" }` for the equality operator `=`.
- [ ] An `=` matcher keeps a row iff the label is present in the derived label set and equals the value.
- [ ] An `=` matcher whose value is the empty string `""` keeps a row where the label is absent or empty.
- [ ] Multiple matchers are ANDed: a series is kept iff it satisfies every matcher.
- [ ] Filtering is applied to the same derived label set (`resource_attributes` U `point.attributes` U `{__name__:name}`) that `to_matrix` groups on.
- [ ] A bare metric name with no brace section returns all series (slice-01 behaviour unchanged).
- [ ] A matcher matching no series returns the success+empty arm (`result:[]`), never an error.
- [ ] The response envelope still satisfies Prism's `isPromSuccess`.

## Outcome KPIs
- **Who**: on-call operator (Sara) and Prism's query client
- **Does what**: narrows a noisy metric to the series matching an equality matcher, server-side
- **By how much**: a labelled query returns EXACTLY the series satisfying all `=` matchers (correctness, including the empty-string absent-label case); 100% of these shapes still round-trip through Prism's validator
- **Measured by**: acceptance tests asserting the kept/excluded series per matcher; contract test through Prism's validators
- **Baseline**: 0% (slice 01 rejects any `{` with a 400; no filtering possible)

## Technical Notes (Optional)
- Parser change in `crates/query-api/src/selector.rs`: `parse` returns the metric name plus a parsed matcher list (the public return type is a DESIGN decision; the name must still drive `pulse.query`).
- Filter applied to `Vec<(Metric, MetricPoint)>` BEFORE `matrix::to_matrix`, on the derived label set computed identically to `merge_labels`.
- `gate-5-mutants-query-api` covers the new parse + filter logic via `--in-diff` (ADR-0042 Verification). The empty-string and ANDed cases are exactly the kind of boundary mutation testing targets.

---

## US-07: Exclude noisy series with an inequality label matcher

### Elevator Pitch
- Before: an operator who wants "every service EXCEPT the noisy batch one" has no way to
  exclude a series; she can only name a single value with `=`, or eyeball the noise.
- After: Sara hits `GET /api/v1/query_range?query=http_requests_total{service.name!="batch"}&start=...&end=...&step=15s` and sees `{ "status":"success", "data":{ "resultType":"matrix", "result":[ ...every series except batch... ] } }`, which Prism plots without the batch line drowning the others.
- Decision enabled: Sara compares the remaining services against each other without the noisy batch series distorting the chart scale, and decides which service is anomalous.

### Problem
During the same incident, Sara wants the opposite of US-06: keep everything except one
noisy series whose huge values flatten the chart for every other service. Prometheus lets
her write `{service.name!="batch"}`. The subtle, correctness-critical part is what `!=`
does to series where the label is ABSENT, and what `!=""` means; if the service gets these
wrong it silently drops or keeps the wrong series, which is worse than refusing.

### Who
- On-call SRE / operator | excluding a known noisy or irrelevant series so the rest are
  readable | needs the absent-label and empty-string `!=` semantics to be exactly right.

### Solution
Extend the matcher grammar with the inequality operator `!=`. A `!=` matcher keeps a row
iff the label is ABSENT, or present with a value different from the matcher value. The
special case `label!=""` keeps only rows where the label is present and non-empty. `!=`
matchers compose with `=` matchers under the same AND rule. Filtering is on the same
derived label set as US-06.

### Domain Examples
### 1: Happy Path - inequality excludes the named series
Tenant "acme-prod" has `http_requests_total` with `service.name` "checkout", "cart", and
"batch". Sara queries `query=http_requests_total{service.name!="batch"}`. The result
contains the checkout and cart series; the batch series is excluded.

### 2: Edge Case - inequality keeps a series where the label is ABSENT
`http_requests_total` has series A with `code="500"` and series B with no `code` label.
Sara queries `query=http_requests_total{code!="500"}`. Series B is KEPT (absent label
satisfies `!=`) and series A is excluded (present and equal to "500"). This is the
correctness-critical absent-label rule for `!=`.

### 3: Boundary - `!=""` keeps only present, non-empty labels
`http_requests_total` has series A with `code="200"` and series B with no `code` label.
Sara queries `query=http_requests_total{code!=""}`. Series A is KEPT (present and
non-empty); series B is EXCLUDED (absent). This is the mirror of the US-06 empty-string
rule.

## UAT Scenarios (BDD)
### Scenario: An inequality matcher excludes the named series
Given tenant "acme-prod" has metric "http_requests_total" with series for service.name "checkout", "cart", and "batch"
When the operator requests query_range for 'http_requests_total{service.name!="batch"}'
Then the result contains the checkout and cart series
And the batch series is excluded
And the response status is "success"

### Scenario: An inequality matcher keeps a series where the label is absent
Given tenant "acme-prod" has metric "http_requests_total" with one series carrying code "500" and one series with no code label
When the operator requests query_range for 'http_requests_total{code!="500"}'
Then the result contains the series with no code label
And the series carrying code "500" is excluded

### Scenario: An inequality against the empty string keeps only present non-empty labels
Given tenant "acme-prod" has metric "http_requests_total" with one series carrying code "200" and one series with no code label
When the operator requests query_range for 'http_requests_total{code!=""}'
Then the result contains only the series carrying code "200"
And the series with no code label is excluded

### Scenario: An equality and an inequality matcher compose under AND
Given tenant "acme-prod" has metric "http_requests_total" with checkout series labelled code "200" and code "500"
When the operator requests query_range for 'http_requests_total{service.name="checkout", code!="500"}'
Then the result contains only the checkout series carrying code "200"
And the checkout series carrying code "500" is excluded

## Acceptance Criteria
- [ ] The matcher grammar accepts the inequality operator `!=` alongside `=`.
- [ ] A `!=` matcher keeps a row iff the label is absent, or present with a value different from the matcher value.
- [ ] A `!=""` matcher keeps only rows where the label is present and non-empty.
- [ ] `=` and `!=` matchers compose under the AND rule.
- [ ] Filtering uses the same derived label set as US-06.
- [ ] The response envelope still satisfies Prism's `isPromSuccess`; an all-excluded result is the calm empty arm.

## Outcome KPIs
- **Who**: on-call operator (Sara)
- **Does what**: excludes a known noisy or irrelevant series so the remaining series are readable, with correct absent-label and empty-string semantics
- **By how much**: a `!=` query returns EXACTLY the series satisfying the inequality, including the absent-label and `!=""` cases; 0 silently dropped or wrongly kept series
- **Measured by**: acceptance tests asserting the absent-label keep, the `!=""` exclude, and the AND composition
- **Baseline**: 0% (no inequality filtering exists)

## Technical Notes (Optional)
- The absent-label and empty-string arms are the regression-prone heart of the feature; each has a dedicated scenario so a mutation that flips an absent-label decision is caught.
- Shares the parser and filter predicate with US-06; kept a separate story because the inequality semantics are an independently demonstrable and independently risky behaviour.

---

## US-08: Reject a regex or malformed matcher honestly

### Elevator Pitch
- Before: an operator who pastes a regex matcher (`=~`) or a malformed brace section might
  get a silently mis-parsed wrong filter, a plausible wrong answer during an incident.
- After: Sara hits `GET /api/v1/query_range?query=http_requests_total{service.name=~"check.*"}&...` and sees HTTP 400 `{ "status":"error", "error":"unsupported query: regex matchers (=~, !~) are not supported at v0; use = or !=" }`, which Prism renders verbatim above the chart with the query input keeping focus.
- Decision enabled: Sara immediately understands the matcher form was rejected and why, and rewrites it using `=` or `!=`.

### Problem
Sara, used to full Prometheus, pastes `{service.name=~"check.*"}` (a regex matcher) or
fat-fingers an unterminated brace. The v0 slice supports only `=` and `!=`. If the parser
guessed, dropped the bad matcher, or 500'd, Sara would be misled at the worst possible
moment, or worse, shown a plausible-looking but wrong set of series. She needs an honest,
readable rejection, exactly as slice 01 rejects `rate()`.

### Who
- On-call operator (PromQL-literate) | pasting a richer matcher form than v0 supports, or a
  typo | needs an honest, specific rejection she can act on, not a silent wrong answer.

### Solution
Parse the `{...}` matcher section strictly. Any matcher using an operator other than `=` or
`!=` (regex `=~`, `!~`, or any other), any malformed matcher (unterminated brace, missing
quotes around the value, empty label name, trailing junk), returns HTTP 400
`{status:error, error:"<clear reason>"}` that Prism's `isPromError` accepts. The message
names what is unsupported and what is accepted (`=` / `!=`). It never echoes the raw query
or a forwarded header value (DD6).

### Domain Examples
### 1: Happy Path - regex matcher rejected
Sara queries `query=http_requests_total{service.name=~"check.*"}`. The service returns
HTTP 400 `{status:error, error:"unsupported query: regex matchers (=~, !~) are not
supported at v0; use = or !="}`.

### 2: Edge Case - unterminated brace rejected
Sara queries `query=http_requests_total{service.name="checkout"` (no closing brace).
Returns HTTP 400 `{status:error, error:"malformed query: the label matcher section is not
closed"}` (or equivalent reason naming the malformed section). The query is NOT
silently treated as a bare name.

### 3: Boundary - matcher value missing quotes rejected
Sara queries `query=http_requests_total{service.name=checkout}` (no quotes around the
value). Returns HTTP 400 status:error naming the missing-quotes problem; it is not parsed
as an unquoted value.

## UAT Scenarios (BDD)
### Scenario: A regex matcher is rejected with a readable reason
Given the operator submits query_range with query 'http_requests_total{service.name=~"check.*"}'
When the service parses the selector
Then the HTTP status is 400
And the response is {status:"error", error:<a message naming regex matchers as unsupported and = / != as accepted>}
And Prism's error validator accepts the response and shows the verbatim text

### Scenario: An unterminated matcher section is rejected, not treated as a bare name
Given the operator submits query_range with query 'http_requests_total{service.name="checkout"' with no closing brace
When the service parses the selector
Then the HTTP status is 400 with status "error"
And the service does NOT silently query the bare metric name
And the message names the matcher section as malformed

### Scenario: A matcher value without quotes is rejected
Given the operator submits query_range with query 'http_requests_total{service.name=checkout}'
When the service parses the selector
Then the HTTP status is 400 with status "error"
And the message names the malformed matcher

### Scenario: A rejection never leaks a forwarded header value
Given the operator's request carries a forwarded Authorization header "Bearer SECRET"
When the service returns a status:error response for an unsupported matcher
Then the error text does not contain "SECRET" or the header value
And the error text does not echo the raw query

## Acceptance Criteria
- [ ] A matcher using any operator other than `=` or `!=` (regex `=~`, `!~`, or other) returns HTTP 400 status:error naming regex/that form as unsupported and `=` / `!=` as accepted.
- [ ] A malformed matcher section (unterminated brace, missing value quotes, empty label name, trailing junk) returns HTTP 400 status:error.
- [ ] A malformed brace section is never silently parsed as a bare metric name or a partial filter.
- [ ] The 400 body satisfies Prism's `isPromError`.
- [ ] The error message never contains the raw query or a forwarded header/credential value.
- [ ] Every rejected form is covered by an executable test (one per form).

## Outcome KPIs
- **Who**: on-call operator and the team
- **Does what**: receives an honest, readable rejection of an unsupported or malformed matcher instead of a silent wrong answer
- **By how much**: 100% of regex/malformed matcher forms return status:error 400; 0 silent mis-answers or partial filters
- **Measured by**: one acceptance test per rejected form; a redaction test for the header/raw-query leak
- **Baseline**: n/a (matchers did not exist before this feature)

## Technical Notes (Optional)
- Extends the slice-01 honest-400 discipline (US-03/US-05) to the matcher grammar. Redaction posture mirrors ADR-0027 section 6 and the existing `the_reason_never_echoes_the_raw_query` test in `selector.rs`.
- Regex support (`=~`, `!~`) is deferred to slice 02b (briefed under `slices/`); this story rejects it explicitly so the boundary is executable.
