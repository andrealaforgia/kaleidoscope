# beacon-durable-alert-state-v0 — KPI instrumentation

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Source KPIs**: `discuss/outcome-kpis.md` (KPI1, KPI2, KPI3, KPI4)

## Instrumentation posture

beacon is a library plus the beacon-server binary, with a single
local-filesystem dependency and no network surface for this feature. So
there is no Prometheus scrape, no dashboard, no alerting webhook. The
"instrumentation" for all four KPIs is the test suite running under CI
Gate 1, with Gate 5 (the NEW `gate-5-mutants-beacon` job, A1) as the
test-quality probe that proves those tests actually measure what they
claim. The CI pass/fail IS the measurement; a red gate IS the alert.
The one runtime signal the feature adds is the startup log line
`recovered alert state rules_recovered=N firing=F pending=P` (DESIGN
DD8 step 2), a human-readable recovery confirmation for the operator.

## Per-KPI -> gate mapping

| KPI | Type | Budget | Measured by (test) | Gate |
|-----|------|--------|--------------------|------|
| KPI1 — state-recovery completeness (North Star, with KPI2) | guardrail | 100% of rule states recovered across restart, including the `since` instant on Pending/Firing (baseline 0% today) | round-trip recovery test: per-rule assert recovered state == pre-restart state | Gate 1 (correctness) + Gate 5 (probe) |
| KPI2 — zero spurious re-fire (North Star, with KPI1) | guardrail | 0 re-fires (down from 1 per firing rule per restart) | restart-survival test: a still-active recovered Firing rule emits no new Firing after recovery | Gate 1 (correctness) + Gate 5 (probe) |
| KPI3 — persist latency | leading | persist/`put` p95 <= 2 ms, 1000 trials, debug, ubuntu-latest | persist micro-benchmark over 1000 WAL appends | Gate 1 |
| KPI4 — recover latency | leading | `open()`/recover p95 <= 1.5 s, 20 trials, 10000 states, debug, ubuntu-latest | recovery micro-benchmark over 20 trials | Gate 1 |

## Metric hierarchy (from outcome-kpis.md)

- **North Star**: alert-state durability completeness — KPI1 (100% of
  rule states, state + pending-since, recovered correctly across
  restart) AND KPI2 (zero spurious re-fires) together. This is the whole
  point of the feature: an operator restart never re-pages on-call for
  an incident already in progress. Both are Gate 1 correctness tests;
  their mutation coverage — the proof that the keyed-latest-wins replay
  (DD4) is actually exercised, not vacuously green — is enforced by the
  new `gate-5-mutants-beacon` job.
- **Leading indicators**: KPI3 persist latency and KPI4 recover latency
  — the durability must be cheap enough to run on every transition and
  fast enough not to delay startup. Each is a Gate 1 timed test.
- **Guardrails (must NOT degrade)**: `transition()` stays a pure
  function with zero I/O (ADR-0037), verified by the existing
  pure-function property tests continuing to pass unchanged; and
  steady-state (no-restart) evaluator behaviour stays identical to today
  (no new emissions, no changed timing). Both run under Gate 1.

## CI-realism calibration (the load-bearing detail)

Both latency budgets are pinned to GitHub Actions `ubuntu-latest` from
commit one, NOT a fast workstation — the explicit lesson from the
2026-05-19 lumen/cinder timing-bump batch, where locally-calibrated
budgets failed on CI for ~2 weeks before being raised. A rule state is a
small `enum + Option<SystemTime>` keyed by rule name, materially lighter
than the storage pillars' OTLP record batches, so the budgets are set
tighter than any pillar (persist <= 2 ms vs strata's 8 ms ingest;
recover <= 1.5 s vs the pillars' 2.5 s) while still carrying honest CI
margin. The two correctness KPIs (1 and 2) are the primary gate; the
latency KPIs (3 and 4) are secondary and must not be chased at the cost
of correctness.

## Guardrail alerting model

| KPI | Alert condition | Channel | Response |
|-----|-----------------|---------|----------|
| KPI1 | round-trip recovery test fails (any rule's state or `since` not recovered) | red Gate 1 in CI | block merge / peer-review escalation — North Star correctness breached |
| KPI2 | restart-survival test sees a spurious Firing emission after recovery | red Gate 1 in CI | block merge — North Star correctness breached |
| KPI3 | persist p95 > 2 ms on ubuntu-latest | red Gate 1 in CI | investigate before merge; do not raise budget without measured justification (2026-05-19 discipline) |
| KPI4 | recover p95 > 1.5 s on ubuntu-latest | red Gate 1 in CI | same as KPI3 |

No external dashboards or pagers. For a library-plus-binary with one
local-filesystem dependency, the test suite under CI plus the startup
recovery log line is the complete and correct instrumentation surface.
