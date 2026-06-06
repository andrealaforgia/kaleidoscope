# Outcome KPIs: perf-kpi-ci-non-gating-v0

## Feature: perf-kpi-ci-non-gating-v0

### Objective

Make a red `gate-1-test` build mean "a real correctness regression",
never "the runner's disk fsync was slow this minute", while keeping the
wall-clock perf KPIs visible as a tracked, non-gating signal — so the
team never learns to ignore red.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Maintainer + PR contributors | Stop seeing perf-induced false reds in `gate-1-test` | 0 perf-attributable `gate-1-test` failures over a rolling 30-day window | Recurring (the named `place_p95` flake; "old, annoying problem") | Classification of `gate-1-test` failures in GitHub Actions run history | Leading (Outcome) |
| 2 | Maintainer reviewing perf trend | Reads per-run p95 numbers from a dedicated job | 100% of `main` CI runs produce readable p95 numbers in the `perf-kpis` job log | Numbers only emitted inside the gating job, inseparable from gate outcome | Presence of p95 values in `perf-kpis` job log per run | Leading (Secondary) |
| 3 | Maintainer interpreting Gate 1 status | Treats every Gate 1 RED as a real defect | 100% of Gate 1 reds are correctness-attributable (0% perf) | Gate 1 reds are a mix of correctness + perf noise, not separable | Classification of `gate-1-test` failures over 30 days (complement of KPI-1) | Leading (Outcome) |
| 4 (guardrail) | Future contributors | Refrain from threshold-chasing durable-op budgets | 0 threshold-raise commits to durable-op KPI literals after the honesty note lands | Past pressure toward threshold-raises (project memory) | Git history of durable-op threshold literal changes | Guardrail |

### Metric Hierarchy

- **North Star**: KPI-1 — zero perf-induced false reds in `gate-1-test`.
  This is the single metric that captures the feature's reason to exist.
- **Leading Indicators**: KPI-3 (every Gate 1 red is correctness, the
  trust-in-red signal); KPI-2 (perf numbers stay visible, predicting that
  a real sustained regression will be observed rather than missed).
- **Guardrail Metrics**: KPI-4 (no threshold-chasing — must NOT degrade);
  and the implicit guardrail that **correctness gating is not loosened**
  (US-03): the count of correctness regressions that reach `main`
  undetected by `gate-1-test` must remain 0.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| 1 | GitHub Actions run history (`gate-1-test`) | Classify each failure as correctness vs perf | Per run; reviewed rolling 30 days | Maintainer |
| 2 | `perf-kpis` job logs | Confirm p95 values present and readable | Per `main` run | Maintainer |
| 3 | GitHub Actions run history (`gate-1-test`) | Classify each red as correctness vs perf | Per run; reviewed rolling 30 days | Maintainer |
| 4 | Git log on durable-op test files | Grep threshold literals for post-feature changes | On change; reviewed quarterly | Maintainer |

### Hypothesis

We believe that **moving the wall-clock perf KPIs out of the
build-gating `gate-1-test` job and into a separate non-gating
`perf-kpis` job** for **the maintainer and PR contributors** will
achieve **a trustworthy red build (every red is a real regression) with
perf still visible**.

We will know this is true when **the maintainer sees 0
perf-attributable Gate-1 reds over 30 days (KPI-1) while still reading
per-run p95 numbers in the `perf-kpis` log on 100% of `main` runs
(KPI-2)**.

### Handoff to DEVOPS (platform-architect)

1. **Data collection**: the `perf-kpis` job must emit each crate's p95 to
   the job log in a form a human can scan (and ideally a form a later
   script could parse). DEVOPS decides log-only vs an uploaded artefact.
2. **Dashboard / monitoring**: none required at v0; the job log is the
   surface. A future trend dashboard is out of scope (note as possible
   follow-up).
3. **Alerting thresholds**: none. The perf job is non-gating by design;
   it must not page or block. Guardrail: it must never fail the workflow.
4. **Baseline measurement**: capture the first few `perf-kpis` runs as the
   informal baseline for the durable-op p95 numbers on `ubuntu-latest`, to
   anchor future "is this a sustained regression?" judgements (US-02
   example 3).
