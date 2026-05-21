# durable-stores-integration-v0 — KPI instrumentation

- **Author**: Apex (`nw-platform-architect`), DEVOPS wave, 2026-05-21.
- **Source KPIs**: `discuss/outcome-kpis.md` (North Star: durability
  completeness of the composed triad; KPI-1 correctness guardrail;
  KPI-2 identity-contract regression guard).

This feature is test-only, so KPI instrumentation is not dashboards or
runtime telemetry. It is a mapping from each outcome KPI to the CI gate
that observes it. A green CI run on main is the only signal required
(per the DISCUSS handoff: "none required — a green CI run on main is the
signal"; pure trunk-based, feedback not a gate).

## KPI to gate mapping

| KPI | What it asserts | Gate that observes it | Data source |
|-----|-----------------|-----------------------|-------------|
| North Star (durability completeness) | both tests in `v1_three_durable_stores_compose` pass | Gate 1 | `test result: ok` line for the target |
| KPI-1 (correctness guardrail) | 100% compose-and-recover fidelity, zero cross-bucket leakage | Gate 1 | non-zero failure count = alert |
| KPI-2 (identity-contract regression guard) | one `&aegis::TenantId` flows to all three adapters; a shape drift fails to compile | Gate 1 (compile + run in the same step) | compile failure of the target = alert |
| Guardrail (wall-clock) | target wall-clock well under 30 s on ubuntu-latest | Gate 1 (observed, not enforced) | cargo test timing line |

All four rows resolve to **Gate 1**
(`cargo test --workspace --all-targets --locked`, `ci.yml:182`), which
auto-discovers the new `[[test]]` block (DEVOPS A2). No new
instrumentation, no new job, no dashboard.

## The timing guardrail is a soft ceiling, not a target

The <30s budget is a guardrail the same Gate 1 run observes via the
cargo test timing line. It is deliberately generous: the real work is
microseconds of tmpfs I/O. It exists only to catch a pathological
regression (for example an accidental fsync-per-record storm), per
DESIGN DD4 and the outcome-kpis CI-realism note. It is never a
performance target, and no per-pillar latency budget is re-asserted
here; those live in the crate-level v1 suites (pulse p95 <= 2 ms, ray
<= 5 ms, strata <= 8 ms, recovery <= 2.5 s).

Because the budget is observational, no automated timing assertion or
threshold job is added. If wall-clock ever approached 30 s on
ubuntu-latest, a maintainer reads it from the existing Gate 1 timing
line — no extra instrumentation is needed to surface it.

## Alerting

- **Threshold**: any non-zero failure count in the target, or a compile
  failure of the target, is the alert (DISCUSS handoff).
- **Channel**: the failing CI run on main. No external alerting is
  configured; this is consistent with the project's pure-trunk-based,
  feedback-not-a-gate posture.
- **Baseline**: 0% — no composed durable-path evidence for the second
  triad exists before this feature (DISCUSS baseline).

## Manual trust matrix (the only "dashboard")

A green CI run on main flips the second-triad cell. Tracked as a manual
checklist, per the DISCUSS handoff:

| Triad | Composed durable-path proof | Status |
|-------|-----------------------------|--------|
| Triad 1 (cinder + sluice + lumen) | `v1_three_adapters_compose_under_restart` | green (prior) |
| Triad 2 (pulse + ray + strata) | `v1_three_durable_stores_compose` | flips green at DELIVER close |
