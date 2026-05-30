# Outcome KPIs: log-query-pagination-v0

Five outcome KPIs. Each has a measurable target and a measurement
method. Consistent with ADR-0050 Decision 8 / ADR-0055 Decision 13 /
ADR-0056 Decision 14, the v0/v1 platform has NO live observability
stack of its own: the contract IS the signal, and every KPI is measured
by the DISTILL-wave acceptance suite
(`crates/log-query-api/tests/slice_01_pagination.rs`) plus the existing
regression suites and the Gate 2 / Gate 5 CI gates, NOT by a runtime
counter or dashboard.

## K1 - Behaviour invariance when `limit`/`offset` are absent

- **Who**: every existing client of `/api/v1/logs` (curl, jq, Prism,
  Marcus's automation) that does NOT send `limit` or `offset`.
- **Does what**: continues to receive the slice-prior response,
  unchanged, on every existing call.
- **By how much**: 100% byte-equality. For any input with neither
  `limit` nor `offset`, the response is byte-equal to the slice-prior
  response for the same inputs.
- **Measured by**: every existing acceptance suite on `/api/v1/logs`
  (`slice_01_logs_read`, `slice_02_caps`, `slice_01_severity_filter`,
  `slice_01_body_contains`, `slice_01_body_regex`) stays GREEN unchanged;
  plus a new US-03 byte-equality assertion against the no-parameter
  response shape.
- **Target**: 100% of the prior suites green; 0 byte-difference on the
  no-parameter path.
- **Baseline**: today's no-parameter response is the gold reference
  (every matched record up to the cap, in observed-time order).

## K2 - Pagination honesty (clean partition; no duplicate, no gap, no leak)

- **Who**: operators and UIs scrolling forward through a multi-page
  result set.
- **Does what**: receive every matched record exactly once across the
  pages that cover the set, with no record repeated and no record
  dropped at a page boundary; and never a record from another tenant.
- **By how much**: for a fixed result set, the ordered concatenation of
  the pages `(offset=k*N, limit=N)` for k = 0, 1, 2, ... equals the full
  result set exactly - 0 duplicates, 0 gaps. Cross-tenant: 0 records of
  tenant A appear in any of tenant B's pages.
- **Measured by**: US-04 partition acceptance test (concatenate three
  pages of size 3 over an eight-record set, assert it equals all eight in
  order); US-07 cross-tenant test (tenant B's pages carry only tenant B's
  records or `[]`); US-06 post-filter composition test.
- **Target**: 100% partition correctness (page union == full set, no
  duplicate, no gap) on the fixture; 0 cross-tenant leak.
- **Baseline**: no pagination primitive exists today; honesty is the new
  promise.

## K3 - `query-http-common` reuse confirmed (no new caps, no new envelope)

- **Who**: the workspace maintainers and the next read-API author.
- **Does what**: the slice consumes the shared scaffold for the cap
  constant and the error envelope without re-implementing or duplicating
  any of it.
- **By how much**: 0 new `MAX_*` constants in `log-query-api`; 0 new
  `error_response`-equivalent helpers; 0 new error envelope shapes;
  `MAX_RESULT_ROWS` consumed from `query_http_common::` for the
  over-cap-`limit` check (US-05c).
- **Measured by**: a workspace grep / CI-static assertion that
  `crates/log-query-api/src/lib.rs` declares no new `const MAX_RESULT_ROWS`
  or `const MAX_WINDOW_SECONDS` and constructs no error body inline (every
  400 goes through `query_http_common::error_response`); plus the
  per-slice LOC budget on the handler.
- **Target**: 0 new duplications; the two new parse helpers and the slice
  expression add under 40 LOC to `lib.rs`.
- **Baseline**: ADR-0054 / ADR-0055 / ADR-0056 established the
  single-source posture; this slice continues it.

## K4 - Invalid `limit`/`offset` return 400 fast, with no store hit

- **Who**: operators and automation that send a malformed `limit` or
  `offset` (non-numeric, negative, `limit=0`, `limit` over cap).
- **Does what**: receive an immediate redacted 400 at parse time, before
  any store work; the raw value is never echoed.
- **By how much**: 100% of malformed-`limit` and malformed-`offset`
  requests are rejected at parse time with the correct literal envelope
  (`"invalid limit"` / `"invalid offset"`) and 0 store queries on those
  paths; 0 of the responses contain the raw parameter value.
- **Measured by**: US-05a / US-05b / US-05c acceptance tests, each
  asserting (a) status 400, (b) the exact literal envelope, (c) the raw
  value absent from the body, (d) a no-store-call assertion (a failing
  store double that would error if touched, mirroring the
  `body_contains` / `body_regex` no-store-call pattern).
- **Target**: 100% parse-time rejection; 0 store hits; 0 raw-value
  echoes.
- **Baseline**: the redaction + fail-fast posture established by
  ADR-0052 / ADR-0055 / ADR-0056; this slice extends it to the two new
  parameters.

## K5 - Cap interaction honest (`limit` over cap rejected; refuse-not-truncate)

- **Who**: operators who ask for a page larger than the platform will
  serve whole, and operators whose window matches more than the cap.
- **Does what**: receive an honest 400 (refuse) rather than a silent
  truncation (clamp); the 100_000-row cap remains the backstop and is
  never weakened by pagination.
- **By how much**: `limit=100000` (at the cap) is served; `limit=100001`
  (over the cap) is a 400 with `"invalid limit"`; a window whose matched
  set exceeds 100_000 is still refused exactly as today; pagination
  removes/reorders/weakens 0 of the existing cap checks.
- **Measured by**: US-05c boundary acceptance test (`limit=100000`
  accepted, `limit=100001` rejected) plus an inline boundary unit test
  (the `>` -> `>=` mutant killer); the existing `slice_02_caps` suite
  stays green unchanged (the window cap and result cap are byte-identical).
- **Target**: inclusive boundary at exactly `MAX_RESULT_ROWS`; over-cap
  rejected; `slice_02_caps` 100% green unchanged.
- **Baseline**: ADR-0050 Decision 2/3 (refuse, never truncate); this
  slice extends the refuse posture to the page-size parameter (FLAG 2).

## Gate alignment (CI, not a runtime KPI)

- **Gate 2 (`cargo public-api`)**: under the recommended handler-side
  cut, the `lumen::LogStore` trait AND `lumen::Predicate` public surfaces
  stay byte-identical to the prior tag (no trait method, no predicate
  field added). `LogsParams` is private, so the two new fields do not
  appear in the public-api diff. Target: 0 unexpected public-surface
  drift.
- **Gate 5 (mutation, 100% kill on modified files)**: the
  `gate-5-mutants-log-query-api` workflow picks up the new `parse_limit`,
  `parse_offset`, and the slice expression. Primary mutation targets: the
  `>` -> `>=` over-cap boundary (US-05c), the `<= 0` zero-rejection
  (US-05a / PIN 6), the `skip`/`take` off-by-one (US-04 honesty), the
  no-store-call order (K4), the per-tenant-scope-before-slice order
  (US-07). Target: 100% kill rate.
