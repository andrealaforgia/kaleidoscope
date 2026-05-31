# Outcome KPIs: perf-kpi-ci-gating-v0

## Feature: perf-kpi-ci-gating-v0

### Objective

Make the local pre-commit hook fast and deterministic by skipping wall-clock KPI
tests when the maintainer has not opted in, while keeping those KPIs enforced as
real gates in CI where the thresholds were tuned. Self-bootstrapping: once
delivered, the hook stops flaking.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| K1 | Maintainer running the local pre-commit hook | completes pre-commit runs without wall-clock perf failures | zero perf-flake failures in local hook runs | repeated load-induced flakes (e.g. 4 to 6 ms vs 3 ms) | local hook run observation under load | Leading |
| K2 | CI gate-1-test job | runs the wall-clock KPI tests (not skipped) | 28 of 28 gated tests execute in CI; zero skipped | n/a (tests currently run in CI; preserved) | gate-1-test CI log shows no skip note for gated tests | Leading |
| K3 | Kaleidoscope KPI gate | retains identical threshold assertions | zero threshold literals changed | current threshold set (see inventory) | diff review of gated test files | Guardrail |
| K4 | Local workspace test run | skips all wall-clock KPI tests uniformly | 28 of 28 tests gated; zero ungated stragglers | 0 of 28 gated today | grep of 11 crates against the inventory | Leading |
| K5 | Maintainer committing changes | commits without `--no-verify` for perf flakes | zero perf-flake bypasses after delivery | repeated `--no-verify` bypasses | absence of perf-flake bypass notes in wave logs | Leading (North Star) |

### Metric Hierarchy

- **North Star**: K5, zero `git commit --no-verify` bypasses attributable to
  wall-clock perf flakes after delivery. This is the behaviour change that the
  whole feature exists to produce; it captures restored hook discipline.
- **Leading Indicators**: K1 (local hook produces no perf failures) and K4
  (complete uniform coverage). These predict K5: if no perf test can fail locally,
  there is no flake to bypass.
- **Guardrail Metrics**: K3 (zero thresholds changed) and K2 (100% of gated tests
  still run in CI). These must NOT degrade. K3 protects gate strictness; K2
  protects gate existence.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| K1 | local pre-commit hook output | run hook under parallel-build load, observe perf failures | per development loop, spot-checked | maintainer |
| K2 | gate-1-test CI log | inspect log for skip note across gated tests | every CI run, spot-checked post-delivery | maintainer |
| K3 | feature git diff | review diff of gated test files for threshold literals | once at delivery, again on any perf-test change | reviewer |
| K4 | source tree | grep 11 crates against the inventory in wave-decisions.md | once at delivery; on any new perf test | maintainer |
| K5 | wave-decisions logs and commit history | check for perf-flake bypass notes | ongoing, reviewed at feature close | maintainer |

### Hypothesis

We believe that gating wall-clock KPI tests behind `KALEIDOSCOPE_PERF_TESTS`
(skip locally, run in CI) for the Kaleidoscope maintainer will achieve a fast,
deterministic pre-commit hook with a still-real CI KPI gate. We will know this is
true when the maintainer commits with zero perf-flake `--no-verify` bypasses (K5)
while CI continues to run all 28 gated tests (K2) against unchanged thresholds
(K3).
