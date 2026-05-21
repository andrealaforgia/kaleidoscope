# Outcome KPIs: durable-stores-integration-v0

## Feature: second-triad (pulse + ray + strata) durable composition guarantee

### Objective

Give Priya the same restart-survival trust for the metrics + traces + profiles
triad that she already has for the first triad, so the durable storage plane is
trustworthy end-to-end before the next milestone.

### CI realism note (hard project lesson, 2026-05-19)

All timing budgets below are stated against GitHub Actions ubuntu-latest, not a
fast workstation. For an integration test the dominant KPI is a correctness
guardrail (compose-and-recover fidelity, zero cross-bucket leakage); any timing
budget is kept deliberately generous and is a guardrail, never the north star.

The crate-level v1 suites already budget pulse ingest p95 <= 2 ms, ray <= 5 ms,
strata <= 8 ms, recovery <= 2.5 s. This integration test does not re-assert
those per-crate budgets; it asserts the composed durability guarantee.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Priya (storage-plane trust owner) | trusts that metrics, traces and profiles all recover under one tenant after a restart, because a composed durable-path test proves it | 100% compose-and-recover fidelity, zero cross-bucket leakage (2 tests green) | 0% — no composed durable-path evidence for the second triad exists today | `cargo test -p integration-suite --test v1_three_durable_stores_compose` -> `test result: ok` | Leading (correctness guardrail) |
| 2 | Any maintainer touching aegis | is alerted at build time if the cross-crate `TenantId` identity contract drifts across the three signal pillars | 100% — drift produces a compile failure, never a silent runtime divergence | No signal-pillar identity tripwire exists today | the same test target compiles and passes; a shape change fails to compile | Leading (regression guard) |

### Metric Hierarchy

- **North Star**: durability completeness of the composed triad — both tests in
  `v1_three_durable_stores_compose` pass, proving metrics + traces + profiles
  recover identically across restart under one tenant with zero leakage.
- **Leading Indicators**: per-pillar recovery counts match pre-restart counts;
  `globex` data never appears under `acme` in any pillar; the identity-contract
  test compiles.
- **Guardrail Metrics** (must NOT degrade):
  - Total wall-clock for the test target stays well under 30 s on
    ubuntu-latest (generous; the real work is microseconds of I/O on tmpfs).
    This is a guardrail, not a target — it exists only to catch a pathological
    regression (e.g. an accidental fsync-per-record storm).
  - The existing first-triad test and the rest of the integration suite remain
    green (no shared-helper regression).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| KPI-1 | `cargo test -p integration-suite --test v1_three_durable_stores_compose` stdout | CI run on every push to main (feedback, not a gate — pure trunk-based) | Per push | platform-architect (DEVOPS) |
| KPI-2 | `cargo build`/`cargo test` of the same target | Compile + run in the same CI step | Per push | platform-architect (DEVOPS) |
| Guardrail (wall-clock) | cargo test timing line on ubuntu-latest | CI run | Per push | platform-architect (DEVOPS) |

### Hypothesis

We believe that a composed durable-path integration test for pulse + ray +
strata, mirroring the first-triad precedent, will give Priya restart-survival
trust for all three signal pillars. We will know this is true when the test
target reports `test result: ok` with both tests passing and zero cross-bucket
leakage, on ubuntu-latest, on every push to main.

### Handoff to DEVOPS

- Data collection: capture the `test result` line for `v1_three_durable_stores_compose` in CI.
- Dashboard/monitoring: none required — a green CI run on main is the signal. Trust matrix can be a manual checklist (triad 1 green, triad 2 green).
- Alerting thresholds: any non-zero failure count in the target, or a compile failure of the target, is the alert.
- Baseline: zero composed durable-path evidence for the second triad exists before this feature; baseline is 0%.
</content>
