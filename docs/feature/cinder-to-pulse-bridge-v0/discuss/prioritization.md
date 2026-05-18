# Prioritization: `cinder-to-pulse-bridge-v0`

## Release Priority

| Priority | Release | Target Outcome | KPI | Rationale |
|----------|---------|---------------|-----|-----------|
| 1 | Slice 01 (place events) | Priya can query Pulse for per-tenant `cinder.place.count` with `tier` attribute. | OK1 (see `outcome-kpis.md`) | Establishes the emission pattern + conventions inherited by slices 02 and 03. Independently shippable and operationally meaningful (placement is the entry-point Cinder event). |
| 2 | Slice 02 (migrate events) | Priya can query Pulse for per-tenant `cinder.migrate.count` with `from`/`to` attributes. | OK2 | Depends on Slice 01 conventions (tier serialisation). Adds multi-attribute emission. |
| 3 | Slice 03 (evaluate events) | Priya can query Pulse for per-tenant `cinder.evaluate.migrated.count` with `value=migrated`. | OK3 | Adds the only `value != 1` encoding and the dual-emission contract. Lowest-frequency operator query of the three. |

## Backlog

| Story | Slice | Priority | Outcome Link | Dependencies |
|-------|-------|----------|-------------|--------------|
| US-01 | 01 | P1 | OK1 | None — first slice |
| US-02 | 02 | P2 | OK2 | US-01 (inherits tier serialisation convention; `migrate` requires a prior `place`) |
| US-03 | 03 | P3 | OK3 | US-01 + US-02 (the `evaluate_at` double-emission test exercises `migrate.count` too) |

## Prioritization Scores (Value x Urgency / Effort, 1-5 scale)

| Story | Value | Urgency | Effort | Score | Rank |
|-------|-------|---------|--------|-------|------|
| US-01 | 5 | 4 | 1 | 20.0 | 1 |
| US-02 | 4 | 3 | 1 | 12.0 | 2 |
| US-03 | 4 | 3 | 2 | 6.0 | 3 |

Scoring notes:
- **Value**: US-01 = 5 because it unlocks the first Cinder observability
  signal at all, which is the largest single jump. US-02 and US-03 = 4
  each: each adds one more observable event type, incremental value.
- **Urgency**: US-01 = 4 because it derisks the wiring pattern that the
  follow-up CLI feature depends on; US-02/US-03 = 3 because they can ship
  in any order behind US-01 without blocking the CLI follow-up.
- **Effort**: US-01 = 1 (smallest method body, no multi-attribute
  serialisation). US-02 = 1 (one extra attribute). US-03 = 2 (the
  cross-event test is the most demanding).

## Riskiest assumption first

The riskiest assumption is **"the LumenToPulseRecorder pattern transfers
1:1 to Cinder's `MetricsRecorder` trait without architectural surprise"**.
Slice 01 validates this assumption end-to-end. If Slice 01 surfaces a
trait-shape mismatch (e.g. Tier needs to be a resource attribute, not a
point attribute; or `record_evaluate`'s `migrated: usize` argument forces
a different metric kind), the cost of discovery is one day's work, not
three days'.

## Post-DISCUSS revisit

Story IDs (US-01, US-02, US-03) are now stable. The Phase 4 outcome KPIs
(OK1, OK2, OK3) are defined in `outcome-kpis.md` and linked from each
slice file. No revisit needed at handoff time.
