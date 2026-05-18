# Wave Decisions — `cinder-to-otlp-json-bridge-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` | Library-only Rust crate addition. No CLI flag, no UI, no HTTP surface in v0. |
| `walking_skeleton` | `no` | Brownfield, isolated, sibling of `cinder-to-pulse-bridge-v0`. There is no UI backbone to span; every story IS a thin end-to-end slice through the writer. |
| `research_depth` | `lightweight` | Two worked precedents already shipped and validated: `LumenToOtlpJsonWriter` (the OTLP-JSON shape) and `CinderToPulseRecorder` (the Cinder event handling). This feature is the intersection of two settled patterns. |
| `jtbd_analysis` | `no` | Single dominant job: "operator wires `--observe-otlp <path>` and gets BOTH `kaleidoscope.lumen` AND `kaleidoscope.cinder` lines in the NDJSON stream". Persona and forces are the same as the Lumen OTLP-JSON writer (already validated by ship). DIVERGE artifacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. Job statement implicit: "operator wants `kaleidoscope.cinder` lines in the same NDJSON file that already carries `kaleidoscope.lumen` lines". | DIVERGE skipped by Andrea's explicit instruction. The writer has exactly one shape that compiles (`impl cinder::MetricsRecorder` writing OTLP-JSON `ResourceMetrics` to a `Write`); design space is collapsed by the trait signatures + the OTLP-JSON encoding the Lumen sibling already produces. |
| No formal JTBD workshop | LOW. Persona, push, pull, anxiety, habit are mirror-image of the Lumen OTLP-JSON writer (already validated by ship in commit `c6b336c`). | Persona + emotional-arc inherited from `cinder-to-pulse-bridge-v0` and `lumen_otlp_json.rs` in `journey-observe-cinder-via-otlp-json-visual.md`. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by `cinder::MetricsRecorder` on the in-edge and by the OTLP-JSON NDJSON contract established by `LumenToOtlpJsonWriter` on the out-edge. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D1: Same three metric names as `CinderToPulseRecorder` (cross-bridge contract)

The metric names are NOT a per-writer decision. They are a cross-bridge
contract locked by the sibling feature `cinder-to-pulse-bridge-v0`:

- `cinder.place.count` — Sum, value=1, point attribute `tier=hot|warm|cold`
- `cinder.migrate.count` — Sum, value=1, point attributes `from=...`, `to=...`
- `cinder.evaluate.migrated.count` — Sum, value=migrated_count (per-tenant
  total from one `evaluate_at` call)

Rationale: operators querying their downstream OTLP collector after the
NDJSON sidecar has forwarded the stream must see exactly the same metric
names whether the bridge in use is `CinderToPulseRecorder` (in-process) or
`CinderToOtlpJsonWriter` (cross-process). Otherwise the choice of bridge
becomes a name-changing operational concern; that is the opposite of the
"transparent observability substrate" job statement.

The string literals will be duplicated between
`crates/self-observe/src/cinder_bridge.rs` (Pulse sink) and
`crates/self-observe/src/cinder_otlp_json.rs` (OTLP-JSON sink) at v0. The
duplication is acknowledged and accepted (see D7 — rule-of-three not yet
reached for extracting a shared constants module).

### D2: Scope name `kaleidoscope.cinder` parallel to `kaleidoscope.lumen`

The OTLP-JSON `scope.name` field identifies the instrumentation library
inside each `ResourceMetrics` envelope. The Lumen writer uses
`kaleidoscope.lumen` (locked in `lumen_otlp_json.rs:138`). This writer
uses `kaleidoscope.cinder` so a downstream collector can group metrics by
producer crate without parsing the metric-name prefix.

### D3: Tier topology as a POINT attribute, not a resource attribute

Inherited verbatim from `cinder-to-pulse-bridge-v0` D4. Tiers
(`hot`/`warm`/`cold`) and migration directions (`from`/`to`) are
per-event facts, not per-process facts. They go in the point's
`attributes` array, not in the top-level `resource.attributes`.

`tenant_id` lives in BOTH places (resource attribute AND point attribute)
because that mirrors the Lumen writer's choice (`lumen_otlp_json.rs:39-43`)
and collectors disagree on which one they prefer. Emitting both is the
safer interop choice.

`Tier` enum (`Hot`/`Warm`/`Cold`) lowercases to `"hot"`/`"warm"`/`"cold"`
in attribute values, matching the Pulse-sink convention.

### D4: `record_evaluate` emits `asInt = migrated_count`, not `asInt = 1`

Inherited verbatim from `cinder-to-pulse-bridge-v0` D2. The Cinder trait
signature `fn record_evaluate(&self, tenant: &TenantId, migrated: usize)`
already carries the meaningful integer. The OTLP-JSON `dataPoints[0].asInt`
field encodes it directly (as a uint64 string per OTLP-JSON convention),
matching the Lumen writer's encoding of `record_count` for ingest.

### D5: Best-effort emission (no error propagation, no panic)

Inherited verbatim from the Lumen OTLP-JSON writer (`lumen_otlp_json.rs:182-189`).
The pattern:

```rust
if let Ok(line) = serde_json::to_string(&payload) {
    if let Ok(mut writer) = self.inner.lock() {
        let _ = writer.write_all(line.as_bytes());
        let _ = writer.write_all(b"\n");
        let _ = writer.flush();
    }
}
```

Rationale: `cinder::MetricsRecorder` trait methods return `()`. They have
no channel to propagate errors anyway. A serialisation failure (impossible
in practice for these hand-rolled structs) or a write failure (disk full,
pipe broken) is silently dropped. The choice mirrors the Lumen writer
exactly so an operator wiring `--observe-otlp <path>` cannot get one
writer to crash on a failure mode the other tolerates.

### D6: Atomic per-line `write_all` + flush; same `Mutex<W>` locking pattern

Critical cross-feature invariant (see `## Operator stream contract` in the
task brief). When BOTH the Lumen writer and this Cinder writer are
wired against the SAME file (the CLI's `--observe-otlp <path>` mode), the
NDJSON stream MUST remain valid no matter which writer contributes the
next line. Concretely:

1. Every line is one complete `ResourceMetrics` envelope (D1+D2+D3 lock
   the shape).
2. Every line is terminated by `\n`.
3. Every line is independently parseable JSON.
4. Two writers writing to the same `Write` MUST NOT interleave bytes
   within a single logical line.

The Lumen writer enforces (4) via the `Mutex<W>` guard around the
write_all + write_all(b"\n") + flush triple
(`lumen_otlp_json.rs:182-189`). Inside that critical section the writer
calls `write_all` twice (line body, then `\n`). This is acceptable
because POSIX small writes to the same FD from the same OS thread holding
the same in-process Mutex do not interleave with writes from OTHER
in-process Mutex holders to the SAME FD — each writer's `Mutex<W>` is
separate (one for `LumenToOtlpJsonWriter`, one for
`CinderToOtlpJsonWriter`) but they wrap DIFFERENT `W` values, even when
those `W`s share an underlying file via `File::try_clone()`.

The DESIGN wave inherits the same `Mutex<W>` pattern and the same
two-write-call structure. If a real production scenario surfaces cross-
process interleaving (one CLI process holding one Mutex, another CLI
process holding its own Mutex, both writing to the same path), the answer
is OS-level atomic append (`O_APPEND` on POSIX), which `OpenOptions::new().append(true)`
already requests at the call site in `kaleidoscope-cli/src/lib.rs:148-152`.
Per-line atomicity under cooperative `O_APPEND` for writes smaller than
PIPE_BUF (4096 on Linux) is the industry-standard assumption for NDJSON
log files. Documented here so the design doc does not re-derive it.

### D7: No shared OTLP-JSON serialisation module yet (rule of three)

The hand-rolled OTLP-JSON serde structs in `lumen_otlp_json.rs:62-120`
(`OtlpResourceMetrics`, `OtlpResource`, `OtlpScopeMetrics`, `OtlpScope`,
`OtlpMetric`, `OtlpSum`, `OtlpNumberPoint`, `OtlpAttr`, `OtlpAttrValue`)
will be **duplicated** into `cinder_otlp_json.rs` at v0. After this
feature ships, the workspace has two instances of the fixed-attribute
shape (Lumen, Cinder). The rule of three says wait for a third.

If/when Sluice or Augur (or any other crate) needs an OTLP-JSON writer
with a different attribute shape (e.g. `Vec<OtlpAttr>` per point instead
of `[OtlpAttr; N]`), that is a separate feature and a separate decision
about whether to extract a shared module. This wave does NOT make that
decision.

The DESIGN wave should resist any temptation to extract prematurely.

The duplication is small (~60 lines of pure data structs with `#[derive(Serialize)]`)
and the cost of getting the abstraction wrong (forcing Lumen and Cinder
to share a parameterised type that doesn't quite fit a future third
caller) is higher than the cost of one more copy. Sandi Metz's rule:
duplication is cheaper than the wrong abstraction.

### D8: Two metrics per `evaluate_at` line of output, not one combined line

When `cinder.evaluate_at` migrates N items for tenant `acme`, Cinder
emits:

- N calls to `record_migrate(acme, from, to)` (one per item)
- 1 call to `record_evaluate(acme, N)`

The writer produces N+1 NDJSON lines for this tenant: N lines of
`cinder.migrate.count` and 1 line of `cinder.evaluate.migrated.count`.
The downstream collector aggregates these naturally.

The writer does NOT combine multiple metrics into a single OTLP-JSON
envelope, even though OTLP-JSON permits `scopeMetrics[].metrics[]` to
hold multiple `Metric` entries. The one-event-per-line rule is the
NDJSON contract this feature shares with the Lumen writer; breaking it
would force the sidecar to know that some lines carry single metrics
and others carry compound metrics. Same shape as the Pulse-sink sibling
(see `cinder-to-pulse-bridge-v0` D3).

### D9: CLI wiring is OUT of scope

Out of scope. The follow-up feature
(`kaleidoscope-cli-wires-cinder-otlp-bridge-v0`, or merged with the Pulse
CLI wiring feature) will plumb the writer into `kaleidoscope-cli`. That
feature owns the choice between:

- Always wire Cinder + Lumen to the same file (current `--observe-otlp <path>`)
- A separate `--observe-cinder-otlp <path>` flag for split streams

This feature ships the library only, with acceptance tests driving the
writer against `cinder::InMemoryTieringStore` + an in-memory `Write`
sink (`Arc<Mutex<Vec<u8>>>`).

### D10: SSOT journey + jobs.yaml are NOT modified in this wave

Same posture as `cinder-to-pulse-bridge-v0` D7. The SSOT operator-incident-
response journey is incident-time focused; this writer serves the
orthogonal "operator gets cross-process observability of platform
internals" journey, which is post-v0 of this feature. The feature-local
journey artefact (`discuss/journey-observe-cinder-via-otlp-json.yaml`)
produced in this wave is NOT promoted to `docs/product/journeys/`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 3 stories, 1 bounded context (`self-observe` crate), 1 new
file (`crates/self-observe/src/cinder_otlp_json.rs`) plus 1 new
acceptance-test file (`crates/self-observe/tests/cinder_to_otlp_json.rs`)
plus 2 modifications (`Cargo.toml`, `lib.rs`). Estimated effort: ~1 day
for an experienced Rust crafter familiar with both precedents. PASSES the
right-sized gate.

## Handoff

Next wave: DESIGN (nw-solution-architect). Inputs delivered:

- `journey-observe-cinder-via-otlp-json-visual.md`
- `journey-observe-cinder-via-otlp-json.yaml`
- `journey-observe-cinder-via-otlp-json.feature`
- `shared-artifacts-registry.md`
- `story-map.md`
- `prioritization.md`
- `user-stories.md`
- `dor-validation.md`
- `outcome-kpis.md`
- `slices/slice-01-place-events-emit-otlp-json-lines.md`
- `slices/slice-02-migrate-events-emit-otlp-json-lines-with-direction.md`
- `slices/slice-03-evaluate-events-emit-otlp-json-lines-with-per-tenant-counts.md`
