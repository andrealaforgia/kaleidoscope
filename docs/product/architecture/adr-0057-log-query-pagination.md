# ADR-0057 — log-query-api `limit` and `offset` pagination parameters

- **Status**: Accepted
- **Date**: 2026-05-30
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `log-query-pagination-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0047 (the lumen log-query-api contract; this ADR
  grows it by two optional parameters on the same route, reusing the
  bare JSON array success shape and the `{status:"error", error}`
  envelope verbatim; cited, NOT modified). ADR-0050 (the read-side
  Earned-Trust caps; the result cap continues to measure the post-filter
  vector before the page slice, and the refuse-not-truncate posture is
  extended to the page-size parameter; cited, NOT modified). ADR-0052,
  ADR-0055, ADR-0056 (the sibling optional read parameters `min_severity`,
  `body_contains`, `body_regex`; this slice follows the same
  parse-and-wire shape and the same redaction posture; cited, NOT
  modified). ADR-0054 (the `query-http-common` extraction; this slice is
  a further consumer of the shared cap constant and envelope helper;
  cited, NOT modified).

## Context

The lumen log-query-api (`crates/log-query-api/src/lib.rs`, route
`GET /api/v1/logs?start=&end=`) today returns one block of every matching
record up to the `MAX_RESULT_ROWS = 100000` cap. An on-call SRE who wants
a small first page, or who wants to scroll a multi-page result set one
screenful at a time, has no way to bound the response size or to skip to
the next page at the API: the workaround is to download the whole block
and trim client-side with `jq '.[:50]'`, paying the full payload cost.

The store already returns records in a stable, deterministic order:
`InMemoryLogStore::ingest` sorts each tenant bucket by
`observed_time_unix_nano` on every ingest (`crates/lumen/src/store.rs:136`,
a stable `sort_by_key`), and `query` / `query_with` iterate that bucket
in order and collect. The result-cap check already operates handler-side
on the returned vector (`crates/log-query-api/src/lib.rs:285`). This
gives a thin seam for pagination: a slice over the returned
`Vec<LogRecord>` needs no new sort and no store change.

Pagination grows the HTTP read contract by two optional parameters and
touches the ADR-0050 cap-interaction semantics (the `limit`-vs-cap
interaction, the refuse-not-truncate posture extended to page size).
ADRs in this repository are immutable; the growth therefore lands as a
new ADR with back-references. `ls docs/product/architecture/adr-0057*`
returns zero hits; 0057 is the next free number.

## Decision

### 1. Handler-side slice within the existing cap

`limit` and `offset` apply as `records.skip(offset).take(limit)` over the
`Vec<LogRecord>` the store returns, in the handler, within the existing
100000-row cap. The `lumen::LogStore` trait and `lumen::Predicate` are
NOT touched; no adapter is edited. Pagination is a windowing stage (HOW
MANY records, from WHICH position), not a filter (WHICH records match),
so it does not belong in `Predicate`. This mirrors how the result-cap
check already operates handler-side on the returned vector.

### 2. `limit` over the cap is rejected, not clamped

A `limit` strictly greater than `MAX_RESULT_ROWS` returns HTTP 400 with
`{"status":"error","error":"invalid limit"}`. The boundary is INCLUSIVE:
`limit=100000` is served, `limit=100001` is refused. The store is NEVER
touched on this path. This is the refuse-not-truncate posture of ADR-0050
Decision 3 extended to page size: a silent clamp would serve a different
page than the one requested.

### 3. `offset` is skip-based

`offset=N` drops the first N records of the ordered, post-filter result
set before `limit` is applied. `offset=0` is the first page. Skip-based
offset is honest over a fixed result set; it does not survive concurrent
ingest (a record ingested between page fetches may shift subsequent
pages). Cursor / keyset pagination is the deferred richer contract.

### 4. No default `limit`

The absence of `limit` returns every matched record up to the cap,
exactly as today; the default `offset` is 0. No default page size is
injected. This preserves backward compatibility for the installed base
of parameter-less clients; the 100000-row cap remains the only backstop.

### 5. Edge cases

`limit=0`, a negative `limit`, and a non-numeric `limit` each return
HTTP 400 with `"invalid limit"`. A negative or non-numeric `offset`
returns HTTP 400 with `"invalid offset"`. An `offset` past the end of the
result set returns HTTP 200 with the calm empty bare array `[]`, NEVER
404 and NEVER 400. The raw value is NEVER echoed on any 400 arm.

### 6. Cap-then-slice order

The handler order is: tenant -> bounds -> window cap -> filters ->
parse `limit`/`offset` -> store -> result cap (on the post-filter,
PRE-slice vector) -> page slice -> serialise. The result cap is measured
BEFORE the page slice, so a window whose matched set exceeds 100000 is
refused exactly as today, before any slice is reached.

### 7. Known limitation: pagination beyond 100000 records needs in-store paging

Because the cap is measured before the slice (Decision 6), handler-side
pagination does NOT permit scrolling beyond 100000 records. A window
whose post-filter matched set exceeds the cap is refused; an operator
with more than 100000 matches must narrow the window (a smaller time
range or a tighter filter) before paginating. In-store pagination
(extending `query_with` with `limit`/`offset` so the store returns only
the requested page) is the future answer; it is deferred because it
extends the `LogStore` trait and every adapter, which is larger than a
carpaccio slice.

## Consequences

### Positive

- The operator controls response size and scrolls forward one page at a
  time, instead of pulling the whole block and trimming with `jq`.
- For a fixed result set, successive pages partition it with no
  duplicate and no gap, resting on the store's stable ascending
  observed-time order; the partition is the slice's central correctness
  promise (pinned by an acceptance test).
- No new crate, no new dependency, no `Cargo.lock` diff; the change is
  confined to `crates/log-query-api/src/lib.rs` (two parse helpers, two
  private `LogsParams` fields, one slice expression).
- The `lumen::LogStore` trait and `lumen::Predicate` public surfaces, the
  error envelope, the tenancy seam, both caps, and every existing
  parameter-less client are preserved unchanged.
- Filter-before-page and tenant-scope-before-page are automatic: the
  vector the handler slices is already the post-filter, per-tenant set.

### Negative

- Pagination cannot scroll beyond 100000 records (Decision 7); an
  operator with a larger matched set must narrow the window. In-store
  paging is the deferred remedy.
- The in-memory slice is an O(n) `skip` over the post-filter vector, but
  `n <= MAX_RESULT_ROWS = 100000`, which is acceptable for v0; in-store
  paging would avoid materialising the skipped prefix.
- Skip-based offset does not survive concurrent ingest; a record
  ingested between page fetches may shift subsequent pages. Cursor-based
  paging is the deferred answer.

## Alternatives considered

### A. In-store pagination now (rejected)

Extend `lumen::LogStore::query_with` with `limit`/`offset` so the store
returns only the page. For: paging beyond the cap; no O(n) skip of a
materialised prefix. Against: it extends the `LogStore` trait and every
adapter (`InMemoryLogStore` and `FileBackedLogStore`), which is larger
than a carpaccio slice and changes the public store surface. Deferred to
a successor slice.

### B. Cursor-based / keyset pagination (rejected)

An opaque cursor token that survives concurrent ingest and avoids
offset drift. For: immunity to concurrent-ingest page drift. Against:
stateful, a richer contract than slice 01 needs; skip-based offset is
the lowest-surprise primitive and honest over a fixed result set.
Deferred until real demand surfaces.

### C. Clamp `limit` to the cap (rejected)

Silently serve 100000 when `limit` exceeds the cap. Against: it serves a
different page than the one requested, the read-side equivalent of a
buffered fsync that lies; it breaks the refuse-not-truncate posture of
ADR-0050 Decision 3. Reject is the honest answer.

### D. A default `limit` when absent (rejected)

Inject an implicit page size (for example `limit=1000`) when the
parameter is missing. Against: it silently breaks the installed base of
parameter-less clients (US-03 backward compatibility). The absence of
`limit` must return today's response byte-for-byte.

## References

- ADR-0050 (read-side caps; the result cap and refuse-not-truncate
  posture), UNCHANGED.
- ADR-0047 (the log-query-api read contract; envelope, redaction, bare
  JSON array), UNCHANGED.
- ADR-0052, ADR-0055, ADR-0056 (the sibling optional read parameters
  `min_severity`, `body_contains`, `body_regex`), UNCHANGED.
- ADR-0054 (`query-http-common`; the shared cap constant and envelope
  helper), UNCHANGED.
- Feature DISCUSS: `docs/feature/log-query-pagination-v0/discuss/`.
- Feature DESIGN: `docs/feature/log-query-pagination-v0/design/`.
