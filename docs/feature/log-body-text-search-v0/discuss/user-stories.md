<!-- markdownlint-disable MD024 -->

# User Stories: log-body-text-search-v0

Slice 01, thin. The log read endpoint `GET /api/v1/logs?start=&end=`, which
today already accepts the optional `min_severity` floor from
log-query-severity-filter-v0, grows ONE further optional request parameter:
`body_contains=<string>`. The parameter restricts the returned `LogRecord`s
to those whose `body` field (the OTLP-shaped `String` body, NOT
`severity_text`, NOT any attribute, NOT a resource attribute) contains the
supplied substring. Default behaviour (parameter absent) is unchanged: every
in-window record is returned (composed with `min_severity` if present).

This is a brownfield carpaccio slice on top of an existing endpoint. The
walking skeleton is implicit in the four sibling slices that already shipped
on `/api/v1/logs` (read endpoint exists, durable store exists, tenant seam
exists, caps exist); no greenfield skeleton is rebuilt.

This is also the FIRST consumer of `query-http-common` (ADR-0054, M-5) born
AFTER the extraction. The slice deliberately exercises the shared crate's
public surface: `MAX_RESULT_ROWS`, `MAX_WINDOW_SECONDS`, `error_response`,
`resolve_tenant_or_refuse`, `parse_time_range`, and the four `REASON_*`
constants are consumed via `query-http-common`; this slice introduces ZERO
new copies of any of them. If `query-http-common` has API-surface gaps for a
brand-new parse-and-wire arm, this slice surfaces them before three more
pillars do.

## System Constraints (cross-cutting)

- The existing `MAX_WINDOW_SECONDS = 86_400` and `MAX_RESULT_ROWS = 100_000`
  caps from ADR-0050 are PRESERVED unchanged. The new filter MUST NOT remove,
  reorder, or weaken either cap. Both constants are consumed from
  `query_http_common::MAX_WINDOW_SECONDS` and
  `query_http_common::MAX_RESULT_ROWS` (ADR-0054); the slice MUST NOT
  re-declare either local to `log-query-api`.
- The error envelope on rejected input is the existing
  `{"status":"error","error":"<reason>"}` shape, emitted via
  `query_http_common::error_response` (ADR-0054 / ADR-0047 Decision 1). No
  new envelope. No new status code. The slice MUST NOT re-implement the
  envelope locally.
- The fail-closed tenancy seam goes through
  `query_http_common::resolve_tenant_or_refuse` (ADR-0054). The slice MUST
  NOT re-implement tenant resolution locally.
- The error text MUST NOT echo the raw parameter value (ADR-0047 redaction
  posture; symmetric with ADR-0050 Decision 7 and ADR-0052 Decision 1). The
  reason text on the empty-string 400 is a static literal; the raw
  `body_contains` value NEVER appears in the response body.
- The bare JSON array success shape (ADR-0047 Decision 1) is preserved. The
  filter changes WHICH records appear, not the shape of the response. The
  empty arm is `[]`, HTTP 200, NEVER 404.
- The `LogRecord.body` field is the OTLP-shaped `String` body
  (`crates/lumen/src/record.rs:54`). It is the ONLY field the substring is
  matched against; `severity_text`, `attributes`, `resource_attributes`, and
  trace-context fields are out of scope for this slice.
- The half-open `[start, end)` window from ADR-0047 § 3 is preserved
  unchanged.
- The `min_severity` parameter from log-query-severity-filter-v0 is
  PRESERVED unchanged. When both parameters are present, both filters apply
  (conjunctive `AND` per `Predicate::matches`); the order of application is
  semantically irrelevant because the composition is `AND`.
- The slice composes with the existing `query_with(&tenant, range,
  &predicate)` seam on `lumen::LogStore` (`crates/lumen/src/store.rs:89`).
  Whether the lumen `Predicate` needs to grow a `body_contains` builder is a
  flag to DESIGN (FLAG 3 below); the user-visible behaviour does not change
  with the answer.

## OUT of scope (DECLARED and DEFERRED)

The following are EXPLICITLY out for slice 01 and named so DESIGN does not
re-discover them as gaps:

- Regex matching on `body`. The slice ships SUBSTRING matching only. Regex
  is a separate future feature (`log-body-regex-search-vN`) with its own
  performance budget and ReDoS posture.
- Case-insensitive matching. The slice ships CASE-SENSITIVE matching only
  (`KAFKA` does NOT match `kafka`). FLAG 2 below names a future
  case-folding parameter as a separate slice.
- Matching across multiple fields (e.g. `body OR attributes`). The slice
  matches `body` ONLY.
- Matching on `severity_text`, `attributes`, or `resource_attributes`. Each
  is a separate slice if and when it earns a third call site.
- Multiple substrings (e.g. `body_contains=foo,bar` or repeated query
  parameters). The slice accepts ONE substring per request. A repeated
  parameter is a future feature; the slice's parser MAY accept the last
  occurrence (axum `serde_qs` default behaviour), but the acceptance suite
  pins only the single-substring contract.
- Unicode normalisation (NFC vs NFD). The slice compares Rust `String`
  bytes; no `unicode-normalization` dependency. A consumer that needs
  normalised matching pre-normalises both sides client-side.
- Configurable maximum substring length. The slice accepts any non-empty
  substring URL-encodable within the standard HTTP request-line / header
  limits the axum stack already enforces. A bounded `MAX_BODY_CONTAINS_LEN`
  is a follow-up if a real consumer demands it; the slice does NOT
  pre-emptively add one.
- A new ADR-0055 written by DISCUSS. ADR drafting belongs to DESIGN
  (Morgan); DISCUSS surfaces the recommendation in FLAG 6 below.

## US-01 Walking skeleton: a known substring narrows the response to matching records

### Elevator Pitch

- **Before**: Sara Mendez, on-call SRE for tenant `acme-prod`, has the
  string `kafka timeout` in hand from a paging alert. She GETs
  `/api/v1/logs?start=1716200000&end=1716200060` for the window around the
  alert and the response carries every record in the window: the two ERROR
  records carrying `kafka timeout`, the 198 unrelated INFO/WARN records,
  and the 40 ERROR records about an unrelated checkout-timeout incident
  that fired in the same minute. She downloads everything and pipes
  through `jq 'map(select(.body | contains("kafka timeout")))'`, which
  costs seconds she does not have and bandwidth she has paid for.
- **After**: Sara runs `curl
  'http://logs.kaleidoscope.acme.internal/api/v1/logs?start=1716200000&end=1716200060&body_contains=kafka%20timeout'
  -H 'X-Tenant: acme-prod'` and the HTTP 200 body is a bare JSON array
  carrying ONLY the two records whose `body` contains `kafka timeout`. The
  198 unrelated records and the 40 unrelated ERROR records are excluded by
  the platform before serialisation. The shape of each record is identical
  to today's response; only the row set is narrower.
- **Decision enabled**: Sara decides which records to inspect next (paging
  the two matching ERRORs by `observed_time_unix_nano`, picking the
  earliest to align with the incident start), without spending attention
  on unrelated noise.

### Problem

Sara Mendez is the on-call SRE for tenant `acme-prod`. Mid-incident she has
a specific error string in hand (paging alert text, runbook step, customer
report). Today she queries
`GET /api/v1/logs?start=1716200000&end=1716200060` and the window holds 240
records: 198 unrelated INFO/WARN, 40 unrelated ERROR (a separate
checkout-timeout incident), and 2 ERROR records whose `body` field is
`"kafka timeout connecting to broker-3"` and `"kafka timeout after 30s on
topic orders"`. She has to find the 2 matching records by hand. Today she
pipes the response through `jq` after downloading all 240 records. The
server-side filter would deliver the same 2 records without the 238 wasted.
The filter is the natural sibling of `min_severity` (log-query-severity-filter-v0):
same endpoint, same envelope, same redaction, same cap interaction; only
the origin of the predicate changes (severity number to substring on the
body string).

### Who

- **Sara Mendez** | SRE on `acme-prod`, mid-incident, terminal + curl + jq
  | Triage urgency: needs to isolate records carrying a specific error
  string within seconds.
- **Marcus Webb** | platform engineer building an automated incident
  classifier that polls `body_contains=<error-signature>` every 60 seconds
  to count occurrences of a known failure mode | Throughput motive:
  payload size and per-poll latency dominate the request budget.
- **Priya Raman** | support engineer triaging a customer ticket that
  quotes a specific error message | Correctness motive: she needs to
  confirm the exact string appeared in the customer's tenant's logs in the
  reported time window.

### Solution

`GET /api/v1/logs` accepts an optional query-string parameter
`body_contains=<string>`. The handler:

1. Resolves the tenant via `query_http_common::resolve_tenant_or_refuse`
   (UNCHANGED).
2. Parses the window via `query_http_common::parse_time_range`
   (UNCHANGED).
3. Enforces the window cap via `query_http_common::MAX_WINDOW_SECONDS` and
   `query_http_common::REASON_WINDOW_TOO_LARGE` (UNCHANGED).
4. Parses `min_severity` if present (UNCHANGED from
   log-query-severity-filter-v0).
5. Parses `body_contains` if present (NEW): an empty value (`?body_contains=`)
   is rejected as a 400 via `query_http_common::error_response` with the
   literal reason `invalid body_contains`. A non-empty value is captured as a
   `String` (URL-decoded by axum/serde).
6. Builds a `lumen::Predicate` that composes the present filters: no filter,
   `min_severity` only, `body_contains` only, or both. The exact builder
   shape on lumen depends on FLAG 3; the user-visible behaviour does NOT.
7. Calls `state.store.query_with(&tenant, range, &predicate)` when ANY
   filter is present; falls through to `state.store.query(&tenant, range)`
   when neither is present (or composes with an empty predicate; the two
   are observably identical).
8. Applies the result cap via `query_http_common::MAX_RESULT_ROWS` and
   `query_http_common::REASON_TOO_MANY_ROWS` on the post-filter records
   vector (UNCHANGED from log-query-severity-filter-v0; the matches vector
   is the one measured).
9. Serialises the bare JSON array (UNCHANGED).

### Domain Examples

#### 1: Happy path — Sara isolates `kafka timeout` from an INFO+ERROR mix

Tenant `acme-prod` has six records in `[1716200000s, 1716200060s)`:

| `observed_time_unix_nano` | `severity_number` | `body` |
|---|---|---|
| `1_716_200_005_000_000_000` | 9 (INFO)   | `checkout: heartbeat` |
| `1_716_200_010_000_000_000` | 17 (ERROR) | `kafka timeout connecting to broker-3` |
| `1_716_200_015_000_000_000` | 17 (ERROR) | `checkout: payment timeout` |
| `1_716_200_020_000_000_000` | 9 (INFO)   | `checkout: heartbeat` |
| `1_716_200_025_000_000_000` | 17 (ERROR) | `kafka timeout after 30s on topic orders` |
| `1_716_200_030_000_000_000` | 9 (INFO)   | `checkout: heartbeat` |

Sara runs
`curl 'http://logs.kaleidoscope.acme.internal/api/v1/logs?start=1716200000&end=1716200060&body_contains=kafka%20timeout' -H 'X-Tenant: acme-prod'`.

Response is HTTP 200 with a bare JSON array of TWO records, in ascending
`observed_time_unix_nano` order: the two records whose `body` contains
`kafka timeout`. The four records whose `body` does NOT contain that
substring are excluded. The shape of each record is identical to today's
response (same field set, same field names, same serialisation).

#### 2: Calm-empty — no record's body contains the substring

Tenant `acme-prod` has the SAME six records as Example 1. Priya runs
`curl '.../api/v1/logs?start=1716200000&end=1716200060&body_contains=cassandra' -H 'X-Tenant: acme-prod'`.
No record's body contains `cassandra`. Response is HTTP 200 with the calm
empty bare array `[]`. The response is NEVER HTTP 404; the absence of a
match is a successful query that returned no rows, byte-identical to the
slice-prior empty-response shape on a window with no records. Priya
distinguishes "substring not in any body in this window" (200 `[]`) from
"the query was malformed" (400 with envelope) from "the store could not be
read" (500 with envelope).

#### 3: Default unchanged — Marcus's old script keeps working

Marcus has an automation script that calls
`curl '.../api/v1/logs?start=1716200000&end=1716200060' -H 'X-Tenant:
acme-prod'` every 60 seconds. The script is NOT updated when slice 01
ships. The response is identical to the response it received the day
before slice 01 (and the day before log-query-severity-filter-v0): every
in-window record, in ascending `observed_time_unix_nano` order. The
backward-compatibility promise (the parameter is optional, absence is
no-filter) is honoured.

#### 4: Empty rejected — `?body_contains=` is a redacted 400

Sara fat-fingers
`curl '.../api/v1/logs?start=1716200000&end=1716200060&body_contains=' -H
'X-Tenant: acme-prod'` (she dropped the actual substring). The handler
recognises an empty value as MEANINGLESS (matching every record's body is
indistinguishable from no filter; this slice refuses the ambiguous case
out loud rather than silently falling through to "no filter"). Response is
HTTP 400 with the existing envelope
`{"status":"error","error":"invalid body_contains"}`. The store is NEVER
touched on this path.

#### 5: Case-sensitive pinned — `KAFKA` does NOT match `kafka`

Tenant `acme-prod` has one record with body `"kafka timeout"` in the
window. Sara runs
`curl '.../api/v1/logs?start=1716200000&end=1716200060&body_contains=KAFKA' -H 'X-Tenant: acme-prod'`.
Response is HTTP 200 with the calm empty bare array `[]`. The substring
match is byte-wise case-sensitive (`grep`-style; ASCII `K` is byte `0x4B`,
ASCII `k` is byte `0x6B`; the bytes differ; no match). Sara learns the
rule from the response and re-runs with `body_contains=kafka` to get the
record.

#### 6: Cross-tenant isolation — tenant B does NOT see tenant A's matches

Tenant `acme-prod` has one record with body `"kafka timeout connecting to
broker-3"` in the window. Tenant `globex-staging` has ZERO records in the
window. Sara, holding the `globex-staging` tenant credential, runs
`curl '.../api/v1/logs?start=1716200000&end=1716200060&body_contains=kafka%20timeout' -H 'X-Tenant: globex-staging'`.
Response is HTTP 200 with the calm empty bare array `[]`. The
`acme-prod` record's body NEVER appears in any tenant other than
`acme-prod`'s responses. The `body_contains` filter is applied AFTER
per-tenant isolation (the existing platform invariant from ADR-0047
§ "Per-tenant isolation"); the filter never widens the tenant scope.

### UAT Scenarios

#### Scenario: A known substring narrows the response to matching records

```gherkin
Given tenant "acme-prod" has six records in the window [1716200000s, 1716200060s):
  | observed_time_secs | severity_number | body                                       |
  | 1716200005         | 9               | checkout: heartbeat                        |
  | 1716200010         | 17              | kafka timeout connecting to broker-3       |
  | 1716200015         | 17              | checkout: payment timeout                  |
  | 1716200020         | 9               | checkout: heartbeat                        |
  | 1716200025         | 17              | kafka timeout after 30s on topic orders    |
  | 1716200030         | 9               | checkout: heartbeat                        |
When Sara GETs /api/v1/logs?start=1716200000&end=1716200060&body_contains=kafka%20timeout
Then the status is 200
And the body is a bare JSON array of exactly two records in ascending observed_time order
And every returned record's body contains the substring "kafka timeout"
And no record whose body lacks "kafka timeout" appears in the response
```

#### Scenario: An unmatched substring returns the calm empty array, never 404

```gherkin
Given tenant "acme-prod" has six records in the window, none of whose body contains "cassandra"
When Priya GETs /api/v1/logs over the window with body_contains=cassandra
Then the status is 200
And the body is the calm empty bare array []
And the status is NOT 404
And the status is NOT 500
```

#### Scenario: Parameter absent returns every record in the window (default unchanged)

```gherkin
Given tenant "acme-prod" has the same six records as the first scenario
When Marcus GETs /api/v1/logs?start=1716200000&end=1716200060 with NO body_contains parameter
Then the status is 200
And the body is a bare JSON array of all six in-window records
And the response is byte-equal to the slice-prior response for the same inputs
```

#### Scenario: An empty body_contains value is a redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly within the cap
When Sara GETs /api/v1/logs over the window with body_contains= (empty)
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid body_contains"}
And the store is NEVER queried on this path
```

#### Scenario: The match is case-sensitive (KAFKA does not match kafka)

```gherkin
Given tenant "acme-prod" has one record whose body is "kafka timeout connecting to broker-3"
And the record's observed_time is inside the window
When Sara GETs /api/v1/logs over the window with body_contains=KAFKA
Then the status is 200
And the body is the calm empty bare array []
And the record whose body is "kafka timeout connecting to broker-3" does NOT appear in the response
```

#### Scenario: Cross-tenant isolation — tenant B never sees tenant A's matches

```gherkin
Given tenant "acme-prod" has one record whose body is "kafka timeout connecting to broker-3" in the window
And tenant "globex-staging" has zero records in the window
When Sara GETs /api/v1/logs over the window with body_contains=kafka%20timeout under tenant "globex-staging"
Then the status is 200
And the body is the calm empty bare array []
And the body NEVER contains the substring "broker-3"
And no record from tenant "acme-prod" appears in the response
```

### Acceptance Criteria

- [ ] An optional query-string parameter `body_contains=<string>` is
      accepted on `GET /api/v1/logs`.
- [ ] When the parameter is present and non-empty, only records whose
      `body` field contains the supplied substring appear in the response.
- [ ] When the parameter is absent, the response is identical to the
      slice-prior response (every in-window record, no body filter applied).
- [ ] An empty value (`?body_contains=`) returns HTTP 400 with the existing
      envelope `{"status":"error","error":"invalid body_contains"}`.
- [ ] The error body does NOT echo the raw `body_contains` parameter value.
      (For the empty-string arm there is no raw value to echo; this AC
      pins the redaction posture for any future arm that adds a
      raw-value-bearing reason.)
- [ ] The match is byte-wise case-sensitive: `KAFKA` does NOT match
      `kafka`. The acceptance scenario above is the documentation source.
- [ ] An unmatched substring returns HTTP 200 with the calm empty bare
      array `[]`, NEVER HTTP 404 and NEVER HTTP 500.
- [ ] The match is applied AFTER per-tenant isolation: tenant B never sees
      a record from tenant A whose body contains the substring.
- [ ] The match composes conjunctively with `min_severity` when both are
      present: a record passes if and only if it satisfies BOTH filters.
- [ ] The window cap and result cap from ADR-0050 are preserved unchanged.
      The result cap measures the post-filter records vector.
- [ ] The bare JSON array response shape from ADR-0047 Decision 1 is
      preserved unchanged.
- [ ] The `lumen::LogStore` trait signatures stay byte-identical to the
      prior tag (Gate 2 `cargo public-api`). The slice does NOT add or
      remove any `LogStore` trait method.
- [ ] The half-open `[start, end)` window from ADR-0047 § 3 is preserved
      unchanged.
- [ ] `query-http-common` (ADR-0054) is the SOLE provider of the cap
      constants (`MAX_RESULT_ROWS`, `MAX_WINDOW_SECONDS`), the reason
      constants (`REASON_WINDOW_TOO_LARGE`, `REASON_TOO_MANY_ROWS`,
      `REASON_INVALID_TIME_RANGE`, `REASON_MISSING_TENANT`), the error
      envelope helper (`error_response`), the tenant seam
      (`resolve_tenant_or_refuse`), and the bounds parser
      (`parse_time_range`). No new copies of any of them appear in
      `log-query-api`.
- [ ] New lines of code in `crates/log-query-api/src/lib.rs` are under 30
      (KPI-3 envelope: a parse helper for the body-contains arm + a
      dispatch branch + a parameter field on the `LogsParams` struct +
      composition of the `min_severity` and `body_contains` predicate
      branches).

### Outcome KPIs

See `outcome-kpis.md` for the full table. Story-level summary:

- **Who**: SRE operators and automation clients of the log read API who
  hold a known error string and need to isolate records carrying it.
- **Does what**: Issue narrowed read requests
  (`body_contains=<substring>`) instead of pulling every in-window record
  and filtering client-side.
- **By how much**: The substring filter is HONEST — every record in the
  response contains the substring in its `body` field (not metadata, not
  by accident); verified by acceptance test.
- **Measured by**: Acceptance test in
  `crates/log-query-api/tests/slice_01_body_contains.rs` asserting (a)
  every returned record's body contains the substring, (b) no record in
  the fixture whose body contains the substring is omitted.
- **Baseline**: 100% of in-window records returned today (no body filter
  exists in the HTTP boundary).

### Technical Notes

- **Existing seam**: `lumen::LogStore` already exposes `query_with(&tenant,
  range, &Predicate)` (`crates/lumen/src/store.rs:89`) and
  `lumen::Predicate` already composes `service` and `min_severity` filters
  via `Predicate::matches` (`crates/lumen/src/predicate.rs:53-66`). The
  slice uses the `query_with` seam.
- **Predicate extension** (FLAG 3): `lumen::Predicate` does NOT today carry
  a `body_contains` field
  (`crates/lumen/src/predicate.rs:25-28` declares only `service` and
  `min_severity`). The slice needs ONE of two shapes:
  1. Add `body_contains: Option<String>` to `lumen::Predicate` with a
     builder `Predicate::body_contains(s: impl Into<String>) -> Self` and
     one new arm in `Predicate::matches` (`record.body.contains(target)`).
  2. Apply the substring filter handler-side on the returned `Vec<LogRecord>`
     after the `query_with` call, leaving `lumen::Predicate` byte-identical.
  Recommendation: shape (1). It keeps the conjunctive AND composition
  honest at the store boundary (the predicate IS the filter; the handler
  does NOT do per-record work), and it lets the v1 columnar substrate
  push the substring scan into the storage adapter where it belongs.
  Shape (2) is a fallback if DESIGN judges the lumen surface change too
  costly to ship in this slice. The user-visible behaviour and the
  acceptance scenarios are identical for both shapes; only the lumen
  surface diff and the mutation-test surface differ.
- **Parsing location**: The `body_contains` parse helper lives in
  `crates/log-query-api/src/lib.rs` alongside `parse_min_severity`. It is
  a separate free function, NOT a method on `lumen::Predicate` (the lumen
  crate stays string-mapping-free; the lumen surface, if shape (1) is
  chosen, takes a `String` already URL-decoded).
- **Order of checks** (mirrors log-query-severity-filter-v0):
  1. Resolve tenant via `query_http_common::resolve_tenant_or_refuse`
     (UNCHANGED).
  2. Parse window via `query_http_common::parse_time_range` (UNCHANGED).
  3. Window-cap check via `query_http_common::MAX_WINDOW_SECONDS`
     (UNCHANGED).
  4. Parse `min_severity` if present (UNCHANGED from
     log-query-severity-filter-v0).
  5. Parse `body_contains` if present (NEW). Empty value -> 400 with
     `invalid body_contains`. Store is NOT touched on this path.
  6. Build the composed predicate (NEW dispatch arm). When ANY filter is
     present, call `query_with`; otherwise call `query`.
  7. Result-cap check on the post-filter records vector
     (`query_http_common::MAX_RESULT_ROWS`; UNCHANGED).
  8. `success_response(records)`.
- **Composition with `min_severity`**: When both parameters are present,
  the composed predicate is conjunctive (`AND`). This matches
  `Predicate::matches` semantics (`crates/lumen/src/predicate.rs:53-66`)
  exactly as the existing service + severity composition does today.
- **Mutation targets** (handed to DESIGN as fertile ground for Gate 5):
  - The substring boundary: a `contains` -> `starts_with` mutant must be
    killed by an acceptance scenario where the substring is in the MIDDLE
    of the body.
  - The substring boundary: a `contains` -> `ends_with` mutant must be
    killed by an acceptance scenario where the substring is at the START
    of the body.
  - The case-sensitivity boundary: a `String::contains` ->
    `to_lowercase().contains` mutant must be killed by the `KAFKA` !=
    `kafka` scenario.
  - The empty-string rejection: a mutant that treats `Some("")` as `None`
    (the unfiltered-fallthrough mutant from
    log-query-severity-filter-v0's `parse_min_severity`) must be killed
    by the empty-string 400 scenario.
  - The redaction on the empty-string 400: the reason text is a literal
    constant; a mutant that interpolates the (empty) raw value into the
    body must be killed by an explicit "body == the literal envelope"
    assertion.
  - The order-of-checks: a mutant that calls the store BEFORE parsing
    `body_contains` (e.g. moves the parse below the `query_with` call)
    must be killed by an assertion that the store is NOT touched on the
    empty-string 400 path.
  - The cross-tenant isolation arm: a mutant that resolves the tenant
    AFTER applying the filter (or that applies the filter against all
    tenants' records) must be killed by the cross-tenant scenario.

### Dependencies

- **Resolved**:
  - ADR-0047 (log-query-api contract).
  - ADR-0050 (read-side caps).
  - ADR-0052 (`min_severity` parameter, composition partner).
  - ADR-0054 (`query-http-common` extraction; M-5).
  - `lumen::LogStore::query_with` (`crates/lumen/src/store.rs:89`).
  - `lumen::Predicate::matches` conjunctive composition
    (`crates/lumen/src/predicate.rs:53-66`).
  - `query_http_common::{MAX_RESULT_ROWS, MAX_WINDOW_SECONDS,
    REASON_WINDOW_TOO_LARGE, REASON_TOO_MANY_ROWS, REASON_MISSING_TENANT,
    REASON_INVALID_TIME_RANGE, parse_time_range, error_response,
    resolve_tenant_or_refuse}`. All present and live in production
    behind ADR-0054.
- **Tracked (not blockers)**: DESIGN flags 1-6 in `wave-decisions.md`.

## US-02 Unknown substring returns the calm empty array

The acceptance content is covered by US-01 Scenario "An unmatched substring
returns the calm empty array, never 404" and Domain Example 2. This story
exists as a separately-named slice unit so the carpaccio gate counts it as
a distinct behavioural promise: the API does NOT use 404 to signal an empty
post-filter result. The Elevator Pitch, persona, problem, solution, AC, and
KPI lines are inherited from US-01; the acceptance evidence is the single
scenario already pinned above.

### Elevator Pitch

- **Before**: Without a deliberate test, a `body_contains` filter that
  matches no record could plausibly return 404 (HTTP-as-resource semantics)
  or even a 500 if the empty-result arm is mishandled. Sara cannot
  distinguish "substring absent from this window" from "the query was
  malformed" from "the platform is broken".
- **After**: A `body_contains` filter with no matches returns HTTP 200 with
  the bare empty array `[]`. Sara reads "200 + `[]`" and knows the platform
  answered honestly: the substring is not in any record's body in this
  window for this tenant.
- **Decision enabled**: Sara concludes the error signature is NOT in this
  window and widens the time range (or accepts the negative finding).

## US-03 Missing body_contains preserves today's behaviour

### Elevator Pitch

- **Before**: Marcus's automation, which today calls `/api/v1/logs` without
  any `body_contains` parameter, MUST keep receiving the slice-prior
  response on every existing call. A change in the no-filter arm would
  break the entire installed base of scripts on day one.
- **After**: The absence of `body_contains` deserialises as `None` and the
  handler keeps its prior dispatch arm (either `query` directly, or
  `query_with` over an empty predicate). The acceptance suite includes a
  byte-equality assertion against the slice-prior response shape.
- **Decision enabled**: Marcus does NOT update his script. The slice ships
  with zero broken clients. Operations team confirms the backward-compat
  contract by running today's `curl` command against the new build and
  comparing the response byte sequence.

This story is the no-regression contract. The acceptance evidence is the
"Parameter absent returns every record in the window" scenario in US-01.

## US-04 Empty body_contains is a redacted 400

### Elevator Pitch

- **Before**: `?body_contains=` could slip through serde as `Some("")`,
  which a naive handler would pass to `String::contains` (every string
  contains the empty substring, so the filter would silently match every
  record — observably indistinguishable from "no filter"). Sara would not
  know whether she dropped the substring or whether her client serialised
  it wrong. The platform would lie quietly by widening the result set
  instead of refusing.
- **After**: An empty `body_contains` value is REJECTED with HTTP 400 and
  the literal envelope `{"status":"error","error":"invalid
  body_contains"}`. The reason text is a static literal; the (empty) raw
  value is NEVER reflected. The store is NEVER touched. Sara sees the 400
  and re-runs with the substring she meant to send.
- **Decision enabled**: Sara distinguishes "I sent a bad request" (400)
  from "the substring is not in this window" (200 + `[]`) from "the
  platform is broken" (500). The slice refuses ambiguity out loud, in
  symmetry with the unknown-severity 400 from
  log-query-severity-filter-v0.

The acceptance evidence is the "An empty body_contains value is a redacted
400" scenario in US-01.

## US-05 Case-sensitive matching is pinned by acceptance test

### Elevator Pitch

- **Before**: Sara might assume `body_contains=KAFKA` matches `kafka` (some
  search tools fold case by default; `grep -i` is muscle memory). Without
  a documented test, the slice's case-sensitivity is folklore.
- **After**: An acceptance test asserts that `body_contains=KAFKA` returns
  the calm empty array against a fixture containing `kafka timeout`. The
  test IS the documentation: Sara reads the test and learns the rule
  before her first incident.
- **Decision enabled**: Sara learns the platform's posture from a place
  she will actually look (the acceptance suite). She runs
  `body_contains=kafka` next time, or `body_contains=KAFKA` if her runbook
  capitalised the string. A future slice that adds case-insensitive
  matching does so as a NEW parameter (FLAG 2) and updates the test, NOT
  as a behaviour change on `body_contains`.

The acceptance evidence is the "The match is case-sensitive" scenario in
US-01.

## US-06 Cross-tenant isolation holds for body_contains

### Elevator Pitch

- **Before**: A new filter is a new dimension along which the per-tenant
  isolation invariant could leak: a careless implementation might apply
  the substring filter ACROSS all tenants' records and then filter by
  tenant, instead of filtering by tenant FIRST and substring within. Such
  a bug would not show up on a single-tenant fixture; it would only
  surface in production when a tenant queried a substring that exists in
  a different tenant's logs.
- **After**: An acceptance test asserts that tenant B receives `[]` when
  querying for a substring that exists in tenant A's records and is
  absent from tenant B's records. The test pins the platform invariant
  (ADR-0047 § "Per-tenant isolation") against the new filter arm. The
  invariant is enforced by the EXISTING `query_with(&tenant, range, ...)`
  seam (the tenant is the first argument; the bucket lookup happens
  before any predicate evaluation in
  `crates/lumen/src/store.rs:166-180`); the test confirms that the
  handler-level dispatch honours the same order.
- **Decision enabled**: Tenant B's operators trust the platform's
  multi-tenant promise without rereading the source. The slice ships with
  the invariant proved against the new arm, not assumed.

The acceptance evidence is the "Cross-tenant isolation" scenario in US-01.

## Flags to DESIGN (do NOT decide in DISCUSS; recommendations recorded for DESIGN to pin)

See `wave-decisions.md` § "Flags to DESIGN" for the full table. Brief
summary (DESIGN reads the wave-decisions table for the full reasoning):

1. **Substring vs regex** — Recommended: SUBSTRING. Regex is a separate
   future slice.
2. **Case-sensitivity** — Recommended: CASE-SENSITIVE (`grep`-style).
3. **Predicate seam in lumen** — Recommended: extend `lumen::Predicate`
   with a `body_contains` builder and one new arm in `Predicate::matches`.
4. **Empty string handling** — Recommended: 400 with the literal reason
   `invalid body_contains`.
5. **Anti-echo on the empty-string 400** — Recommended: the 400 body is
   the literal envelope `{"status":"error","error":"invalid
   body_contains"}`; the raw value is NEVER interpolated.
6. **ADR-0055 (small)** — Recommended: YES if FLAG 3 lands as a lumen
   surface extension; NO if FLAG 3 lands as handler-side filtering only.
