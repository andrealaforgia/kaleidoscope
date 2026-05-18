# Story Map: `cinder-to-otlp-json-bridge-v0`

## User: Priya the platform operator

## Goal

See Cinder's tier-management events (place / migrate / evaluate) appear
as one OTLP-JSON `ResourceMetrics` line per event in the same NDJSON
file that the Lumen OTLP-JSON writer already writes to, so a single
sidecar forwarding to a single OTLP/HTTP collector produces a complete
platform view (`kaleidoscope.lumen` AND `kaleidoscope.cinder`) without
any change to the sidecar, the collector, or the dashboard.

## Backbone

The journey has exactly three activities, one per Cinder event type.
Each activity is a complete vertical slice (wire -> exercise -> inspect
the NDJSON sink) because the writer is library-only and the test
substrate (`cinder::InMemoryTieringStore` + `SharedBuf(Arc<Mutex<Vec<u8>>>)`)
makes every event observable in a single test.

| Activity 1: place events | Activity 2: migrate events | Activity 3: evaluate events |
|---|---|---|
| Writer emits `cinder.place.count` line with `tier` point attribute | Writer emits `cinder.migrate.count` line with `from`/`to` point attributes | Writer emits `cinder.evaluate.migrated.count` line with `asInt=migrated.to_string()` |
| Per-tenant isolation via `resource.attributes[0].value.stringValue` | Per-tenant isolation, failed-migrate quiescence (Cinder does not call `record_migrate` on `Err(UnknownItem)`) | Per-tenant aggregation across an evaluate call |
| NDJSON shape pinned: one record per line, trailing `\n`, every line independently parseable JSON | Direction-attribute correctness for all six migration legs | Zero-migration tenant case (no line emitted) + dual-emission contract (N migrate lines + 1 evaluate line per tenant per evaluate call) |

## Walking Skeleton

Per `wave-decisions.md`, the walking-skeleton concept is N/A for this
backend-only feature. The writer has no UI backbone to span. Every story
**is** a thin end-to-end slice through the writer's emission path into
the NDJSON sink. The three slices share the same vertical structure;
they differ only by which Cinder event the slice exercises.

Equivalent statement: **the smallest valuable writer is one that handles
one event type end-to-end with the documented OTLP-JSON shape**. Slice 01
ships that. Slices 02 and 03 add the remaining two event types.

## Release Slices

### Slice 01 — Place events emit OTLP-JSON lines

- **Outcome**: a sidecar reading the NDJSON sink sees one
  `cinder.place.count` line per `cinder.place` call, under the calling
  tenant's resource attribute, with the entry tier as a point attribute.
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Verifies**: emission path end-to-end, the full OTLP-JSON envelope
  shape (resource, scope, metric, sum, dataPoint), per-tenant resource-
  attribute isolation, the `tier` point-attribute serialisation
  convention, the NDJSON line-termination invariant, the `Send + Sync`
  bound, and the compile-time check that the writer plugs into
  `cinder::MetricsRecorder`.
- **Effort**: ~4 hours (clone the Lumen writer file, swap two methods'
  metric names + attribute shapes, add the three-tier test).

### Slice 02 — Migrate events emit OTLP-JSON lines with direction attributes

- **Outcome**: a sidecar sees one `cinder.migrate.count` line per
  successful `cinder.migrate` call, with `from` and `to` point
  attributes; failed migrates produce no line.
- **Stories**: `US-02`.
- **Verifies**: multi-attribute point emission (`from` + `to`), failed-
  migrate quiescence (no spurious line on `UnknownItem` because Cinder
  doesn't call the recorder), per-tenant resource-attribute isolation
  under two simultaneous opposite-direction migrations.
- **Effort**: ~2 hours (adds one method body to the writer, no
  architectural change).

### Slice 03 — Evaluate events emit OTLP-JSON lines with per-tenant counts

- **Outcome**: a sidecar sees one `cinder.evaluate.migrated.count` line
  per (tenant, evaluate-call) pair where Cinder migrated at least one
  item for that tenant, with `asInt` equal to the migrated count;
  zero-migration tenants produce no line.
- **Stories**: `US-03`.
- **Verifies**: the value-encodes-count convention (`asInt =
  migrated.to_string()`, NOT `asInt = "1"` + extra attribute), the dual-
  emission contract (per-item migrate + per-tenant evaluate, both
  written to the SAME NDJSON sink), the zero-migration tenant case.
- **Effort**: ~3 hours (third method body + the cross-event-type test
  that exercises both `migrate.count` and `evaluate.migrated.count` from
  one `evaluate_at` call against the same NDJSON sink).

## Priority Rationale

Outcome impact is identical across the three slices: each unlocks one
event type's worth of cross-process operator-visible Cinder metric.
Ordering is determined by **dependency on the substrate** and
**incremental risk**:

1. **Slice 01 first** because `record_place` is the simplest possible
   shape (one tenant, one tier attribute, one `asInt=1` point). It
   establishes the OTLP-JSON envelope shape (all the serde structs that
   slices 02 and 03 will reuse), the scope-name constant, the lowercase-
   tier helper, the timestamp source, the per-line `write_all + b"\n" +
   flush` triple, and the `Mutex<W>` locking pattern. Slices 02 and 03
   inherit these decisions without re-litigating them.
2. **Slice 02 second** because `record_migrate` adds the second point
   attribute (multi-attribute emission) without changing the value-
   encoding. It also introduces the failed-call quiescence test.
3. **Slice 03 third** because `record_evaluate` is the only one that
   diverges from `asInt="1"`. Doing it last means slices 01 and 02 are
   not perturbed by the decision (recorded in `wave-decisions.md` D4)
   to encode `migrated_count` directly as the point's `asInt` string.
   Slice 03 also carries the only multi-event-from-one-call test (the
   `evaluate_at` dual emission), which is the highest-information-
   density test in the suite.

If schedule pressure forces a partial ship, **Slice 01 alone is
shippable and operationally meaningful**: it lights up the cross-process
collector for Cinder placements, which is the first thing an operator
looks at when onboarding a new tenant. Slice 02 alone is **not**
independently shippable in front of Slice 01 because the migrate test
needs a successful place first (Cinder `migrate` errors on never-placed
items). Slice 03 alone is independently shippable but of lower value
than Slice 01 (operators read place/migrate metrics more frequently than
the evaluate metric).

## Cross-bridge alignment

The story-map structure is intentionally identical to
`cinder-to-pulse-bridge-v0/discuss/story-map.md`:

- Same three activities (place / migrate / evaluate)
- Same three slices in the same order
- Same priority rationale (simplest shape first, dual-emission last)
- Same story IDs (US-01, US-02, US-03)

This is not accidental — it reflects the cross-bridge contract locked in
`wave-decisions.md` D1. An operator reading the two features side by
side should see two implementations of the same conceptual story, with
a different sink. The DESIGN wave should resist any temptation to
restructure for "originality"; sameness IS the contract here.

## Scope Assessment: PASS

- 3 stories
- 1 bounded context (`self-observe` crate)
- 2 integration points (`cinder::MetricsRecorder` trait on the in-edge,
  `std::io::Write + Send + Sync` on the out-edge — both stable surfaces
  already exercised by sibling features)
- 1 new source file + 1 new test file + 2 line-level modifications
  (`Cargo.toml`, `lib.rs`)
- Estimated effort: ~1 day for an experienced Rust crafter familiar
  with both precedents (`LumenToOtlpJsonWriter` and
  `CinderToPulseRecorder`)

The feature is right-sized. No splitting required.
