# DESIGN Decisions - log-query-pagination-v0

Morgan (nw-solution-architect), DESIGN wave, propose mode, application
scope. This slice grows `GET /api/v1/logs` by two optional query-string
parameters, `limit` and `offset`, applied as a handler-side slice over
the `Vec<LogRecord>` the store already returns in stable
`observed_time_unix_nano` order. It is the immediate sibling in SHAPE of
`log-body-text-search-v0` (ADR-0055) and `log-body-regex-search-v0`
(ADR-0056) - parse plus wire of optional parameters on the same route -
but DIFFERENT in KIND: pagination is a windowing stage over the result
vector, not a filter over `lumen::Predicate`. The six DISCUSS flags are
pinned below.

## DESIGN Decisions

### DD1: Handler-side slice within the existing cap (pins FLAG 1)

`limit` and `offset` apply as `skip(offset).take(limit)` over the
`Vec<LogRecord>` the store returns, in the handler, within the existing
100000-row cap. The `lumen::LogStore` trait and `lumen::Predicate` are
NOT touched: no trait method changes, no predicate field is added, no
adapter is edited. Pure parse plus wire in
`crates/log-query-api/src/lib.rs`.

Rationale: this is the thinnest cut, and it mirrors the way the
result-cap check ALREADY operates handler-side on the returned vector
(`lib.rs:285`, `records.len() > MAX_RESULT_ROWS`). Pagination is a
windowing question (HOW MANY of the matched records, from WHICH
position), answered by a slice over the result vector, not a filter
question (WHICH records match), which is what `Predicate` answers. The
store already returns records in a stable, deterministic order
(`store.rs:136`, `sort_by_key` on every ingest), so the slice needs no
new sort. In-store pagination (extend `query_with` with `limit`/`offset`
so the store returns only the page) is more memory-efficient at very
large windows but extends the trait and every adapter; it is annotated
as future work below and in ADR-0057. It is deliberately deferred.

### DD2: `limit` over the cap is rejected, not clamped (pins FLAG 2)

A `limit` strictly greater than `MAX_RESULT_ROWS` (100000) returns
HTTP 400 with the literal envelope
`{"status":"error","error":"invalid limit"}`. The boundary is
INCLUSIVE: `limit=100000` is served, `limit=100001` is refused. The
store is NEVER touched on the over-cap path.

Rationale: consistency with the refuse-not-truncate posture of ADR-0050
Decision 2/3. A silent clamp (serve 100000 and pretend that was the ask)
is the read-side equivalent of a buffered fsync that lies: the operator
asks for a page the platform has already committed never to serve whole,
and the honest answer is to refuse out loud rather than to quietly serve
a different page than the one requested. Clamp is the rejected
alternative.

### DD3: `offset` is skip-based (pins FLAG 3)

`offset=N` drops the first N records of the ordered, post-filter result
set before `limit` is applied. The page slice is
`records.into_iter().skip(offset).take(limit_or_max).collect()`.

Rationale: the simplest, lowest-surprise primitive, and honest over a
FIXED result set (the union of `(offset=0, limit=N)`,
`(offset=N, limit=N)`, ... covers the set with no duplicate and no gap,
resting on the store's stable order). The slice does NOT promise
snapshot isolation across requests: a record ingested between page 1 and
page 2 may shift subsequent pages. Cursor / keyset pagination (an opaque
token that survives concurrent ingest and avoids offset drift) is the
richer contract, deferred to a successor slice once real demand
surfaces.

### DD4: No default `limit` when absent (pins FLAG 4)

The absence of `limit` returns every matched record up to the cap,
exactly as today. The default `offset` is 0 (no records skipped); there
is NO default `limit`. The `(limit, offset) == (None, None)` path is the
existing dispatch path, byte-unchanged; no default page size is
injected.

Rationale: backward compatibility (US-03). An implicit default page size
(for example `limit=1000`) would silently break the installed base of
scripts that fetch the whole block. The 100000-row cap remains the only
backstop. An explicit default is the rejected alternative.

### DD5: `limit=0` is invalid; `offset` past the end is a calm empty page (pins FLAG 5)

`limit=0` returns HTTP 400 with `{"status":"error","error":"invalid
limit"}`. A negative or non-numeric `limit` returns the same 400. A
negative or non-numeric `offset` returns HTTP 400 with
`{"status":"error","error":"invalid offset"}`. `offset=0` is VALID (the
first page). An `offset` greater than or equal to the number of matched
records returns HTTP 200 with the calm empty bare array `[]`, NEVER 404
and NEVER 400.

Rationale: `limit=0` (PIN 6) carries no information an absent request
would not, and a zero is far more likely a client bug (an uninitialised
page-size variable) than a deliberate ask; refusing it out loud is
symmetric with the empty-`body_contains` / empty-`body_regex` posture of
ADR-0055 / ADR-0056. Offset-past-end (PIN 4): "page 50 of a 3-page
result" is a well-formed request that legitimately has no rows, the same
calm-empty posture the contract already uses for a filter that matches
nothing.

### DD6: ADR-0057 is written, citing ADR-0050 without modifying it (pins FLAG 6)

A small ADR-0057 records the pagination contract: the two new optional
parameters, the handler-side cut, the cap-then-slice order, and the
known limitation that handler-side pagination cannot page beyond 100000
records. ADR-0057 cites ADR-0050 (caps), ADR-0047 (contract), ADR-0052 /
ADR-0055 / ADR-0056 (sibling read parameters), and ADR-0054
(`query-http-common`); none is modified. `ls
docs/product/architecture/adr-0057*` returns zero hits; 0057 is the next
free number.

Rationale: pagination touches the ADR-0050 cap-interaction semantics
(the `limit`-vs-cap interaction, the refuse-not-truncate posture
extended to page size) and grows the HTTP read contract by two optional
parameters; it merits a durable, cross-referenced record.

## Reuse Analysis

| Component | File | Decision | Justification |
|-----------|------|----------|---------------|
| `LogsParams` | `crates/log-query-api/src/lib.rs` | EXTEND (add `limit: Option<String>`, `offset: Option<String>`) | Natural additive extension parallel to the existing `min_severity` / `body_contains` / `body_regex` `Option<String>` fields. A missing parameter deserialises as `None` and the handler keeps its prior behaviour (US-03). `Option<String>` so the parse-time 400 fires before any store work, symmetric with the existing parameters. |
| `parse_limit`, `parse_offset` | `crates/log-query-api/src/lib.rs` (new free fns) | NEW | Mirror the `parse_body_contains` / `parse_body_regex` shape (`fn parse_x(raw: &str) -> Result<_, &'static str>`). No existing helper parses a bounded non-negative integer; the severity / body helpers parse different value classes. Extending one of them would conflate unrelated value domains. ~15 LOC each. |
| `query_http_common::MAX_RESULT_ROWS` | `crates/query-http-common/src/lib.rs` | REUSE | The over-cap `limit` check (US-05c) compares against the shared cap constant; the slice re-declares no `MAX_*` constant locally (ADR-0054 single-source posture, KPI-K3). |
| `query_http_common::error_response` | `crates/query-http-common/src/lib.rs` | REUSE | Every `limit` / `offset` 400 goes through the shared envelope helper; no new envelope, no inline error body (KPI-K3). |
| `query_http_common::resolve_tenant_or_refuse`, `parse_time_range` | `crates/query-http-common/src/lib.rs` | REUSE | The fail-closed tenancy seam and the bounds parser are reused verbatim; not re-implemented. |
| `lumen::LogStore` trait | `crates/lumen/src/store.rs` | UNCHANGED | Handler-side slice (DD1); no in-store pagination. `query` / `query_with(&tenant, range, &predicate)` stay byte-identical to the prior tag. The store already returns the stable-ordered, per-tenant vector the handler slices. |
| `lumen::Predicate` | `crates/lumen/src/predicate.rs` | UNCHANGED | Pagination is a windowing stage, not a filter (DD1); no predicate field is added. |

No unjustified CREATE NEW. The two new helpers are the only new
components and each is a direct mirror of an existing parse helper in the
same file.

## Cap interaction

This is the central pin of the slice and Morgan pins it explicitly.

Under the handler-side cut (DD1), the existing 100000-row result cap
applies to the result set PRE-slice. The store returns up to the cap; the
result-cap 400 (`records.len() > MAX_RESULT_ROWS`, `lib.rs:285`) is
measured on the post-filter vector BEFORE the page slice; the page slice
runs only AFTER the cap check has passed.

The handler order is PINNED as:

1. `resolve_tenant_or_refuse` (fail-closed 401 - UNCHANGED).
2. `parse_time_range` (400 before the store - UNCHANGED).
3. Window cap (400 if `end - start > MAX_WINDOW_SECONDS` - UNCHANGED).
4. Parse filters: `min_severity`, `body_contains`, mutual-exclusion
   check, `body_regex` (each a 400 arm before the store - UNCHANGED).
5. Parse `limit` and `offset` (NEW; each a parse-time 400 before the
   store: invalid limit, invalid offset, over-cap limit).
6. `store.query` / `store.query_with` (UNCHANGED).
7. Result cap check (400 if `records.len() > MAX_RESULT_ROWS` -
   UNCHANGED; measured on the post-filter, PRE-slice vector).
8. Page slice `skip(offset).take(limit)` (NEW; runs only after the cap
   check passes).
9. `success_response` (200, bare JSON array, `[]` when the page is
   empty - UNCHANGED).

Honest consequence, documented in ADR-0057: handler-side pagination does
NOT permit scrolling beyond 100000 records. A window whose post-filter
matched set exceeds 100000 is refused at step 7, exactly as today,
BEFORE any page slice is reached; an operator with more than 100000
matches must narrow the window (a smaller time range or a tighter
filter) before paginating. This is a known limitation of slice 01;
in-store pagination (deferred, DD1) is the future answer that would let
the store page beyond the cap by returning only the requested page.

## Architecture Summary

- **Pattern**: modular monolith, hexagonal (ports-and-adapters). The
  `lumen::LogStore` driven port and the tenant seam are the only
  collaborators; the new logic is two pure parse helpers plus one slice
  expression, all in the driving adapter (`log-query-api`).
- **Paradigm**: Rust idiomatic - data plus free functions; no
  inheritance, no `dyn` where generics suffice (matches CLAUDE.md and the
  existing handler shape).
- **Key components**: `LogsParams` (extended), `parse_limit` (new),
  `parse_offset` (new), the handler `skip(offset).take(limit)` slice
  (new), all in `crates/log-query-api/src/lib.rs`. No new crate, no new
  module.
- **Earned-Trust**: no new driven adapter, no new external dependency, no
  new I/O boundary. The existing ADR-0047 startup probe runs unchanged.
  The two new parse helpers are pure functions over `&str` with no
  effects; their contract is enforced by inline unit tests (boundary,
  zero-rejection, redaction) plus the DISTILL acceptance suite. No new
  probe surface is introduced because no new dependency on the lying
  world (filesystem, clock, subprocess, vendor SDK) is added.

## Technology Stack

No new dependency. The slice uses only the standard library
(`str::parse::<usize>`, `Iterator::skip`, `Iterator::take`) plus the
already-present `axum` / `serde` deserialisation and the in-workspace
`query-http-common`. `usize::from_str` is the entire numeric-parse
surface; a leading `-` makes the string non-parseable as `usize`, which
is exactly the rejection the contract wants (DD5). No `Cargo.toml` edit
in any crate.

## DEVOPS Handoff

- **No new crate.** The change is confined to
  `crates/log-query-api/src/lib.rs` (two helpers, two `LogsParams`
  fields, the parse arms, the slice expression).
- **No new dependency.** No `Cargo.toml` change anywhere; no
  `Cargo.lock` diff. Standard-library numeric parse and iterator
  adapters only.
- **`lumen` is NOT touched.** No trait method, no predicate field, no
  adapter edit. Gate 2 `cargo public-api` on `lumen` shows zero drift
  (the `LogStore` trait and `Predicate` public surfaces are
  byte-identical to the prior tag). `LogsParams` is private, so the two
  new fields do not appear in any public-api diff.
- **Gate 5 coverage.** The existing `gate-5-mutants-log-query-api`
  workflow picks up `parse_limit`, `parse_offset`, and the slice
  expression via `cargo mutants --in-diff origin/main`. Primary mutation
  targets: the `>` -> `>=` over-cap boundary (US-05c), the zero-rejection
  on `limit` (US-05a / PIN 6), the `skip` / `take` off-by-one (US-04
  honesty), the per-tenant-scope-before-slice order (US-07), the
  no-store-call order on the invalid-parse arms (K4). No
  `gate-5-mutants-lumen` involvement (lumen unchanged).
- **Slim DEVOPS.** No new workflow, no new build target, no new
  external-integration contract test (no third-party API is consumed; the
  store call is an in-process trait method against the first-party
  `FileBackedLogStore`). The existing CI gates apply unchanged.
- **External integrations**: none. No consumer-driven contract test
  recommendation.

## Upstream Changes

None. No DISCOVER or prior-wave assumption is changed. The slice is
purely additive on top of the existing `/api/v1/logs` contract and the
existing `lumen` and `query-http-common` seams, all of which were read
directly during this DESIGN wave and confirmed to behave as DISCUSS
described (the store returns all matching records ordered by
`observed_time_unix_nano` with no in-store limit/offset; the result cap
fires at `lib.rs:285` on the post-filter vector). The DISCUSS
recommendations on all six flags are accepted as pinned above.
