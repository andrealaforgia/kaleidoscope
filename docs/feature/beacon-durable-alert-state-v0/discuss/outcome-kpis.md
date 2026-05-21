# Outcome KPIs — beacon-durable-alert-state-v0

## Feature: beacon-durable-alert-state-v0

### Objective

When an operator restarts beacon-server, every in-flight alert state
survives intact and on-call is never re-paged for an incident already
in progress. Closed by the value slice (slice 02).

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | beacon-server (on behalf of operator Priya) | recovers in-flight rule states across a restart | 100% of rule states recovered, including the `since` instant on Pending/Firing | 0% today (every rule resets to Inactive) | round-trip recovery test asserting recovered state == pre-restart state for every rule | Leading (correctness guardrail) |
| 2 | on-call operator | is re-paged for a still-firing alert after a restart | 0 spurious re-fires (down from 1 per firing rule per restart today) | 1 re-page per firing rule per restart | test: a Firing rule whose condition stays active emits no new Firing after recovery | Leading (correctness guardrail) |
| 3 | beacon-server | persists a state transition (WAL append) | p95 <= 2 ms on GitHub Actions ubuntu-latest | n/a (no persistence today) | `append`/persist p95 over 1000 trials in CI debug build | Leading (latency budget) |
| 4 | beacon-server | recovers all rule states at startup | p95 <= 1.5 s for 10000 rule states on GitHub Actions ubuntu-latest | n/a | `open()`/recover p95 over 20 trials in CI debug build | Leading (latency budget) |

### Metric Hierarchy

- **North Star**: alert-state durability completeness — 100% of rule
  states (state + pending-since) recovered correctly across restart,
  with zero spurious re-fires (KPI 1 + KPI 2 together).
- **Leading Indicators**: persist p95 (KPI 3), recover p95 (KPI 4) —
  the durability must be cheap enough to run on every transition and
  fast enough not to delay startup.
- **Guardrail Metrics** (must NOT degrade):
  - `transition()` stays a pure function — zero I/O inside it
    (ADR-0037). Verified by the existing pure-function property tests
    continuing to pass unchanged.
  - Steady-state (no restart) evaluator behaviour identical to today
    after slice 01 — no new emissions, no changed timing.

### CI realism note (project lesson, 2026-05-19)

All latency budgets (KPI 3, KPI 4) are pinned to GitHub Actions
`ubuntu-latest`, not a workstation. The storage pillars use ingest p95
in the low-millisecond range and recovery p95 <= 2.5 s. A rule-state
store is far lighter (a small map of `enum + Option<SystemTime>` keyed
by rule name, not OTLP record batches), so the budgets are tighter
(persist <= 2 ms, recover <= 1.5 s) while still carrying honest CI
margin. The two correctness KPIs (1 and 2) are the primary gate; the
latency KPIs are secondary and must not be chased at the cost of
correctness.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | round-trip recovery test | per-rule assert recovered == pre-restart | every CI run | beacon-server tests |
| 2 | restart-survival test | assert no Firing emission for still-active recovered Firing rule | every CI run | beacon-server tests |
| 3 | persist micro-benchmark | p95 over 1000 trials, debug build | every CI run | beacon-state-store tests |
| 4 | recovery micro-benchmark | p95 over 20 trials, 10000 states, debug build | every CI run | beacon-state-store tests |

### Hypothesis

We believe that holding per-rule state in a durable `RuleStateStore`
(WAL + snapshot + recovery) for beacon-server will achieve zero
re-paging of in-flight alerts across restarts. We will know this is
true when beacon-server recovers 100% of rule states with their
pending-since instants and emits zero spurious Firing notifications for
rules whose condition was already active before the restart.

### Handoff to DEVOPS (platform-architect)

- **Data collection**: persist-latency and recovery-latency micro-
  benchmarks emitted from the test suite; recovery-count log line at
  startup.
- **Alerting thresholds**: persist p95 > 2 ms or recover p95 > 1.5 s on
  ubuntu-latest is a regression signal.
- **Baseline**: durability completeness baseline is 0% today — establish
  the 100% target with the round-trip test before release.
