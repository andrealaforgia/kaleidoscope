<!-- markdownlint-disable MD024 -->

# User Stories: log-body-regex-search-v0

Slice 01, thin. The log read endpoint `GET /api/v1/logs?start=&end=`, which
today already accepts the optional `min_severity` floor (ADR-0052) AND the
optional `body_contains` byte-substring filter (ADR-0055, shipped at commit
1bfa609), grows ONE further optional request parameter: `body_regex=<pattern>`.
The parameter restricts the returned `LogRecord`s to those whose `body` field
(the OTLP-shaped `String` body, NOT `severity_text`, NOT any attribute, NOT a
resource attribute) is matched by the supplied regular expression. Default
behaviour (parameter absent) is unchanged: every in-window record is returned
(composed with `min_severity` if present).

This is a brownfield carpaccio slice on top of an existing endpoint. The
walking skeleton is implicit in the slices that already shipped on
`/api/v1/logs` (read endpoint exists, durable store exists, tenant seam
exists, caps exist, severity floor exists, byte-substring filter exists); no
greenfield skeleton is rebuilt.

The slice is the immediate sibling of `log-body-text-search-v0`. The
contract growth shape is parallel: ONE optional query-string parameter on
the same route, ONE new parse helper, ONE new dispatch arm, ONE new (or
reused) 400 reason class, filter-BEFORE-cap interaction preserved. The
substantive difference is the matching grammar: a full regular expression
via the workspace's existing `regex` crate (already a direct dependency of
`query-api` per ADR-0046, already in `Cargo.lock` at 1.12.3), NOT a
byte-substring.

The slice is also the direct beneficiary of `gate-5-mutants-lumen-v0`
(chiuso d96a807): the new `Predicate::body_regex` arm will be mutation-tested
automatically by the workspace mutants gate the day it lands. The
combination of `query-http-common` reuse (ADR-0054) + `regex` crate reuse
(ADR-0046) + mutation-gate reuse (gate-5-mutants-lumen-v0) means this slice
ships almost nothing new at the platform level; the value is concentrated
in ONE new parse helper, ONE new predicate arm, ONE compiled-regex field.

## System Constraints (cross-cutting)

- The existing `MAX_WINDOW_SECONDS = 86_400` and `MAX_RESULT_ROWS = 100_000`
  caps from ADR-0050 are PRESERVED unchanged. The new filter MUST NOT
  remove, reorder, or weaken either cap. Both constants are consumed from
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
  posture; symmetric with ADR-0050 Decision 7, ADR-0052 Decision 1,
  ADR-0055 DD5). The reason text on every 400 arm is a static literal; the
  raw `body_regex` value NEVER appears in the response body.
- The bare JSON array success shape (ADR-0047 Decision 1) is preserved. The
  filter changes WHICH records appear, not the shape of the response. The
  empty arm is `[]`, HTTP 200, NEVER 404.
- The `LogRecord.body` field is the OTLP-shaped `String` body
  (`crates/lumen/src/record.rs:54`). It is the ONLY field the regex is
  matched against; `severity_text`, `attributes`, `resource_attributes`,
  and trace-context fields are out of scope for this slice.
- The half-open `[start, end)` window from ADR-0047 § 3 is preserved
  unchanged.
- The `min_severity` parameter from log-query-severity-filter-v0 is
  PRESERVED unchanged. When both `min_severity` and `body_regex` are
  present, both filters apply (conjunctive `AND` per `Predicate::matches`).
- The `body_contains` parameter from log-body-text-search-v0 (ADR-0055) is
  PRESERVED unchanged. The slice does NOT alter the byte-substring
  semantics, the case-sensitive pin, the 1024-byte cap, or the
  `invalid body_contains` reason literal. See PIN below on the
  combination of `body_contains` + `body_regex` in the same request.
- The slice composes with the existing `query_with(&tenant, range,
  &predicate)` seam on `lumen::LogStore` (`crates/lumen/src/store.rs:89`).
  The `LogStore` trait signatures stay byte-identical to the prior tag.
- The regex grammar is the workspace's existing `regex` crate (RE2-derived,
  linear-time, no catastrophic backtracking). The same crate is already a
  direct dependency of `query-api` for the metric-label `=~` / `!~`
  matchers (ADR-0046 Decision 1). This slice adds the `regex` crate as a
  direct dependency of `lumen` (NEW; see FLAG 5 below).

## PINs (confirmed against the source by DISCUSS; DESIGN to record verbatim)

The following pins are derived from a direct read of the post-`body_contains`
source tree. DISCUSS surfaces them so DESIGN does not re-discover them.

### PIN 1: Regex grammar is the `regex` crate's default syntax

The workspace already ships the `regex` crate at version 1 (a direct
dependency of `query-api` per ADR-0046; transitively in `Cargo.lock` at
1.12.3 for every crate that pulls it in). The grammar is RE2-derived,
linear-time, with no catastrophic backtracking — chosen precisely because
the pattern is exposed user input and a backtracking engine would be a
ReDoS surface. This slice adopts the same grammar; an operator who knows
`query-api`'s `=~` matchers knows this slice's `body_regex` syntax.

### PIN 2: Case-sensitive by default; `(?i)` inline flag for case-insensitive

Symmetric with ADR-0055 DD2 (`body_contains` is byte-wise case-sensitive).
An operator who wants case-insensitive matching uses the standard inline
flag: `body_regex=(?i)kafka` matches both `kafka` and `KAFKA`. The slice
does NOT add a separate `case_sensitive=false` query parameter; the
`regex` crate's inline flag syntax is the operator-controlled escape hatch.
Rationale: muscle-memory honesty (grep is case-sensitive by default; `grep
-i` exists for the opt-in case), and zero new parameter surface.

### PIN 3: Length cap is 1024 bytes; reused from ADR-0055 DD6

The handler rejects any non-empty `body_regex` value whose byte length
strictly exceeds 1024 bytes. The cap value matches the `body_contains` cap
exactly for consistency: an operator who learns the rule once learns it
for every body-related parameter. The boundary is INCLUSIVE: 1024 bytes is
served, 1025 bytes is refused. A `MAX_BODY_REGEX_LEN` constant lives next
to `MAX_BODY_CONTAINS_LEN` in `crates/log-query-api/src/lib.rs`.

### PIN 4: Compile failure is a redacted 400; raw pattern is NEVER echoed

A `body_regex` value that the `regex` crate refuses to compile (unbalanced
paren, invalid escape, unknown class, etc.) is rejected at the HTTP
boundary with HTTP 400 and the literal envelope
`{"status":"error","error":"invalid body_regex"}`. The store is NEVER
touched on this path. The raw pattern is NEVER interpolated into the
response (the `regex::Error::Display` impl would leak parts of the
pattern); the redaction posture is symmetric with ADR-0055 DD5,
ADR-0052 Decision 1, ADR-0050 Decision 7, and ADR-0047 Decision 1.

### PIN 5: Empty string is the same redacted 400

`?body_regex=` arrives as `Some("")` from serde. The empty pattern is
meaningless on `Regex::is_match` (the `regex` crate accepts the empty
pattern as a Regex that matches every position, which would silently
match every record — observably indistinguishable from no filter). The
slice refuses the ambiguity out loud: HTTP 400 with
`{"status":"error","error":"invalid body_regex"}`. The store is NEVER
touched on this path.

### PIN 6: No default anchoring; `Regex::is_match` semantics

The regex is evaluated via `Regex::is_match(&record.body)`, which returns
`true` iff the regex matches ANYWHERE in the body. There is NO implicit
`^` or `$` anchoring; an operator who wants whole-body matching writes
`^pattern$` explicitly, and an operator who wants prefix matching writes
`^pattern`. Symmetric with the metric-label `=~` matchers in ADR-0046
(unanchored by default, with `^` / `$` available for explicit anchoring).
Multiline semantics follow the `regex` crate's defaults; an operator can
opt into multiline via the inline `(?m)` flag.

### PIN 7: `body_contains` AND `body_regex` are MUTUALLY EXCLUSIVE at slice 01

The current handler dispatch (`crates/log-query-api/src/lib.rs:195-212`,
verified by direct read) is a four-arm cross product of `min_severity` x
`body_contains`. Growing the cross product to `min_severity` x
`body_contains` x `body_regex` would yield EIGHT arms — testing surface
explodes, and the semantic question "what does it mean to send BOTH a
substring filter and a regex filter on the same body?" deserves its own
deliberate answer (intersection? union? error?), not a quiet AND default.

At slice 01 the slice REFUSES the ambiguous case: a request with BOTH
`body_contains=` AND `body_regex=` present is HTTP 400 with the literal
envelope `{"status":"error","error":"specify body_regex or body_contains,
not both"}`. The store is NEVER touched on this path. Future slices MAY
relax the rule (e.g. AND-compose them under a `Predicate::body_contains` +
`Predicate::body_regex` cross-product) once a real operator use case earns
the testing surface; the slice 01 carpaccio deliberately defers the
question.

The reason text is a NEW literal class (`"specify body_regex or
body_contains, not both"`); it differs from `"invalid body_regex"` because
the failure surface is different (neither value is syntactically invalid;
they are mutually exclusive). DESIGN may rename or relocate this literal;
DISCUSS confirms the user-visible posture is "explicit error, never
ambiguity".

## OUT of scope (DECLARED and DEFERRED)

The following are EXPLICITLY out for slice 01 and named so DESIGN does not
re-discover them as gaps:

- A configurable regex backend (PCRE, alternative engines). The slice
  uses the `regex` crate ONLY. ReDoS-safe linear-time matching is the
  posture.
- Matching across multiple fields (`body OR attributes`). The slice
  matches `LogRecord.body` ONLY.
- Matching on `severity_text`, `attributes`, or `resource_attributes`. Each
  is a separate slice if and when it earns a third call site.
- Whole-word matching as a separate parameter. An operator who wants
  whole-word matching writes the regex `\bpattern\b` explicitly.
- A configurable result limit specific to `body_regex` queries. The
  ADR-0050 result cap applies unchanged on the post-filter records.
- A per-request regex-compile timeout or per-record match timeout. The
  `regex` crate's linear-time guarantee is the slice's protection; the
  acceptance suite pins a sufficiently large fixture (200_000+ records
  in the cross-mutex check scenarios) to surface any pathological
  slowdown in CI.
- A pre-compiled regex cache across requests. Each request compiles its
  own `Regex`; the compile cost is paid once per request, fail-fast on
  invalid syntax. A cache is a future optimisation if real usage demands.
- Multiple regexes in one request (`body_regex=foo&body_regex=bar`). The
  slice accepts ONE regex per request; the axum/serde default behaviour
  on a repeated parameter is the LAST occurrence wins, but the
  acceptance suite pins only the single-regex contract.
- Combining `body_contains` AND `body_regex` in the same request — see
  PIN 7. Slice 01 returns 400; future slices MAY relax.
- A new ADR written by DISCUSS. ADR drafting belongs to DESIGN
  (Morgan); DISCUSS surfaces the recommendation in FLAG 5 below.

## US-01 Walking skeleton: a known regex pattern matches a family of variations

### Elevator Pitch

- **Before**: Maria Santos, on-call SRE for tenant `acme-prod`, has a
  paging alert that fires for any kafka timeout, but the production
  service emits the failure in three distinct shapes:
  `"kafka timeout connecting to broker-3"`,
  `"kafka request timed out after 30s on topic orders"`, and
  `"kafka: connection timed out (broker-7)"`. With today's `body_contains`
  filter she has to run THREE separate queries (`body_contains=kafka
  timeout`, `body_contains=kafka request timed out`, `body_contains=kafka:
  connection timed out`) and union the results in her head, or fall back
  to downloading the whole window and grepping client-side.
- **After**: Maria runs `curl
  'http://logs.kaleidoscope.acme.internal/api/v1/logs?start=1716200000&end=1716200060&body_regex=kafka.%2Atimeout'
  -H 'X-Tenant: acme-prod'` and the HTTP 200 body is a bare JSON array
  carrying ALL records whose `body` field matches the regex
  `kafka.*timeout` — every shape of the same failure family in ONE
  query. The shape of each record is identical to today's response;
  only the row set is the union of every regex-matched record in the
  window.
- **Decision enabled**: Maria sees the full failure family in one
  pass, decides which broker / topic appears in every shape, and
  isolates the root cause without manual query unioning.

### Problem

Maria Santos is the on-call SRE for tenant `acme-prod`. A platform
incident produces error messages in several closely-related but
distinct shapes (the kafka client library and the application code
each emit a different sentence for the same underlying network
failure). Today's `body_contains` filter, shipped in
`log-body-text-search-v0` (commit 1bfa609), restricts the response to
records whose `body` field contains an exact byte substring. To
isolate every variation of "kafka timeout" Maria runs three or four
separate `body_contains` queries and reconciles the results, or
downloads the entire window and pipes through `grep -E`. Both
workflows pay attention cost in seconds Maria does not have during
an active incident.

The natural sibling of `body_contains` is a `body_regex` parameter:
same endpoint, same envelope, same redaction, same cap interaction;
only the matching grammar changes (byte-substring to regular
expression). The implementation lights up automatically on both
adapters via the existing `Predicate::matches` seam (the same shape
that made `body_contains` a four-line predicate growth on lumen
under ADR-0055 § 10).

### Who

- **Maria Santos** | SRE on `acme-prod`, mid-incident, terminal +
  curl + jq | Triage urgency: needs to isolate every shape of a
  failure family within seconds.
- **Marcus Webb** | platform engineer building an automated incident
  classifier that polls `body_regex=<failure-signature>` every 60
  seconds to count occurrences of a known failure family with several
  message shapes | Throughput motive: payload size and per-poll
  latency dominate the request budget.
- **Priya Raman** | support engineer triaging a customer ticket
  whose attached log excerpt quotes a message in a slightly different
  form than what production currently emits (the message has rotated
  between three shapes over the last release) | Correctness motive:
  she needs to confirm a regex covering all known shapes finds the
  message somewhere in the customer's tenant's window.

### Solution

`GET /api/v1/logs` accepts an optional query-string parameter
`body_regex=<pattern>`. The handler:

1. Resolves the tenant via
   `query_http_common::resolve_tenant_or_refuse` (UNCHANGED).
2. Parses the window via `query_http_common::parse_time_range`
   (UNCHANGED).
3. Enforces the window cap via
   `query_http_common::MAX_WINDOW_SECONDS` and
   `query_http_common::REASON_WINDOW_TOO_LARGE` (UNCHANGED).
4. Parses `min_severity` if present (UNCHANGED from
   log-query-severity-filter-v0).
5. Parses `body_contains` if present (UNCHANGED from
   log-body-text-search-v0).
6. Parses `body_regex` if present (NEW): an empty value is rejected
   400 with literal `invalid body_regex`; an over-cap value (>1024
   bytes) is rejected 400 with the same literal; a value that the
   `regex` crate refuses to compile is rejected 400 with the same
   literal. The store is NEVER touched on any of these arms.
7. Mutual-exclusion check (NEW): if BOTH `body_contains` AND
   `body_regex` are present, reject 400 with literal
   `specify body_regex or body_contains, not both`. The store is
   NEVER touched on this path.
8. Builds a `lumen::Predicate` carrying the present filters. The
   exact predicate field shape on lumen depends on FLAG 2; the
   user-visible behaviour does NOT.
9. Calls `state.store.query_with(&tenant, range, &predicate)` when
   ANY filter is present; falls through to
   `state.store.query(&tenant, range)` when no filter is present.
10. Applies the result cap via
    `query_http_common::MAX_RESULT_ROWS` on the post-filter records
    vector (UNCHANGED).
11. Serialises the bare JSON array (UNCHANGED).

### Domain Examples

#### 1: Happy path — Maria isolates all kafka-timeout shapes in one regex query

Tenant `acme-prod` has eight records in `[1716200000s, 1716200060s)`:

| `observed_time_unix_nano` | `severity_number` | `body` |
|---|---|---|
| `1_716_200_005_000_000_000` | 9 (INFO)   | `checkout: heartbeat` |
| `1_716_200_010_000_000_000` | 17 (ERROR) | `kafka timeout connecting to broker-3` |
| `1_716_200_015_000_000_000` | 17 (ERROR) | `checkout: payment timeout` |
| `1_716_200_020_000_000_000` | 9 (INFO)   | `checkout: heartbeat` |
| `1_716_200_025_000_000_000` | 17 (ERROR) | `kafka request timed out after 30s on topic orders` |
| `1_716_200_030_000_000_000` | 9 (INFO)   | `checkout: heartbeat` |
| `1_716_200_035_000_000_000` | 17 (ERROR) | `kafka: connection timed out (broker-7)` |
| `1_716_200_040_000_000_000` | 17 (ERROR) | `redis: GET timeout on key user-42` |

Maria runs
`curl 'http://logs.kaleidoscope.acme.internal/api/v1/logs?start=1716200000&end=1716200060&body_regex=kafka.%2Atimeout' -H 'X-Tenant: acme-prod'`.

The decoded pattern is `kafka.*timeout`. Response is HTTP 200 with a
bare JSON array of THREE records, in ascending
`observed_time_unix_nano` order: the records at `t=10`, `t=25`, and
`t=35` whose `body` field matches the regex. The five other records
(three heartbeats, one payment-timeout, one redis-timeout) are
excluded. Maria sees every shape of the kafka-timeout failure family
in one pass.

#### 2: Calm-empty — no record's body matches the regex

Tenant `acme-prod` has the SAME eight records as Example 1. Priya
runs
`curl '.../api/v1/logs?start=1716200000&end=1716200060&body_regex=cassandra.%2Atimeout' -H 'X-Tenant: acme-prod'`.
No record's body matches `cassandra.*timeout`. Response is HTTP 200
with the calm empty bare array `[]`. The response is NEVER HTTP 404;
the absence of a match is a successful query that returned no rows.

#### 3: Default unchanged — Marcus's old script keeps working

Marcus's automation calls
`curl '.../api/v1/logs?start=1716200000&end=1716200060' -H 'X-Tenant:
acme-prod'` every 60 seconds. The script is NOT updated when this
slice ships. The response is identical to the response it received
the day before slice 01 (and the day before
log-body-text-search-v0). The backward-compatibility promise is
honoured.

#### 4a: Invalid regex syntax — `body_regex=foo(bar` is a redacted 400

Maria fat-fingers
`curl '.../api/v1/logs?start=1716200000&end=1716200060&body_regex=foo(bar' -H 'X-Tenant: acme-prod'`.
The unbalanced parenthesis means the `regex` crate's compiler
refuses the pattern. Response is HTTP 400 with the literal envelope
`{"status":"error","error":"invalid body_regex"}`. The store is
NEVER touched on this path. The raw pattern (`foo(bar`) NEVER appears
in the response body.

#### 4b: Empty string — `body_regex=` is the same redacted 400

Maria fat-fingers `curl
'.../api/v1/logs?start=1716200000&end=1716200060&body_regex=' -H
'X-Tenant: acme-prod'` (she dropped the actual pattern). Response is
HTTP 400 with the literal envelope
`{"status":"error","error":"invalid body_regex"}`. The store is
NEVER touched.

#### 4c: Over-cap length — `body_regex=` of 1025 bytes is the same redacted 400

Maria's automation pastes a long regex with embedded literals.
A 1025-byte `body_regex` value is rejected: HTTP 400 with the
literal envelope `{"status":"error","error":"invalid body_regex"}`.
The store is NEVER touched. The raw value (a 1025-byte payload)
NEVER appears in the response. A 1024-byte value is served (inclusive
boundary, symmetric with `body_contains`).

#### 5: Case-sensitive pinned — `body_regex=kafka` does NOT match `KAFKA`

Tenant `acme-prod` has one record with body `"KAFKA timeout"` in
the window. Maria runs `body_regex=kafka`. Response is HTTP 200
with the calm empty bare array `[]`. The regex match is byte-wise
case-sensitive by default (the `regex` crate's default mode). Maria
learns the rule from the response and re-runs with
`body_regex=(?i)kafka` (the inline case-insensitive flag) to match
both shapes; she gets the record back.

#### 6: Mutual exclusion — sending BOTH `body_contains` AND `body_regex` is a redacted 400

Maria's automation has a bug: it constructs the URL by appending
`&body_contains=kafka` to a query that already carries
`&body_regex=kafka.*timeout`. Request URL is
`.../api/v1/logs?start=...&end=...&body_contains=kafka&body_regex=kafka.%2Atimeout`.
Response is HTTP 400 with the literal envelope
`{"status":"error","error":"specify body_regex or body_contains, not both"}`.
The store is NEVER touched on this path. Maria fixes the
automation to send EXACTLY one of the two.

#### 7: Cross-tenant isolation — tenant B does NOT see tenant A's regex matches

Tenant `acme-prod` has one record with body `"kafka timeout"` in
the window. Tenant `globex-staging` has ZERO records in the window.
Maria, holding the `globex-staging` tenant credential, runs
`body_regex=kafka.%2Atimeout` against `globex-staging`. Response is
HTTP 200 with the calm empty bare array `[]`. The `acme-prod`
record's body NEVER appears in any tenant other than `acme-prod`'s
responses. The `body_regex` filter is applied AFTER per-tenant
isolation (ADR-0047 § "Per-tenant isolation"); the filter never
widens the tenant scope.

### UAT Scenarios

#### Scenario: A known pattern matches all shapes of the failure family

```gherkin
Given tenant "acme-prod" has eight records in the window [1716200000s, 1716200060s):
  | observed_time_secs | severity_number | body                                                  |
  | 1716200005         | 9               | checkout: heartbeat                                   |
  | 1716200010         | 17              | kafka timeout connecting to broker-3                  |
  | 1716200015         | 17              | checkout: payment timeout                             |
  | 1716200020         | 9               | checkout: heartbeat                                   |
  | 1716200025         | 17              | kafka request timed out after 30s on topic orders     |
  | 1716200030         | 9               | checkout: heartbeat                                   |
  | 1716200035         | 17              | kafka: connection timed out (broker-7)                |
  | 1716200040         | 17              | redis: GET timeout on key user-42                     |
When Maria GETs /api/v1/logs?start=1716200000&end=1716200060&body_regex=kafka.%2Atimeout
Then the status is 200
And the body is a bare JSON array of exactly three records in ascending observed_time order
And every returned record's body matches the regex kafka.*timeout
And no record whose body does not match the regex appears in the response
```

#### Scenario: An unmatched pattern returns the calm empty array, never 404

```gherkin
Given tenant "acme-prod" has eight records in the window, none of whose body matches "cassandra.*timeout"
When Priya GETs /api/v1/logs over the window with body_regex=cassandra.%2Atimeout
Then the status is 200
And the body is the calm empty bare array []
And the status is NOT 404
And the status is NOT 500
```

#### Scenario: Parameter absent returns every record in the window (default unchanged)

```gherkin
Given tenant "acme-prod" has the same eight records as the first scenario
When Marcus GETs /api/v1/logs?start=1716200000&end=1716200060 with NO body_regex parameter
Then the status is 200
And the body is a bare JSON array of all eight in-window records
And the response is byte-equal to the slice-prior response for the same inputs
```

#### Scenario: An invalid regex pattern is a redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly within the cap
When Maria GETs /api/v1/logs over the window with body_regex=foo(bar
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid body_regex"}
And the body NEVER contains the substring "foo(bar"
And the store is NEVER queried on this path
```

#### Scenario: An empty body_regex value is the same redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly within the cap
When Maria GETs /api/v1/logs over the window with body_regex= (empty)
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid body_regex"}
And the store is NEVER queried on this path
```

#### Scenario: An over-cap body_regex value is the same redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly within the cap
When Maria GETs /api/v1/logs over the window with body_regex equal to 1025 bytes
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid body_regex"}
And the body does NOT contain any byte of the raw oversize value
And the store is NEVER queried on this path
```

#### Scenario: The match is case-sensitive by default

```gherkin
Given tenant "acme-prod" has one record whose body is "KAFKA timeout connecting to broker-3"
And the record's observed_time is inside the window
When Maria GETs /api/v1/logs over the window with body_regex=kafka
Then the status is 200
And the body is the calm empty bare array []
And the record whose body is "KAFKA timeout connecting to broker-3" does NOT appear
```

#### Scenario: Body_contains and body_regex are mutually exclusive

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly within the cap
When Maria GETs /api/v1/logs over the window with BOTH body_contains=kafka AND body_regex=kafka.%2Atimeout
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"specify body_regex or body_contains, not both"}
And the store is NEVER queried on this path
```

#### Scenario: Cross-tenant isolation — tenant B never sees tenant A's regex matches

```gherkin
Given tenant "acme-prod" has one record whose body is "kafka timeout connecting to broker-3" in the window
And tenant "globex-staging" has zero records in the window
When Maria GETs /api/v1/logs over the window with body_regex=kafka.%2Atimeout under tenant "globex-staging"
Then the status is 200
And the body is the calm empty bare array []
And the body NEVER contains the substring "broker-3"
And no record from tenant "acme-prod" appears in the response
```

### Acceptance Criteria

- [ ] An optional query-string parameter `body_regex=<pattern>` is
      accepted on `GET /api/v1/logs`.
- [ ] When the parameter is present and the pattern is valid, only
      records whose `body` field is matched by the regex appear in
      the response.
- [ ] When the parameter is absent, the response is identical to the
      slice-prior response (every in-window record, no body filter
      applied beyond the existing `min_severity` and `body_contains`
      arms).
- [ ] An empty value (`?body_regex=`) returns HTTP 400 with the
      literal envelope
      `{"status":"error","error":"invalid body_regex"}`.
- [ ] A value whose byte length strictly exceeds 1024 bytes returns
      HTTP 400 with the same literal envelope.
- [ ] A value that the `regex` crate refuses to compile returns HTTP
      400 with the same literal envelope. The store is NEVER touched
      on the compile-failure path.
- [ ] The error body NEVER echoes the raw `body_regex` parameter
      value, including (a) the empty value, (b) the over-cap value,
      and (c) the invalid-syntax value.
- [ ] The match is byte-wise case-sensitive by default: `body_regex=kafka`
      does NOT match a record whose body is `KAFKA timeout`.
      Operators can opt into case-insensitive matching via the
      inline `(?i)` flag.
- [ ] The match is unanchored by default: `body_regex=timeout`
      matches a record whose body is `kafka timeout connecting to
      broker-3`. Operators who want anchored matching write `^pattern`
      or `pattern$` explicitly.
- [ ] An unmatched pattern returns HTTP 200 with the calm empty
      bare array `[]`, NEVER HTTP 404 and NEVER HTTP 500.
- [ ] The match is applied AFTER per-tenant isolation: tenant B
      never sees a record from tenant A whose body matches.
- [ ] When BOTH `body_contains` AND `body_regex` are present in the
      same request, the response is HTTP 400 with the literal
      envelope
      `{"status":"error","error":"specify body_regex or body_contains, not both"}`.
      The store is NEVER touched on this path.
- [ ] The match composes conjunctively with `min_severity` when
      both are present: a record passes iff it satisfies BOTH the
      severity floor AND the regex match.
- [ ] The window cap and result cap from ADR-0050 are preserved
      unchanged. The result cap measures the post-filter records
      vector.
- [ ] The bare JSON array response shape from ADR-0047 Decision 1
      is preserved unchanged.
- [ ] The `lumen::LogStore` trait signatures stay byte-identical
      to the prior tag (Gate 2 `cargo public-api`). The slice does
      NOT add or remove any `LogStore` trait method.
- [ ] The half-open `[start, end)` window from ADR-0047 § 3 is
      preserved unchanged.
- [ ] `query-http-common` (ADR-0054) remains the SOLE provider of
      the cap constants, the reason constants, the error envelope
      helper, the tenant seam, and the bounds parser. No new copies
      of any of them appear in `log-query-api`.

### Outcome KPIs

See `outcome-kpis.md` for the full table. Story-level summary:

- **Who**: SRE operators and automation clients of the log read
  API who hold a known regex pattern for a failure family and need
  to isolate records matching the family in one request.
- **Does what**: Issue narrowed read requests
  (`body_regex=<pattern>`) instead of running multiple
  `body_contains` queries or downloading the whole window and
  grepping client-side.
- **By how much**: The regex filter is HONEST — every record in
  the response is matched by the regex (per `Regex::is_match`); no
  record matched by the regex in the fixture is omitted.
- **Measured by**: Acceptance test in
  `crates/log-query-api/tests/slice_01_body_regex.rs` (DISTILL
  output) asserting (a) every returned record's body is matched by
  the regex, (b) no record in the fixture whose body matches the
  regex is omitted.
- **Baseline**: 100% of in-window records returned today on a
  no-filter request; today Maria runs 3-4 separate `body_contains`
  queries to approximate the regex behaviour.

### Technical Notes

- **Existing seam**: `lumen::LogStore::query_with(&tenant, range,
  &Predicate)` is unchanged. `lumen::Predicate` today carries
  `service`, `min_severity`, and `body_contains` (verified by
  direct read of `crates/lumen/src/predicate.rs:25-33`); the slice
  grows it with one more field.
- **Predicate extension shape** (FLAG 2): TWO candidate shapes:
  1. `body_regex: Option<Regex>` — the compiled regex stored in
     the predicate; HTTP handler compiles ONCE on parse, hands the
     compiled `Regex` to the predicate via a `body_regex(regex:
     Regex) -> Self` builder. `Predicate::matches` calls
     `regex.is_match(&record.body)`. Cost: one compile per request
     (paid at parse time, fail-fast on invalid); per-record match
     is the linear-time `is_match` call.
  2. `body_regex: Option<String>` — the raw pattern stored in the
     predicate; `Predicate::matches` compiles on every call. Cost:
     N compiles per request (one per record scanned), which
     dominates the per-record match cost. REJECTED on cost
     grounds.
  Recommendation: shape (1). Symmetric with `query-api`'s
  matrix-filter compile pattern (ADR-0046 Decision 3: "Compile the
  regex matchers ONCE, before the row scan"), confirmed by direct
  read of `crates/query-api/src/lib.rs:190-195`.
- **Parsing location**: The `body_regex` parse helper lives in
  `crates/log-query-api/src/lib.rs` alongside `parse_min_severity`
  and `parse_body_contains`. It enforces (a) empty rejection, (b)
  1024-byte cap, (c) `Regex::new` compile. All three failure modes
  return the same literal reason `"invalid body_regex"`. The lumen
  crate stays free of HTTP-shaped parsing; lumen takes a
  pre-compiled `Regex`.
- **Lumen `Cargo.toml` dependency**: `regex = "1"` is NOT today a
  direct dependency of `lumen` (verified by direct read of
  `crates/lumen/Cargo.toml`). The slice adds it; the workspace
  already pins `regex` to `=1.12.3` via `query-api`'s direct
  dependency (ADR-0046 Decision 1), so the new direct dependency
  resolves to the same version with zero `Cargo.lock` change. This
  IS a public-surface change on `lumen` (a new `Regex` type
  appears on the `Predicate::body_regex` builder signature) and
  the deps tree of `lumen` grows; FLAG 5 below records the
  ADR-0056 recommendation.
- **Order of checks** (mirrors and extends
  log-body-text-search-v0):
  1. Resolve tenant via
     `query_http_common::resolve_tenant_or_refuse` (UNCHANGED).
  2. Parse window via `query_http_common::parse_time_range`
     (UNCHANGED).
  3. Window-cap check (UNCHANGED).
  4. Parse `min_severity` if present (UNCHANGED).
  5. Parse `body_contains` if present (UNCHANGED).
  6. Parse `body_regex` if present (NEW). Empty / over-cap /
     compile-failure all return 400 with `invalid body_regex`. Store
     is NOT touched on these paths.
  7. Mutual-exclusion check (NEW). If BOTH `body_contains` AND
     `body_regex` are `Some`, return 400 with `specify body_regex
     or body_contains, not both`. Store is NOT touched on this path.
  8. Build the composed predicate (NEW dispatch arm). The dispatch
     grows from the current 4-arm cross product
     (`min_severity` x `body_contains`) to a 6-arm cross product
     (`min_severity` x exactly-one-of `{none, body_contains,
     body_regex}`). Two of the eight theoretical arms (both
     body filters present) are pruned by step 7.
  9. Result-cap check on the post-filter records vector (UNCHANGED).
  10. `success_response(records)`.
- **Composition with `min_severity`**: When both `min_severity`
  and `body_regex` are present, the composed predicate is
  conjunctive (`AND`). This matches `Predicate::matches` semantics
  exactly.
- **`Predicate` equality / `is_empty`**: `Regex` does NOT
  implement `PartialEq` or `Eq`, so the existing
  `#[derive(PartialEq, Eq)]` on `Predicate`
  (`crates/lumen/src/predicate.rs:24`) will break the moment a
  `Regex` field lands. DESIGN must either (a) drop `Eq` / `PartialEq`
  from the derive, (b) hand-implement `PartialEq` comparing the
  regex as_str() (cheap, but `Eq` is not honest because two
  patterns may behave equivalently), or (c) keep the predicate
  field as `Option<String>` and compile in the handler-call path
  (FLAG 2 shape 2; rejected on cost). Recommendation: drop
  `PartialEq` and `Eq` from the derive on `Predicate`. The trait
  is not used anywhere in the production code path; the lumen
  acceptance suite compares predicates by behaviour, not by
  equality. Verified by `grep` of the lumen tests.
- **Mutation targets** (handed to DESIGN as fertile ground for the
  Gate 5 mutants lumen workflow that landed in
  gate-5-mutants-lumen-v0 d96a807):
  - The regex match call: a `Regex::is_match` ->
    `Regex::is_match_at(.., 0)` mutant must be killed by a
    fixture where the match is in the MIDDLE of the body.
  - The case-sensitivity boundary: a mutant that compiles with
    the case-insensitive flag set by default (e.g.
    `RegexBuilder::new(p).case_insensitive(true).build()`) must
    be killed by the `kafka` != `KAFKA` scenario.
  - The empty-string rejection: a mutant that treats `Some("")`
    as `None` (the unfiltered-fallthrough mutant from
    log-body-text-search-v0) must be killed by the
    empty-string 400 scenario.
  - The invalid-regex 400: a mutant that catches `Regex::new`
    errors and falls through to "no filter" instead of returning
    400 must be killed by the invalid-syntax scenario.
  - The over-cap rejection: a `>` -> `>=` mutant on the
    1024-byte cap must be killed by an
    `accepts_input_at_exactly_the_cap` unit test (1024 is
    INCLUSIVELY accepted) and a `rejects_input_at_1025_bytes`
    unit test.
  - The redaction on every 400 arm: the reason text is a literal
    constant; a mutant that interpolates the raw value into the
    body must be killed by an explicit "body == literal envelope"
    assertion.
  - The mutual-exclusion 400: a mutant that drops the check (and
    silently AND-composes both body filters) must be killed by
    the mutual-exclusion scenario.
  - The order-of-checks: a mutant that calls the store BEFORE
    parsing `body_regex` must be killed by an assertion that the
    store is NOT touched on any 400 path.
  - The cross-tenant isolation arm: a mutant that resolves the
    tenant AFTER applying the filter must be killed by the
    cross-tenant scenario.

### Dependencies

- **Resolved**:
  - ADR-0047 (log-query-api contract).
  - ADR-0050 (read-side caps).
  - ADR-0052 (`min_severity` parameter).
  - ADR-0054 (`query-http-common` extraction; M-5).
  - ADR-0055 (`body_contains` parameter; immediate sibling).
  - ADR-0046 (`regex` crate use in `query-api` for label
    matchers; the grammar and compile-once pattern).
  - `lumen::LogStore::query_with` (`crates/lumen/src/store.rs:89`).
  - `lumen::Predicate::matches` conjunctive composition
    (`crates/lumen/src/predicate.rs:66-84`, post-`body_contains`).
  - `query_http_common::{MAX_RESULT_ROWS, MAX_WINDOW_SECONDS,
    REASON_WINDOW_TOO_LARGE, REASON_TOO_MANY_ROWS,
    REASON_MISSING_TENANT, REASON_INVALID_TIME_RANGE,
    parse_time_range, error_response, resolve_tenant_or_refuse}`.
  - `regex = "1"` (already direct in `query-api/Cargo.toml`; new
    direct in `lumen/Cargo.toml`; same version pin via
    `Cargo.lock`).
  - `gate-5-mutants-lumen` CI workflow (shipped in
    gate-5-mutants-lumen-v0, d96a807) — new
    `Predicate::body_regex` arm benefits automatically.
- **Tracked (not blockers)**: DESIGN flags 1-6 in
  `wave-decisions.md`.

## US-02 Unknown pattern returns the calm empty array

The acceptance content is covered by US-01 Scenario "An unmatched
pattern returns the calm empty array, never 404" and Domain Example
2. This story exists as a separately-named slice unit so the
carpaccio gate counts it as a distinct behavioural promise: the API
does NOT use 404 to signal an empty post-filter result.

### Elevator Pitch

- **Before**: Without a deliberate test, a `body_regex` filter that
  matches no record could plausibly return 404 (HTTP-as-resource
  semantics) or even a 500 if the empty-result arm is mishandled.
  Maria cannot distinguish "pattern absent from this window" from
  "the query was malformed" from "the platform is broken".
- **After**: A `body_regex` filter with no matches returns HTTP 200
  with the bare empty array `[]`. Maria reads "200 + `[]`" and knows
  the platform answered honestly: the pattern is not in any record's
  body in this window for this tenant.
- **Decision enabled**: Maria concludes the failure family is NOT
  in this window and widens the time range (or accepts the negative
  finding).

## US-03 Missing body_regex preserves today's behaviour

### Elevator Pitch

- **Before**: Marcus's automation, which today calls `/api/v1/logs`
  without any `body_regex` parameter (and may or may not carry
  `min_severity` and `body_contains`), MUST keep receiving the
  slice-prior response on every existing call. A change in the
  no-`body_regex` arm would break the installed base of scripts.
- **After**: The absence of `body_regex` deserialises as `None`
  and the handler keeps its prior dispatch arms (the existing
  4-arm cross product of `min_severity` x `body_contains`). The
  acceptance suite includes a byte-equality assertion against the
  slice-prior response shape.
- **Decision enabled**: Marcus does NOT update his script. The
  slice ships with zero broken clients.

This story is the no-regression contract. The acceptance evidence
is the "Parameter absent returns every record in the window"
scenario in US-01.

## US-04a Invalid regex syntax is a redacted 400

### Elevator Pitch

- **Before**: A malformed pattern (`foo(bar`, `[a-`, `\K`) is
  silently ambiguous. The platform might (a) crash, (b) hang
  trying to compile, (c) match every record by accident, (d)
  match no record by accident, or (e) leak parts of the pattern
  in an error message. None of these is a useful answer to
  Maria.
- **After**: An invalid pattern is REJECTED with HTTP 400 and the
  literal envelope
  `{"status":"error","error":"invalid body_regex"}`. The reason
  text is a static literal; the raw pattern is NEVER reflected.
  The store is NEVER touched. Maria sees the 400 and re-runs with
  a fixed pattern.
- **Decision enabled**: Maria distinguishes "I sent a bad
  pattern" (400) from "the pattern is not in this window" (200 +
  `[]`) from "the platform is broken" (500). She also gets the
  fail-fast guarantee: an invalid pattern never costs a store
  scan.

The acceptance evidence is the "An invalid regex pattern is a
redacted 400" scenario in US-01.

## US-04b Empty body_regex is the same redacted 400

### Elevator Pitch

- **Before**: `?body_regex=` could slip through serde as
  `Some("")`. The `regex` crate accepts the empty pattern as a
  Regex matching every position; without the explicit rejection
  the filter would silently match every record (observably
  indistinguishable from "no filter"). Maria would not know
  whether she dropped the pattern or whether her client
  serialised it wrong.
- **After**: An empty `body_regex` value is REJECTED with HTTP
  400 and the same literal envelope used for invalid syntax. The
  store is NEVER touched.
- **Decision enabled**: Maria recognises the typo, re-runs with
  the pattern she meant to send. The slice refuses ambiguity out
  loud, symmetric with `body_contains` and `min_severity`.

The acceptance evidence is the "An empty body_regex value is the
same redacted 400" scenario in US-01.

## US-04c Over-cap body_regex is the same redacted 400

### Elevator Pitch

- **Before**: An unbounded `body_regex` length lets a malicious
  client ship megabytes inside a query-string parameter. Even on
  a linear-time engine, a sufficiently exotic pattern over a
  large input can be slow; bounding the pattern length bounds
  the worst-case parse cost.
- **After**: A `body_regex` value whose byte length strictly
  exceeds 1024 returns HTTP 400 with the same literal envelope
  used for empty and invalid arms. The cap is INCLUSIVELY 1024
  (1024 served, 1025 refused); the value matches `body_contains`
  for consistency. The raw oversize value is NEVER reflected.
  The store is NEVER touched.
- **Decision enabled**: Maria's runbook learns the 1024-byte
  rule; the platform's abuse surface stays bounded.

The acceptance evidence is the "An over-cap body_regex value is
the same redacted 400" scenario in US-01.

## US-05 Case-sensitive matching is pinned by acceptance test

### Elevator Pitch

- **Before**: Maria might assume `body_regex=kafka` matches
  `KAFKA` (some regex engines fold case by default; `grep -i` is
  muscle memory). Without a documented test, the slice's
  case-sensitivity is folklore.
- **After**: An acceptance test asserts that `body_regex=kafka`
  returns the calm empty array against a fixture containing
  `KAFKA timeout`. The test IS the documentation: Maria reads
  the test and learns the rule before her first incident, and
  she learns the inline `(?i)` flag as the opt-in escape hatch.
- **Decision enabled**: Maria knows the platform's posture
  ("case-sensitive by default, `(?i)` for case-insensitive")
  from a place she will actually look (the acceptance suite). A
  future slice that adds a default case-folding flag does so as
  a documented behaviour change with its own ADR, NOT as a
  silent grammar drift.

The acceptance evidence is the "The match is case-sensitive by
default" scenario in US-01.

## US-06 body_contains and body_regex are mutually exclusive at slice 01

### Elevator Pitch

- **Before**: A client that constructs request URLs by query
  parameter concatenation could accidentally send BOTH
  `body_contains=` AND `body_regex=` on the same request. The
  semantic question "what does it mean to send both?" deserves a
  deliberate answer (intersection? union? error?), not a quiet
  AND-default that surprises the operator after the fact.
- **After**: A request carrying BOTH parameters returns HTTP 400
  with the literal envelope
  `{"status":"error","error":"specify body_regex or body_contains, not both"}`.
  The store is NEVER touched. The client gets an unambiguous
  error rather than a silent dispatch into "AND-compose both
  filters" or "the last one parsed wins".
- **Decision enabled**: The client fixes the URL construction
  bug and re-runs with exactly ONE of the two. A future slice
  MAY relax the rule once a real operator use case earns the
  testing surface (the dispatch grows from 6-arm to 8-arm and
  the `Predicate::matches` carries both arms); the slice 01
  carpaccio defers the question.

The acceptance evidence is the "Body_contains and body_regex are
mutually exclusive" scenario in US-01.

## US-07 Cross-tenant isolation holds for body_regex

### Elevator Pitch

- **Before**: A new filter is a new dimension along which the
  per-tenant isolation invariant could leak: a careless
  implementation might apply the regex filter ACROSS all
  tenants' records and then filter by tenant. Such a bug would
  not show up on a single-tenant fixture; it would only surface
  in production when a tenant queried a regex that matches a
  different tenant's logs.
- **After**: An acceptance test asserts that tenant B receives
  `[]` when querying for a regex that matches tenant A's records
  and matches no record in tenant B's window. The test pins the
  platform invariant (ADR-0047 § "Per-tenant isolation") against
  the new filter arm. The invariant is enforced by the EXISTING
  `query_with(&tenant, range, ...)` seam (the tenant is the
  first argument; the bucket lookup precedes any predicate
  evaluation).
- **Decision enabled**: Tenant B's operators trust the
  platform's multi-tenant promise without rereading the source.
  The slice ships with the invariant proved against the new arm,
  not assumed.

The acceptance evidence is the "Cross-tenant isolation" scenario
in US-01.

## Flags to DESIGN (do NOT decide in DISCUSS; recommendations recorded for DESIGN to pin)

See `wave-decisions.md` § "Flags to DESIGN" for the full table.
Brief summary (DESIGN reads the wave-decisions table for the full
reasoning):

1. **Regex compile location** — Recommended: handler-side compile
   at parse time (fail-fast 400 on invalid; store NEVER touched
   on compile failure). Symmetric with ADR-0046 Decision 3.
2. **Predicate field type** — Recommended: compiled `Option<Regex>`
   on `Predicate` (one compile per request; per-record
   `is_match` is linear-time). The alternative `Option<String>`
   (compile per record) is rejected on cost.
3. **Length cap value** — Recommended: 1024 bytes, INCLUSIVE,
   reusing the `body_contains` value.
4. **Mutual exclusion vs body_contains** — Recommended: mutually
   exclusive at slice 01; both present is 400 with the literal
   `specify body_regex or body_contains, not both`. Future
   slices MAY relax.
5. **ADR-0056** — Recommended: YES. The slice adds a new direct
   dependency to `lumen/Cargo.toml` (`regex = "1"`), and the
   `lumen::Predicate` public surface grows by one new pub
   builder method whose signature mentions a `Regex` type. Both
   are visible in `cargo public-api` diff.
6. **Anchoring / multiline defaults** — Recommended:
   `Regex::is_match` (unanchored, single-line) as the default;
   operators use inline flags (`^`, `$`, `(?m)`) for explicit
   anchoring or multiline. Symmetric with `query-api` label
   matchers.
