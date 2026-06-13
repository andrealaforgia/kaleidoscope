# Outcome KPIs — `consolidated-runtime-v0`

## Feature: consolidated-runtime-v0 (C1 — the consolidation spine)

### Objective

Make Kaleidoscope a system you can experiment with: telemetry sent at time T is queryable at
time T from the same process, with no restart — so "bring up the stack, send a metric, look"
works by construction instead of failing by construction.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Experimenter (Andrea / contributor / CI) | Queries back telemetry they just sent, without restarting any process | 0% → 100% of send-then-query attempts return the data | 0% (loop fails today; needs a query-process restart) | Single-process ingest-then-query acceptance test, per signal | Leading (Outcome) |
| 2 | Experimenter | Sees freshly-ingested telemetry appear quickly | Queryable within 1 second of ingest acknowledgement (p95) | n/a (never appears without a restart today) | Test measuring ingest-ack → query-returns interval | Leading (Secondary) |
| 3 | Experimenter | Brings up the whole stack with one command across all three signals | 1 command, 3 of 3 signals live, all 5 ports on one process | 5 binaries hand-launched with restart ordering | US-05 single-process acceptance test; manual one-command run | Leading (Outcome) |
| 4 | Multi-tenant operator/reviewer | Confirms cross-tenant reads return nothing in the consolidated process | 0 cross-tenant leaks (100% of cross-tenant reads empty) | 0 leaks under separate processes (no regression) | US-02 acceptance test (ingest as acme, read as globex) | Guardrail |

### Metric Hierarchy

- **North Star**: **live-visibility** — the fraction of send-then-query attempts where telemetry
  ingested after the runtime started is returned by a subsequent query with no restart. Target
  100%; baseline 0%. This single metric is the whole point of C1; if it is not 100%, the feature
  has not delivered.
- **Leading Indicators**: freshness latency (ingest-ack → query-returns, p95 < 1s, KPI 2);
  signal coverage (1/3 after Slice 1, 3/3 after Slice 2, KPI 3); one-command startup (KPI 3).
- **Guardrail Metrics** (must NOT degrade): cross-tenant leaks = 0 (KPI 4); existing read-auth
  stays fail-closed when configured; per-record fsync durability unchanged; no port-bind
  conflicts on the consolidated process; ingest accept rate not reduced versus the standalone
  gateway.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| 1 Live-visibility | Acceptance/integration tests | Ingest then query in one process; assert value returned | Every CI run touching the feature | DISTILL (acceptance-designer) |
| 2 Freshness latency | Same test, timestamped | Measure ingest-ack → query-returns interval, p95 | Every CI run | DISTILL / DEVOPS |
| 3 One-command + coverage | US-05 test; manual run | Single command boots all signals; query all three | Per release of the slice | DISTILL; Andrea (manual) |
| 4 Cross-tenant leaks | US-02 test | Ingest tenant A, read tenant B; assert empty | Every CI run | DISTILL |

### Hypothesis

We believe that a single-process consolidated runtime sharing one `Arc<Store>` per signal
between the ingest sink and the query router, for the experimenter running Kaleidoscope locally,
will make telemetry queryable the instant it is ingested. We will know this is true when the
experimenter queries back a metric, a log, and a trace they just sent — with no restart of any
process — 100% of the time, within 1 second (p95), while cross-tenant reads still return
nothing.

### Notes for DEVOPS (platform-architect)

- Instrument the ingest-ack → query-returns interval so KPI 2 can be tracked once the run story
  (C2) and generator (C3) land; for v0 the acceptance test is the measurement.
- The freshness target (p95 < 1s) is the SLO-shaped guardrail the later run story should not
  regress.
- Watch the fixed-port 4317/4318 flake (project memory) when designing CI for the consolidated
  process: bind ephemeral ports in tests, sweep+retry.
