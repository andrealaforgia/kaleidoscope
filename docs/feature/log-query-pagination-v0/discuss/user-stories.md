<!-- markdownlint-disable MD024 -->

# User Stories: log-query-pagination-v0

Slice 01, thin. The log read endpoint `GET /api/v1/logs?start=&end=`,
which today already accepts the optional `min_severity` floor (ADR-0052),
the optional `body_contains` byte-substring filter (ADR-0055), and the
optional `body_regex` regular-expression filter (ADR-0056), grows TWO
further optional request parameters: `limit=<n>` and `offset=<n>`. The
two parameters let the operator scroll a large result set ONE page at a
time instead of receiving a single block all the way up to the
`MAX_RESULT_ROWS = 100_000` cap. Default behaviour (both parameters
absent) is unchanged: every in-window record (composed with any present
filters) is returned, exactly as today, up to the existing cap.

This is a brownfield carpaccio slice on top of an existing endpoint. The
walking skeleton is implicit in the slices that already shipped on
`/api/v1/logs` (read endpoint exists, durable store exists, tenant seam
exists, caps exist, severity floor exists, byte-substring filter exists,
regex filter exists); no greenfield skeleton is rebuilt.

The slice is the immediate sibling of `log-body-text-search-v0` and
`log-body-regex-search-v0` in SHAPE - parse + wire of optional
query-string parameters on the same route, reusing `query-http-common`
for the envelope, the caps, the tenant seam, and the bounds parser - but
it is DIFFERENT in KIND in one important respect: `limit` and `offset` do
NOT narrow WHICH records match (a filter question, answered by
`lumen::Predicate`); they narrow HOW MANY of the already-matched,
already-ordered records are returned and from WHICH position (a
windowing question, answered by a slice over the result vector). The
recommended slice 01 cut therefore applies `limit` and `offset`
HANDLER-SIDE, on the `Vec<LogRecord>` the store already returns in stable
`observed_time_unix_nano` order, WITHIN the existing 100_000-row cap. The
`lumen::LogStore` trait and `lumen::Predicate` are NOT extended. See
FLAG 1 below; the in-store alternative is deliberately deferred to a
successor slice.

## System Constraints (cross-cutting)

- The existing `MAX_WINDOW_SECONDS = 86_400` and
  `MAX_RESULT_ROWS = 100_000` caps from ADR-0050 are PRESERVED unchanged.
  Pagination does NOT remove, reorder, or weaken either cap. Both
  constants are consumed from `query_http_common::MAX_WINDOW_SECONDS` and
  `query_http_common::MAX_RESULT_ROWS` (ADR-0054); the slice MUST NOT
  re-declare either local to `log-query-api`. The 100_000-row cap remains
  the backstop: an operator may paginate WITHIN the cap; a window whose
  matched set exceeds the cap is refused, exactly as today, and the
  operator narrows the window (see FLAG 2 for the `limit > cap`
  interaction).
- The error envelope on rejected input is the existing
  `{"status":"error","error":"<reason>"}` shape, emitted via
  `query_http_common::error_response` (ADR-0054 / ADR-0047 Decision 1).
  No new envelope. No new status code. The slice MUST NOT re-implement
  the envelope locally.
- The fail-closed tenancy seam goes through
  `query_http_common::resolve_tenant_or_refuse` (ADR-0054). The slice
  MUST NOT re-implement tenant resolution locally.
- The error text MUST NOT echo the raw parameter value (ADR-0047
  redaction posture; symmetric with ADR-0050 Decision 7, ADR-0052
  Decision 1, ADR-0055 DD5, ADR-0056 PIN 4). The reason text on every
  400 arm is a static literal; the raw `limit` / `offset` value NEVER
  appears in the response body.
- The bare JSON array success shape (ADR-0047 Decision 1) is preserved.
  Pagination changes HOW MANY records appear and from WHICH position, not
  the shape of the response. The empty arm is `[]`, HTTP 200, NEVER 404
  (an offset past the end of the result set is a calm empty page, not an
  error; see PIN 4).
- The half-open `[start, end)` window from ADR-0047 § 3 is preserved
  unchanged.
- The store's record ordering - ascending `observed_time_unix_nano`,
  established on ingest (`crates/lumen/src/store.rs:136`) - is the STABLE
  ORDER over which `limit` and `offset` operate. Pagination honesty
  (PIN 3, US-04) depends on this ordering being deterministic for a fixed
  tenant and window; the slice does NOT introduce a new sort.
- The `min_severity`, `body_contains`, and `body_regex` parameters from
  the predecessor slices are PRESERVED unchanged. When any of them is
  present alongside `limit` / `offset`, the filters apply FIRST (at the
  store, via `Predicate`), and `limit` / `offset` apply to the
  POST-FILTER, post-order result vector (US-06). Pagination is the LAST
  stage of the pipeline.
- The recommended slice 01 cut does NOT alter the `lumen::LogStore`
  trait signatures and does NOT extend `lumen::Predicate` (FLAG 1).
  `query_with(&tenant, range, &predicate)` and `query(&tenant, range)`
  stay byte-identical to the prior tag; the slice composes with them and
  applies the page slice in the handler.

## PINs (confirmed against the source by DISCUSS; DESIGN to record verbatim)

The following pins are derived from a direct read of the
post-`body_regex` source tree. DISCUSS surfaces them so DESIGN does not
re-discover them.

### PIN 1: The store already returns records in a stable, deterministic order

`InMemoryLogStore::ingest` sorts each tenant bucket by
`observed_time_unix_nano` on every ingest
(`crates/lumen/src/store.rs:136`), and both `query` and `query_with`
iterate that bucket in order and collect (`store.rs:150-156`,
`store.rs:173-179`). The returned `Vec<LogRecord>` is therefore in
ascending observed-time order for a fixed tenant and window. This is the
order over which `offset` skips and `limit` takes; pagination honesty
(US-04) rests on it. The slice does NOT introduce a tie-breaker for
records that share an identical `observed_time_unix_nano`; the existing
stable-sort behaviour governs, and DESIGN should note that two records
with identical observed-time keep their relative ingest order under
`sort_by_key` (a stable sort).

### PIN 2: Handler-side slice is the thinnest cut; the store is untouched

The recommended cut applies `offset` and `limit` as a slice over the
`Vec<LogRecord>` the store returns, in the handler, AFTER the store call
and BEFORE (or as part of) the result-cap check. No `LogStore` trait
method changes; no `Predicate` field is added; no adapter is touched.
This mirrors the way the result-cap check itself already operates on the
returned vector handler-side (ADR-0050 Decision 4,
`crates/log-query-api/src/lib.rs:285`). The in-store alternative (extend
`query_with` with `limit`/`offset`) is more memory-efficient at very
large windows but extends the trait and every adapter; it is FLAG 1,
recommended as future work.

### PIN 3: Pagination honesty rests on a single, stable ordering

For a fixed tenant, window, and filter set, the result vector is
deterministic and stably ordered (PIN 1). Therefore the union of
`(offset=0, limit=N)` and `(offset=N, limit=N)` and so on covers the
full result set with NO duplicate and NO gap, provided the underlying
data does not change between requests. The slice does NOT promise
snapshot isolation across requests (a record ingested between page 1 and
page 2 may shift subsequent pages); it promises that, for a fixed result
set, the page slices partition it cleanly. This is the honest scope of a
skip/offset pagination contract (FLAG 3); cursor-based pagination, which
WOULD survive concurrent ingest, is deferred.

### PIN 4: An offset past the end of the result set is a calm empty page, NOT an error

When `offset` is greater than or equal to the number of matched records,
the page slice is empty. The response is HTTP 200 with the calm empty
bare array `[]`, NEVER HTTP 404 and NEVER HTTP 400. Rationale: asking
for "page 50 of a 3-page result" is a well-formed request that
legitimately has no rows; it is the same calm-empty posture the contract
already uses for a filter that matches nothing (ADR-0047 Decision 1,
ADR-0055, ADR-0056). `offset=0` is always valid (the first page). See
FLAG 5.

### PIN 5: `limit` and `offset` are independent optional parameters

Either may be present without the other. `limit` alone takes the first N
records (offset defaults to 0). `offset` alone skips the first N records
and returns the remainder up to the existing cap (limit defaults to
"unbounded within the cap" - i.e. no explicit page size, the cap is the
backstop). Both absent is today's behaviour exactly (US-03). DESIGN
pins the default-offset-is-0 and default-limit-is-absent semantics; see
FLAG 4.

### PIN 6: `limit = 0` semantics need a deliberate answer

`?limit=0` is syntactically a valid non-negative integer but semantically
asks for "a page of zero records". DISCUSS recommends treating `limit=0`
as INVALID (HTTP 400, `"invalid limit"`) rather than as a valid
empty-page request, because (a) an empty page carries no information an
absent request would not, (b) `limit=0` is far more likely a client bug
(an uninitialised page-size variable) than a deliberate ask, and (c)
refusing it out loud is symmetric with the refuse-not-ambiguity posture
the predecessor slices took for the empty `body_contains` / `body_regex`
values. `offset=0`, by contrast, is VALID (it is the first page). See
FLAG 5; DESIGN confirms.

## OUT of scope (DECLARED and DEFERRED)

The following are EXPLICITLY out for slice 01 and named so DESIGN does
not re-discover them as gaps:

- In-store pagination (extending `lumen::LogStore::query_with` with
  `limit` / `offset` so the store returns only the page). More efficient
  at very large windows; extends the trait and every adapter. FLAG 1;
  recommended as future work. Slice 01 paginates handler-side WITHIN the
  existing cap.
- Cursor-based / keyset pagination (an opaque cursor token that survives
  concurrent ingest and avoids the offset-drift of skip-based paging).
  FLAG 3; deferred. Slice 01 ships skip-based `offset`.
- A total-count header or a `next`/`prev` link envelope (e.g.
  `X-Total-Count`, RFC 5988 `Link` headers, or a `{data, pagination}`
  wrapper). The success shape stays the bare JSON array (ADR-0047
  Decision 1). An operator computes "is there a next page?" by observing
  whether the returned page is full (`len == limit`). A richer pagination
  envelope is a successor slice if real demand surfaces.
- A default page size when `limit` is absent (e.g. an implicit
  `limit=1000`). FLAG 4; recommended AGAINST for backward compatibility
  (US-03). The existing 100_000-row cap remains the only backstop.
- Snapshot isolation across paginated requests. The slice promises a
  clean partition of a FIXED result set (PIN 3), not immunity to
  concurrent ingest. Cursor-based paging (deferred) is the future answer
  to concurrent-ingest drift.
- Raising or per-pillar tuning of `MAX_RESULT_ROWS`. The 100_000 cap is
  preserved unchanged (ADR-0050 Decision 2).
- A new ADR written by DISCUSS. ADR drafting belongs to DESIGN (Morgan);
  DISCUSS surfaces the recommendation in FLAG 6 below (recommended:
  small ADR-0057 citing ADR-0050 without modifying it).

## US-01 Walking skeleton: `limit` returns the first N records

### Elevator Pitch

- **Before**: Maria Santos, on-call SRE for tenant `acme-prod`, runs a
  log query over a busy 10-minute window and the response is a single
  JSON block of every matching record up to the 100_000-row cap. She
  cannot ask for "just the first 50" to eyeball the most recent slice of
  activity without downloading the whole block and trimming client-side
  with `jq '.[:50]'`.
- **After**: Maria runs `curl
  'http://logs.kaleidoscope.acme.internal/api/v1/logs?start=1716200000&end=1716200600&limit=50'
  -H 'X-Tenant: acme-prod'` and the HTTP 200 body is a bare JSON array of
  exactly the FIRST 50 records, in the store's ascending
  `observed_time_unix_nano` order. The shape of each record is identical
  to today's response; only the row COUNT is bounded to 50.
- **Decision enabled**: Maria controls the size of the response. She
  asks for a small first page, eyeballs it, and decides whether to widen
  the page or narrow the window before pulling more.

### Problem

Maria Santos is the on-call SRE for tenant `acme-prod`. During an
incident she queries `/api/v1/logs` over a busy window and receives one
JSON block carrying every matching record up to the 100_000-row cap.
When she only wants to glance at the leading edge of the result set - the
first handful of records, to confirm the shape of the data before
committing to a larger pull - she has no way to bound the response size
at the API. She downloads the whole block and trims with `jq '.[:50]'`,
paying the full payload and bandwidth cost for 50 records she keeps. The
natural primitive is a `limit=<n>` parameter that bounds the returned
page to the first N records of the ordered result set.

### Who

- **Maria Santos** | SRE on `acme-prod`, mid-incident, terminal + curl +
  jq | Triage urgency: wants a small, fast first page to confirm the data
  shape before a larger pull.
- **Marcus Webb** | platform engineer building an automated log-scroller
  UI that fetches one fixed-size page at a time and renders it
  incrementally | Throughput motive: per-page latency and payload size
  dominate the request budget; a bounded page is the unit of work.
- **Priya Raman** | support engineer paging through a customer's log
  window in a terminal, who wants a screenful at a time rather than a
  10_000-line wall of text | Readability motive: a bounded page is the
  unit she can actually read.

### Solution

`GET /api/v1/logs` accepts an optional query-string parameter
`limit=<n>`. The handler:

1. Resolves the tenant, parses and caps the window, parses
   `min_severity` / `body_contains` / `body_regex` if present (ALL
   UNCHANGED from the predecessor slices).
2. Parses `limit` if present (NEW): a non-numeric, negative, or zero
   value is rejected HTTP 400 with the literal `invalid limit`
   (FLAG 5 / PIN 6); a value strictly greater than `MAX_RESULT_ROWS`
   is rejected HTTP 400 with `invalid limit` (FLAG 2). The store is
   NEVER touched on any of these arms.
3. Calls the existing store seam (`query` or `query_with`) for the
   tenant, window, and any filters - UNCHANGED.
4. Applies the result cap on the post-filter records vector (UNCHANGED;
   the cap is the backstop).
5. Takes the FIRST `limit` records of the ordered, post-filter vector
   (NEW handler-side slice). When `limit` is absent, takes the whole
   vector (UNCHANGED).
6. Serialises the bare JSON array (UNCHANGED).

The exact location of the page slice relative to the result-cap check,
and whether the slice happens handler-side or in-store, are DESIGN
pins (FLAG 1, FLAG 2); the user-visible behaviour is fixed here.

### Domain Examples

#### 1: Happy path - Maria asks for the first 3 records

Tenant `acme-prod` has eight records in `[1716200000s, 1716200600s)`,
returned in ascending observed-time order:

| `observed_time_unix_nano` | `body` |
|---|---|
| `1_716_200_005_000_000_000` | `checkout: heartbeat 1` |
| `1_716_200_010_000_000_000` | `kafka timeout connecting to broker-3` |
| `1_716_200_015_000_000_000` | `checkout: heartbeat 2` |
| `1_716_200_020_000_000_000` | `redis: GET timeout on key user-42` |
| `1_716_200_025_000_000_000` | `checkout: heartbeat 3` |
| `1_716_200_030_000_000_000` | `kafka request timed out after 30s` |
| `1_716_200_035_000_000_000` | `checkout: heartbeat 4` |
| `1_716_200_040_000_000_000` | `kafka: connection timed out (broker-7)` |

Maria runs `curl
'.../api/v1/logs?start=1716200000&end=1716200600&limit=3' -H 'X-Tenant:
acme-prod'`. Response is HTTP 200 with a bare JSON array of the FIRST
THREE records (at `t=5`, `t=10`, `t=15`), in ascending observed-time
order. The five later records are not in this page.

#### 2: Limit larger than the result set - Maria gets everything, calmly

Tenant `acme-prod` has the SAME eight records. Maria runs `limit=100`.
The result set has only eight records, fewer than the requested 100.
Response is HTTP 200 with a bare JSON array of ALL EIGHT records. A
limit larger than the available rows is NOT an error; the page is simply
the whole (shorter) result set. The response is byte-equal to the
no-limit response for this window.

#### 3: Invalid limit - `limit=0`, `limit=-5`, `limit=abc` are redacted 400s

Maria fat-fingers `limit=0` (an uninitialised page-size variable in her
script). Response is HTTP 400 with the literal envelope
`{"status":"error","error":"invalid limit"}`. The store is NEVER
touched. The same 400 fires for `limit=-5` (negative) and `limit=abc`
(non-numeric). The raw value NEVER appears in the response body. Maria
fixes the page size and re-runs.

### UAT Scenarios

#### Scenario: A limit returns the first N records in order

```gherkin
Given tenant "acme-prod" has eight records in the window [1716200000s, 1716200600s) in ascending observed-time order
When Maria GETs /api/v1/logs?start=1716200000&end=1716200600&limit=3
Then the status is 200
And the body is a bare JSON array of exactly the first three records in ascending observed_time order
And the fourth-through-eighth records do NOT appear in the response
```

#### Scenario: A limit larger than the result set returns the whole set

```gherkin
Given tenant "acme-prod" has eight records in the window
When Maria GETs /api/v1/logs over the window with limit=100
Then the status is 200
And the body is a bare JSON array of all eight in-window records
And the response is byte-equal to the no-limit response for the same inputs
```

#### Scenario: A zero limit is a redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly within the cap
When Maria GETs /api/v1/logs over the window with limit=0
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid limit"}
And the store is NEVER queried on this path
```

#### Scenario: A negative or non-numeric limit is a redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod"
And the window parses cleanly within the cap
When Maria GETs /api/v1/logs over the window with limit=-5
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid limit"}
And the body NEVER contains the substring "-5"
And the store is NEVER queried on this path
```

### Acceptance Criteria

- [ ] An optional query-string parameter `limit=<n>` is accepted on
      `GET /api/v1/logs`.
- [ ] When `limit` is present and valid, the response carries at most the
      first `limit` records of the ordered, post-filter result set, in
      ascending `observed_time_unix_nano` order.
- [ ] A `limit` larger than the number of matched records returns the
      whole (shorter) set; it is NOT an error.
- [ ] `limit=0`, a negative `limit`, and a non-numeric `limit` each
      return HTTP 400 with the literal envelope
      `{"status":"error","error":"invalid limit"}`. The store is NEVER
      touched on any of these arms.
- [ ] The error body NEVER echoes the raw `limit` value.
- [ ] When `limit` is absent, the response is identical to the
      slice-prior response (every matched record up to the cap).

### Outcome KPIs

See `outcome-kpis.md` for the full table. Story-level summary:

- **Who**: SRE operators and automation clients of the log read API who
  need a bounded first page rather than the whole result block.
- **Does what**: Issue `limit`-bounded read requests instead of pulling
  the whole block and trimming client-side with `jq '.[:N]'`.
- **By how much**: The first page carries EXACTLY `min(limit, matched)`
  records, in stable order; no record beyond position `limit` appears.
- **Measured by**: Acceptance test in
  `crates/log-query-api/tests/slice_01_pagination.rs` (DISTILL output)
  asserting the page is the first `limit` records in order.
- **Baseline**: 100% of matched records returned today on a no-limit
  request; today Maria trims with `jq '.[:50]'` client-side.

### Technical Notes

- **Existing seam**: `lumen::LogStore::query` and `query_with` are
  UNCHANGED. The page slice is applied handler-side on the returned
  `Vec<LogRecord>` (recommended; FLAG 1). The store returns records in
  stable ascending observed-time order (PIN 1,
  `crates/lumen/src/store.rs:136`).
- **Parse location**: a `parse_limit` helper lives in
  `crates/log-query-api/src/lib.rs` alongside `parse_min_severity`,
  `parse_body_contains`, and `parse_body_regex`. It enforces (a)
  numeric, (b) strictly positive (rejects 0, negative, non-numeric all
  with `"invalid limit"`), (c) at most `MAX_RESULT_ROWS` (FLAG 2). All
  rejections return the same literal reason.
- **Parameter type on `LogsParams`**: `limit: Option<String>` (parsed in
  the handler, symmetric with the existing `Option<String>` parameters)
  so the parse-time 400 fires before any store work. DESIGN may instead
  use a typed `Option<u64>` with a serde-rejection-to-400 mapping; the
  user-visible behaviour is fixed by the AC.
- **Slice-vs-cap order** (FLAG 2): if `limit` is capped at
  `MAX_RESULT_ROWS`, then a `limit`-bounded page can never itself exceed
  the cap; the existing result-cap check still measures the post-filter
  vector before the page slice (so a window whose matched set exceeds the
  cap is still refused, exactly as today - the operator narrows the
  window). DESIGN pins the exact order.

### Dependencies

- **Resolved**: ADR-0047 (contract), ADR-0050 (caps), ADR-0052
  (`min_severity`), ADR-0054 (`query-http-common`), ADR-0055
  (`body_contains`), ADR-0056 (`body_regex`).
  `lumen::LogStore::query` / `query_with` (`crates/lumen/src/store.rs`).
  `query_http_common::{MAX_RESULT_ROWS, error_response,
  resolve_tenant_or_refuse, parse_time_range}`.
- **Tracked (not blockers)**: DESIGN flags 1-6 in `wave-decisions.md`.

## US-02 `offset` skips to the next page

### Elevator Pitch

- **Before**: Having seen the first page with `limit=50`, Maria has no
  way to ask for the SECOND page - records 51 through 100. The API has no
  notion of "skip the first N".
- **After**: Maria runs `curl
  '.../api/v1/logs?start=...&end=...&limit=50&offset=50' -H 'X-Tenant:
  acme-prod'` and the HTTP 200 body is the records from the 51st through
  the 100th of the ordered result set.
- **Decision enabled**: Maria scrolls forward through the result set one
  page at a time, advancing `offset` by the page size on each request.

### Problem

Maria has bounded her first page with `limit` (US-01) but the result set
is larger than one page. To see the records beyond the first page she
needs a way to SKIP the records she has already seen. The natural
primitive is an `offset=<n>` parameter that skips the first N records of
the ordered result set before applying `limit`.

### Who

- **Maria Santos** | SRE scrolling forward through a multi-page result
  set during triage | wants the next screenful.
- **Marcus Webb** | log-scroller UI advancing `offset` by the page size
  on each scroll event | the offset is the scroll position.

### Domain Examples

#### 1: Happy path - second page of size 3

Tenant `acme-prod` has the same eight records as US-01 Example 1. Maria
runs `limit=3&offset=3`. Response is HTTP 200 with the records at
positions 4, 5, 6 (`t=20`, `t=25`, `t=30`). The first three (already
seen) and the last two are not in this page.

#### 2: Offset alone - skip the first two, take the rest (up to the cap)

Maria runs `offset=2` with no `limit`. Response is HTTP 200 with records
at positions 3 through 8 (six records), the remainder of the set after
skipping the first two.

#### 3: Offset past the end - calm empty page

Maria runs `offset=100` against the eight-record set. Position 100 is
past the end. Response is HTTP 200 with the calm empty bare array `[]`,
NEVER 404 (PIN 4). Maria reads "200 + `[]`" and knows she has scrolled
past the last page.

### UAT Scenarios

#### Scenario: Offset skips to the next page

```gherkin
Given tenant "acme-prod" has eight records in the window in ascending observed-time order
When Maria GETs /api/v1/logs over the window with limit=3 and offset=3
Then the status is 200
And the body is a bare JSON array of exactly the fourth, fifth, and sixth records in order
And the first three records do NOT appear
```

#### Scenario: Offset alone returns the remainder after the skip

```gherkin
Given tenant "acme-prod" has eight records in the window
When Maria GETs /api/v1/logs over the window with offset=2 and no limit
Then the status is 200
And the body is a bare JSON array of the third-through-eighth records in order
```

#### Scenario: Offset past the end is a calm empty page, never 404

```gherkin
Given tenant "acme-prod" has eight records in the window
When Maria GETs /api/v1/logs over the window with offset=100
Then the status is 200
And the body is the calm empty bare array []
And the status is NOT 404
And the status is NOT 400
```

### Acceptance Criteria

- [ ] An optional query-string parameter `offset=<n>` is accepted on
      `GET /api/v1/logs`.
- [ ] When `offset` is present, the response skips the first `offset`
      records of the ordered, post-filter result set before applying any
      `limit`.
- [ ] `offset=0` is valid and returns the first page (no records
      skipped).
- [ ] An `offset` greater than or equal to the number of matched records
      returns HTTP 200 with the calm empty bare array `[]`, NEVER 404 and
      NEVER 400.
- [ ] When `offset` is absent, no records are skipped (the default offset
      is 0).

### Outcome KPIs

See `outcome-kpis.md`. Story-level summary:

- **Who**: operators and UIs scrolling forward through a multi-page
  result set.
- **Does what**: advance `offset` to retrieve successive pages.
- **By how much**: the page at `offset=k, limit=n` is exactly records
  `k+1` through `k+n` of the ordered set (or fewer at the tail).
- **Measured by**: acceptance test asserting the offset page is the
  correct slice of the ordered set.
- **Baseline**: no second-page primitive exists today.

### Technical Notes

- `parse_offset` helper alongside `parse_limit`. Enforces (a) numeric,
  (b) non-negative (`offset=0` valid; negative and non-numeric rejected
  with `"invalid offset"`). There is NO upper cap on `offset` itself; an
  oversized offset simply yields the calm empty page (PIN 4).
- Handler applies `offset` then `limit` as a single slice over the
  ordered post-filter vector:
  `records.into_iter().skip(offset).take(limit_or_max).collect()`.
  DESIGN pins the exact expression and the saturation behaviour when
  `offset` exceeds the vector length (which yields an empty iterator,
  hence `[]`).

### Dependencies

- Same as US-01. US-02 depends on US-01's `limit` parse for the combined
  `limit`+`offset` page; the two parameters are independent at the parse
  layer (PIN 5) but composed at the slice.

## US-03 Missing `limit`/`offset` preserves today's behaviour

### Elevator Pitch

- **Before**: Marcus's automation, which today calls `/api/v1/logs`
  without any `limit` or `offset` parameter (and may carry
  `min_severity` / `body_contains` / `body_regex`), MUST keep receiving
  the slice-prior response on every existing call. A change in the
  no-pagination arm would break the installed base of scripts.
- **After**: The absence of `limit` AND `offset` deserialises as `None`
  for both, and the handler returns every matched record up to the cap,
  exactly as today. The acceptance suite includes a byte-equality
  assertion against the slice-prior response shape.
- **Decision enabled**: Marcus does NOT update his script. The slice
  ships with zero broken clients.

### Problem

This story is the no-regression contract. Every existing client of
`/api/v1/logs` calls it without `limit` or `offset`. Their responses MUST
be byte-identical to the slice-prior responses for the same inputs. A
default page size, or a changed dispatch when the parameters are absent,
would silently break them.

### Who

- **Marcus Webb** | automation owner whose scripts predate this slice |
  must not be updated.
- **Every existing curl / jq / Prism client** | unchanged.

### Domain Examples

#### 1: No pagination parameters - every matched record, exactly as today

Marcus's automation calls
`.../api/v1/logs?start=1716200000&end=1716200600` every 60 seconds. The
script is NOT updated when this slice ships. The response is byte-equal
to the response it received the day before slice 01. The
backward-compatibility promise is honoured.

#### 2: No pagination, with a filter present - filter applies, no paging

Marcus calls `.../api/v1/logs?start=...&end=...&min_severity=WARN` with
no `limit` and no `offset`. The response is every WARN-or-above record in
the window, up to the cap, exactly as the day before slice 01. The
filter behaviour is unchanged; pagination is simply absent.

### UAT Scenarios

#### Scenario: No pagination parameters returns every matched record (default unchanged)

```gherkin
Given tenant "acme-prod" has eight records in the window
When Marcus GETs /api/v1/logs?start=1716200000&end=1716200600 with NO limit and NO offset
Then the status is 200
And the body is a bare JSON array of all eight in-window records
And the response is byte-equal to the slice-prior response for the same inputs
```

#### Scenario: No pagination with a filter present is unchanged

```gherkin
Given tenant "acme-prod" has eight records, three of which are WARN-or-above
When Marcus GETs /api/v1/logs over the window with min_severity=WARN and NO limit and NO offset
Then the status is 200
And the body is a bare JSON array of exactly the three WARN-or-above records
And the response is byte-equal to the slice-prior response for the same inputs
```

### Acceptance Criteria

- [ ] When neither `limit` nor `offset` is present, the response is
      byte-equal to the slice-prior response for the same inputs.
- [ ] The no-pagination dispatch path is the existing one (no default
      page size is injected).
- [ ] Every existing acceptance scenario on `/api/v1/logs`
      (`slice_01_logs_read`, `slice_02_caps`, `slice_01_severity_filter`,
      `slice_01_body_contains`, `slice_01_body_regex`) stays green
      unchanged.

### Outcome KPIs

See `outcome-kpis.md` (K1: behaviour invariance). Baseline: today's
no-parameter response is the gold reference.

### Technical Notes

- The no-pagination path is `(limit, offset) == (None, None)`: skip the
  slice entirely, serialise the whole post-filter vector. The default
  offset is 0 and there is NO default limit (FLAG 4). The cap stays the
  only backstop.

### Dependencies

- Same as US-01.

## US-04 Pagination honesty: pages partition the set with no duplicate and no gap

### Elevator Pitch

- **Before**: Without a deliberate test, a careless slice implementation
  could overlap pages (page 2 repeats the last record of page 1) or drop
  a record between pages (an off-by-one on `offset` or `limit`). Maria
  cannot trust that scrolling shows her every record exactly once.
- **After**: An acceptance test asserts that, for a fixed result set, the
  concatenation of `(offset=0, limit=N)`, `(offset=N, limit=N)`,
  `(offset=2N, limit=N)`, ... equals the full result set in order, with
  NO duplicate and NO gap. The test IS the honesty guarantee.
- **Decision enabled**: Maria trusts that paging through the set shows
  every record exactly once, so a count or a scan across pages is
  correct.

### Problem

Skip/offset pagination is correct only if the page boundaries align
exactly: page `k` ends where page `k+1` begins, with no overlap and no
omission. An off-by-one in the `skip`/`take` expression would silently
corrupt every multi-page scan. The honesty must be PINNED by an
acceptance test over a fixed, stably-ordered result set (PIN 1, PIN 3).

### Who

- **Maria Santos** | counting or scanning across pages during triage |
  needs each record exactly once.
- **Marcus Webb** | log-scroller UI rendering successive pages | a
  duplicate or a gap is a visible bug.

### Domain Examples

#### 1: Three pages of size 3 over an eight-record set partition it cleanly

Tenant `acme-prod` has the eight records of US-01 Example 1. Maria (or
the UI) fetches three pages: `(offset=0,limit=3)` -> records 1-3;
`(offset=3,limit=3)` -> records 4-6; `(offset=6,limit=3)` -> records 7-8
(a short final page). Concatenated in order, the three pages equal all
eight records, each appearing exactly once, in ascending observed-time
order. No record is repeated; none is missing.

#### 2: Page boundary at an exact multiple - no overlap

With `limit=4`: `(offset=0,limit=4)` -> records 1-4;
`(offset=4,limit=4)` -> records 5-8. Record 4 appears ONLY in page 1;
record 5 appears ONLY in page 2. The boundary is clean (offset is the
count already consumed, not the last index seen).

#### 3: Records sharing an observed-time keep a stable relative order across pages

Tenant `acme-prod` has two records at the identical
`observed_time_unix_nano` `1_716_200_010_000_000_000` (ingested in a
known order A then B). Across any page boundary that falls between them,
A precedes B consistently (PIN 1: `sort_by_key` is stable). The
partition holds even with observed-time ties.

### UAT Scenarios

#### Scenario: Successive pages partition the result set with no duplicate and no gap

```gherkin
Given tenant "acme-prod" has eight records in the window in a fixed, stable order
When the pages (offset=0,limit=3), (offset=3,limit=3), (offset=6,limit=3) are fetched in turn
Then each page is HTTP 200
And the ordered concatenation of the three pages equals all eight records exactly
And no record appears in more than one page
And no in-window record is absent from the union of the pages
```

#### Scenario: A page boundary at an exact multiple does not overlap

```gherkin
Given tenant "acme-prod" has eight records in the window in a fixed, stable order
When the pages (offset=0,limit=4) and (offset=4,limit=4) are fetched
Then the fourth record appears ONLY in the first page
And the fifth record appears ONLY in the second page
And the union of the two pages is all eight records with no duplicate
```

### Acceptance Criteria

- [ ] For a fixed result set, the ordered concatenation of successive
      pages `(offset=k*N, limit=N)` for k = 0, 1, 2, ... equals the full
      result set, in order.
- [ ] No record appears in more than one page of a partition.
- [ ] No matched record is absent from the union of the pages covering
      the set.
- [ ] `offset` counts records consumed (page `k` of size `N` is
      `offset=k*N`), so a page boundary at an exact multiple does not
      overlap or skip.
- [ ] Records sharing an identical `observed_time_unix_nano` keep a
      stable relative order across page boundaries (PIN 1).

### Outcome KPIs

See `outcome-kpis.md` (K2: pagination honesty). This is the slice's
central correctness promise.

### Technical Notes

- The honesty rests on PIN 1 (stable order) and PIN 3 (fixed result set
  partitions cleanly). DESIGN pins the `skip(offset).take(limit)`
  semantics: `offset` is the number of records to drop from the front
  (NOT a 1-based index), so page `k` of size `N` begins at `offset=k*N`.
- The slice does NOT promise snapshot isolation across requests (PIN 3);
  the honesty guarantee is over a FIXED result set. Concurrent-ingest
  drift is the deferred cursor-pagination concern (FLAG 3).

### Dependencies

- Depends on US-01 (`limit`) and US-02 (`offset`).

## US-05a Invalid `limit` is a redacted 400

### Elevator Pitch

- **Before**: A malformed `limit` (`abc`, `-5`, `0`) is silently
  ambiguous. The platform might (a) treat it as no limit and return the
  whole block, (b) saturate to some default, or (c) panic on the parse.
  None of these is a useful answer.
- **After**: An invalid `limit` is REJECTED with HTTP 400 and the literal
  envelope `{"status":"error","error":"invalid limit"}`. The reason is a
  static literal; the raw value is NEVER reflected. The store is NEVER
  touched. Maria sees the 400 and re-runs with a valid page size.
- **Decision enabled**: Maria distinguishes "I sent a bad page size"
  (400) from "this page is empty" (200 + `[]`) from "the platform is
  broken" (500). She also gets the fail-fast guarantee: an invalid
  `limit` never costs a store scan.

### Problem

`limit` is operator-supplied and may be non-numeric, negative, or zero.
Each must be refused out loud at parse time, before the store is touched,
with a redacted 400. `limit=0` in particular (PIN 6) is treated as
invalid: an empty page carries no information an absent request would
not, and a zero is far more likely a client bug than a deliberate ask.

### Who

- **Maria Santos** | hand-typing a `limit` and occasionally fat-fingering
  it | needs a clear, fast rejection.
- **Marcus Webb** | automation that may emit an uninitialised page-size
  variable (`limit=0`) | needs the bug surfaced, not swallowed.

### Domain Examples

#### 1: Zero limit - `limit=0` is invalid

Marcus's script emits `limit=0` from an uninitialised variable. Response
is HTTP 400 with `{"status":"error","error":"invalid limit"}`. The store
is NEVER touched. (PIN 6: `limit=0` is invalid, not a valid empty page.)

#### 2: Negative limit - `limit=-5` is invalid

Maria types `limit=-5`. Response is HTTP 400 with the same literal. The
raw `-5` NEVER appears in the response body.

#### 3: Non-numeric limit - `limit=abc` is invalid

Maria types `limit=abc`. Response is HTTP 400 with the same literal. The
store is NEVER touched.

### UAT Scenarios

#### Scenario: A zero limit is refused out loud

```gherkin
Given the handler resolves a valid tenant "acme-prod" and the window parses within the cap
When Maria GETs /api/v1/logs over the window with limit=0
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid limit"}
And the store is NEVER queried on this path
```

#### Scenario: A negative limit is refused and redacted

```gherkin
Given the handler resolves a valid tenant "acme-prod" and the window parses within the cap
When Maria GETs /api/v1/logs over the window with limit=-5
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid limit"}
And the body NEVER contains the substring "-5"
And the store is NEVER queried on this path
```

#### Scenario: A non-numeric limit is refused

```gherkin
Given the handler resolves a valid tenant "acme-prod" and the window parses within the cap
When Maria GETs /api/v1/logs over the window with limit=abc
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid limit"}
And the store is NEVER queried on this path
```

### Acceptance Criteria

- [ ] `limit=0` returns HTTP 400 with
      `{"status":"error","error":"invalid limit"}` (PIN 6; FLAG 5).
- [ ] A negative `limit` returns the same 400.
- [ ] A non-numeric `limit` returns the same 400.
- [ ] The error body NEVER echoes the raw `limit` value.
- [ ] The store is NEVER touched on any invalid-`limit` path.

### Outcome KPIs

See `outcome-kpis.md` (K4: invalid params return 400 fast, no store
hit).

### Technical Notes

- `parse_limit(raw) -> Result<u64, &'static str>` returns
  `Err("invalid limit")` for non-numeric, `<= 0`, and over-cap inputs
  (the over-cap arm is US-05c / FLAG 2). The `0`-is-invalid rule is
  PIN 6 / FLAG 5; DESIGN confirms.

### Dependencies

- Same as US-01.

## US-05b Invalid `offset` is a redacted 400

### Elevator Pitch

- **Before**: A malformed `offset` (`abc`, `-1`) is silently ambiguous.
  The platform might treat a negative offset as zero, wrap it, or panic.
- **After**: An invalid `offset` is REJECTED with HTTP 400 and the literal
  envelope `{"status":"error","error":"invalid offset"}`. The raw value
  is NEVER reflected. The store is NEVER touched. `offset=0` is VALID
  (the first page) and is NOT rejected.
- **Decision enabled**: Maria distinguishes a bad scroll position (400)
  from an empty page past the end (200 + `[]`).

### Problem

`offset` is operator-supplied and may be non-numeric or negative. Each
must be refused out loud at parse time, before the store is touched, with
a redacted 400. Unlike `limit`, `offset=0` is VALID - it is the first
page - and a large offset past the end is a calm empty page (PIN 4), NOT
an error. Only negative and non-numeric values are rejected.

### Who

- **Maria Santos** | hand-typing an `offset` | needs a clear rejection
  for a malformed value but a calm empty page for an over-large one.
- **Marcus Webb** | UI computing `offset` arithmetically | a negative
  offset signals a computation bug.

### Domain Examples

#### 1: Negative offset - `offset=-1` is invalid

Maria types `offset=-1`. Response is HTTP 400 with
`{"status":"error","error":"invalid offset"}`. The store is NEVER
touched. The raw `-1` NEVER appears in the response.

#### 2: Non-numeric offset - `offset=abc` is invalid

Maria types `offset=abc`. Response is HTTP 400 with the same literal. The
store is NEVER touched.

#### 3: Zero offset - `offset=0` is valid (the first page)

Maria runs `offset=0&limit=3`. Response is HTTP 200 with the first three
records. `offset=0` is the first page, NOT a rejected value.

### UAT Scenarios

#### Scenario: A negative offset is refused and redacted

```gherkin
Given the handler resolves a valid tenant "acme-prod" and the window parses within the cap
When Maria GETs /api/v1/logs over the window with offset=-1
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid offset"}
And the body NEVER contains the substring "-1"
And the store is NEVER queried on this path
```

#### Scenario: A non-numeric offset is refused

```gherkin
Given the handler resolves a valid tenant "acme-prod" and the window parses within the cap
When Maria GETs /api/v1/logs over the window with offset=abc
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid offset"}
And the store is NEVER queried on this path
```

#### Scenario: A zero offset is valid and returns the first page

```gherkin
Given tenant "acme-prod" has eight records in the window in ascending observed-time order
When Maria GETs /api/v1/logs over the window with offset=0 and limit=3
Then the status is 200
And the body is a bare JSON array of the first three records in order
```

### Acceptance Criteria

- [ ] A negative `offset` returns HTTP 400 with
      `{"status":"error","error":"invalid offset"}`.
- [ ] A non-numeric `offset` returns the same 400.
- [ ] `offset=0` is VALID and returns the first page (not rejected).
- [ ] The error body NEVER echoes the raw `offset` value.
- [ ] The store is NEVER touched on any invalid-`offset` path.

### Outcome KPIs

See `outcome-kpis.md` (K4).

### Technical Notes

- `parse_offset(raw) -> Result<u64, &'static str>` returns
  `Err("invalid offset")` for non-numeric and negative inputs; accepts
  `0` and any non-negative integer. There is NO upper cap on `offset`;
  an over-large offset yields the calm empty page (PIN 4 / US-02
  Example 3), NOT a 400.

### Dependencies

- Same as US-02.

## US-05c `limit` over the result cap is a redacted 400

### Elevator Pitch

- **Before**: An unbounded `limit` lets a client ask for, say,
  `limit=5000000` - far above the 100_000-row cap the platform refuses to
  serve in a single response. Without a deliberate answer the handler
  might clamp silently (serve 100_000 and pretend that was the ask) or
  attempt to slice beyond the cap.
- **After**: A `limit` strictly greater than `MAX_RESULT_ROWS` (100_000)
  returns HTTP 400 with the literal envelope
  `{"status":"error","error":"invalid limit"}`. The store is NEVER
  touched. The cap interaction is honest: the platform refuses, never
  silently truncates (ADR-0050 Decision 3).
- **Decision enabled**: Maria learns the 100_000 ceiling on page size
  from the response, the same way she learns the window cap; the
  refuse-not-truncate posture is consistent across the read side.

### Problem

The platform already refuses to serve more than 100_000 rows in one
response (ADR-0050 Decision 2/3: refuse, never truncate). A `limit`
greater than the cap is therefore a contradiction: the operator is asking
for a page the platform has already committed never to serve whole.
DISCUSS recommends rejecting it out loud (HTTP 400, `"invalid limit"`)
rather than clamping silently, for consistency with the
refuse-not-truncate posture of ADR-0050. This is FLAG 2; DESIGN confirms
reject vs clamp.

### Who

- **Maria Santos** | who may type a large round number as a page size |
  needs the ceiling surfaced, not silently applied.
- **Marcus Webb** | automation that computes a page size and could
  overshoot the cap | needs the overshoot surfaced as a bug.

### Domain Examples

#### 1: Limit at exactly the cap - `limit=100000` is served

Maria runs `limit=100000` against a window with fewer than 100_000
matched records. Response is HTTP 200 with the whole (shorter) set. The
boundary is INCLUSIVE: a `limit` of exactly `MAX_RESULT_ROWS` is valid
(symmetric with the cap boundary in ADR-0050 Decision 2, which is `>`
not `>=`).

#### 2: Limit over the cap - `limit=100001` is a redacted 400

Maria runs `limit=100001`. Response is HTTP 400 with
`{"status":"error","error":"invalid limit"}`. The store is NEVER touched.
The raw value NEVER appears in the response.

#### 3: Far-over-cap limit - `limit=5000000` is the same redacted 400

Marcus's automation overshoots with `limit=5000000`. Same 400, same
literal. The platform refuses the page size; it does NOT clamp to
100_000 and pretend.

### UAT Scenarios

#### Scenario: A limit at exactly the cap is served

```gherkin
Given tenant "acme-prod" has eight records in the window
When Maria GETs /api/v1/logs over the window with limit=100000
Then the status is 200
And the body is a bare JSON array of all eight in-window records
```

#### Scenario: A limit over the cap is a redacted 400

```gherkin
Given the handler resolves a valid tenant "acme-prod" and the window parses within the cap
When Maria GETs /api/v1/logs over the window with limit=100001
Then the status is 400
And the body is the existing error envelope {"status":"error","error":"invalid limit"}
And the body NEVER contains the substring "100001"
And the store is NEVER queried on this path
```

### Acceptance Criteria

- [ ] A `limit` of exactly `MAX_RESULT_ROWS` (100_000) is VALID (the
      boundary is inclusive).
- [ ] A `limit` strictly greater than `MAX_RESULT_ROWS` returns HTTP 400
      with `{"status":"error","error":"invalid limit"}` (FLAG 2:
      reject, not clamp).
- [ ] The store is NEVER touched on the over-cap-`limit` path.
- [ ] The error body NEVER echoes the raw `limit` value.

### Outcome KPIs

See `outcome-kpis.md` (K5: cap interaction honest).

### Technical Notes

- The over-cap check lives inside `parse_limit`, comparing against
  `query_http_common::MAX_RESULT_ROWS`. The boundary is `>` (inclusive at
  the cap), symmetric with ADR-0050 Decision 2. FLAG 2 records the
  reject-vs-clamp decision; DISCUSS recommends reject.
- Mutation target: a `>` -> `>=` mutant on the over-cap check must be
  killed by a unit test pinning `limit=100000` accepted and
  `limit=100001` rejected.

### Dependencies

- Same as US-01. Depends on `query_http_common::MAX_RESULT_ROWS`.

## US-06 Pagination composes with the existing filters

### Elevator Pitch

- **Before**: The filters (`min_severity`, `body_contains`, `body_regex`)
  narrow WHICH records match; pagination narrows HOW MANY are returned.
  Without a deliberate test, the order of operations is folklore: does
  `limit=10` take the first 10 records OF THE WINDOW (then filter) or the
  first 10 OF THE FILTERED SET? The two give different answers.
- **After**: An acceptance test asserts that `min_severity=WARN&limit=10`
  returns the first 10 WARN-or-above records - filters apply FIRST, then
  pagination. The page is over the POST-FILTER, ordered set.
- **Decision enabled**: Maria paginates an ALREADY-FILTERED result set;
  her page contains only records that match her filter, exactly as she
  expects.

### Problem

Pagination is the LAST stage of the pipeline: tenant -> window -> filters
-> order -> page. If `limit`/`offset` applied BEFORE the filter, a page
could contain zero matching records (the first 10 records of the window
might all be filtered out), which is surprising and useless. The slice
pins filter-before-page: the page is taken over the post-filter, ordered
vector.

### Who

- **Maria Santos** | paginating a severity-filtered or regex-filtered
  result set during triage | expects each page to contain only matching
  records.
- **Marcus Webb** | UI that combines a filter chip with a page scroll |
  the page must be of the filtered set.

### Domain Examples

#### 1: `min_severity=WARN&limit=2` - first two WARN-or-above records

Tenant `acme-prod` has the eight records of US-01 Example 1, of which
four are ERROR (kafka/redis lines) and four are INFO (heartbeats). Maria
runs `min_severity=WARN&limit=2`. The filter keeps the four ERROR
records; `limit=2` takes the first two of THOSE (the records at `t=10`
and `t=20`). The INFO heartbeats are excluded by the filter and never
counted toward the page.

#### 2: `body_regex=kafka.*timeout&offset=1&limit=1` - second regex match

Tenant `acme-prod` has three records matching `kafka.*timeout` (at
`t=10`, `t=30`, `t=40`). Maria runs
`body_regex=kafka.%2Atimeout&offset=1&limit=1`. The filter keeps the
three matches; `offset=1&limit=1` takes the SECOND of those (the record
at `t=30`). Pagination operates over the post-filter set only.

#### 3: Filter excludes everything - page is empty

Maria runs `min_severity=FATAL&limit=10` against a window with no FATAL
records. The filter keeps zero records; the page of the empty set is the
calm empty bare array `[]`, HTTP 200, regardless of `limit`.

### UAT Scenarios

#### Scenario: limit applies to the post-filter set, not the raw window

```gherkin
Given tenant "acme-prod" has eight records, four of which are ERROR and four INFO, in ascending observed-time order
When Maria GETs /api/v1/logs over the window with min_severity=WARN and limit=2
Then the status is 200
And the body is a bare JSON array of exactly the first two ERROR records in order
And no INFO record appears in the response
And no ERROR record beyond the second appears in the response
```

#### Scenario: offset and limit page the post-filter regex set

```gherkin
Given tenant "acme-prod" has three records whose body matches "kafka.*timeout" in ascending observed-time order
When Maria GETs /api/v1/logs over the window with body_regex=kafka.%2Atimeout, offset=1, and limit=1
Then the status is 200
And the body is a bare JSON array of exactly the second kafka-timeout record
And the first and third kafka-timeout records do NOT appear
```

#### Scenario: A filter that excludes everything yields a calm empty page

```gherkin
Given tenant "acme-prod" has no FATAL records in the window
When Maria GETs /api/v1/logs over the window with min_severity=FATAL and limit=10
Then the status is 200
And the body is the calm empty bare array []
```

### Acceptance Criteria

- [ ] `limit` and `offset` apply to the POST-FILTER, ordered result set,
      not to the raw window.
- [ ] The pipeline order is tenant -> window -> filters -> order -> page.
- [ ] `min_severity=WARN&limit=N` returns the first N WARN-or-above
      records.
- [ ] `body_regex=<p>&offset=k&limit=n` returns records `k+1`..`k+n` of
      the regex-matched set.
- [ ] A filter that matches nothing yields the calm empty page `[]`
      regardless of `limit`/`offset`.

### Outcome KPIs

See `outcome-kpis.md` (K2: honesty over the post-filter set).

### Technical Notes

- Because the recommended cut applies pagination handler-side AFTER the
  store returns (PIN 2), and the store already applies the predicate
  (filters) inside `query_with`, filter-before-page is automatic: the
  `Vec<LogRecord>` the handler slices is ALREADY the post-filter set. No
  extra wiring is needed; the slice operates on whatever vector the
  store returned. DESIGN confirms the result-cap-then-slice order
  (FLAG 2).

### Dependencies

- Depends on US-01, US-02, and the existing filter slices (ADR-0052,
  ADR-0055, ADR-0056).

## US-07 Cross-tenant isolation holds under pagination

### Elevator Pitch

- **Before**: A new windowing stage is a new dimension along which the
  per-tenant isolation invariant could leak: a careless implementation
  might paginate across a shared vector before the tenant scope is
  applied. Such a bug would not show on a single-tenant fixture; it would
  surface only when a tenant's page boundary fell across another tenant's
  records.
- **After**: An acceptance test asserts that tenant B's paginated request
  returns only tenant B's records (or `[]`), never a record from tenant
  A, regardless of `limit`/`offset`. The invariant is enforced by the
  EXISTING `query`/`query_with(&tenant, ...)` seam (the tenant is the
  first argument; the bucket lookup precedes any slice).
- **Decision enabled**: Tenant B's operators trust the multi-tenant
  promise without rereading the source.

### Problem

Pagination operates over the result vector the store returns. The store
returns a PER-TENANT vector (`query`/`query_with` take `&tenant` first
and look up that tenant's bucket, `crates/lumen/src/store.rs:143`,
`store.rs:166`). The page slice therefore operates on tenant-scoped data
by construction. The invariant must be PINNED against the new arm: a
paginated request under tenant B never returns tenant A's records.

### Who

- **Maria Santos** | holding the `globex-staging` credential | must never
  see `acme-prod` records via a page boundary.
- **Tenant B's operators** | trust the platform's isolation promise |
  unchanged.

### Domain Examples

#### 1: Tenant B paginates and sees only its own records

Tenant `acme-prod` has eight records in the window; tenant
`globex-staging` has three. Maria, holding the `globex-staging`
credential, runs `limit=2&offset=0`. Response is HTTP 200 with the first
TWO of `globex-staging`'s THREE records. No `acme-prod` record appears,
regardless of the page boundary.

#### 2: Tenant B's offset past its own end is a calm empty page (not tenant A's data)

Tenant `globex-staging` has three records. Maria runs `offset=10` under
`globex-staging`. Position 10 is past `globex-staging`'s three records.
Response is HTTP 200 with `[]` - NOT a spillover into `acme-prod`'s
records at positions 4-10 of some shared vector. The offset is over
`globex-staging`'s scoped set only.

### UAT Scenarios

#### Scenario: A paginated request under tenant B never returns tenant A's records

```gherkin
Given tenant "acme-prod" has eight records in the window
And tenant "globex-staging" has three records in the window
When Maria GETs /api/v1/logs over the window with limit=2 and offset=0 under tenant "globex-staging"
Then the status is 200
And the body is a bare JSON array of exactly the first two globex-staging records
And no record from tenant "acme-prod" appears in the response
```

#### Scenario: Tenant B's offset past its own end is a calm empty page, not tenant A's data

```gherkin
Given tenant "acme-prod" has eight records in the window
And tenant "globex-staging" has three records in the window
When Maria GETs /api/v1/logs over the window with offset=10 under tenant "globex-staging"
Then the status is 200
And the body is the calm empty bare array []
And no record from tenant "acme-prod" appears in the response
```

### Acceptance Criteria

- [ ] A paginated request under tenant B returns only tenant B's records
      (or `[]`), never a record from tenant A, for any `limit`/`offset`.
- [ ] `offset` and `limit` operate over the per-tenant scoped result
      vector, never a cross-tenant shared vector.
- [ ] An `offset` past tenant B's own result set returns `[]`, never
      tenant A's records at that position.

### Outcome KPIs

See `outcome-kpis.md` (K2 honesty applies per-tenant).

### Technical Notes

- The isolation is enforced by the EXISTING `query`/`query_with`
  per-tenant bucket lookup (`crates/lumen/src/store.rs:143`,
  `store.rs:166`); the handler slices the per-tenant vector the store
  returned. No new isolation logic is added; the invariant is inherited
  and PINNED against the new arm.
- Mutation target: a mutant that applies the page slice BEFORE the
  per-tenant scope (or against a shared vector) is killed by the
  cross-tenant scenario.

### Dependencies

- Depends on US-01, US-02, and the existing per-tenant isolation
  invariant (ADR-0047 § "Per-tenant isolation").

## Flags to DESIGN (do NOT decide in DISCUSS; recommendations recorded for DESIGN to pin)

See `wave-decisions.md` § "Flags to DESIGN" for the full table and
reasoning. Brief summary:

1. **Handler-side vs in-store pagination** - Recommended: handler-side
   slice over the returned `Vec<LogRecord>` WITHIN the existing cap (no
   `LogStore` trait change, no `Predicate` extension; parse + wire only).
   In-store pagination (extend `query_with` with `limit`/`offset`) is
   more efficient at very large windows but extends the trait and every
   adapter; deferred to a successor slice.
2. **`limit` over the cap: reject vs clamp** - Recommended: REJECT
   (HTTP 400, `"invalid limit"`) for consistency with the
   refuse-not-truncate posture of ADR-0050 Decision 3. Clamp (silently
   serve 100_000) is the rejected alternative.
3. **Offset semantics: skip-based vs cursor-based** - Recommended:
   skip-based (`offset=N` drops the first N of the ordered post-filter
   set). Cursor/keyset pagination (survives concurrent ingest) is
   deferred.
4. **Default `limit` when absent** - Recommended: NO default (the
   absence of `limit` returns every matched record up to the cap, exactly
   as today; US-03 backward compatibility). The 100_000-row cap remains
   the only backstop. An explicit default (e.g. `limit=1000`) is the
   rejected alternative.
5. **`limit=0` and offset-past-end semantics** - Recommended: `limit=0`
   is INVALID (HTTP 400, `"invalid limit"`; PIN 6); `offset` past the
   end of the result set is a calm empty page (HTTP 200, `[]`; PIN 4),
   NOT an error.
6. **ADR-0057 yes/no** - Recommended: YES, a small ADR. Pagination
   touches the ADR-0050 cap-interaction semantics (the `limit`-vs-cap
   interaction, the refuse-not-truncate posture extended to page size)
   and grows the HTTP read contract by two optional parameters; it
   merits a durable record. The ADR cites ADR-0050 (and ADR-0047,
   ADR-0054, ADR-0055, ADR-0056) without modifying any.
