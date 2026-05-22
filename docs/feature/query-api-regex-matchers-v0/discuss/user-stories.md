<!-- markdownlint-disable MD024 -->

# User Stories: query-api-regex-matchers-v0

British English. No em dashes. Author: `nw-product-owner` (Luna).

This feature lands the regex label matchers `=~` and `!~` behind the SAME selector
parser and the SAME `keep_row` filter that shipped `=`/`!=` in
`query-api-label-matchers-v0` (commit 0171388). Pulse now identifies series by full
label set (`pulse-series-identity-v0`, commit 5ea579b), so `query` fans out and the
filter sees real per-series labels. Story IDs continue from the predecessor feature
(US-06..US-08); this feature adds US-09..US-11.

Today `selector::parse` rejects any `=~`/`!~` operator with an honest 400
(`regex_reason()`: "regex matchers (=~, !~) are not supported at v0; use = or !=").
This feature makes them work. The boundary that US-08 made executable becomes a
shipped behaviour.

## System Constraints

- The response envelope is UNCHANGED. Every success/empty path still satisfies
  Prism's `isPromSuccess` (status==='success' AND `Array.isArray(data.result)`) and
  every error path satisfies `isPromError` (status==='error' AND typeof error==='string').
  The regex feature changes only WHICH series the success arm carries, plus adds one
  new 400 arm (invalid regex), never the envelope shape.
- NO change to the public `query_api::router` signature. The behaviour rides the
  existing `/api/v1/query_range` HTTP handler, exercised via the `oneshot` pattern
  exactly as the slice-03 acceptance suite does.
- The metric name still selects the metric via `pulse.query(&tenant, &MetricName, range)`.
  Regex matchers FILTER the translated result by the DERIVED label set of each row
  (`metric.resource_attributes UNION point.attributes UNION {__name__: name}`;
  `crates/query-api/src/matrix.rs merge_labels`), point attributes winning over
  resource attributes, `__name__` authoritative. Identical to the `=`/`!=` slice.
- Regex matchers are FULLY ANCHORED at both ends, the Prometheus rule. A regex matcher
  `label=~"re"` keeps a series iff the label value matches `^(?:re)$`, i.e. the WHOLE
  value matches, not a substring. So `service.name=~"check"` does NOT match "checkout";
  `service.name=~"check.*"` does. `!~` is the exact negation of `=~`.
- Absent label is treated as the empty string, consistent with the `=`/`!=` slice. This
  produces a SECOND correctness matrix on top of the full-anchor one, and it must be
  enumerated as explicit acceptance criteria (see US-10).
- All matchers are still ANDed. Regex and exact matchers (`=`, `!=`) can mix freely in
  one selector.
- An invalid regex (a syntax error in the pattern) returns an honest 400
  ("invalid regex matcher"), never a silent pass and never a plausible wrong answer. A
  valid-but-never-matching regex yields the calm `result: []` at HTTP 200.
- Tenant scoping (US-04, shipped) is unchanged: every query is scoped to one resolved
  aegis TenantId, fail-closed. Matchers filter WITHIN the tenant's data only.
- status:error bodies must never echo the raw query or a forwarded header/credential
  value (DD6 redaction symmetry; ADR-0027 section 6). This now includes the invalid-regex
  reason: name the matcher as invalid without echoing the offending pattern text.
- The regex ENGINE is a DESIGN-wave concern. DISCUSS pins the SEMANTICS (full anchoring,
  the absent-label matrix, invalid-regex 400) and leaves the engine choice and the exact
  anchoring mechanism to the solution-architect. See `wave-decisions.md`.

---

## US-09: Filter a metric to the series whose label matches a pattern

### Elevator Pitch

- Before: an operator mid-incident can only name exact label values (`route="/api/orders"`,
  then `route="/api/payments"`, one at a time) or exclude them one by one. She cannot say
  "every route under `/api/`" in a single query, so she eyeballs a dozen exact filters or a
  noisy unfiltered plot at the worst possible moment.
- After: Priya hits `GET /api/v1/query_range?query=http_requests_total{route=~"/api/.*"}&start=1716200000&end=1716200060&step=15s` and sees `{ "status":"success", "data":{ "resultType":"matrix", "result":[ ...only the series whose route fully matches /api/.* ... ] } }`, which Prism plots as just the API routes.
- Decision enabled: Priya reads the API-route subset as a group, spots which route family is anomalous, and decides where to dig next, all from one pattern query.

### Problem

Priya Nandakumar is an on-call SRE for tenant "acme-prod" during a latency incident. The
`http_requests_total` metric carries a `route` point attribute with dozens of distinct
values: `/api/orders`, `/api/payments`, `/api/users`, `/health`, `/metrics`, `/static/app.js`.
She wants to compare the API routes against each other without the health-check and static
noise. Her muscle memory from Prometheus is `{route=~"/api/.*"}`, but query-api today
returns a 400 telling her regex is not supported. She is forced to issue one exact-match
query per route, or read a noisy plot, mid-incident.

### Who

- On-call SRE / operator | filtering a noisy metric to a FAMILY of series by a pattern
  rather than naming each exact value | wants the matching subset, server-side, in one query.
- Prism's HTTP query client | forwards the raw `name{route=~"..."}` query verbatim and needs
  a response its pinned `isPromSuccess` validator still accepts.

### Solution

Turn the rejected `=~` operator into a real matcher operator in the selector grammar.
The metric name selects the metric via `pulse.query`; each `=~` matcher then filters the
returned rows by their derived label set, keeping a row iff the label value FULLY matches
the regex (anchored at both ends, the Prometheus rule). Regex matchers AND with each other
and with `=`/`!=`. Kept rows group into the matrix exactly as today; the response envelope
is unchanged. An empty surviving set is the calm `result: []` success arm.

### Domain Examples

### 1: Happy Path - a prefix pattern keeps a route family

Tenant "acme-prod" has `http_requests_total` with `route` point attributes "/api/orders",
"/api/payments", "/health", and "/metrics". Priya queries
`query=http_requests_total{route=~"/api/.*"}`. The result contains the "/api/orders" and
"/api/payments" series; "/health" and "/metrics" are excluded.

### 2: Edge Case - full anchoring excludes a substring-only match

`http_requests_total` for "acme-prod" has `service.name` "checkout" and "checkout-canary".
Priya queries `query=http_requests_total{service.name=~"check"}`. NEITHER series is returned:
"checkout" does not FULLY match "check" (Prometheus anchors both ends). She rewrites to
`service.name=~"check.*"` and both series return. This is the full-anchor rule.

### 3: Boundary - a regex matcher composes with an exact matcher under AND

`http_requests_total` has checkout series with `route` "/api/orders" carrying `code="200"`
and `code="500"`. Priya queries
`query=http_requests_total{route=~"/api/.*", code="200"}`. Only the "/api/orders" series
carrying `code="200"` is returned; the `code="500"` one and any non-/api route are excluded
(both matchers must hold).

## UAT Scenarios (BDD)

### Scenario: A prefix pattern keeps the matching route family

Given tenant "acme-prod" has metric "http_requests_total" with route series "/api/orders", "/api/payments", "/health", and "/metrics"
When the operator requests query_range for 'http_requests_total{route=~"/api/.*"}' over a covering range
Then the response status is "success" and resultType is "matrix"
And the result contains only the "/api/orders" and "/api/payments" series
And the "/health" and "/metrics" series do not appear
And Prism's success validator accepts the response

### Scenario: A regex matcher is fully anchored, so a substring does not match

Given tenant "acme-prod" has metric "http_requests_total" with service.name "checkout" and "checkout-canary"
When the operator requests query_range for 'http_requests_total{service.name=~"check"}'
Then the result is empty, because neither value fully matches the unanchored-looking pattern "check"
And when the operator instead requests 'http_requests_total{service.name=~"check.*"}'
Then both the "checkout" and "checkout-canary" series are returned

### Scenario: A regex matcher composes with an equality matcher under AND

Given tenant "acme-prod" has metric "http_requests_total" with a route "/api/orders" series carrying code "200" and one carrying code "500"
When the operator requests query_range for 'http_requests_total{route=~"/api/.*", code="200"}'
Then the result contains only the "/api/orders" series carrying code "200"
And the code "500" series is excluded

### Scenario: A pattern that matches nothing returns the calm empty arm

Given tenant "acme-prod" has metric "http_requests_total" with no route matching "/admin/.*"
When the operator requests query_range for 'http_requests_total{route=~"/admin/.*"}'
Then the response status is "success" and resultType is "matrix"
And data.result is an empty array
And Prism renders the calm empty state, not an error banner

## Acceptance Criteria

- [ ] The selector parser accepts the `=~` operator: `name{ label=~"pattern" }` parses to a regex matcher rather than returning the slice-01 regex 400.
- [ ] A `=~` matcher keeps a row iff the label value in the derived label set FULLY matches the pattern (anchored at both ends, equivalent to `^(?:pattern)$`).
- [ ] A pattern naming a proper substring of a value (for example `=~"check"` against "checkout") does NOT keep that series; only a full match keeps it.
- [ ] A `=~` matcher ANDs with `=`, `!=`, and other regex matchers: a series is kept iff it satisfies every matcher.
- [ ] Filtering uses the same derived label set (`resource_attributes` U `point.attributes` U `{__name__:name}`) that `to_matrix` groups on.
- [ ] A `=~` matcher that matches no series returns the success+empty arm (`result:[]`) at HTTP 200, never an error.
- [ ] The response envelope still satisfies Prism's `isPromSuccess`.

## Outcome KPIs

- **Who**: on-call operator (Priya) and Prism's query client
- **Does what**: filters a noisy metric to the FAMILY of series whose label fully matches a pattern, in one server-side query, instead of issuing one exact filter per value
- **By how much**: a `=~` query returns EXACTLY the series whose label fully matches the anchored pattern (correctness, including the substring-does-not-match case); 100% of these shapes still round-trip through Prism's validator
- **Measured by**: acceptance tests asserting the kept/excluded series for prefix, substring-anchor, AND-composition, and empty cases; contract test through Prism's validator
- **Baseline**: 0% (today every `=~` returns a 400; no pattern filtering is possible)

## Technical Notes (Optional)

- Flips the `regex_reason()` arm in `crates/query-api/src/selector.rs read_operator`: `=~` now
  yields a regex `MatchOp` variant instead of `Err(regex_reason())`. The parser change is the
  smaller half; the match semantics are the correctness-critical half.
- The filter arm lands in `crates/query-api/src/matrix.rs matches`, alongside the existing
  `Equal`/`NotEqual` arms, applied to the same derived label set via `keep_row`.
- The regex engine and the exact anchoring mechanism are a DESIGN decision (flagged in
  `wave-decisions.md`). DISCUSS pins only the observable semantics.
- `gate-5-mutants-query-api` covers the new parse + filter logic via `--in-diff` (ADR-0042
  Verification). The full-anchor boundary (substring must not match) is a prime mutation target.

---

## US-10: Get the absent-label and empty-pattern regex cases exactly right

### Elevator Pitch

- Before: when an operator filters by a regex over a label that some series do not carry
  (`env=~".+"`, `env!~""`), a naive engine silently drops or keeps the wrong series, a
  plausible wrong answer mid-incident, which is worse than refusing.
- After: Priya hits `GET /api/v1/query_range?query=http_requests_total{env=~".+"}&start=1716200000&end=1716200060&step=15s` and sees `{ "status":"success", "data":{ "resultType":"matrix", "result":[ ...only series with a present, non-empty env... ] } }`, with the absent-env series correctly excluded.
- Decision enabled: Priya trusts that a pattern filter over a sometimes-absent label keeps exactly the right series, so she reasons about the filtered set without second-guessing whether absent-label series were silently mishandled.

### Problem

During the same incident, Priya filters `http_requests_total` by a regex over the `env`
label, which some series carry ("prod", "staging") and some do not carry at all. The subtle,
correctness-critical part is what `=~` and `!~` do when the label is ABSENT, and what an
empty pattern (`""`) and a non-empty-required pattern (`".+"`) mean. Prometheus treats an
absent label as the empty string for regex matchers, consistent with `=`/`!=`. If query-api
gets these wrong it silently drops or keeps series the operator expects, the worst kind of
mid-incident lie. This is a SECOND correctness matrix on top of the full-anchor one and each
arm needs its own pinned scenario.

### Who

- On-call SRE / operator | applying a regex filter over a label that is present on some
  series and absent on others | needs the absent-label and empty/`.+` pattern semantics to
  be exactly the Prometheus rule, not a naive engine guess.

### Solution

Treat an absent label as the empty string before the full-anchor regex test, the same
absent-as-empty convention the `=`/`!=` slice uses. This yields the four Prometheus arms:
`=~""` matches absent-or-empty; `=~".+"` requires a present non-empty value; `!~""` keeps
present non-empty; `!~".+"` keeps absent-or-empty. `!~` is the exact negation of `=~` on the
same absent-as-empty value. Each arm is filtered on the same derived label set as US-09 and
pinned by a dedicated scenario.

### Domain Examples

### 1: Happy Path - `=~""` keeps the absent-label series

`http_requests_total` for "acme-prod" has series A with `env="prod"` and series B with no
`env` label at all. Priya queries `query=http_requests_total{env=~""}`. Series B (absent
`env`, treated as empty, fully matches the empty pattern) is KEPT; series A (present,
non-empty) is EXCLUDED.

### 2: Edge Case - `=~".+"` requires a present non-empty value

Same data: series A `env="prod"`, series B with no `env`. Priya queries
`query=http_requests_total{env=~".+"}`. Series A is KEPT (a present non-empty value fully
matches `.+`); series B is EXCLUDED (the empty string does not match `.+`).

### 3: Boundary - `!~""` and `!~".+"` are the exact negations

Same data. `query=http_requests_total{env!~""}` keeps series A (present non-empty) and
excludes series B (absent treated as empty matches `""`, so `!~""` excludes it). The mirror,
`query=http_requests_total{env!~".+"}`, keeps series B (absent-or-empty) and excludes series A
(present non-empty matches `.+`, so `!~".+"` excludes it).

## UAT Scenarios (BDD)

### Scenario: An empty pattern keeps the series where the label is absent

Given tenant "acme-prod" has metric "http_requests_total" with one series carrying env "prod" and one series with no env label
When the operator requests query_range for 'http_requests_total{env=~""}'
Then the result contains only the series with no env label
And the series carrying env "prod" is excluded

### Scenario: A non-empty-required pattern keeps only present non-empty labels

Given tenant "acme-prod" has metric "http_requests_total" with one series carrying env "prod" and one series with no env label
When the operator requests query_range for 'http_requests_total{env=~".+"}'
Then the result contains only the series carrying env "prod"
And the series with no env label is excluded

### Scenario: A negated empty pattern keeps only present non-empty labels

Given tenant "acme-prod" has metric "http_requests_total" with one series carrying env "prod" and one series with no env label
When the operator requests query_range for 'http_requests_total{env!~""}'
Then the result contains only the series carrying env "prod"
And the series with no env label is excluded

### Scenario: A negated non-empty-required pattern keeps the absent-or-empty series

Given tenant "acme-prod" has metric "http_requests_total" with one series carrying env "prod" and one series with no env label
When the operator requests query_range for 'http_requests_total{env!~".+"}'
Then the result contains only the series with no env label
And the series carrying env "prod" is excluded

### Scenario: A negated pattern keeps a series where the label is absent

Given tenant "acme-prod" has metric "http_requests_total" with one series carrying env "prod" and one series with no env label
When the operator requests query_range for 'http_requests_total{env!~"prod"}'
Then the result contains the series with no env label
And the series carrying env "prod" is excluded

## Acceptance Criteria

The regex absent-label matrix, treating an absent label as the empty string and anchoring
the pattern at both ends:

- [ ] `label=~""` keeps a row iff the label is ABSENT or present-and-empty (the empty value fully matches the empty pattern).
- [ ] `label=~".+"` keeps a row iff the label is PRESENT and non-empty (the empty string does not fully match `.+`).
- [ ] `label!~""` keeps a row iff the label is PRESENT and non-empty (the exact negation of `=~""`).
- [ ] `label!~".+"` keeps a row iff the label is ABSENT or present-and-empty (the exact negation of `=~".+"`).
- [ ] `label!~"pattern"` (non-empty pattern) keeps a row iff the label is ABSENT, or present with a value that does NOT fully match the pattern.
- [ ] `!~` is the exact negation of `=~` evaluated on the same absent-as-empty value.
- [ ] Every arm above is covered by a dedicated executable scenario, so a mutation flipping an absent-label decision is caught.

## Outcome KPIs

- **Who**: on-call operator (Priya)
- **Does what**: applies a regex filter over a sometimes-absent label and gets exactly the right series, with correct absent-label, empty-pattern, and `.+` semantics
- **By how much**: each of the five absent-label/empty-pattern arms returns EXACTLY the expected series; 0 silently dropped or wrongly kept series across the matrix
- **Measured by**: one acceptance scenario per arm asserting the kept/excluded series; the matrix is also pinned by unit tests on the pure `matches` predicate
- **Baseline**: 0% (regex matchers do not exist before this feature; no absent-label regex behaviour to measure)

## Technical Notes (Optional)

- The absent-as-empty rule is the same one the `=`/`!=` slice collapses to in `matrix.rs matches`
  (`labels.get(name).unwrap_or("")`). The regex arms layer the full-anchor test on top of that
  same derived value, so the absent-label behaviour falls out of one rule rather than four
  special cases. This is the regression-prone heart of the feature.
- Kept as a separate story from US-09 because the absent-label matrix is an independently risky
  and independently demonstrable behaviour, exactly as US-07 split from US-06 in the `=`/`!=`
  slice. Shares the parser and filter with US-09.

---

## US-11: Reject an invalid regex pattern honestly

### Elevator Pitch

- Before: an operator who fat-fingers a regex (`route=~"/api/("` with an unclosed group)
  might get a 500, a silent pass that keeps every series, or a plausible wrong answer, none of
  which tells her the pattern was the problem.
- After: Priya hits `GET /api/v1/query_range?query=http_requests_total{route=~"/api/("}&start=1716200000&end=1716200060&step=15s` and sees HTTP 400 `{ "status":"error", "error":"invalid regex matcher" }`, which Prism renders verbatim above the chart with the query input keeping focus.
- Decision enabled: Priya immediately understands the regex itself was malformed (not the metric, not the range), fixes the pattern, and re-runs, instead of being misled by a wrong or empty result.

### Problem

Priya, under incident pressure, mistypes a regex: an unclosed group `/api/(`, a dangling
quantifier `*abc`, or another syntax error. The pattern is syntactically a valid matcher
(quoted value, `=~` operator) so it parses as a matcher, but the regex inside is invalid. If
query-api passed it to the engine and let a panic become a 500, or silently treated an
uncompilable pattern as matching everything (or nothing), Priya would be misled at the worst
moment. She needs an honest, specific 400 that names the regex as invalid, exactly as the
`=`/`!=` slice rejects a malformed matcher. The reason must not echo the offending pattern,
preserving the DD6 redaction posture.

### Who

- On-call operator (PromQL-literate) | submitting a syntactically well-formed matcher whose
  regex pattern is invalid | needs an honest, specific rejection she can act on, not a 500, a
  silent wrong answer, or a misleading empty result.

### Solution

When a `=~` or `!~` matcher's pattern fails to compile, return HTTP 400
`{status:error, error:"invalid regex matcher"}` that Prism's `isPromError` accepts. The
message names the regex as invalid and never echoes the raw query or a forwarded header value
(DD6). A pattern that compiles but matches nothing is NOT an error: it is the calm
`result: []` success arm at HTTP 200 (covered by US-09). The distinction is sharp: invalid
syntax is a 400; valid-but-never-matching is a 200 empty.

### Domain Examples

### 1: Happy Path - an unclosed group is rejected

Priya queries `query=http_requests_total{route=~"/api/("` (closing brace present, but the
regex `/api/(` has an unclosed group). The service returns HTTP 400
`{status:error, error:"invalid regex matcher"}`. The query is NOT silently answered as
matching everything or nothing.

### 2: Edge Case - a dangling quantifier in a negative matcher is rejected

Priya queries `query=http_requests_total{service.name!~"*abc"}` (a quantifier with nothing to
repeat). Returns HTTP 400 status:error naming the regex as invalid. The `!~` operator is
parsed fine; the pattern is what fails.

### 3: Boundary - a valid-but-never-matching pattern is NOT an error

Priya queries `query=http_requests_total{route=~"/admin/.*"}` against data with no /admin
routes. This is a VALID regex that simply matches nothing, so the service returns HTTP 200
`{status:success, data:{result:[]}}`, the calm empty arm, NOT a 400. Invalid syntax and
no-match are different outcomes.

## UAT Scenarios (BDD)

### Scenario: An invalid regex pattern is rejected with a readable reason

Given the operator submits query_range with query 'http_requests_total{route=~"/api/("' whose regex has an unclosed group
When the service compiles the regex matcher
Then the HTTP status is 400
And the response is {status:"error", error:<a message naming the regex matcher as invalid>}
And Prism's error validator accepts the response and shows the verbatim text

### Scenario: An invalid negative regex pattern is rejected

Given the operator submits query_range with query 'http_requests_total{service.name!~"*abc"}' whose regex has a dangling quantifier
When the service compiles the regex matcher
Then the HTTP status is 400 with status "error"
And the message names the regex matcher as invalid

### Scenario: A valid pattern that matches nothing is the calm empty success, not an error

Given tenant "acme-prod" has metric "http_requests_total" with no route matching "/admin/.*"
When the operator requests query_range for 'http_requests_total{route=~"/admin/.*"}'
Then the HTTP status is 200 with status "success"
And data.result is an empty array
And the response is NOT a 400 error

### Scenario: An invalid-regex rejection never leaks a forwarded header value

Given the operator's request carries a forwarded Authorization header "Bearer SECRET"
When the service returns a status:error response for an invalid regex matcher
Then the error text does not contain "SECRET" or the header value
And the error text does not echo the raw query or the offending pattern

## Acceptance Criteria

- [ ] A `=~` or `!~` matcher whose pattern fails to compile returns HTTP 400 status:error naming the regex matcher as invalid.
- [ ] An invalid regex never becomes a 500, a panic, or a silent "match everything" / "match nothing" answer.
- [ ] A valid regex that happens to match no series returns the calm `result:[]` success arm at HTTP 200, never a 400.
- [ ] The 400 body satisfies Prism's `isPromError`.
- [ ] The invalid-regex error message never contains the raw query, the offending pattern, or a forwarded header/credential value (DD6).
- [ ] Each rejected form (unclosed group, dangling quantifier) and the never-matching-200 distinction is covered by an executable test.

## Outcome KPIs

- **Who**: on-call operator (Priya) and the team
- **Does what**: receives an honest, readable rejection of an invalid regex instead of a 500, a silent wrong answer, or a misleading empty result
- **By how much**: 100% of invalid regex patterns return status:error 400; 0 panics, 0 silent mis-answers; 100% of valid-but-never-matching patterns stay a calm 200 empty
- **Measured by**: one acceptance test per invalid form, a test asserting valid-but-empty stays 200, and a redaction test for the header/raw-query/pattern leak
- **Baseline**: n/a (regex matchers did not parse before this feature; today every `=~`/`!~` is a blanket 400)

## Technical Notes (Optional)

- Extends the slice-01 honest-400 discipline (US-08) from "regex is unsupported" to "this
  specific regex is invalid". The redaction posture mirrors ADR-0027 section 6 and the existing
  `the_reason_never_echoes_the_raw_query` test in `selector.rs`.
- WHERE compilation happens (at parse time in `selector.rs`, or at filter-build time before
  `keep_row`) and WHICH regex engine compiles it are DESIGN decisions; DISCUSS pins only that an
  invalid pattern is a 400 and a never-matching valid pattern is a 200 empty.
- The sharp invalid-vs-never-matching distinction is the prime mutation target for this story.
