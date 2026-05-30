# DISCUSS Decisions - log-query-pagination-v0

## Key Decisions

- [D1] **Backend feature, brownfield, no walking skeleton rebuilt.** The
  endpoint `/api/v1/logs` already exists with the durable store, the
  tenant seam, the caps, and three filter parameters (`min_severity`,
  `body_contains`, `body_regex`). This slice grows the contract by two
  optional parameters (`limit`, `offset`); the platform walking skeleton
  is NOT re-built. US-01 is the walking skeleton OF THIS SLICE. (see:
  story-map.md § Walking skeleton)
- [D2] **Pagination is a windowing stage, not a filter.** `limit` and
  `offset` do NOT narrow WHICH records match (a `Predicate` question);
  they narrow HOW MANY of the already-matched, already-ordered records
  are returned and from WHICH position. This is why the recommended cut
  does NOT extend `lumen::Predicate`. (see: user-stories.md § System
  Constraints, FLAG 1)
- [D3] **Recommended cut: handler-side slice within the existing cap.**
  Apply `skip(offset).take(limit)` over the `Vec<LogRecord>` the store
  already returns in stable observed-time order, in the handler, within
  the 100_000-row cap. No `LogStore` trait change, no `Predicate`
  extension, no adapter edit. Pure parse + wire in
  `crates/log-query-api/src/lib.rs`. (see: user-stories.md PIN 2, FLAG 1)
- [D4] **Pagination honesty is the central correctness promise.** For a
  fixed result set, successive pages partition it with no duplicate and
  no gap, resting on the store's stable ascending-observed-time order
  (PIN 1). Pinned by the US-04 partition acceptance test. The slice does
  NOT promise snapshot isolation across requests (PIN 3); concurrent-
  ingest drift is the deferred cursor-pagination concern. (see:
  user-stories.md US-04, PIN 3)
- [D5] **Filter-before-page; pagination is the LAST pipeline stage.** The
  pipeline is tenant -> window -> filters -> order -> page. Under the
  handler-side cut this is automatic: the store already applied the
  predicate, so the vector the handler slices is the post-filter set.
  Pinned by US-06. (see: user-stories.md US-06)
- [D6] **Redaction and fail-fast extended to the two new parameters.**
  Every invalid-`limit`/`offset` 400 is a static-literal reason
  (`"invalid limit"` / `"invalid offset"`) emitted before the store is
  touched; the raw value is never echoed. Symmetric with ADR-0052 /
  ADR-0055 / ADR-0056. (see: user-stories.md US-05a/b/c)

## Requirements Summary

- **Primary user need**: an operator (Maria Santos, on-call SRE) needs
  to scroll a large log result set ONE page at a time - bound a first
  page with `limit`, advance through pages with `offset` - instead of
  receiving a single block all the way up to the 100_000-row cap and
  trimming client-side with `jq`. The pages must be honest: each record
  appears exactly once across the pages, with no duplicate, no gap, and
  no cross-tenant leak.
- **Walking skeleton scope**: US-01 (`limit` returns the first N
  records) - the slice primitive that exercises the full Parse -> Slice
  -> Verify pipeline. Ships with US-05a (invalid `limit`) and US-05c
  (`limit` over cap) as the same `parse_limit` helper.
- **Feature type**: Backend (read-side HTTP API extension; parse + wire
  pattern, sibling in shape to `body_contains` / `body_regex`).

## Constraints Established

- `MAX_WINDOW_SECONDS = 86_400` and `MAX_RESULT_ROWS = 100_000` (ADR-0050)
  preserved unchanged; consumed from `query_http_common::`; the slice
  re-declares neither. The 100_000-row cap remains the backstop;
  pagination is WITHIN it.
- The error envelope `{"status":"error","error":"<reason>"}` is reused
  via `query_http_common::error_response`; no new envelope, no new status
  code.
- The fail-closed tenancy seam goes through
  `query_http_common::resolve_tenant_or_refuse`; not re-implemented.
- The error text never echoes the raw parameter value (redaction;
  ADR-0047 Decision 1, ADR-0050 Decision 7).
- The bare JSON array success shape (ADR-0047 Decision 1) is preserved;
  the empty arm is `[]`, HTTP 200, NEVER 404 (an offset past the end is a
  calm empty page).
- The store's stable ascending `observed_time_unix_nano` order
  (`crates/lumen/src/store.rs:136`) is the order over which `offset`
  skips and `limit` takes; the slice introduces no new sort.
- The existing `min_severity` / `body_contains` / `body_regex` filters
  are preserved unchanged; `limit`/`offset` apply to the post-filter set.
- Under the recommended cut, the `lumen::LogStore` trait AND
  `lumen::Predicate` public surfaces stay byte-identical to the prior
  tag (no trait method, no predicate field added).

## Flags to DESIGN

These are decisions for Morgan to pin in DESIGN. DISCUSS records a
recommendation for each but does NOT decide; the user-visible behaviour
in `user-stories.md` is written against the recommendation.

| # | Flag | DISCUSS recommendation | Rationale |
|---|---|---|---|
| 1 | **Handler-side vs in-store pagination** | Handler-side slice over the returned `Vec<LogRecord>` within the existing cap. No `LogStore` trait change, no `Predicate` extension. | Thinnest cut; parse + wire only; mirrors the way the result-cap check already operates handler-side on the returned vector (`lib.rs:285`). In-store pagination (extend `query_with` with `limit`/`offset`) is more memory-efficient at very large windows but extends the trait and every adapter; deferred to a successor slice. |
| 2 | **`limit` over the cap: reject vs clamp** | REJECT (HTTP 400, `"invalid limit"`). The boundary is inclusive: `limit=100000` served, `limit=100001` refused. | Consistency with ADR-0050 Decision 3 (refuse, never truncate). A silent clamp (serve 100_000 and pretend that was the ask) is the read-side equivalent of a buffered fsync that lies. Clamp is the rejected alternative. |
| 3 | **Offset semantics: skip-based vs cursor-based** | Skip-based (`offset=N` drops the first N of the ordered post-filter set). | Simplest, lowest-surprise primitive; honest over a fixed result set (PIN 3). Cursor/keyset pagination (survives concurrent ingest, no offset drift) is a richer contract deferred to a successor slice once real demand surfaces. |
| 4 | **Default `limit` when absent** | NO default. The absence of `limit` returns every matched record up to the cap, exactly as today. | Backward compatibility (US-03): an implicit default page size would silently break the installed base of scripts that fetch the whole block. The 100_000-row cap remains the only backstop. An explicit default (e.g. `limit=1000`) is the rejected alternative. |
| 5 | **`limit=0` and offset-past-end semantics** | `limit=0` is INVALID (HTTP 400, `"invalid limit"`). `offset` past the end of the result set is a calm empty page (HTTP 200, `[]`), NOT an error. `offset=0` is valid (the first page). | `limit=0` (PIN 6): an empty page carries no information an absent request would not, and a zero is far more likely a client bug (uninitialised page-size variable) than a deliberate ask; refusing it out loud is symmetric with the empty-`body_contains` / empty-`body_regex` posture. Offset-past-end (PIN 4): "page 50 of a 3-page result" is a well-formed request that legitimately has no rows - the same calm-empty posture the contract uses for a filter that matches nothing. |
| 6 | **ADR-0057 yes/no** | YES, a small ADR. | Pagination touches the ADR-0050 cap-interaction semantics (the `limit`-vs-cap interaction, refuse-not-truncate extended to page size) and grows the HTTP read contract by two optional parameters; it merits a durable, cross-referenced record. The ADR cites ADR-0050 (and ADR-0047, ADR-0054, ADR-0055, ADR-0056) without modifying any. |

## Upstream Changes

None. No DISCOVER or prior-wave assumption is changed. The slice is
purely additive on top of the existing `/api/v1/logs` contract and the
existing `lumen` and `query-http-common` seams, all of which were read
directly and confirmed to exist as described. No DIVERGE artifacts were
present for this feature (Decision 4 JTBD = No; lightweight research
depth); the absence is noted here as the only upstream gap, and it is
not a risk for a brownfield parse + wire slice whose job is a direct
sibling of three shipped slices.
