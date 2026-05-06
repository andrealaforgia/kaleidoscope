# Sieve v0 — outcome KPIs

Six measurable outcomes with numeric targets and CI-enforced
verification mechanisms. Targets are written as the v0 baseline;
post-v0 evolution belongs in successor releases.

The principal user is **Riley, the SRE** running Kaleidoscope in
production. Each KPI is something Riley can verify is true today
without reading source code.

---

## KPI 1 — Error-bearing traces are never sampled away

- **Who**: Riley, SRE.
- **Does what**: trusts Sieve to retain every error trace regardless
  of the configured rate.
- **By how much**: 100% retention of error-bearing traces across
  all rates in `[0.0, 1.0]`.
- **Measured by**: `slice_02_error_bias` integration test asserts
  retention at rates 0.0, 0.1, 0.5, 1.0 on fixture traces with
  `status.code == ERROR`. CI Gate 1 (cargo test) runs this on every
  push.
- **Baseline**: greenfield. The CI invariant is the baseline at v0.
- **Guardrail**: any commit that drops an error trace at any rate
  fails the gate and cannot land.

## KPI 2 — Configured non-error rate is statistically honoured

- **Who**: Riley, SRE.
- **Does what**: configures a non-error rate and gets the volume
  reduction that rate implies.
- **By how much**: ±3% on a 10000-trace deterministic-seed fixture
  for rate 0.5; ±2 traces on the boundaries (rate 0.0 keeps at most
  2; rate 1.0 keeps at least 9998).
- **Measured by**: `slice_03_non_error_rate` integration test asserts
  kept-counts at three rates. CI Gate 1 runs this on every push.
- **Baseline**: greenfield. The fixture is deterministic so the
  assertion is non-flaky.
- **Guardrail**: any commit that breaks the rate band fails the gate.

## KPI 3 — Trace coherence is preserved across batches

- **Who**: Riley, SRE.
- **Does what**: chases a latency spike in Prism / Tempo and sees
  whole traces, never half-traces.
- **By how much**: the variance of decision outcomes across 100
  calls with the same `trace_id` is exactly zero.
- **Measured by**: `slice_04_trace_id_determinism` integration test
  queries the same `trace_id` 100 times and asserts the decision
  count for one outcome is exactly 100.
- **Baseline**: greenfield.
- **Guardrail**: a non-deterministic Sieve fails this gate.

## KPI 4 — Logs and metrics are unaffected by sampling

- **Who**: Riley, SRE; Sasha, platform engineer.
- **Does what**: deploys Sieve in a production pipeline carrying
  logs and metrics for SLOs and dashboards; logs/metrics arrive
  unfiltered.
- **By how much**: 100% of logs in equals 100% of logs out at every
  rate; same for metrics.
- **Measured by**: `slice_05_logs_metrics_passthrough` integration
  test runs 100 logs and 100 metrics through Sieve at rate 0.0 and
  asserts in-count equals out-count.
- **Baseline**: greenfield.
- **Guardrail**: any commit that drops a log or metric fails the
  gate.

## KPI 5 — Operator visibility into sampling decisions

- **Who**: Riley, SRE.
- **Does what**: reads the Sieve summary in the operator's log
  aggregator and confirms the configured rate is being applied.
- **By how much**: 100% of summary windows emit exactly one INFO
  event with `target = "sieve"` carrying `kept`, `dropped`, and
  `rate` fields. Per-decision DEBUG events are emitted when DEBUG
  is enabled.
- **Measured by**: `slice_06_observability` integration test
  captures `tracing` events at DEBUG and INFO and asserts the
  vocabulary.
- **Baseline**: greenfield.
- **Guardrail**: a missing summary or a wrongly-targeted event
  fails the gate.

## KPI 6 — Walking-skeleton round-trip is fast and deterministic

- **Who**: Sasha, platform engineer; CI infrastructure.
- **Does what**: runs the Sieve test suite in CI and gets quick,
  reliable feedback.
- **By how much**: every `slice_*` test binary completes in under 5
  seconds on the runner; mutation testing on the slice's diff
  reaches 100% kill rate.
- **Measured by**: CI Gate 1 (cargo test) wall time per binary; CI
  Gate 5 (cargo mutants) per-package kill-rate report.
- **Baseline**: greenfield. The harness and Aperture both meet this
  bar; Sieve inherits the discipline.
- **Guardrail**: a slow test binary or a survived mutant fails the
  respective gate.

---

## Guardrail metrics (CI invariants per ADR-0005)

| Invariant | Mechanism |
|---|---|
| `forbid(unsafe_code)` | `forbid(unsafe_code)` in `crates/sieve/src/lib.rs`; clippy gates verify on every commit. |
| 100% mutation kill rate | `cargo mutants --package sieve --in-diff` per slice; per ADR-0005 Gate 5. |
| Apache supply chain | `cargo deny check` (Gate 4); the `xxhash-rust` crate is BSL-1.0 / MIT (permissive); no copyleft transitive runtime deps. |
| AGPL containment | Sieve is `AGPL-3.0-or-later`; consumers in the same workspace consume via dev-dep when needed for tests. |
| Public-API lock | `cargo public-api -p sieve` (Gate 2) keeps the public surface stable; semver-checks (Gate 3) confirms additive-only changes between releases. |

## What is NOT measured at v0

- Throughput / latency benchmarks. Sieve is a library at v0; the
  operator's overall pipeline throughput is the relevant metric and
  belongs in DEVOPS observability rather than a per-crate KPI.
- Per-tenant quotas. Need Aegis; deferred to v1+.
- Memory footprint of an in-memory tail-sampling window. Not in v0
  scope.
