# Story Map: perf-kpi-ci-gating-v0

## User: Kaleidoscope maintainer (working the local development loop and reading CI)

## Goal: keep the pre-commit hook fast and deterministic while the wall-clock KPI gate stays real in CI

## Backbone

The journey is a test-infra delivery: identify what to gate, add the guard,
enable enforcement in the right place, and verify nothing slipped or weakened.

| Identify | Guard | CI-enable | Verify |
|----------|-------|-----------|--------|
| Inventory all wall-clock KPI tests (US-04 scope) | Add env-var guard so tests skip locally when not opted in (US-01) | Set the opt-in variable in the gate-1-test job (US-02) | Confirm thresholds unchanged (US-03) |
| Distinguish wall-clock from functional tests (US-04) | Apply the guard uniformly to all 28 tests (US-04) | | Confirm complete coverage, no stragglers (US-04) |
| | | | Document the pattern for future perf tests (US-05) |

---

### Walking Skeleton

The thinnest end-to-end slice that proves the whole mechanism on a single test:

- **Identify**: pin one confirmed flaker, `lumen::v1_slice_01_wal_durability::ingest_p95_latency_under_three_milliseconds`.
- **Guard**: add the env-var early-return guard to that one test (US-01).
- **CI-enable**: set `KALEIDOSCOPE_PERF_TESTS` in the gate-1-test job (US-02).
- **Verify**: locally the test skips (hook green under load); in CI it runs and
  enforces 3 ms (US-03 confirms the threshold literal is untouched).

This skeleton exercises every moving part (guard, local skip, CI opt-in,
threshold preservation) against one test before fanning out to all 28. US-01 is
the walking skeleton story.

### Release 1: deterministic local hook, real CI gate (primary outcome)

- Stories: US-01 (local skip), US-02 (CI enforces), US-03 (thresholds unchanged),
  US-04 (complete coverage of all 28 tests).
- Target outcome: zero perf-flake bypasses locally (K1, K5) while 100% of gated
  tests still run in CI (K2) with zero thresholds changed (K3) and complete
  coverage (K4).
- Rationale: this is the whole point of the feature. The local hook becomes
  deterministic and the KPI gate stays real, only relocated to the controlled
  environment.

### Release 2: documented pattern (durability outcome)

- Stories: US-05 (mechanism documented in ADR-0058).
- Target outcome: future perf tests follow the same guard, preventing regression
  of the flake problem.
- Rationale: lower urgency than Release 1; protects the gain over time rather than
  delivering it. Optional and non-blocking.

---

## Priority Rationale

Priority order, by outcome impact and dependency:

1. **US-01 (walking skeleton, P1)** — highest leverage. Validates the entire
   guard-plus-skip mechanism end-to-end on one test. Until this works, nothing
   else matters. Directly attacks the bypass pain (K1, K5).
2. **US-02 (P1)** — must land in the same release as US-01. Without it, making
   tests skippable risks them never running anywhere. Derisks the fatal
   assumption that "skippable" does not mean "silently disabled" (K2). Depends on
   the guard contract from US-01.
3. **US-04 (P1)** — fans the proven mechanism out to all 28 tests. High value
   (eliminates every residual flake source) but mechanical once US-01 proves the
   pattern. Depends on US-01.
4. **US-03 (P1)** — a guardrail verified throughout US-01 and US-04 rather than a
   separate build step. Zero effort beyond disciplined diffing; high value because
   it protects gate integrity (K3).
5. **US-05 (P2, optional)** — durability. Documents the pattern so the fix sticks.
   Lower urgency; does not block the release that delivers the outcome. Depends on
   the DESIGN decision on the mechanism (flags 1, 3, 6).

Tie-breaking applied: Walking Skeleton (US-01) first, then the riskiest assumption
(US-02, "skippable does not mean disabled"), then highest-value fan-out (US-04).

## Scope Assessment: PASS

- 5 user stories (US-05 optional), under the 10-story oversize signal.
- Bounded contexts touched: test files, one CI workflow job, one ADR. The change
  is cross-cutting across 11 crates' test directories but is a single uniform edit
  pattern, not 11 independent outcomes. It is one coherent deliverable.
- No production source change. No walking-skeleton integration sprawl.
- Right-sized: one mechanism, applied uniformly, demonstrable in a single session.

No split required.
