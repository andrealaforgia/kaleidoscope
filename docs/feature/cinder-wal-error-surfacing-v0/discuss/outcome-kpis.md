# Outcome KPIs: cinder-wal-error-surfacing-v0

## Feature: cinder-wal-error-surfacing-v0

### Objective

Make cinder's (and, for uniformity, sluice's) tier-persistence operations fail loud and stay consistent
with disk, so an operator never reads a placement that will vanish on restart and always learns when the
disk cannot persist — closing the acked-but-not-durable lie on the live gateway path.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| KPI-1 | cinder `FileBackedTieringStore::place` (live gateway path) | surfaces WAL persistence failures instead of swallowing them | swallow sites 1 -> 0; failing-disk `place` AC falsifiable in-suite (passes only when error surfaced AND memory consistent) | 1 swallow site (`file_backed.rs:270-278`), 0 surfacing tests | failing-substrate acceptance test + `cargo mutants` kill on the swallow site | Leading |
| KPI-2 | Priya on the live ingest path (`flush()`) | learns whether an ingest durably persisted its tier metadata (never a falsely-green success) | silent tier-persist failures in ingest 100% -> 0% (always surfaced per D2) | `flush()` ignores the absent error channel; ingest reported green regardless | ingest-with-failing-disk acceptance test asserting D2 behaviour | Leading |
| KPI-3 | cinder `evaluate_at` under a policy sweep (`evaluate-policy`) | surfaces sweep WAL failures and reports a count equal to the durably-migrated items | per-migration swallow (1 loop site) -> 0; reported count == durable count | `evaluate_at` swallows each migration's WAL error; count overstates durability | failing-substrate sweep test + `cargo mutants` on the swallow site and count logic | Leading |
| KPI-4 | sluice `FileBackedQueue` (unwired today; future live consumers) | surfaces WAL persistence failures at dequeue/ack/nack instead of swallowing | swallow sites 3 -> 0; storage-pillar posture uniform | 3 swallow sites (`file_backed.rs:346,356,366`) | failing-substrate sluice tests + `cargo mutants` on the three sites | Leading |

### Metric Hierarchy

- **North Star**: number of tier-persistence swallow sites across the storage pillars in scope
  (cinder + sluice) reaches **0/4** with a falsifiable surfacing test per site.
- **Leading Indicators**: each failing-substrate AC passes only when the error is surfaced AND
  in-memory state stays consistent with disk (KPI-1, KPI-3, KPI-4); the live ingest path never reports a
  clean success on a swallowed `place` error (KPI-2).
- **Guardrail Metrics** (must NOT degrade):
  - **Healthy-disk behaviour unchanged**: every negative-control scenario (healthy disk places/migrates
    normally, placement readable AND durable across reopen) still passes. No regression in the
    1194-test suite's graceful-restart durability assertions.
  - **No torn memory on failure**: a failed `place`/migration leaves the in-memory map exactly as before
    (the prior durable value is intact; no partial mutation).
  - **Write-ahead ordering preserved**: WAL append precedes the memory mutation at every changed site
    (the `migrate()` discipline, generalised).
  - **Mutation kill rate 100%** on modified files (ADR-0005 Gate 5) — the `?` / abort-before-memory
    must not be deletable or reorderable without a surviving test.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| KPI-1 | cinder acceptance suite (failing `FsyncBackend` substrate via `open_with_fsync_backend`) | test pass/fail + `cargo mutants --in-diff` | per CI run on the feature branch | DELIVER crafter |
| KPI-2 | kaleidoscope-cli ingest acceptance test (failing disk) | test pass/fail asserting D2 behaviour | per CI run | DELIVER crafter |
| KPI-3 | cinder sweep acceptance test (failing substrate) | test pass/fail + `cargo mutants` | per CI run | DELIVER crafter |
| KPI-4 | sluice acceptance test (failing substrate) | test pass/fail + `cargo mutants` | per CI run | DELIVER crafter |
| Guardrail (healthy-disk) | existing cinder/sluice/integration durability suites | full `cargo test` workspace | per CI run | DELIVER crafter |

### Hypothesis

We believe that surfacing the WAL persistence failure (with write-ahead ordering) in cinder's `place`
and `evaluate_at` — and in sluice's three swallow sites — for the platform operator will achieve a
storage layer that never acks a non-durable tier decision. We will know this is true when **cinder and
sluice surface a `PersistenceFailed` outcome on a failing-disk substrate at 4/4 previously-swallowing
sites, the live ingest path never reports a falsely-green success, and every negative-control
healthy-disk scenario still passes** — proven in-suite by the failing-substrate tests and held by the
100% mutation gate.

### Note on measurement frequency

This is a pre-production correctness-hardening feature on an Earned-Trust pillar, not a usage-driven
product feature. The KPIs are therefore in-suite falsifiability and mutation-kill measures (the
project's K6 north-star idiom: agents emit raw test/mutation observations; deterministic CI scores
them), not post-release behavioural analytics. There is no production telemetry baseline to collect
because the defect is a latent durability lie, not a usage pattern.

## Handoff to DEVOPS

The platform-architect needs, from this file:

1. **Data collection requirements**: the failing-substrate acceptance tests and `cargo mutants --in-diff`
   results per modified file are the instrumentation; no new runtime metric or dashboard.
2. **Dashboard/monitoring needs**: none new. If a future feature wires a runtime
   `cinder.place.persist_failed` / sweep-failure counter, that is a separate observability feature.
3. **Alerting thresholds**: the guardrail — mutation kill rate must be 100% on modified files
   (ADR-0005 Gate 5); any negative-control regression is a hard stop.
4. **Baseline measurement**: 4/4 swallow sites present today; target 0/4 with a falsifiable test each.
