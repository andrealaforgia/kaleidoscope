# Pulse v1 — KPI instrumentation

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-20
- **Source KPIs**: `discuss/outcome-kpis.md` (KPI1, KPI2, KPI3)

## Instrumentation posture

Pulse v1 is a library crate with no runtime service, so there is no
dashboard, no Prometheus scrape, no alerting webhook. The
"instrumentation" for all three KPIs is the acceptance-test suite
running under CI Gate 1, with Gate 5 (the NEW `gate-5-mutants-pulse`
job, A1) as the test-quality probe that proves those tests actually
measure what they claim. The CI pass/fail IS the measurement; a red
gate IS the alert.

## Per-KPI → gate mapping

| KPI | Type | Budget | Measured by (test) | Gate | Verified |
|-----|------|--------|--------------------|------|----------|
| KPI1 — ingest latency | leading | p95 ≤ 2 ms, 1 000 trials, debug, 100-point batch | `pulse::tests::v1_slice_01_wal_durability::ingest_p95_latency_under_two_milliseconds` | Gate 1 | YES — test name + method present in outcome-kpis.md §KPI1 |
| KPI2 — recovery time | leading | p95 ≤ 2.5 s, 20 trials, debug, 10 000 points snapshot+WAL | `pulse::tests::v1_slice_02_snapshot::recovery_p95_latency_under_two_and_a_half_seconds` | Gate 1 | YES — test name + method present in outcome-kpis.md §KPI2 |
| KPI3 — durability completeness (North Star) | guardrail | 100% survive drop-and-reopen, zero loss, zero duplication | `pulse::tests::v1_slice_02_snapshot` parallel-store equality | Gate 1 (correctness) + Gate 5 (probe) | YES — parallel-store equality method present in outcome-kpis.md §KPI3 |

## Metric hierarchy (from outcome-kpis.md)

- **North Star**: KPI3 durability completeness — the whole point of
  the v1 adapter is that metrics survive restart. Enforced by the
  Gate 1 parallel-store equality test; its mutation coverage is
  enforced by the new `gate-5-mutants-pulse` job.
- **Leading indicators**: KPI1 ingest latency and KPI2 recovery time
  — they predict whether durability is usable in a long-lived
  process. Each is a Gate 1 timed test.
- **Guardrails**: KPI3 must stay at 100% regardless of timing; KPI1
  and KPI2 must not regress past budget on CI.

## CI-realism calibration (the load-bearing detail)

Both latency budgets are set against GitHub Actions `ubuntu-latest`
from commit one, NOT a fast workstation. This is the explicit lesson
from the 2026-05-19 lumen/cinder timing-bump batch, where budgets
calibrated locally failed on CI for ~2 weeks before being raised.
KPI1 (2 ms) and KPI2 (2.5 s) adopt the post-bump Lumen v1 / Cinder v1
figures up front. The "alert" (a red Gate 1) therefore fires on a
genuine regression, not on substrate noise.

## Guardrail alerting model

| KPI | Alert condition | Channel | Response |
|-----|-----------------|---------|----------|
| KPI3 | parallel-store equality test fails | red Gate 1 in CI | block merge / peer-review escalation — correctness invariant breached |
| KPI1 | ingest p95 > 2 ms | red Gate 1 in CI | investigate before merge; do not raise budget without measured justification (the 2026-05-19 discipline) |
| KPI2 | recovery p95 > 2.5 s | red Gate 1 in CI | same as KPI1 |

No external dashboards or pagers — for a library with one local-FS
dependency, the test suite under CI is the complete and correct
instrumentation surface.
