# DISCUSS Decisions: perf-kpi-ci-gating-v0

## Key Decisions

- [D1] Feature type: Infrastructure (cross-cutting test-infra). Rationale: changes
  test files, the CI workflow, and docs only; no production source under
  `crates/*/src/`. (see: user-stories.md System Constraints)
- [D2] Walking skeleton: No standalone skeleton feature, but US-01 acts as the
  thin end-to-end slice (one test, guard, CI opt-in, threshold preserved) before
  fan-out. (see: story-map.md Walking Skeleton)
- [D3] UX research depth: Lightweight. The journey is a maintainer development-loop
  flow; happy path is the dominant case. (orchestrator decision)
- [D4] JTBD: No. The job is well understood (eliminate load-induced perf flakes
  from the local hook without weakening the CI KPI gate). (orchestrator decision)
- [D5] Skip semantics: early return that PASSES with an stderr note, never a panic.
  A panic would be indistinguishable from a real failure and would not solve the
  bypass problem. (see: user-stories.md System Constraints, US-01)

## Requirements Summary

- Primary need: the maintainer needs the local pre-commit hook to be fast and
  deterministic (no load-induced wall-clock flakes forcing `--no-verify`), while
  the wall-clock KPIs remain real, enforced gates in CI on `ubuntu-latest` where
  the thresholds were tuned. Mechanism: an env-var guard (`KALEIDOSCOPE_PERF_TESTS`)
  that skips the gated tests locally (variable absent) and runs them in CI
  (variable set in the gate-1-test job).
- Walking skeleton scope: US-01 on a single confirmed flaker
  (`lumen ingest_p95_latency_under_three_milliseconds`), then fan-out via US-04.
- Feature type: Infrastructure (test-infra, cross-cutting across 11 crates' tests).

## Constraints Established

- No production source change (`crates/*/src/` untouched).
- No KPI threshold, sample count, or percentile index changed (US-03 / K3).
- Guard SKIPS (early return, passes) when the variable is absent; never panics.
- Variable contract is presence-based (recommended): any value means opt-in;
  absence means skip. Exact contract for empty-string is flag 2 to DESIGN.
- CI sets the variable in the `gate-1-test` job-level `env` block with a LITERAL
  value. Do NOT reference a workflow-level `${{ env.X }}` (project memory: GitHub
  Actions job-level env evaluation quirk; the same quirk is already worked around
  for `NIGHTLY_PIN` in the gate-2 and gate-3 jobs).
- No crate bumped to 1.0.0.
- British English; no em dashes in body text.

## Flags to DESIGN (Morgan, solution-architect)

1. **Guard mechanism**: shared helper versus inline check. Inline shape confirmed
   feasible from the grounding: each gated test is a plain `#[test] fn` whose body
   starts with the measurement, so a preamble
   `if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() { eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run"); return; }`
   drops in cleanly. Recommended: inline for the first slice (no new crate, no new
   dev-dependency). Morgan to weigh whether a tiny shared test-util helper reduces
   duplication across 28 sites enough to justify the wiring. Note: tests live in
   `tests/*.rs` integration targets across 11 crates, so a shared helper would need
   a shared dev-dependency crate or a copied `common/mod.rs` per crate.
2. **Environment-variable name and contract**: name `KALEIDOSCOPE_PERF_TESTS`
   (pinned). Contract: presence-based (`is_err()` to detect absence) recommended,
   so any value including `1` opts in. Morgan to pin whether empty-string counts as
   set or unset (`is_err()` treats empty-string as SET; if "1"-only is desired,
   use a value check instead).
3. **Skip mechanism**: early-return (test passes, body not executed) versus
   `#[ignore]` plus `--include-ignored`. Recommended: early-return. It is surgical
   and per-test; `#[ignore]` plus `--include-ignored` in CI would also re-activate
   any other unrelated ignored tests, which is broader than intended. Morgan
   decides.
4. **Exact list of tests to gate**: the complete inventory is below (28 tests,
   11 crates). DESIGN must reconcile any perf test added between now and DELIVER
   against this list.
5. **CI workflow change**: add a job-level `env` block to `gate-1-test` only
   (lines 136 to 182 of `.github/workflows/ci.yml`; the `cargo test --workspace`
   invocation is at line 182). No other gate job runs `cargo test`. Hard-code the
   literal value (see Constraints).
6. **ADR-0058**: recommended YES. The decision of WHERE the KPIs are enforced
   (CI only, not the local hook) is a policy decision worth recording. ADR-0058
   should cite ADR-0005 (the five gates; Gate 1 is `cargo test`) WITHOUT modifying
   it. This backs US-05.

## Complete Inventory of Wall-Clock KPI Tests in Scope

All 28 tests measure wall-clock time with `std::time::Instant` and assert a p95
temporal threshold. Each is a `#[test] fn` (sync) unless noted. Crate to file to
test name to threshold to unit:

| # | Crate | Test file | Test fn | Threshold | Unit |
|---|-------|-----------|---------|-----------|------|
| 1 | lumen | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_three_milliseconds | 3 ms (3_000 µs) | ms (CONFIRMED FLAKER) |
| 2 | lumen | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms | ms |
| 3 | lumen | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | ms |
| 4 | lumen | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | s |
| 5 | pulse | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | ms (CONFIRMED FLAKER) |
| 6 | pulse | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms | ms |
| 7 | pulse | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_fifty_milliseconds | 50 ms | ms |
| 8 | pulse | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | s |
| 9 | ray | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_two_milliseconds | 2 ms | ms |
| 10 | ray | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_five_milliseconds | 5 ms | ms |
| 11 | ray | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | ms |
| 12 | ray | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | s |
| 13 | strata | tests/slice_01_walking_skeleton.rs | ingest_p95_latency_under_five_milliseconds | 5 ms | ms |
| 14 | strata | tests/v1_slice_01_wal_durability.rs | ingest_p95_latency_under_eight_milliseconds | 8 ms | ms |
| 15 | strata | tests/slice_02_structured_query.rs | query_p95_latency_under_ten_milliseconds | 10 ms | ms |
| 16 | strata | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | s |
| 17 | cinder | tests/slice_01_walking_skeleton.rs | get_tier_p95_latency_under_fifty_microseconds | 50 µs | µs |
| 18 | cinder | tests/v1_slice_01_wal_durability.rs | place_p95_latency_under_two_hundred_microseconds | 200 µs | µs |
| 19 | cinder | tests/slice_02_lifecycle.rs | evaluate_p95_latency_under_five_milliseconds | 5 ms | ms |
| 20 | cinder | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_seconds | 5 s | s |
| 21 | sluice | tests/slice_01_walking_skeleton.rs | enqueue_and_dequeue_p95_under_fifty_microseconds | 50 µs | µs |
| 22 | sluice | tests/v1_slice_01_wal_durability.rs | enqueue_p95_latency_under_three_hundred_microseconds | 300 µs | µs |
| 23 | sluice | tests/v1_slice_02_snapshot.rs | recovery_p95_latency_under_five_hundred_milliseconds | 500 ms | ms |
| 24 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | persist_p95_latency_under_two_milliseconds | 2 ms | ms |
| 25 | beacon | tests/v1_slice_02_filebacked_durable_recovery.rs | recovery_p95_latency_under_one_and_a_half_seconds | 1.5 s | s |
| 26 | augur | tests/slice_01_zscore.rs | observe_p95_latency_under_ten_microseconds | 10 µs | µs |
| 27 | augur | tests/slice_02_rare_event.rs | observe_p95_latency_under_twenty_microseconds | 20 µs | µs |
| 28 | aegis | tests/slice_01_validate.rs | validate_p95_latency_under_two_milliseconds | 2 ms | ms |

Total: 28 wall-clock KPI tests across 11 crates (lumen, pulse, ray, strata,
cinder, sluice, beacon, augur, aegis).

### Explicitly OUT of scope (functional, not wall-clock)

- `aperture::slice_06_forwarding_sink::forwarding_sink_accepted_event_includes_downstream_latency_ms_field`
  — asserts the PRESENCE of a `latency_ms` field in a structured stderr event; it
  does not measure wall-clock time against a temporal threshold. NOT gated.
- Any test that uses `Instant`/`elapsed()` only for timeout or wait-until-ready
  scaffolding (for example, integration-suite restart tests, aperture graceful
  shutdown / backpressure, spark flush deadline) without asserting a p95 latency
  threshold. These appeared in the broad initial grep but carry no
  `fn *_p95_*` / `*_latency_under_*` threshold assertion and are NOT gated.
- Any test asserting functional recovery correctness ("recovery produces identical
  state") rather than recovery latency. NOT gated.

### Grounding method (auditable)

1. Broad grep over `crates/*/tests/**` for `Instant::now`, `p95`, `latency_under`,
   `throughput`, `elapsed()` returned 48 candidate files.
2. Narrowed to test-function signatures matching
   `fn [a-z0-9_]*(p95|latency|throughput|elapsed|under_*(milli|micro|second|nano|ms))`,
   yielding the 28 p95/latency functions above plus the one aperture functional
   match (excluded on inspection).
3. Separate grep for throughput/per-second/qps named tests returned no matches.
4. Spot-confirmed the body shape of `lumen ingest_p95_latency_under_three_milliseconds`
   (warm-up loop, 1000 samples, `samples[950]` p95, `assert!(p95 <= 3_000 ...)`)
   and the two confirmed flakers' signatures.

## Upstream Changes

- None. No DISCOVER or DIVERGE artifacts exist for this feature.

## Risks

- **No DIVERGE grounding** (no `recommendation.md` / `job-analysis.md`). Acceptable
  for a narrow, well-understood test-infra change; the job is unambiguous and the
  inventory is empirically grounded. Noted per the workflow gate.
- **Inventory drift**: a perf test added between DISCUSS and DELIVER would be
  ungated. Mitigation: US-04 AC and ADR-0058 (US-05) make the pattern the standard;
  DESIGN must reconcile against this inventory at DELIVER.
- **Shared-helper wiring cost**: a single helper across 11 integration-test crates
  needs a shared dev-dependency or per-crate `common/mod.rs`. Flag 1 leaves the
  inline-versus-helper trade to DESIGN; inline avoids this risk for the first slice.
