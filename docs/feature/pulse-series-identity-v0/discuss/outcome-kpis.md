# Outcome KPIs: pulse-series-identity-v0

British English. No em dashes.

These KPIs measure one thing: a metric emitted by several services is preserved as one
correctly-labelled series per service, through ingest and through a durable restart. The
baseline is today's collapse, where every multi-service metric returns one series wearing the
last-ingested service's labels (verified against code in wave-decisions.md, facts 1-3).

## Feature: pulse-series-identity-v0

### Objective

Within this feature, a metric is identified by its full label set so that a consumer querying
a multi-service metric reads one correct series per service, on the live path and across a
durable restart, instead of one collapsed series wearing the wrong service's labels.

### North Star

**Per-service provenance survives ingest and recovery.**

- **Who**: Pulse's ingest/query consumer (query-api, and the on-call operator behind it).
- **Does what**: ingests a metric emitted by several services under one tenant and queries it
  back as one correctly-labelled series per service, instead of one collapsed series.
- **By how much**: 100% of distinct `resource_attributes` under a shared metric name preserved
  as distinct series; 0 series whose `resource_attributes` are overwritten by a later ingest.
- **Measured by**: the acceptance suite ingesting >= 2 services under one name and asserting
  each returned series carries its own `resource_attributes`, on the live path and after a
  restart.
- **Baseline**: 0% today (every multi-service metric collapses to one series wearing the
  last-ingested service's labels).

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By |
|---|-----|-----------|-------------|----------|-------------|
| 1 | Ingest/query consumer | preserves per-service provenance under a shared name | 100% of distinct label sets kept as distinct series | 0% (name-only keying collapses them) | US-01 happy-path acceptance scenario |
| 2 | Ingest/query consumer | keeps each service's labels intact across ingests | 0 cross-service `resource_attributes` overwrites | every later ingest overwrites today | US-01: assert neither service's labels overwrite the other's |
| 3 | Durable-store consumer | keeps distinct series across a snapshot + reopen | 100% present and correctly labelled | n/a (distinct series do not exist today) | US-02 snapshot-path scenario |
| 4 | Durable-store consumer | keeps distinct series across a WAL-only reopen | 100% present and correctly labelled | n/a | US-02 WAL-replay scenario |

### Metric Hierarchy

- **North Star**: per-service provenance survives ingest and recovery (KPIs 1-4 together).
- **Leading indicators**: KPI 1 (distinct at ingest) and KPI 2 (no overwrite) are the earliest
  signal; if they hold, the durable KPIs 3-4 follow because live ingest and WAL replay share
  `apply_ingest` (wave-decisions.md, fact 6).
- **Guardrail metrics** (must NOT degrade):
  - `MetricStore` trait signature unchanged: 0 signature changes, verified by compile of
    existing consumers plus code review (`query` already returns `Vec<(Metric, MetricPoint)>`).
  - Point attributes untouched: `MetricPoint.attributes` stays per-point and never splits or
    merges a series, verified by the US-01 edge scenario.
  - Identical full label set merges, not duplicates: two ingests of one label set yield one
    series with points ascending by `time_unix_nano`, verified by the US-01 boundary scenario.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1-2 (live identity) | acceptance suite | ingest >= 2 services under one name, query, assert distinct correctly-labelled series | every CI run | acceptance-designer (DISTILL) |
| 3 (snapshot reopen) | acceptance suite | snapshot + drop + reopen, query, assert distinct series | every CI run | acceptance-designer |
| 4 (WAL-only reopen) | acceptance suite | drop + reopen with no snapshot (WAL replay), query, assert distinct series | every CI run | acceptance-designer |
| Guardrails | compile + code review + acceptance | trait compiles unchanged; point-attr and merge scenarios green | every CI run | crafter + acceptance-designer |

These are correctness KPIs measured as pass/fail in the acceptance suite, not telemetry: Pulse
is library-only at v0 with no running daemon to instrument (wave-decisions.md, scope boundary).
A KPI that is 100% or 0% here means every relevant acceptance scenario passes or fails; the
"distinct label sets" and "overwrites" counts are over the scenarios' fixtures, not production
traffic. No production baseline collection is needed because there is no production data.

### Hypothesis

We believe that keying a series by its full label set (metric name + `resource_attributes`),
applied in the shared `apply_ingest`, will make per-service provenance survive both live ingest
and durable recovery. We will know this is true when one acceptance run ingests checkout then
cart under one name and gets two distinct, correctly-labelled series back, and the same two
series survive a snapshot+reopen and a WAL-only reopen. We will know it is false if any path
returns a single collapsed series or re-merges the two on restart.

### Handoff to DEVOPS

No instrumentation, dashboards, or alerting thresholds are required: Pulse is a library with no
running surface and no production data. The platform-architect needs nothing here beyond
awareness that these KPIs are enforced in the acceptance suite, not via telemetry.
