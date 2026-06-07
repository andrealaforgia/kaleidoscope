# Outcome KPIs — speed-up-local-precommit-v0

## Feature: speed-up-local-precommit-v0

### Objective

Give the committing maintainer a local pre-commit gate that finishes in
five minutes or less, while the deep I/O-bound suite gates in CI and a
regular cadence keeps eyes on it — so frequent commits stay in-flow and
`--no-verify` stops being tempting.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Committing maintainer (human / crafter agent) | completes the full pre-commit hook | p95 wall-clock <= 5 min | 10-20 min (full `--all-targets --workspace`) | `time` around the hook over a sample of real commits | Leading |
| 2 | Committing maintainer | has cheap/common mistakes (unit break, fmt, clippy) still caught locally | 100% of unit/fmt/clippy failure classes still rejected | 100% caught today (at 10-20 min cost) | negative-control injection per class (US-02 scenarios) | Guardrail |
| 3 | Committing maintainer | retains deep-suite coverage on every push | 100% of deep suite still executed in CI; 0 tests deleted | deep suite runs in CI today (and redundantly locally) | CI gate-1 invocation diff + crate test-file count | Guardrail |
| 4 | Committing maintainer (human / agent) | detects a deep-only CI regression via the cadence | within 1 cadence interval (target same session / < 1 h) | unbounded once local block removed without mitigation | presence of watch command + documented cadence; time-to-detection on next deep-only red | Leading |
| 5 | Committing maintainer | reaches for `git commit --no-verify` | trends toward 0 (fast hook removes the incentive) | non-zero today (long wait drives bypass) | informal observation / commit-author recollection (no instrumentation) | Secondary |

### Metric Hierarchy

- **North Star**: KPI 1 — pre-commit hook p95 wall-clock <= 5 min.
- **Leading Indicators**: KPI 4 (deep-only regression detection latency
  via the cadence); KPI 5 (declining `--no-verify` reach).
- **Guardrail Metrics** (must NOT degrade): KPI 2 (cheap-mistake
  detection stays 100%); KPI 3 (deep coverage stays 100% in CI, zero test
  deletions, CI not weakened).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | local hook run | `time` wrapper or hook self-timing | per-commit sample after DELIVER | maintainer |
| 2 | local hook run | inject broken unit/fmt/clippy, observe reject | once at DELIVER (acceptance) | acceptance-designer |
| 3 | ci.yml + crate tree | diff gate-1 invocation; count `tests/*.rs` | once at DELIVER + on review | reviewer / DESIGN |
| 4 | CI Actions / `gh` | watch command output on the cadence | per documented cadence | maintainer / agent |
| 5 | git history / recollection | qualitative | ad hoc | maintainer |

### Hypothesis

We believe that slimming the local pre-commit hook to a fast test subset
(while keeping fmt, clippy, deny) and establishing a CI-results-watching
cadence, for the committing maintainer, will achieve a sub-5-minute local
gate without losing cheap-mistake detection or deep coverage. We will know
this is true when the maintainer completes the hook in <= 5 min (p95),
unit/fmt/clippy breaks are still rejected locally, the deep suite still
runs 100% in CI, and deep-only regressions are caught within one cadence
interval.

### Notes on measurability (smell-test honesty)

- KPIs 1-4 are measurable with current tooling (`time`, a diff, the
  `gh`/watch command). KPI 5 is qualitative by nature (no per-developer
  `--no-verify` instrumentation exists, and adding it is out of scope and
  against the project's no-effort-instrumentation grain) — it is recorded
  as a secondary, observation-only signal, not a gate.
- The 5-minute target is Andrea's explicit acceptance bar ("I would accept
  5 mins"), carried verbatim from the origin brief.

## Handoff to DEVOPS (platform-architect)

- **Data collection**: the local hook should make its elapsed wall-clock
  visible (so KPI 1 is observable without an external `time` wrapper) —
  DESIGN/DEVOPS decide whether the hook self-reports timing.
- **Watch mechanism**: the CI-results-watching command/script (D3) and its
  documented cadence are DEVOPS-owned infrastructure; KPI 4 depends on it.
- **No alerting thresholds / dashboards** beyond the watch command — this
  is a local-developer-experience feature, not a production SLO.
