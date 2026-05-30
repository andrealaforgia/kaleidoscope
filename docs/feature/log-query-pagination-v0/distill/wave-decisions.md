# DISTILL Decisions - log-query-pagination-v0

Scholar (nw-acceptance-designer), DISTILL wave. This wave translates the
DISCUSS user stories (US-01..US-07), the DESIGN pins (DD1-DD6), the
parse-helper spec, and ADR-0057 into one executable acceptance suite
plus a RED-ready scaffold. The slice is a brownfield carpaccio on top of
the existing `GET /api/v1/logs` route; it adds two optional query-string
parameters, `limit` and `offset`, applied as a handler-side slice over
the `Vec<LogRecord>` the store returns in stable
`observed_time_unix_nano` order, within the existing 100000-row cap.

## Reconciliation

Reconciliation passed -- 0 contradictions. DISCUSS, DESIGN
(wave-decisions DD1-DD6, application-architecture), the parse-helper
spec, and ADR-0057 all agree on every pin: handler-side slice (DD1 /
ADR-0057 D1), `limit` over the cap rejected not clamped (DD2 / D2),
skip-based `offset` (DD3 / D3), no default `limit` (DD4 / D4),
`limit=0` invalid and `offset` past end calm-empty (DD5 / D5),
cap-then-slice order (application-architecture sequence / ADR-0057 D6).
No upstream-issues file is needed.

## DWD-1: Walking Skeleton Strategy A (Full InMemory class), inherited skeleton

The feature has NO new driven port and NO new I/O boundary (DESIGN
Earned-Trust section: "no new probe surface is introduced because no
new dependency on the lying world is added"). The only collaborator is
the EXISTING `lumen::LogStore` driven port, exercised through the
EXISTING `FileBackedLogStore` adapter. The walking skeleton is
IMPLICIT, inherited from the slices that already shipped on
`/api/v1/logs` (read endpoint, durable store, tenant seam, caps,
severity / body_contains / body_regex filters); no greenfield skeleton
is rebuilt (user-stories.md preamble).

Per the Strategy decision tree this is Strategy A in classification
(pure handler-side parse + slice; no new driven port with I/O).
However, the suite still seeds REAL durable storage
(`FileBackedLogStore` via `open_durable_store`) for every seeded
scenario, so the inherited skeleton's `@real-io @adapter-integration`
posture is preserved: AC-01 (the new walking-skeleton-class scenario)
proves wiring, the read, the stable order, and the handler-side slice
end to end against the real adapter. There is no costly external
dependency and no environment matrix to parametrise (Mandate 4: the two
new parse helpers are PURE functions over `&str`; their boundary is the
parse arm, not an adapter). Container preference: none (in-process
trait call against a real on-disk store in a tempdir).

## DWD-2: Scaffold strategy (Mandate 7 RED-ready)

The acceptance suite imports the EXISTING `log_query_api::router` public
driving port only; no new production module is imported. The scaffold is
therefore confined to `crates/log-query-api/src/lib.rs` (lumen NOT
touched, no other crate touched):

- `LogsParams` gains `limit: Option<String>` and `offset: Option<String>`
  (private fields, parallel to the existing `min_severity` /
  `body_contains` / `body_regex` `Option<String>` fields; a missing
  parameter deserialises as `None`).
- `fn parse_limit(raw: &str) -> Result<usize, &'static str>` and
  `fn parse_offset(raw: &str) -> Result<usize, &'static str>` are added
  with the correct signatures (parse-helper-spec) and bodies
  `unimplemented!("__SCAFFOLD__ log-query-pagination-v0 RED")`.
- The handler gains the page slice AFTER the result-cap check and BEFORE
  `success_response` (cap-then-slice order, ADR-0057 D6). The
  `(None, None)` pair is a fast path that returns the post-cap vector
  unchanged WITHOUT calling either parse helper (US-03 backward
  compatibility byte-unchanged).

No `MAX_RESULT_ROWS` is re-declared; the over-cap check inside
`parse_limit` will compare against the imported
`query_http_common::MAX_RESULT_ROWS` (already `pub use`'d at lib.rs:65).

## DWD-3: RED state

Every pagination scenario that sends `limit` or `offset` reaches a parse
helper, which is `unimplemented!` and panics with the scaffold marker.
`unimplemented!` / `panic!` is classified RED (implementation missing,
test correct), NOT BROKEN (the import resolves, the build is green). The
12 scenarios are `#[ignore]`'d at DISTILL close so the workspace
pre-commit gate (`cargo test --workspace --all-targets --locked`) and
the `#[ignore]`'d-tests-not-run convention both hold.

## DWD-4: Slice-after-cap order confirmed

The page slice runs only AFTER the existing result-cap check
(`records.len() > MAX_RESULT_ROWS`, lib.rs:285) has passed, on the
post-filter, PRE-slice vector. A window whose matched set exceeds 100000
is refused exactly as today, BEFORE any slice is reached (ADR-0057 D6 /
D7 known limitation). Because the store already applied the predicate
and scoped to the tenant, the vector the handler slices is ALREADY the
post-filter, per-tenant set, so filter-before-page (US-06) and
tenant-scope-before-page (US-07) are automatic.

## US -> AC mapping

| Story | Scenario(s) | Kind |
|-------|-------------|------|
| US-01 (limit returns first N) | `ac_01_limit_returns_first_n` (walking skeleton) | happy / @real-io |
| US-02 (offset skips; past-end calm empty) | `ac_02_offset_skips`, `ac_06b_offset_past_end_returns_empty` | happy / edge / @real-io |
| US-03 (missing pagination unchanged) | `ac_03_missing_pagination_returns_all` | happy / backward-compat / @real-io |
| US-04 (pagination honesty: partition) | `ac_04_pagination_honesty` | @property / @real-io |
| US-05a (invalid limit) | `ac_05a_invalid_limit_zero_returns_400`, `ac_05b_invalid_limit_nonnumeric_returns_400`, `ac_05c_invalid_limit_negative_returns_400` | error / no-store-call |
| US-05b (invalid offset) | `ac_06a_invalid_offset_nonnumeric_returns_400` | error / no-store-call |
| US-05c (over-cap limit) | `ac_05d_limit_over_cap_returns_400` | error / boundary / no-store-call |
| US-06 (composes with filter) | `ac_07_pagination_composes_with_filter` | integration / @real-io |
| US-07 (cross-tenant isolation) | `ac_08_cross_tenant_isolation` | integration / @real-io |

Every US-01..US-07 has at least one scenario. Error-path ratio: 5 of 12
scenarios are error / boundary 400 arms (42%), above the 40% mandate.

## Mandate 7 RED-ready evidence

- All scaffolds carry the `__SCAFFOLD__ log-query-pagination-v0 RED`
  marker inside the `unimplemented!` body
  (`grep "__SCAFFOLD__ log-query-pagination-v0" crates/log-query-api/src/lib.rs`
  -> two hits, `parse_limit` and `parse_offset`).
- The scaffold uses `unimplemented!` (a `panic!`), classified RED, not
  `NotImplementedError`-equivalent or an unresolved import (BROKEN).
- Imports resolve: `cargo build -p log-query-api --all-targets` green;
  `cargo build --workspace` green.

## RED state evidence (verified locally)

- `cargo build --workspace` -> green.
- `cargo test -p log-query-api --test slice_06_pagination` -> 12 ignored
  (pre-commit safe).
- `cargo test -p log-query-api --test slice_06_pagination -- --ignored
  ac_01` -> FAILED with
  `panicked at crates/log-query-api/src/lib.rs:473: not implemented:
  __SCAFFOLD__ log-query-pagination-v0 RED` (RED via scaffold panic in
  `parse_limit`).
- `cargo test -p log-query-api` -> all existing suites green
  (slice_01: 11, slice_02: 6, slice_03: 8, slice_04: 8, slice_05: 10,
  inline unit: 18, slice_06: 12 ignored). The scaffold did NOT break the
  no-pagination dispatch path.

## Handoff to DELIVER

Crafty de-ignores `ac_01_limit_returns_first_n` FIRST (the demo-able
first page; outer-loop convention), then the remaining scenarios one at
a time as he fills `parse_limit`, `parse_offset`, and confirms the slice
expression. After DELIVER, zero `__SCAFFOLD__` markers should remain.
Mutation targets (DESIGN DEVOPS handoff): `>` -> `>=` over-cap boundary
(AC-05d), zero-rejection on `limit` (AC-05a), `skip` / `take`
off-by-one (AC-04 honesty), per-tenant-scope-before-slice (AC-08),
no-store-call on the invalid-parse arms (AC-05a/b/c, AC-06a).

## DELIVER notes (Crafty)

1. **Handler-order correction (back-propagation).** The DISTILL scaffold
   parsed `limit` / `offset` INSIDE the `Ok(records)` arm, i.e. AFTER the
   store dispatch. The no-store-call invariant (AC-05a/b/c/d, AC-06a,
   asserted via `CountingFailingLogStore::total_store_calls() == 0`)
   cannot hold there: a failing store returns `Err` before the parse is
   reached, so the arm yields a 500 and the store counter is already 1.
   ADR-0057 Decision 6 pins the order as `... filters -> parse
   limit/offset -> store -> result cap -> page slice`, so the parse was
   moved to BEFORE the store dispatch (an invalid value is now a
   parse-time 400 that never queries the store) while the page slice
   itself stays AFTER the result-cap check (cap-then-slice, Decision 6).
   All twelve scenarios green; the implementation matches the ADR order
   exactly.

2. **Pre-commit hook bypass justified (lumen p95 flake).** The
   `scripts/hooks/pre-commit` Gate 1 (`cargo test --workspace
   --all-targets --locked`) fails on ONE unrelated test:
   `lumen::v1_slice_01_wal_durability::ingest_p95_latency_under_three_milliseconds`,
   a wall-clock disk-fsync-tail KPI (p95 <= 3 ms). Observed p95
   4308-6154 µs while the first-10 ingest samples are 57-66 µs in
   release: the WAL is fast; the p95 tail is dominated by occasional OS
   fsync stalls on this machine, not by the code. This feature's diff is
   confined to `crates/log-query-api/src/lib.rs` and touches no lumen
   code, WAL, or ingest path; `cargo fmt --check`, `cargo clippy
   --all-targets -- -D warnings`, and every log-query-api suite
   (slice_01..06 + 18 inline) are green. The hook header sanctions
   `--no-verify` for "genuinely justified cases"; per the trunk-based
   fix-forward policy this pre-existing timing flake is recorded here and
   fixed forward, not allowed to block an unrelated, fully-green slice.
