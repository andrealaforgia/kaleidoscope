# Application Architecture: perf-kpi-ci-gating-v0

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-31.
British English; no em dashes in body text.

This document pins the exact guard text, the exact 28-site inventory, the
per-file change profile, the CI-versus-local behaviour matrix, and the
verification recipe. It is the implementation contract Crafty executes in
DELIVER as a single atomic slice.

## The guard pattern

The guard is the FIRST statement of each gated test body, byte-identical at all
28 sites. Copy verbatim:

```rust
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
```

Placement: immediately after the `fn ..._p95_...() {` opening brace, before any
existing statement (before `temp_base(...)`, before `InMemoryMetricStore::new`,
before any warm-up loop, before any `Instant::now`). Nothing that allocates,
opens a store, or takes a timing measurement may run above it.

Semantics:

- `is_err()` is true ONLY when the variable is absent (unset). Then the test
  prints the note to stderr and returns, so it is reported as PASSED with no
  measurement taken.
- `is_err()` is false when the variable is present with ANY value, including
  `1`, `true`, or the empty string (`Ok("")`). Then the guard falls through and
  the test runs its full measurement and threshold assertion exactly as before.

Why this exact shape: it is four lines, contains no per-site detail, and so is
identical everywhere. Identical text at every site means a grep
(`KALEIDOSCOPE_PERF_TESTS`) returns exactly 28 hits in the perf crates, and no
mutation of one site can differ from the others undetected. It uses only
`std::env`, so there is no dependency, no `Cargo.toml` edit, and no shared crate
to wire.

## Test inventory

All 28 tests measure wall-clock time with `std::time::Instant` and assert a p95
threshold. Each is a plain `#[test] fn` (sync). The guard goes at the top of
each body. Thresholds are UNCHANGED by this feature (US-03).

| # | Crate | Test file | Test fn | Threshold |
|---|-------|-----------|---------|-----------|
| 1 | lumen | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_three_milliseconds | 3 ms (CONFIRMED FLAKER) |
| 2 | lumen | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms |
| 3 | lumen | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms |
| 4 | lumen | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 5 | pulse | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms (CONFIRMED FLAKER) |
| 6 | pulse | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms |
| 7 | pulse | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_fifty_milliseconds | 50 ms |
| 8 | pulse | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 9 | ray | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms |
| 10 | ray | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_five_milliseconds | 5 ms |
| 11 | ray | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms |
| 12 | ray | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 13 | strata | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_five_milliseconds | 5 ms |
| 14 | strata | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_eight_milliseconds | 8 ms |
| 15 | strata | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms |
| 16 | strata | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 17 | cinder | tests/slice_01_walking_skeleton.rs | get_tier_p95_latency_under_fifty_microseconds | 50 us |
| 18 | cinder | tests/v1_slice_01_wal_durability.rs | place_p95_latency_under_two_hundred_microseconds | 200 us |
| 19 | cinder | tests/slice_02_lifecycle.rs | evaluate_p95_latency_under_five_milliseconds | 5 ms |
| 20 | cinder | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s |
| 21 | sluice | tests/slice_01_walking_skeleton.rs | enqueue_and_dequeue_p95_under_fifty_microseconds | 50 us |
| 22 | sluice | tests/v1_slice_01_wal_durability.rs | enqueue_p95_latency_under_three_hundred_microseconds | 300 us |
| 23 | sluice | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_hundred_milliseconds | 500 ms |
| 24 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | persist_p95_latency_under_two_milliseconds | 2 ms |
| 25 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | recovery_p95_latency_under_one_and_a_half_seconds | 1.5 s |
| 26 | augur | tests/slice_01_zscore.rs | observe_p95_latency_under_ten_microseconds | 10 us |
| 27 | augur | tests/slice_02_rare_event.rs | observe_p95_latency_under_twenty_microseconds | 20 us |
| 28 | aegis | tests/slice_01_validate.rs | validate_p95_latency_under_two_milliseconds | 2 ms |

11 crates: lumen, pulse, ray, strata, cinder, sluice, beacon, augur, aegis.

### Explicitly out of scope

- `aperture::slice_06_forwarding_sink::forwarding_sink_accepted_event_includes_downstream_latency_ms_field`
  asserts the PRESENCE of a `latency_ms` field, not a wall-clock threshold. NOT gated.
- Any test using `Instant`/`elapsed()` only for timeout or wait-until-ready
  scaffolding with no p95 threshold assertion. NOT gated.
- Any test asserting functional recovery correctness ("recovery produces
  identical state") rather than recovery latency. NOT gated.

## Changes Per File

Approximately 29 files, all EXTEND, all additive.

| File | Change | Lines added |
|------|--------|-------------|
| 24 single-perf-test files (rows 1, 3, 4, 6, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 26, 27, 28, plus lumen slice_01) | Guard preamble at top of the one perf test body | +4 each |
| beacon tests/v1_slice_02_filebacked_durable_recovery.rs (rows 24, 25) | Guard preamble at top of BOTH perf test bodies | +8 (4 per fn) |
| .github/workflows/ci.yml | Job-level `env:` block on `gate-1-test` | +2 (`env:` line plus the `KALEIDOSCOPE_PERF_TESTS: "1"` line) |

Total perf-test files touched: 28 tests across 28 file-locations, which is 27
distinct files (beacon's two perf tests share one file). Plus `ci.yml`. So
approximately 28 distinct files in the workspace, all additive, every test-file
edit being the same four-line block.

The CI edit, exact shape, added between `needs: gate-4-deny` (line 139) and
`steps:` of the `gate-1-test` job:

```yaml
    env:
      KALEIDOSCOPE_PERF_TESTS: "1"
```

The literal `"1"` is hardcoded. It is NOT `${{ env.KALEIDOSCOPE_PERF_TESTS }}`,
consistent with the gate-2 and gate-3 `NIGHTLY_PIN` workaround already in the
file (lines 86, 250, 358).

The pre-commit hook `scripts/hooks/pre-commit` is UNCHANGED: its line 92
`cargo test --workspace --all-targets --locked` runs with the variable absent,
which is the local-skip mechanism by design.

## CI vs local matrix

| Environment | Sets `KALEIDOSCOPE_PERF_TESTS`? | `is_err()` | Perf tests | Outcome |
|-------------|--------------------------------|------------|-----------|---------|
| Local pre-commit hook | No | true | Skip (early return, stderr note) | Hook green deterministically under load; no `--no-verify` |
| Local `cargo test --workspace` (plain) | No | true | Skip | Same as hook; fast and deterministic |
| Local opt-in `KALEIDOSCOPE_PERF_TESTS=1 cargo test ...` | Yes (developer) | false | Run, enforce thresholds | Developer gets local perf confidence on an idle machine |
| CI `gate-1-test` job | Yes (job-level `env`, literal `"1"`) | false | Run, enforce thresholds | KPIs are a real gate on ubuntu-latest; a real regression turns gate-1-test red and blocks merge |
| CI gates 2, 3, 4, 5 | No (no `cargo test`) | n/a | n/a | Unaffected; they do not run the perf tests |

## Verification

Local, variable absent (the pre-commit scenario):

```sh
cargo test --workspace --all-targets --locked
```

Expect: each of the 28 perf tests prints
`perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run` to stderr and reports
as `ok`; the workspace run exits 0 with no wall-clock perf failure, even under
machine load (US-01, US-04 AC).

Local, variable present (the opt-in scenario):

```sh
KALEIDOSCOPE_PERF_TESTS=1 cargo test -p lumen ingest_p95
```

Expect: the test performs its full 1000-sample measurement and asserts
`p95 <= 3_000` exactly as before; no skip note appears (US-01 example 2).

CI (the gate scenario): the `gate-1-test` job sets the variable, so the CI log
shows the 28 perf tests executing their measurements with no skip note, and a
p95 above its threshold on ubuntu-latest fails the assertion and blocks the
merge (US-02 AC).

Coverage check (US-04): grep the 11 perf crates for the guard token and confirm
28 hits, one per gated test, and that no functional (non-temporal) test was
altered:

```sh
grep -rn 'KALEIDOSCOPE_PERF_TESTS' crates/{lumen,pulse,ray,strata,cinder,sluice,beacon,augur,aegis}/tests
```

Threshold integrity check (US-03): a diff of the gated files shows only the
four-line preamble added per test, every threshold literal (`3_000`, `10_000`,
and the rest) byte-for-byte unchanged.
