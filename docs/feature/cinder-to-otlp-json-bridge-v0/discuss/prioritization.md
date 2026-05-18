# Prioritization: `cinder-to-otlp-json-bridge-v0`

## Release Priority

| Priority | Release | Target Outcome | KPI | Rationale |
|----------|---------|---------------|-----|-----------|
| 1 | Slice 01 (place events) | A sidecar reading the NDJSON sink sees one `cinder.place.count` OTLP-JSON line per `cinder.place` call, scope `kaleidoscope.cinder`, per-tenant resource attribute, `tier` point attribute. | OK1 (see `outcome-kpis.md`) | Establishes the OTLP-JSON envelope shape + scope-name + lowercase-tier + NDJSON-line conventions inherited by slices 02 and 03. Independently shippable and operationally meaningful (placement is the entry-point Cinder event). |
| 2 | Slice 02 (migrate events) | Sidecar sees `cinder.migrate.count` lines per successful migrate, with `from`/`to` point attributes. | OK2 | Depends on Slice 01 conventions (envelope shape, tier serialisation). Adds multi-attribute emission and failed-call quiescence. |
| 3 | Slice 03 (evaluate events) | Sidecar sees `cinder.evaluate.migrated.count` lines per (tenant, evaluate-call) with non-zero migrations, `asInt=migrated.to_string()`. | OK3 | Adds the only `asInt != "1"` encoding and the dual-emission contract. Lowest-frequency operator query of the three. |

## Backlog

| Story | Slice | Priority | Outcome Link | Dependencies |
|-------|-------|----------|-------------|--------------|
| US-01 | 01 | P1 | OK1 | None — first slice. Has the most "envelope shape" work because all the OTLP-JSON serde structs are introduced here. |
| US-02 | 02 | P2 | OK2 | US-01 (inherits the envelope structs + the lowercase-tier helper + the Mutex<W> pattern). |
| US-03 | 03 | P3 | OK3 | US-01 + US-02 (the cross-event-type test in this slice exercises BOTH `migrate.count` and `evaluate.migrated.count` lines against the same NDJSON sink). |

## Prioritization Scores (Value x Urgency / Effort, 1-5 scale)

| Story | Value | Urgency | Effort | Score | Rank |
|-------|-------|---------|--------|-------|------|
| US-01 | 5 | 4 | 2 | 10.0 | 1 |
| US-02 | 4 | 3 | 1 | 12.0 | 2 |
| US-03 | 4 | 3 | 2 | 6.0 | 3 |

Scoring notes:

- **Value**: US-01 = 5 because it unlocks the first cross-process Cinder
  observability signal at all, which is the largest single jump. US-02
  and US-03 = 4 each: each adds one more observable event type,
  incremental value.
- **Urgency**: US-01 = 4 because it derisks the OTLP-JSON envelope
  serialisation pattern that the follow-up CLI feature depends on (and
  derisks the cross-bridge metric-name contract with the Pulse-sink
  sibling — D1). US-02/US-03 = 3 because they can ship in any order
  behind US-01 without blocking the CLI follow-up.
- **Effort**: US-01 = 2 (introduces all the serde structs even though
  only one record_* method is implemented; ~4h). US-02 = 1 (one extra
  attribute, ~2h). US-03 = 2 (the dual-emission cross-event test is the
  most demanding test in the suite; ~3h).

Note: US-02 has a higher V x U / E score than US-01 numerically (12.0 vs
10.0). The ordering still puts US-01 first because of the **riskiest-
assumption-first** rule below — Slice 01 derisks the entire OTLP-JSON
envelope shape, and a failure there forces a re-think of slices 02 and
03. The score is a heuristic, not a hard ordering rule.

## Riskiest assumption first

The riskiest assumption is **"the LumenToOtlpJsonWriter OTLP-JSON
envelope shape transfers 1:1 to Cinder's `MetricsRecorder` trait without
spec surprise"**. Specifically:

- Lumen's `record_ingest(tenant, record_count)` emits ONE point per call
  with `asInt = record_count.to_string()`. Cinder's `record_place` and
  `record_migrate` emit ONE point per call with `asInt = "1"`. Cinder's
  `record_evaluate` matches Lumen's shape with `asInt = migrated.to_string()`.
- Lumen's writer uses ONE point attribute (`tenant_id`). Cinder's writer
  uses ZERO-to-TWO additional point attributes on top of `tenant_id`.
  The `[OtlpAttr; 1]` array type in Lumen's serde structs needs to be
  parameterised (or replaced with `Vec<OtlpAttr>`, or hand-rolled per-
  metric).

Slice 01 validates this assumption end-to-end against a real Cinder
event with one extra point attribute (`tier`). If Slice 01 surfaces a
shape mismatch (e.g. the `[OtlpAttr; N]` fixed-size array pattern from
Lumen does not generalise cleanly to Cinder's variable attribute count
and forces a `Vec<OtlpAttr>` reshape that ripples through the resource
attributes too), the cost of discovery is one slice's worth of work,
not three slices' worth.

The DESIGN wave should preserve Slice 01's role as the shape derisker;
do not start with Slice 03 ("more interesting test") even if the score
math invites it.

## Post-DISCUSS revisit

Story IDs (US-01, US-02, US-03) are now stable. The Phase 4 outcome KPIs
(OK1, OK2, OK3, plus OK4 guardrail) are defined in `outcome-kpis.md`
and linked from each slice file. No revisit needed at handoff time.

## Parallel-feature observation

Both `cinder-to-pulse-bridge-v0` and this feature share the same
priority structure and the same story IDs. That is intentional (see the
"Cross-bridge alignment" section in `story-map.md`). The two features
could in principle be developed in parallel by different crafters, since
they touch disjoint files (`cinder_bridge.rs` vs `cinder_otlp_json.rs`)
and disjoint test files. In practice the Pulse-sink sibling shipped
first, so this feature inherits the cross-bridge metric-name
contract as a constraint rather than co-designing it.
