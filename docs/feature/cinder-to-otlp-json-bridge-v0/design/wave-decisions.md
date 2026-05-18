# Wave Decisions — `cinder-to-otlp-json-bridge-v0` / DESIGN

Decisions made by `nw-solution-architect` (Morgan) during the DESIGN
wave for this feature. Each decision is propose-mode: 2–3 options
enumerated, one recommended, rationale traceable to DISCUSS artefacts
and to the worked precedents (`LumenToOtlpJsonWriter`,
`CinderToPulseRecorder`).

The DISCUSS wave (`discuss/wave-decisions.md`) already locked the
behaviour contract (D1–D10). This DESIGN wave decides the **shape** of
the code that realises that contract.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Source |
|----------|-------|--------|
| `interaction_mode` | `propose` | `/nw-design` Decision 1 |
| `feature_type` | `backend` (library-only) | DISCUSS wave-decisions D9 |
| `walking_skeleton` | `no` | DISCUSS wave-decisions pre-wave |
| `architect_specialisation` | `nw-solution-architect` (application-level) | Andrea's Decision 0 |
| Three metric names | `cinder.place.count`, `cinder.migrate.count`, `cinder.evaluate.migrated.count` | DISCUSS D1 (cross-bridge contract with ADR-0038) |
| Scope name | `kaleidoscope.cinder` | DISCUSS D2 |
| Tier serialisation | lowercase string (`"hot"`/`"warm"`/`"cold"`) | DISCUSS D3 |
| `record_evaluate` value | `migrated.to_string()` (NOT `"1"`) | DISCUSS D4 |
| Emission posture | best-effort, `let _ = ...`, no panic | DISCUSS D5 |
| Atomicity pattern | `Mutex<W>` + `write_all(body) + write_all(b"\n") + flush` inside critical section | DISCUSS D6 |
| Serde-struct duplication | duplicate from `lumen_otlp_json.rs` at v0 (rule of three not reached) | DISCUSS D7 |
| Emission cardinality | one OTLP-JSON line per `MetricsRecorder` call (no batching, no compound metrics) | DISCUSS D8 |
| CLI wiring | OUT of scope for this feature | DISCUSS D9 |
| SSOT journey + jobs.yaml | NOT modified in this wave | DISCUSS D10 |

---

## Reuse Analysis (RCA F-1 hard gate)

| Existing component | Path | Decision | Why |
|--------------------|------|----------|-----|
| `self_observe::LumenToOtlpJsonWriter` | `crates/self-observe/src/lumen_otlp_json.rs` | **REUSE THE SHAPE** (not the type) | The OTLP-JSON envelope serde structs (`OtlpResourceMetrics`, `OtlpResource`, `OtlpScopeMetrics`, `OtlpScope`, `OtlpMetric`, `OtlpSum`, `OtlpNumberPoint`, `OtlpAttr`, `OtlpAttrValue`) are duplicated into `cinder_otlp_json.rs` per DISCUSS D7. The Mutex<W> + emit pattern, the best-effort emission triple (lumen_otlp_json.rs:182-189), the `time_unix_nano` derivation (lumen_otlp_json.rs:142-146), and the `tenant_id` double-emission (resource + point) are all replicated 1:1. The struct *types* cannot be unified because Cinder's per-event attribute cardinality is different (place: 1 → 2; migrate: 0 → 3; evaluate: 0 → 1 attributes, depending on D2 below). |
| `self_observe::CinderToPulseRecorder` | `crates/self-observe/src/cinder_bridge.rs` | **REUSE THE EVENT-HANDLING SHAPE** (not the type) | Same `impl cinder::MetricsRecorder` shape: three trait methods (`record_place`, `record_migrate`, `record_evaluate`), same domain mapping (place → 1 attribute, migrate → 2 attributes, evaluate → 0 extra attributes). The `tier_lowercase` helper (cinder_bridge.rs:109-115) is duplicated verbatim (DISCUSS D7 covers the OTLP-JSON serde structs; the tier helper is a 7-line free function that is cheaper to copy than to share). Cannot reuse the type because the sink is `Mutex<W>` here, not `Arc<dyn MetricStore>`. |
| `cinder::InMemoryTieringStore` | `crates/cinder/src/store.rs` | **REUSE as acceptance-test driver** | Sibling Pulse feature established this seam (ADR-0038 §3 / DD1). The dual-emission contract (D8 here, D3 in the Pulse sibling) is naturally expressed only when Cinder's `evaluate_at` cascade runs end-to-end; direct invocation of `record_evaluate(&acme, 5)` would not trigger the five `record_migrate` calls Cinder makes. |
| `cinder::CapturingRecorder` | `crates/cinder/src/metrics.rs:57-110` | **REJECTED** as additional assertion target | Same reason as ADR-0038 §3 Alternative 3: Cinder ships its own in-tree tests against `CapturingRecorder`; using it here would duplicate Cinder's coverage without adding writer-specific evidence. The writer's contract terminates at the byte sequence on the `Write` sink. |
| `SharedBuf(Arc<Mutex<Vec<u8>>>)` test substrate | `crates/self-observe/tests/lumen_to_otlp_json.rs:54-64` | **REUSE THE PATTERN, duplicate the code** | The 11-line `SharedBuf` definition and the `collect_lines` helper (lumen_to_otlp_json.rs:66-73) are copied into `tests/cinder_to_otlp_json.rs`. Rule of three: extraction into a `tests/common.rs` module becomes warranted when a third OTLP-JSON-writer test file (e.g. `sluice_to_otlp_json.rs`) lands. v0 keeps the duplication explicit and local. |
| Production `File` handle wiring | `kaleidoscope-cli/src/lib.rs:139-160` | **OUT OF SCOPE** (DISCUSS D9) | The CLI follow-up feature plumbs the writer behind `--observe-otlp <path>`. v0 of this feature ships only the library; acceptance tests use `SharedBuf`. |
| `self-observe` crate itself | `crates/self-observe/` | **REUSE** | The lib.rs doc comment at lines 44-47 explicitly anticipates the `CinderToOtlpJsonWriter` addition. No new crate is created. |

**Hard-gate result**: PASS. Every load-bearing component has been
investigated for reuse before any new component is proposed. The only
new artefacts are one production source file (`cinder_otlp_json.rs`),
one test file (`cinder_to_otlp_json.rs`), one `Cargo.toml` `[[test]]`
block, one `lib.rs` re-export line, and one ADR (ADR-0039). No new
crate, no new dependency, no new architectural concept.

---

## In-wave decisions

### DD1: Module file location → `crates/self-observe/src/cinder_otlp_json.rs`

**Options considered**:

| # | Option | Pros | Cons |
|---|--------|------|------|
| 1 | `crates/self-observe/src/cinder_otlp_json.rs` (file-flat sibling) | Matches the established file-flat pattern (`lumen_bridge.rs`, `lumen_otlp_json.rs`, `cinder_bridge.rs`); naming convention parallel to Lumen (`lumen_otlp_json.rs` → `cinder_otlp_json.rs`); zero refactoring of existing files. | Crate root will hold N=4 sibling bridge files after this ships (acceptable). |
| 2 | `crates/self-observe/src/bridges/cinder_otlp_json.rs` (new subdirectory) | Anticipates the file-count growth (Sluice / Augur / Ray / Strata + their OTLP-JSON variants). | Premature at N=4 sibling files (DISCUSS D7 rule-of-three logic applied to layout). Forces a retrospective move of `lumen_bridge.rs` / `lumen_otlp_json.rs` / `cinder_bridge.rs` (or accepts inconsistency). The ADR-0038 §4 deferral applies identically here. |
| 3 | Embed in `cinder_bridge.rs` as `pub mod otlp_json` inside the Pulse-sink file | Co-locates both Cinder sinks; shared `tier_lowercase` import. | Conflates two unrelated sinks (one writes to Pulse, one writes to NDJSON); breaks the parallelism with the Lumen pair (lumen_bridge.rs and lumen_otlp_json.rs are separate files); ADR-0038 §4 explicitly chose separate files for the Lumen pair and the Cinder Pulse-sink follows; this would be the only outlier. |

**Recommendation**: Option 1. Same rationale as ADR-0038 §4. The
`bridges/` subdirectory becomes warranted at ~8 sibling files (Sluice,
Augur, Ray, Strata Pulse-sink and OTLP-JSON-sink variants); v0 of this
feature does not pre-pay the refactor.

### DD2: Attribute-array shape — per-method fixed-size arrays vs `Vec<OtlpAttr>` vs enum-shaped points

The Lumen writer uses `[OtlpAttr; 1]` for both the resource-attribute
slot and the point-attribute slot (`lumen_otlp_json.rs:71, 103`). This
works because every Lumen event has exactly 1 point attribute
(`tenant_id`). Cinder's three events have different point-attribute
cardinality:

| Cinder method | Point attributes (per DISCUSS + sibling Pulse) | Cardinality |
|---------------|------------------------------------------------|-------------|
| `record_place(tenant, tier)` | `tenant_id` (mirroring lumen_otlp_json.rs:39-43) + `tier` | **2** |
| `record_migrate(tenant, from, to)` | `tenant_id` + `from` + `to` | **3** |
| `record_evaluate(tenant, migrated)` | `tenant_id` only (count is in `asInt`, not an attribute) | **1** |

**Options considered**:

| # | Option | Pros | Cons |
|---|--------|------|------|
| 1 | Three separate `OtlpNumberPoint*` structs per method, each with a fixed-size `[OtlpAttr; N]` (N=2 for place, N=3 for migrate, N=1 for evaluate); the envelope structs (`OtlpResourceMetrics`, `OtlpScopeMetrics`, etc.) become generic over a `P: Serialize` payload position. | Zero heap allocation per event (matches the Lumen writer's allocation profile exactly for the attributes slot); the compiler enforces the per-method attribute cardinality at type-check time. | Three near-duplicated struct definitions; per-method monomorphisation of the envelope; more code; the per-event-type rigidity prevents adding a fourth attribute (e.g. a future `cause: &str` on migrate) without a new struct variant. |
| 2 | One `OtlpNumberPoint` struct with `attributes: Vec<OtlpAttr<'a>>`; the Lumen-style fixed-size arrays at the envelope level (`scope_metrics: [_; 1]`, `metrics: [_; 1]`, `data_points: [_; 1]`) are preserved because Cinder still emits exactly one metric per line per DISCUSS D8. | One struct serves all three methods; trivial to extend with a fourth attribute on any event; the heap allocation is one `Vec` per emission (≤3 entries, allocation will most likely fit in a small-vector inline buffer the allocator handles cheaply); the allocation matches the cost of the existing `CinderToPulseRecorder`'s `BTreeMap<String, String>` per emission (cinder_bridge.rs:119, 125, 132), which is the established cost basis at this seam. | One `Vec<OtlpAttr>` allocation per event (acceptable: the writer is on a best-effort observability path, not a hot path; the operator's NDJSON sink is the bottleneck, not the emit). The `[OtlpAttr; 1]` resource-attributes array at the envelope level is preserved (still exactly one resource attribute: `tenant_id`). |
| 3 | An enum `enum OtlpCinderPoint<'a> { Place([OtlpAttr<'a>; 2]), Migrate([OtlpAttr<'a>; 3]), Evaluate([OtlpAttr<'a>; 1]) }` with `Serialize` derived per variant. | Type-system encodes the per-method attribute cardinality; no heap allocation. | Most code of all three options; the enum's `Serialize` derivation produces a tagged JSON shape unless `#[serde(untagged)]` is added, which is one more easy-to-get-wrong piece of cleverness; the operator never sees the enum, only the JSON, so the type-system value of the enum is local to the writer and not externally observable. |

**Recommendation**: Option 2. The allocation cost matches the
established cost basis at the sibling seam (cinder_bridge.rs already
allocates a `BTreeMap<String, String>` per emission, which is more
expensive than a single small `Vec<OtlpAttr>`). The single-struct shape
makes the code legible at the rate-of-three threshold (extracting a
shared module for future writers becomes mechanical: one struct, not
three). The per-method allocation cost is bounded (≤3 entries; the
`Vec` will typically fit in the allocator's smallest size class). The
type-safety upside of Option 1 is purchased at a high readability cost
that the cross-bridge consistency goal (one mental model across every
`XxxToOtlpJsonWriter` in `self-observe`) does not justify. Option 3
adds JSON-serde subtleties for no operator-observable benefit.

The fixed-size `[T; 1]` arrays at the envelope level (`scope_metrics`,
`metrics`, `data_points`) are PRESERVED unchanged from the Lumen
writer — Cinder still emits exactly one metric per line per DISCUSS D8.
Only the point-`attributes` slot becomes `Vec<OtlpAttr<'a>>`. This
mirrors the structural difference from Lumen exactly: the cardinality
that changed is the per-point attribute count; nothing else changed.

### DD3: Acceptance-test seam → `cinder::InMemoryTieringStore` drives, `SharedBuf(Arc<Mutex<Vec<u8>>>)` captures, `serde_json::Value` asserts

**Options considered**:

| # | Option | Pros | Cons |
|---|--------|------|------|
| 1 | Drive `cinder::InMemoryTieringStore`; sink into `SharedBuf(Arc<Mutex<Vec<u8>>>)`; parse captured bytes line-by-line as `serde_json::Value`; assert against the parsed JSON tree. **Mirrors the Lumen OTLP-JSON tests verbatim** (`tests/lumen_to_otlp_json.rs:54-73, 75-128, 130-189, 191-219`). **Mirrors the Cinder Pulse-sink test seam** for the driver side (ADR-0038 §3). | One canonical pattern across the four writer test files; the dual-emission contract from DISCUSS D8 is naturally expressed in Slice 03's tests (one `evaluate_at` call produces both per-item migrate lines and per-tenant evaluate lines in the same sink); `serde_json::Value` assertions are robust to JSON whitespace and field-ordering variations the writer might unintentionally introduce. | Tests entangle with Cinder behaviour (a Cinder regression that breaks the `record_migrate` cascade from `evaluate_at` will break this writer's Slice 03 dual-emission test even if the writer is correct). Same trade-off as ADR-0038 §3 Alternative 2; the same acceptance applies here. |
| 2 | Drive the writer directly (`writer.record_place(&acme, Tier::Hot)`), bypassing Cinder; sink into `SharedBuf`; parse and assert. | No entanglement with Cinder; smaller test surface; bridge-only assertions. | Cannot express the dual-emission contract (one `evaluate_at` produces N migrate lines AND 1 evaluate line); Slice 03 would need to manually emit the cascade (six direct calls to simulate one `evaluate_at(t0+25h)` over 5 placed items), which is brittle and a low-fidelity reproduction of the real wiring. Identical rejection to ADR-0038 §3 Alternative 2. |
| 3 | Drive the writer through `cinder::InMemoryTieringStore`; sink into a real `tempfile::NamedTempFile`; parse the file contents. | Exercises real `File::write_all` semantics including `O_APPEND` (closer to the post-v0 CLI integration). | New `tempfile` dev-dependency (not currently in the workspace); v0 scope is the LIBRARY contract on a generic `W: Write + Send + Sync`, not the real-file integration (DISCUSS D9 + the `shared-artifacts-registry.md > file_handle` MEDIUM-risk note assign the real-file semantics to the CLI follow-up feature's tests); adds substrate complexity v0 does not need. |

**Recommendation**: Option 1. Identical posture to ADR-0038 §3 and to
the Lumen OTLP-JSON tests already shipped. Consistency across the four
writer test files (`lumen_to_pulse.rs`, `lumen_to_otlp_json.rs`,
`cinder_to_pulse.rs`, `cinder_to_otlp_json.rs`) outweighs the
test-isolation upside of Option 2; the entanglement risk is the same
one already accepted on the Pulse-sink sibling and has not caused
incidents.

**Compile-time probe** (in Slice 01, covers all slices):

```rust
#[test]
fn the_writer_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>();
}
```

Mirrors `tests/lumen_to_pulse.rs:204-212` and `tests/cinder_to_pulse.rs`'s
equivalent (added by the sibling feature). This is the subtype-check
layer of the Earned Trust contract (Principle 12c).

### DD4: Stub posture for `record_migrate` / `record_evaluate` in Slice 01 → empty no-op `impl` (NOT `todo!()`)

Slice 01 ships `record_place` implemented and the other two methods
as stubs. Slice 01's tests do not exercise `record_migrate` or
`record_evaluate`. The question is whether the stubs should panic on
invocation or silently do nothing.

**Options considered**:

| # | Option | Pros | Cons |
|---|--------|------|------|
| 1 | Empty `impl` body: `fn record_migrate(&self, _t, _f, _to) {}`, same for `record_evaluate`. | Compiles immediately; preserves the writer's `Send + Sync` test in Slice 01; matches `NoopRecorder`'s exact behaviour for the two un-implemented methods, so Slice 01's tests cannot accidentally trigger a panic by exercising the wrong Cinder operation. | A future maintainer writing Slice 02's first test before implementing `record_migrate` would see "0 lines, no error" instead of a loud panic, masking the missing implementation. |
| 2 | `todo!()` body: `fn record_migrate(&self, _t, _f, _to) { todo!() }`, same for `record_evaluate`. | Loudly panics if Slice 01 accidentally invokes the un-implemented path; signals to Slice 02 implementers that the method body must be replaced. | Slice 01's "no event no byte" test would still pass (no Cinder call invokes record_migrate without a prior matching record_place + tier policy event), so the loudness gain is theoretical. If a future test refactor causes a stray call, the panic is the diagnostic. |
| 3 | `unimplemented!()` body (semantically equivalent to `todo!()` in stable Rust). | Same as Option 2, with a slightly more semantic name. | Same as Option 2. |

**Recommendation**: Option 1 (empty no-op). Slice 02 and Slice 03 are
the very next slices; their RED phase tests are the loudness mechanism
that catches a missing implementation (the test will assert the
expected line shape, find an empty sink, and fail). Adding `todo!()`
panics adds noise to the Slice 01 build with no diagnostic benefit
during the brief Slice-01-only window. The DELIVER wave will replace
each stub in turn under acceptance-test pressure; the discipline
guarantees no stub ships to v0 release.

This option also matches the Pulse-sink sibling's behaviour precisely
(the sibling's `record_migrate` and `record_evaluate` are no-op
emissions if their `Cinder` event happens before the bridge is wired
in, by virtue of `Cinder`'s own cascade not firing).

### DD5: ADR scope → one ADR (ADR-0039)

**Options considered**:

| # | Option | Pros | Cons |
|---|--------|------|------|
| 1 | One ADR (ADR-0039): public surface, crate layout, per-event emission contract, test seam, file location. | Matches the established per-crate-public-API ADR convention (ADR-0011, ADR-0018, ADR-0022, ADR-0026, ADR-0033, ADR-0038); records the cross-bridge metric-name contract, the NDJSON-validity contract, and the Mutex-atomicity inheritance in one place; minimal paperwork. | None significant. |
| 2 | Two ADRs (ADR-0039 public surface + ADR-0040 cross-bridge OTLP-JSON serde-struct duplication convention). | Maximum traceability; the OTLP-JSON serde-struct duplication choice (DISCUSS D7) earns its own audit-trail artefact. | Premature formalisation: the duplication convention has only two exemplars (Lumen writer at v0, Cinder writer here). Third bridge (Sluice/Augur/Ray/Strata OTLP-JSON writer) makes the convention worth formalising. The same rule-of-three logic ADR-0038 §5 Alternative 5 applied to the cross-bridge test-seam convention applies here to the serde-struct convention. |
| 3 | Zero ADRs (rely on this feature's `design/` artefacts plus the existing ADR-0038). | Less paperwork. | Inconsistent with the convention. The Cinder OTLP-JSON writer is the second new public type in `self-observe` (the first being the Lumen OTLP-JSON writer, which pre-dated the per-crate-public-API ADR convention and so has no dedicated ADR; ADR-0038 retro-fitted the convention for the Cinder Pulse-sink). Skipping an ADR here would leave a documentation gap exactly where the cross-bridge NDJSON-validity invariant (DISCUSS D6) and the cross-bridge metric-name contract (DISCUSS D1) need a referenceable artefact. |

**Recommendation**: Option 1. Justified for the same reasons ADR-0038
was: the new writer is a public type in a crate that other crates and
the CLI follow-up will depend on; the public surface and the per-event
emission contract are exactly what `cargo public-api` will lock in CI
(Gate 2 per ADR-0005). One ADR per public type per crate is the
project's established convention; this feature continues it.

---

## Scope assessment (Elephant Carpaccio gate)

Right-sized for one DELIVER session. Concretely:

- 1 new production source file (`crates/self-observe/src/cinder_otlp_json.rs`, ~150 lines including the duplicated serde structs, the `Mutex<W>` wrapper, the three trait methods, the `emit` helper, the `tier_lowercase` helper).
- 1 new acceptance-test file (`crates/self-observe/tests/cinder_to_otlp_json.rs`, ~250 lines covering Slices 01/02/03 blocks plus the compile-time Send+Sync probe).
- 1 modification to `crates/self-observe/src/lib.rs` (2 lines: `mod cinder_otlp_json;` and `pub use cinder_otlp_json::CinderToOtlpJsonWriter;`).
- 1 modification to `crates/self-observe/Cargo.toml` (1 new `[[test]]` block; the `cinder` dependency line was added by the Pulse-sink sibling).
- 1 new ADR (`docs/product/architecture/adr-0039-cinder-to-otlp-json-bridge-public-api-and-crate-layout.md`).
- 1 brief.md APPEND (new section, no modification of existing sections).

PASSES the right-sized gate. Estimated DELIVER effort: ~1 day for an
experienced Rust crafter familiar with both precedents.

---

## Handoff

Next wave: DISTILL (`nw-acceptance-designer`).

Inputs delivered to DISTILL:

- `docs/feature/cinder-to-otlp-json-bridge-v0/design/wave-decisions.md` (this file)
- `docs/feature/cinder-to-otlp-json-bridge-v0/design/application-architecture.md` (propose-mode walkthrough)
- `docs/product/architecture/brief.md` — new section `## Application Architecture — cinder-to-otlp-json-bridge-v0` appended
- `docs/product/architecture/adr-0039-cinder-to-otlp-json-bridge-public-api-and-crate-layout.md`

Followed by DEVOPS (`nw-platform-architect`).
