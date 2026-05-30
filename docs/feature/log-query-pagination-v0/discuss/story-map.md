# Story Map: log-query-pagination-v0

## Backbone (user activity sequence)

The operator's activity over `GET /api/v1/logs` is a left-to-right
pipeline. Pagination adds ONE new stage (Slice) at the tail and ONE new
gate (Parse) at the head, leaving the existing stages byte-identical.

```text
  Parse  ───────────────►  Slice  ───────────────►  Verify
  ─────                    ─────                     ──────
  Read the page-request    Window the ordered,       Trust the page is
  parameters and refuse    post-filter result set:   honest: the pages
  the malformed ones out   skip `offset`, take       partition the set
  loud, before the store   `limit`, within the cap   with no duplicate,
  is touched.              and per-tenant scope.      no gap, no leak.
```

| Activity | Backbone step | Stories |
|---|---|---|
| **Parse** | Accept and validate `limit` / `offset`; refuse malformed values fast, before the store | US-05a (invalid limit), US-05b (invalid offset), US-05c (limit over cap) |
| **Slice** | Apply `offset` then `limit` over the ordered, post-filter, per-tenant vector | US-01 (limit first N), US-02 (offset skips), US-03 (defaults unchanged), US-06 (composes with filters), US-07 (cross-tenant) |
| **Verify** | Pin the honesty invariant: pages partition the set cleanly | US-04 (pagination honesty) |

The three activities are NOT three releases; they are the three phases a
single request passes through. The walking skeleton crosses all three in
ONE thin slice.

## Walking skeleton

**US-01 - `limit` returns the first N records.**

US-01 is the minimum end-to-end slice that exercises the full Parse ->
Slice -> Verify pipeline:

- **Parse**: it introduces `parse_limit` and the first
  `"invalid limit"` 400 arm (the negative/zero/non-numeric guard).
- **Slice**: it introduces the handler-side `take(limit)` over the
  ordered post-filter vector.
- **Verify**: its happy-path acceptance test asserts the page is exactly
  the first N records in order - the seed of the honesty invariant US-04
  generalises.

US-01 ships a complete, demonstrable behaviour: an operator runs
`?limit=3` and sees the first three records. Every other story extends
this one - `offset` (US-02) adds the front skip; the defaults (US-03)
pin the absent-parameter path; honesty (US-04) generalises the
partition; composition (US-06) and isolation (US-07) pin the invariants
against existing seams. None of them is meaningful without US-01's slice
primitive landing first.

This is a brownfield carpaccio slice: the walking skeleton for the
endpoint itself shipped long ago (read endpoint, durable store, tenant
seam, caps, filters all exist). US-01 is the walking skeleton OF THIS
SLICE, not of the platform.

## Carpaccio slicing

The recommended cut applies pagination HANDLER-SIDE over the returned
`Vec<LogRecord>`, within the existing 100_000-row cap (FLAG 1). This is
the thinnest cut that delivers the operator value:

- It does NOT extend the `lumen::LogStore` trait.
- It does NOT extend `lumen::Predicate`.
- It does NOT touch either store adapter.
- It is pure parse + wire inside `crates/log-query-api/src/lib.rs`: two
  new parse helpers (`parse_limit`, `parse_offset`), two new
  `LogsParams` fields, one new handler-side slice expression, two new
  literal 400 reason classes.

The whole slice is one crafter dispatch (well under a day): the parse
helpers mirror `parse_body_contains` exactly in shape, and the slice
expression is a single `skip(offset).take(limit)` over a vector the
handler already holds.

### Taste tests

- **Thin?** Yes - no new components, no new crate, no new trait method,
  no new adapter. Two parse helpers and one slice expression.
- **New abstraction shipped first?** No new abstraction is needed; the
  slice reuses the existing returned-vector seam (the same vector the
  result-cap check already operates on, `lib.rs:285`).
- **Disproves a pre-commitment?** Yes - US-04 (pagination honesty)
  disproves "skip/offset paging is correct" if the page boundaries
  overlap or gap; the partition test is the named hypothesis.
- **Production-data acceptance criterion?** The acceptance fixtures use
  realistic SRE log bodies (kafka/redis timeout strings, heartbeats),
  not synthetic `record-1` placeholders; the honesty test uses an
  eight-record set partitioned into real pages.
- **Two slices identical except for scale?** No - each story is a
  distinct behavioural promise (limit, offset, defaults, honesty,
  invalid-limit, invalid-offset, over-cap, composition, isolation).

## Priority Rationale

Priority order is by (a) what the slice primitive depends on, (b)
learning leverage (pin the riskiest invariant early), and (c) the
operator's natural workflow (bound a first page, then scroll).

| Order | Story | Why here |
|---|---|---|
| 1 | **US-01** `limit` (walking skeleton) | The slice primitive. Nothing pages without `take(limit)` landing first. Highest dependency leverage. |
| 2 | **US-05a** invalid `limit` | Ships WITH US-01: the `parse_limit` guard (zero/negative/non-numeric 400) is the same helper US-01 introduces. Fail-fast posture pinned immediately. |
| 3 | **US-05c** `limit` over cap | Ships WITH US-01: the over-cap arm of `parse_limit`. Pins the refuse-not-truncate cap interaction (FLAG 2) - the riskiest semantic decision - early. |
| 4 | **US-02** `offset` | The second primitive (front skip). Depends on US-01's slice expression; completes the page-windowing. |
| 5 | **US-05b** invalid `offset` | Ships WITH US-02: the `parse_offset` guard. Pins `offset=0` valid, negative/non-numeric 400. |
| 6 | **US-04** pagination honesty | The central correctness invariant. Generalises US-01 + US-02 into the partition guarantee. Highest learning leverage: disproves "skip/offset paging is correct" if it fails. |
| 7 | **US-03** defaults unchanged | The no-regression contract. Pinned once the paging path exists, to prove the absent-parameter path is byte-equal to today. |
| 8 | **US-06** composes with filters | Pins filter-before-page over the existing predicate seam. Automatic under the handler-side cut (PIN 2) but PINNED by test. |
| 9 | **US-07** cross-tenant isolation | Pins the multi-tenant invariant against the new windowing stage. Inherited from the per-tenant store seam but PINNED by test. |

US-01, US-05a, US-05c form the walking-skeleton cluster (one parse
helper, one slice expression, the cap interaction). US-02 + US-05b add
the offset primitive. US-04 is the honesty keystone. US-03, US-06, US-07
are invariant-pinning stories over seams the slice does not change.

## Scope Assessment: PASS

9 stories, 1 bounded context (`log-query-api`, reusing `query-http-common`
and the existing `lumen` seams unchanged under the recommended cut),
0 walking-skeleton integration points beyond the existing
endpoint/store/tenant/cap seams. Estimated effort: under one day of
crafter dispatch (two parse helpers mirroring `parse_body_contains`, one
`skip().take()` slice expression, two literal reason classes).

Oversized signals check:
- Stories: 9 - within the right-size envelope (the nine are thin
  behavioural promises over ONE parameter pair, several of them
  invariant-pins over unchanged seams; they do NOT each carry their own
  component).
- Bounded contexts / modules: 1 (`log-query-api`). The recommended cut
  does NOT touch `lumen`.
- Integration points: 0 new (reuses endpoint, store, tenant seam, caps,
  filters).
- Effort: under one day.
- Independent shippable outcomes: 1 (paginated read). Not splittable into
  separate features.

None of the oversized signals fire. Scope is right-sized; no split
required.
