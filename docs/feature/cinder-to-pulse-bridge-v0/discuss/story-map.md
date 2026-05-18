# Story Map: `cinder-to-pulse-bridge-v0`

## User: Priya the platform operator

## Goal

See Cinder's tier-management events (place / migrate / evaluate) as
queryable Pulse metric points under per-tenant + per-metric-name
partitions, using the same `MetricStore::query` API she already uses for
Lumen.

## Backbone

The journey has exactly three activities, one per Cinder event type. Each
activity is a complete vertical slice (wire -> exercise -> query) because
the bridge is library-only and the test substrate (`InMemoryTieringStore`
+ `InMemoryMetricStore`) makes every event observable in a single test.

| Activity 1: place events | Activity 2: migrate events | Activity 3: evaluate events |
|---|---|---|
| Bridge emits `cinder.place.count` with `tier` attribute | Bridge emits `cinder.migrate.count` with `from`/`to` attributes | Bridge emits `cinder.evaluate.migrated.count` with `value=migrated` |
| Per-tenant isolation | Per-tenant isolation | Per-tenant aggregation across an evaluate call |
| Tier-attribute correctness across Hot/Warm/Cold | Direction-attribute correctness for all migration legs | Zero-migration tenant case (no point emitted) |

## Walking Skeleton

Per `wave-decisions.md`, the walking-skeleton concept is N/A for this
backend-only feature. The bridge has no UI backbone to span. Every story
**is** a thin end-to-end slice through the bridge's emission path into
Pulse's query path. The three slices share the same vertical structure;
they differ only by which Cinder event the slice exercises.

Equivalent statement: **the smallest valuable bridge is one that handles
one event type end-to-end**. Slice 01 ships that. Slices 02 and 03 add
the remaining two event types.

## Release Slices

### Slice 01 — Place events land in Pulse

- **Outcome**: Priya can answer "how many items did tenant `acme` place
  in Hot in the last hour?" from a Pulse query against
  `cinder.place.count`.
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Verifies**: emission path end-to-end, per-tenant isolation, the
  `tier` point attribute serialisation convention.
- **Effort**: ~3 hours (clone Lumen bridge pattern, swap two methods,
  test).

### Slice 02 — Migrate events land in Pulse with direction attributes

- **Outcome**: Priya can answer "how many Hot->Warm migrations did
  tenant `acme` perform in the last hour?" from a Pulse query against
  `cinder.migrate.count`.
- **Stories**: `US-02`.
- **Verifies**: multi-attribute point emission (`from` + `to`), failed-
  migrate quiescence (no spurious point on `UnknownItem`), per-tenant
  isolation under two simultaneous migrations.
- **Effort**: ~2 hours (adds one method to the bridge, no architectural
  change).

### Slice 03 — Evaluate events land in Pulse with per-tenant counts

- **Outcome**: Priya can answer "in this evaluate run, how many items
  were migrated per tenant?" from a Pulse query against
  `cinder.evaluate.migrated.count`.
- **Stories**: `US-03`.
- **Verifies**: the value-encodes-count convention (not value=1 + attr),
  the dual-emission contract (per-item migrate AND per-tenant evaluate),
  the zero-migration tenant case (Cinder emits no `record_evaluate` for
  that tenant; the bridge therefore emits no Pulse point — asserted
  explicitly to lock the contract).
- **Effort**: ~3 hours (third method + the cross-event-type test that
  exercises both `migrate.count` and `evaluate.migrated.count` from one
  `evaluate_at` call).

## Priority Rationale

Outcome impact is identical across the three slices: each unlocks one
event type's worth of operator-visible Pulse metric. Ordering is
determined by **dependency on the substrate** and **incremental risk**:

1. **Slice 01 first** because `record_place` is the simplest possible
   shape (one tenant, one tier attribute, one value=1 point). It
   establishes the emission pattern, the `let _ = pulse.ingest(...)`
   error-swallow convention, the timestamp source, and the lowercase
   tier serialisation. Slices 02 and 03 inherit these decisions
   without re-litigating them.
2. **Slice 02 second** because `record_migrate` adds the second point
   attribute (multi-attribute emission) without changing the value-
   encoding. It also introduces the failed-call quiescence test.
3. **Slice 03 third** because `record_evaluate` is the only one that
   diverges from `value=1`. Doing it last means slices 01 and 02 are
   not perturbed by the decision (recorded in `wave-decisions.md` D2)
   to encode `migrated_count` directly as the point value. Slice 03 also
   carries the only multi-event-from-one-call test (the `evaluate_at`
   double-emission), which is the highest-information-density test in
   the suite.

If schedule pressure forces a partial ship, Slice 01 alone is shippable
and operationally meaningful: it lights up Pulse for Cinder placements,
which is the first thing Priya looks at when onboarding a new tenant.
Slice 02 alone is **not** independently shippable in front of Slice 01
because it depends on `place` happening first (Cinder `migrate` errors
on never-placed items). Slice 03 alone is independently shippable but
of lower value than Slice 01 (operators read `cinder.migrate.count` and
`cinder.place.count` more frequently than the evaluate metric).

## Scope Assessment: PASS

- 3 stories
- 1 bounded context (`self-observe` crate)
- 2 integration points (`cinder::MetricsRecorder` trait on the in-edge,
  `pulse::MetricStore` trait on the out-edge — both stable v0 traits
  already shipped)
- 1 new source file + 1 new test file + 2 line-level modifications
  (Cargo.toml, lib.rs)
- Estimated effort: ~1 day for an experienced Rust crafter familiar with
  the LumenToPulseRecorder precedent

The feature is right-sized. No splitting required.
