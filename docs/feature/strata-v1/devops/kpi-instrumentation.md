# Strata v1 — KPI instrumentation

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Source KPIs**: `discuss/outcome-kpis.md` (KPI1, KPI2, KPI3)

## Instrumentation posture

Strata v1 is a library crate with no runtime service, so there is no
dashboard, no Prometheus scrape, no alerting webhook. The
"instrumentation" for all three KPIs is the acceptance-test suite
running under CI Gate 1, with Gate 5 (the NEW `gate-5-mutants-strata`
job, A1) as the test-quality probe that proves those tests actually
measure what they claim. The CI pass/fail IS the measurement; a red
gate IS the alert.

## Per-KPI -> gate mapping

| KPI | Type | Budget | Measured by (test) | Gate | Verified against outcome-kpis.md |
|-----|------|--------|--------------------|------|----------------------------------|
| KPI1 — ingest latency | leading | p95 <= 8 ms, 1 000 trials, debug, 100-profile batch (8 ms, NOT Ray's 5 ms or Pulse's 2 ms, because a Profile is the heaviest payload of any pillar — set from the field set, the highest budget of the six) | `strata::tests::v1_slice_01_wal_durability::ingest_p95_latency_under_eight_milliseconds` | Gate 1 | YES — test name + method present in outcome-kpis.md §KPI1 |
| KPI2 — recovery time | leading | p95 <= 2.5 s, 20 trials, debug, 2 000 profiles snapshot+WAL | `strata::tests::v1_slice_02_snapshot::recovery_p95_latency_under_two_and_a_half_seconds` | Gate 1 | YES — test name + method present in outcome-kpis.md §KPI2 |
| KPI3 — durability completeness (North Star) | guardrail | 100% survive drop-and-reopen, zero loss, zero duplication, full sample payload intact | `strata::tests::v1_slice_02_snapshot` parallel-store comparison | Gate 1 (correctness) + Gate 5 (probe) | YES — parallel-store comparison method present in outcome-kpis.md §KPI3 |

## Metric hierarchy (from outcome-kpis.md)

- **North Star**: KPI3 durability completeness — the whole point of
  the v1 adapter is that profiles survive restart with their full
  payload. Enforced by the Gate 1 parallel-store comparison test,
  which MUST cover profiles surviving across WAL only AND across
  snapshot+WAL, AND the empty-`service.name` profile that is
  intentionally dropped (absent both before and after recovery). Its
  mutation coverage is enforced by the new `gate-5-mutants-strata`
  job — the only enforcer of the single-`apply_ingest` no-drift
  guarantee (DESIGN DD4).
- **Leading indicators**: KPI1 ingest latency and KPI2 recovery time —
  they predict whether durability is usable in a long-lived process
  given the heavy payload. Each is a Gate 1 timed test. A `Profile`
  carries `samples: Vec<Sample>` (each a stack of `Vec<u64>` location
  ids, `Vec<i64>` values, a `BTreeMap` attribute map) plus the pprof
  tables and `string_table: Vec<String>` — materially more
  JSON-encoding work per batch than 100 spans or 100 metric points,
  which is exactly why KPI1 is 8 ms.
- **Guardrails**: KPI3 must stay at 100% regardless of timing; KPI1
  and KPI2 must not regress past budget on CI.

## CI-realism calibration (the load-bearing detail)

Both latency budgets are set against GitHub Actions `ubuntu-latest`
from commit one, NOT a fast workstation. This is the explicit lesson
from the 2026-05-19 lumen/cinder timing-bump batch, where budgets
calibrated locally failed on CI for ~2 weeks before being raised. KPI2
(2.5 s) adopts the post-bump Pulse v1 / Ray v1 / Cinder v1 / Lumen v1
figure up front. KPI1 (8 ms) is set deliberately HIGHER than Ray's
5 ms and Pulse's 2 ms directly from the `Profile` field set — the
heaviest payload of any pillar — with the GitHub Actions IO +
serialise variance (roughly twice the local figure) baked in. Setting
it right from DISCUSS rather than bumping it at DELIVER is the
discipline the 2026-05-19 batch taught. The "alert" (a red Gate 1)
therefore fires on a genuine regression, not on substrate noise.

## Guardrail alerting model

| KPI | Alert condition | Channel | Response |
|-----|-----------------|---------|----------|
| KPI3 | parallel-store comparison test fails (WAL-only path, snapshot+WAL path, or empty-service edge case) | red Gate 1 in CI | block merge / peer-review escalation — correctness invariant breached |
| KPI1 | ingest p95 > 8 ms | red Gate 1 in CI | investigate before merge; do not raise budget without measured justification (the 2026-05-19 discipline) |
| KPI2 | recovery p95 > 2.5 s | red Gate 1 in CI | same as KPI1 |

No external dashboards or pagers — for a library with one local-FS
dependency, the test suite under CI is the complete and correct
instrumentation surface.
