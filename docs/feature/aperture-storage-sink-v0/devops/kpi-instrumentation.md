# aperture-storage-sink-v0 - KPI instrumentation

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Source KPIs**: `discuss/outcome-kpis.md` (KPI-1..KPI-5)

## Instrumentation posture

The `aperture-storage-sink` crate is exercised in CI as a library plus
a host binary booted under integration tests; there is no Kaleidoscope-
hosted dashboard, no Prometheus scrape, no alerting webhook (the gateway
runtime is operator-owned). The "instrumentation" for every KPI is the
acceptance-test suite running under CI Gate 1, with the NEW
`gate-5-mutants-aperture-storage-sink` job (A1) as the test-quality
probe that proves those tests actually measure what they claim. The CI
pass/fail IS the measurement; a red gate IS the alert. KPI-4's latency
budget is asserted on GitHub Actions `ubuntu-latest`, the same runner
class the budget was calibrated against (the 2026-05-19 CI-realism
lesson), so the budget is honest from the first commit.

## Per-KPI -> gate mapping

| KPI | Type | Budget / target | Measured by (test) | Gate | Verified against outcome-kpis.md |
|-----|------|-----------------|--------------------|------|----------------------------------|
| KPI-1 - logs round-trip fidelity + durability | leading (North Star) | 100% of accepted log records queryable from lumen post-restart, field-faithful, zero loss | logs slice round-trip integration test: export -> restart -> query, assert field equality | Gate 1 (correctness) + Gate 5 (probe) | YES - round-trip method present §KPI-1 |
| KPI-2 - traces round-trip fidelity + durability | leading (North Star) | 100% of accepted spans queryable from ray post-restart; ids, parent, kind, status, events, links faithful | traces slice round-trip integration test | Gate 1 (correctness) + Gate 5 (probe) | YES - §KPI-2 |
| KPI-3 - metrics round-trip fidelity + durability | leading (North Star) | 100% of accepted gauge/sum points queryable from pulse post-restart; name, unit, kind, value, attributes faithful | metrics slice round-trip integration test (gauge + sum; skip-event assertion for unsupported types, DD8) | Gate 1 (correctness) + Gate 5 (probe) | YES - §KPI-3 |
| KPI-4 - accept latency | guardrail | p95 translate + persist <= 50 ms per payload on ubuntu-latest | bench / timing harness on ubuntu-latest | Gate 1 | YES - §KPI-4, pinned to ubuntu-latest |
| KPI-5 - no silent loss / no partial persistence | guardrail | 0 accepted-but-absent records; an untranslatable record is refused (nothing written), naming the field | property test: every accepted record queries back; refused records write nothing (DD7 atomic refusal) | Gate 1 (correctness) + Gate 5 (probe) | YES - §KPI-5 |

## Metric hierarchy (from outcome-kpis.md)

- **North Star**: round-trip fidelity + durability (KPI-1/2/3 at 100%)
  - an OTLP payload accepted by the gateway is queryable from the
  pillar after a process restart with faithful field mapping. Enforced
  by the per-signal Gate 1 round-trip tests; their mutation coverage is
  enforced by the new `gate-5-mutants-aperture-storage-sink` job, the
  only mechanism that proves a behaviourally-mutated translator (a
  flipped severity map, a dropped attribute, a wrong id-length branch)
  is killed.
- **Leading indicators**: per-signal post-restart queryability (KPI-1
  logs, KPI-2 traces, KPI-3 metrics). Each is a Gate 1 round-trip test.
- **Guardrails**: accept-latency budget on CI hardware (KPI-4); no
  silent loss / no partial persistence (KPI-5). KPI-5 is the
  correctness guardrail - fidelity is worthless if records can vanish -
  and is the DD7 atomic-refusal invariant Gate 5 must prove the tests
  can detect.

## CI-realism calibration

KPI-4's p95 translate + persist <= 50 ms is set against GitHub Actions
`ubuntu-latest` from commit one, not a fast workstation - the explicit
2026-05-19 lesson where locally-calibrated budgets failed on CI for
~2 weeks before being raised. Setting it right from DISCUSS rather than
bumping it at DELIVER is the discipline. The "alert" (a red Gate 1)
therefore fires on a genuine regression, not on substrate noise.

## Guardrail alerting model

| KPI | Alert condition | Channel | Response |
|-----|-----------------|---------|----------|
| KPI-1/2/3 | round-trip test fails (record absent post-restart, or a field differs) | red Gate 1 in CI | block merge / peer-review escalation - North Star correctness breached |
| KPI-4 | translate + persist p95 > 50 ms on ubuntu-latest | red Gate 1 in CI | investigate before merge; do not raise budget without measured justification (2026-05-19 discipline) |
| KPI-5 | an accepted record is absent, OR a refused record left a partial write | red Gate 1 in CI | block merge - DD7 atomic-refusal invariant breached |

No external dashboards or pagers on the Kaleidoscope side. For a sink
whose only driven dependency is the local filesystem, the test suite
under CI Gate 1 plus the Gate 5 mutation probe is the complete and
correct instrumentation surface. Operator-side monitoring of the
running gateway is operator-owned, same posture as aperture.
