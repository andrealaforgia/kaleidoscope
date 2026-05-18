# Wave Decisions — `cinder-to-pulse-bridge-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` | Library-only Rust crate addition. No CLI, no UI, no HTTP surface in v0. |
| `walking_skeleton` | `no` | The walking-skeleton concept does not apply: every story IS a thin end-to-end slice through the bridge. There is no UI activity backbone to span. |
| `research_depth` | `lightweight` | Worked precedent (`LumenToPulseRecorder`) already shipped and validated. The clone exercise has design value precisely because the pattern is settled. |
| `jtbd_analysis` | `no` | Skipped. The job served (operator observes Cinder tier transitions) is a known shape — same operator persona that Lumen already serves, same outcome (queryable metrics under per-tenant partitions). DIVERGE artifacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. Job statement implicit: "operator observes Cinder tier transitions the same way they observe Lumen ingest/query". | DIVERGE skipped by Andrea's explicit instruction. The bridge has exactly one shape that compiles (`impl cinder::MetricsRecorder` writing into `pulse::MetricStore`); design space is collapsed by the trait signatures. |
| No formal JTBD workshop | LOW. Persona, push, pull, anxiety, habit are mirror-image of the Lumen bridge (already validated by ship). | Persona + emotional-arc inherited from Lumen bridge in `journey-observe-cinder-tier-transitions-visual.md`. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D1: One bridge, three metric names — not one combined metric with an `event` attribute

Per-event-type metric naming follows the Lumen bridge convention. Metrics
are:

- `cinder.place.count` — Sum, value=1, point attribute `tier=hot|warm|cold`
- `cinder.migrate.count` — Sum, value=1, point attributes `from=...`, `to=...`
- `cinder.evaluate.migrated.count` — Sum, value=migrated_count (per-tenant total
  from one `evaluate_at` call)

Rationale: parallels `lumen.ingest.count` + `lumen.query.count`. Operators
querying with `pulse_store.query(&tenant, &MetricName::new("cinder.migrate.count"), TimeRange::all())`
get a clean filter by event type without needing predicate composition. Using a
single `cinder.events` metric with an `event=place|migrate|evaluate` attribute
would force every query to use `query_with(predicate)`, breaking the symmetry
with Lumen and forcing operators to learn a second query idiom.

### D2: `record_evaluate` emits value = migrated_count, not value = 1

The Cinder trait signature `fn record_evaluate(&self, tenant: &TenantId, migrated: usize)`
already carries the meaningful integer. Encoding it as the point's `value`
makes the metric directly aggregatable: an operator querying
`cinder.evaluate.migrated.count` and summing values gets "total items migrated
across all evaluate calls in the window". Encoding it as `value=1` with an
attribute `migrated_count=5` is OTLP-legal but throws away the natural
aggregation.

This matches the Lumen bridge's choice: `record_ingest(tenant, record_count)`
emits `value=record_count`, not `value=1` with an attribute.

### D3: Cinder emits BOTH `record_migrate` (per item) AND `record_evaluate` (per tenant) from `evaluate_at`

This is **Cinder's existing behaviour**, not a bridge choice. The bridge
faithfully forwards both. An `evaluate_at` that migrates 5 items for `acme`
and 2 for `globex` produces:

- 5 points on `cinder.migrate.count` for `acme` (one per item, with `from`/`to` attrs)
- 2 points on `cinder.migrate.count` for `globex`
- 1 point on `cinder.evaluate.migrated.count` for `acme` (value=5)
- 1 point on `cinder.evaluate.migrated.count` for `globex` (value=2)

The acceptance tests assert this double-emission explicitly (see `US-03`
scenarios), because a future reader looking at Pulse output without this
clarification would suspect a bug.

### D4: Tier topology lands as a POINT attribute, not a resource attribute

Tiers (`hot`/`warm`/`cold`) and migration directions (`from`/`to`) are
per-event facts, not per-process facts. They go in `MetricPoint.attributes`
(BTreeMap<String,String>), not `Metric.resource_attributes`. Matches OTLP
semantic conventions: resource attributes describe the emitting process;
point attributes describe the observation.

The `Tier` enum (`Hot`/`Warm`/`Cold`) lowercases to `"hot"`/`"warm"`/`"cold"`
in the attribute value so operators query with stable lowercase strings.

### D5: Best-effort emission (swallow `MetricStoreError`)

Matches the Lumen bridge. `MetricStoreError` is an empty enum at v0 so no
error path actually exists; the explicit `let _ =` is forward-compatible for
v1+. Cinder's `MetricsRecorder` trait methods return `()` — they have no
channel to propagate errors anyway. A future loud-emission variant would be a
separate bridge type (`CinderToPulseRecorderStrict` or similar), not a
configuration flag.

### D6: No CLI wiring in this feature

Out of scope. The follow-up feature (`kaleidoscope-cli-wires-cinder-bridge-v0`
or similar) will plumb the bridge into the operator binary. This feature
ships the library only, with acceptance tests driving the wiring against an
`InMemoryTieringStore` + `InMemoryMetricStore` pair.

### D7: SSOT journey + jobs.yaml are NOT modified in this wave

The operator-incident-response SSOT journey is incident-time focused; this
bridge serves the orthogonal "operator observes platform internals" journey,
which is post-v0 of this feature. A new SSOT journey entry will be promoted
when the CLI follow-up feature ships an operator-visible surface.

The `discuss/journey-observe-cinder-tier-transitions.yaml` produced in this
wave is the feature-local journey artefact, not an SSOT promotion candidate.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 3 stories, 1 bounded context (`self-observe` crate), 1 new file
(`crates/self-observe/src/cinder_bridge.rs`) plus 1 new acceptance-test file
(`crates/self-observe/tests/cinder_to_pulse.rs`) plus 2 modifications
(`Cargo.toml`, `lib.rs`). Estimated effort: ~1 day for an experienced Rust
crafter familiar with the precedent. PASSES the right-sized gate.

## Handoff

Next wave: DESIGN (nw-solution-architect). Inputs delivered:

- `journey-observe-cinder-tier-transitions-visual.md`
- `journey-observe-cinder-tier-transitions.yaml`
- `journey-observe-cinder-tier-transitions.feature`
- `shared-artifacts-registry.md`
- `story-map.md`
- `prioritization.md`
- `user-stories.md`
- `dor-validation.md`
- `outcome-kpis.md`
- `slices/slice-01-place-events-land-in-pulse.md`
- `slices/slice-02-migrate-events-land-in-pulse-with-direction.md`
- `slices/slice-03-evaluate-events-land-in-pulse-with-per-tenant-counts.md`
