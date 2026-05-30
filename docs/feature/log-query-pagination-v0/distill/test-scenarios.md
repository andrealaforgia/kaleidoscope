# Test Scenarios - log-query-pagination-v0

Scholar (nw-acceptance-designer), DISTILL wave. The executable SSOT is
`crates/log-query-api/tests/slice_06_pagination.rs` (12 scenarios, all
`#[ignore]`'d at DISTILL close, RED-ready via the
`__SCAFFOLD__ log-query-pagination-v0 RED` panic in `parse_limit` /
`parse_offset`). This document is the human-readable scenario map. Every
scenario drives the single public driving port
`log_query_api::router(store, tenant)` via `oneshot`; seeded scenarios
use a REAL durable `FileBackedLogStore`, the four no-store-call 400 arms
use a `CountingFailingLogStore` double.

The canonical window is `[1716200000s, 1716200600s)`. The ten-record
fixture seeds bodies "rec-01" .. "rec-10" in ascending observed-time
order so page membership is crisp.

## AC table

| AC-id | Story | Given | When | Then | HTTP | Body |
|-------|-------|-------|------|------|------|------|
| ac_01_limit_returns_first_n | US-01 | acme-prod has ten records (rec-01..rec-10) in a real durable store, ascending order | GET window with `limit=3` | first three records (rec-01, rec-02, rec-03) in order; rec-04..rec-10 absent | 200 | bare array of 3 |
| ac_02_offset_skips | US-02 | acme-prod has ten records | GET window with `limit=3&offset=3` | fourth-through-sixth records (rec-04, rec-05, rec-06) in order | 200 | bare array of 3 |
| ac_03_missing_pagination_returns_all | US-03 | acme-prod has ten records | GET window with NO limit and NO offset | every in-window record (all ten) in order | 200 | bare array of 10 |
| ac_04_pagination_honesty | US-04 | acme-prod has ten records, fixed stable order | fetch (offset=0,limit=5) then (offset=5,limit=5) | ordered union equals all ten; no duplicate; no gap | 200 (each) | two bare arrays, union = 10 |
| ac_05a_invalid_limit_zero_returns_400 | US-05a | valid tenant, window within cap | GET window with `limit=0` | literal `invalid limit`; store NEVER queried | 400 | `{"status":"error","error":"invalid limit"}` |
| ac_05b_invalid_limit_nonnumeric_returns_400 | US-05a | valid tenant, window within cap | GET window with `limit=abc` | same literal; store NEVER queried | 400 | `{"status":"error","error":"invalid limit"}` |
| ac_05c_invalid_limit_negative_returns_400 | US-05a | valid tenant, window within cap | GET window with `limit=-5` | same literal; body NEVER contains "-5"; store NEVER queried | 400 | `{"status":"error","error":"invalid limit"}` |
| ac_05d_limit_over_cap_returns_400 | US-05c | valid tenant, window within cap | GET window with `limit=100001` | same literal; body NEVER contains "100001"; store NEVER queried (boundary inclusive at 100000) | 400 | `{"status":"error","error":"invalid limit"}` |
| ac_06a_invalid_offset_nonnumeric_returns_400 | US-05b | valid tenant, window within cap | GET window with `offset=xyz` | literal `invalid offset` (distinct class); store NEVER queried | 400 | `{"status":"error","error":"invalid offset"}` |
| ac_06b_offset_past_end_returns_empty | US-02 | acme-prod has five records | GET window with `offset=100` | calm empty page; NEVER 404, NEVER 400 | 200 | `[]` |
| ac_07_pagination_composes_with_filter | US-06 | acme-prod has mixed-severity window (3 INFO, 4 WARN: warn-01..warn-04) | GET window with `min_severity=WARN&limit=2` | first two WARN-or-above records (warn-01, warn-02) in order; no INFO | 200 | bare array of 2 |
| ac_08_cross_tenant_isolation | US-07 | acme-prod has ten records; globex-staging has zero | GET window with `limit=2&offset=0` under globex-staging | calm empty page; body NEVER contains "rec-"; no acme-prod record | 200 | `[]` |

Driving port: every scenario enters through `log_query_api::router`
(the HTTP driving adapter) via `oneshot`. No internal component
(`parse_limit`, `parse_offset`, the slice expression) is invoked
directly from the acceptance suite; the parse helpers are exercised
indirectly through the handler, and the parse-helper-spec inline test
cases (boundary, zero-rejection, redaction) belong to the DELIVER inner
loop as `#[cfg(test)]` unit tests.

## Self-review checklist

- [x] **Mandate 7 RED-not-BROKEN**: scaffold uses `unimplemented!`
  (`panic!`, classified RED); imports resolve; workspace builds green;
  `ac_01` panics with `__SCAFFOLD__ log-query-pagination-v0 RED`.
- [x] **Every US has an AC**: US-01 -> ac_01; US-02 -> ac_02, ac_06b;
  US-03 -> ac_03; US-04 -> ac_04; US-05a -> ac_05a/b/c; US-05b -> ac_06a;
  US-05c -> ac_05d; US-06 -> ac_07; US-07 -> ac_08. No story uncovered.
- [x] **Pagination honesty test**: ac_04 asserts two contiguous pages
  partition the ten-record set with no duplicate (membership cross-check)
  and no gap (ordered union equals the full set). Tagged `@property`.
- [x] **Anti-echo**: ac_05c asserts the body never contains "-5";
  ac_05d asserts the body never contains "100001". The raw value is a
  static-literal-redacted 400.
- [x] **Invalid limit / offset**: limit=0 (ac_05a), limit=abc (ac_05b),
  limit=-5 (ac_05c), limit=100001 (ac_05d), offset=xyz (ac_06a) each
  return the correct literal; the offset literal ("invalid offset") is a
  DISTINCT class from the limit literal ("invalid limit").
- [x] **offset-past-end calm-empty**: ac_06b asserts 200 + `[]`, never
  404, never 400 (PIN 4).
- [x] **compose-with-filter**: ac_07 asserts `limit` applies to the
  POST-FILTER ordered set (the first two records of the WINDOW are INFO;
  the page is the first two WARN records), proving filter-before-page.
- [x] **cap-then-slice order**: enforced structurally by the scaffold
  (the slice runs AFTER the result-cap check at lib.rs:285) and pinned
  by ac_05d (over-cap `limit` refused at parse time, before the store).
- [x] **no-store-call on 400 arms**: ac_05a/b/c, ac_05d, ac_06a each
  assert `store.total_store_calls() == 0` via the CountingFailingLogStore
  double.
- [x] **Business-language Gherkin in doc-comments**: each scenario's
  `///` block frames Maria's user goal (bounded first page, scroll
  forward, clear rejection), not HTTP mechanics.
- [x] **Driving port only (Mandate 1 / CM-A)**: the suite imports
  `log_query_api::router` and the `lumen` types for the store double;
  it imports zero internal `log-query-api` components.
- [x] **Error-path ratio**: 5 of 12 scenarios (42%) are error / boundary
  400 arms, above the 40% mandate.
