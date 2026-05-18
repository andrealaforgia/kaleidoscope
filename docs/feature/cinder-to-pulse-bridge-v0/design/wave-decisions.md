# Wave Decisions — `cinder-to-pulse-bridge-v0` / DESIGN

**Author**: `nw-solution-architect` (Morgan)
**Date**: 2026-05-18
**Mode**: PROPOSE (Decision 1 of `/nw-design` — read DISCUSS + worked
precedent, present 2-3 options per load-bearing decision, recommend one
with rationale.)

## Inputs read (in dependency order)

1. `CLAUDE.md` — paradigm declaration (Rust idiomatic, data + free
   functions + traits where polymorphism is genuinely needed).
2. `docs/product/architecture/brief.md` — extends, does not replace.
3. `docs/product/journeys/incident-response.yaml` — SSOT journey;
   confirmed orthogonal to this feature (post-v0 surface, not
   incident-time).
4. `docs/feature/cinder-to-pulse-bridge-v0/discuss/*` — D1-D7 wave
   decisions, three user stories (US-01/02/03), shared-artifacts
   registry, three slice files, BDD feature file.
5. `crates/self-observe/src/lumen_bridge.rs` — the precedent we mirror.
6. `crates/self-observe/src/lib.rs` — module structure we extend.
7. `crates/self-observe/Cargo.toml` — dependency posture we extend.
8. `crates/self-observe/tests/lumen_to_pulse.rs` — test-seam pattern
   already shipped and validated.
9. `crates/cinder/src/metrics.rs` — `MetricsRecorder` trait + test seams.
10. `crates/cinder/src/store.rs` — `InMemoryTieringStore` + the
    `evaluate_at` dual-emission behaviour.
11. `crates/cinder/src/tier.rs` — `Tier` enum.
12. `crates/pulse/src/store.rs` — `MetricStore` trait + adapter +
    `MetricStoreError` (empty enum at v0).
13. Sample ADRs: `adr-0001`, `adr-0011`, `adr-0018`, `adr-0022`,
    `adr-0026`, `adr-0033` — ADR style + crate-layout precedent.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Source |
|----------|-------|--------|
| Design scope | Application / components only | Pre-wave Decision 0 |
| Interaction mode | PROPOSE | Pre-wave Decision 1 |
| Skipped: domain modelling | We consume existing trait types | Pre-wave Decision 0 |
| Skipped: system / infrastructure modelling | In-process library, no deployment shape | Pre-wave Decision 0 |
| Three separate metric names | `cinder.place.count`, `cinder.migrate.count`, `cinder.evaluate.migrated.count` | DISCUSS D1 |
| `value = migrated_count` on evaluate, not `value = 1 + attribute` | Natural aggregation; mirror Lumen choice | DISCUSS D2 |
| Dual emission (migrate + evaluate) preserved | Cinder behaviour the bridge inherits | DISCUSS D3 |
| Tier topology as POINT attribute | Per-event fact, not per-process; lowercase strings | DISCUSS D4 |
| Best-effort emission (swallow `MetricStoreError`) | Matches Lumen; trait method returns `()` | DISCUSS D5 |
| No CLI in v0 | Library only; CLI is a follow-up feature | DISCUSS D6 |
| SSOT journey + jobs.yaml unmodified | Orthogonal to incident response | DISCUSS D7 |

## Reuse Analysis (MANDATORY, RCA F-1 hard gate)

Before proposing any new component, the codebase was searched for
components with overlapping responsibilities.

| Existing component | Path | Overlap with proposed work | Decision |
|--------------------|------|----------------------------|----------|
| `LumenToPulseRecorder` | `crates/self-observe/src/lumen_bridge.rs` | Identical shape (impl of an `XxxMetricsRecorder` trait writing into `pulse::MetricStore`). Different domain: Lumen vs Cinder. | **REUSE THE SHAPE, NOT THE TYPE.** A trait-generic bridge `Box<dyn Recorder>` would couple Cinder's and Lumen's trait shapes (different method signatures, different parameter sets). A single generic bridge cannot exist because Rust traits do not unify across crates. We **clone the shape** (struct + `new` + `emit` + `impl Recorder`) into a new `cinder_bridge.rs`. The shape-clone is intentional: it is the project's bridge convention. |
| `cinder::NoopRecorder` | `crates/cinder/src/metrics.rs:32-38` | Default no-op recorder for unwired deployments. | Not relevant — it is the *thing we replace* in the bridge wiring; not a reuse candidate. |
| `cinder::CapturingRecorder` | `crates/cinder/src/metrics.rs:57-110` | In-process test helper. Captures `RecordedEvent::{Place,Migrate,Evaluate}` to a `Vec` for assertion. | **CONSIDERED FOR ACCEPTANCE TESTS, REJECTED.** It asserts what *Cinder intends to emit*, which is upstream behaviour already covered by Cinder's own tests. The cinder-to-pulse bridge's contract is "Cinder emission becomes Pulse points" — the assertion must be against Pulse, not against an intermediate. Using `CapturingRecorder` would skip the bridge entirely. (Decision DD1 below.) |
| `pulse::InMemoryMetricStore` | `crates/pulse/src/store.rs:89-212` | In-process `MetricStore` adapter; the natural query-side test seam. | **REUSE IN ACCEPTANCE TESTS.** Exactly mirrors `lumen_to_pulse.rs` precedent: construct `Arc<InMemoryMetricStore>`, hand to bridge, query through the same `Arc`. |
| `cinder::InMemoryTieringStore` | `crates/cinder/src/store.rs:89-233` | In-process `TieringStore` that calls `record_place` / `record_migrate` / `record_evaluate` on its recorder. | **REUSE IN ACCEPTANCE TESTS.** Drives the bridge through Cinder's full call surface, which is the realistic operator wiring (US-01 Elevator Pitch: Priya replaces `NoopRecorder` with the bridge). Slice 03 in particular relies on `InMemoryTieringStore::evaluate_at` producing the dual emission — testing the bridge through Cinder, not around it, is what makes the dual-emission assertion meaningful. |
| `self_observe::LumenToOtlpJsonWriter` | `crates/self-observe/src/lumen_otlp_json.rs` | OTLP-JSON cross-process variant of the Lumen bridge. | Out of scope for v0. Listed for completeness; a future `CinderToOtlpJsonWriter` follows the same shape after the in-workspace bridge ships. |
| `self-observe` crate itself | `crates/self-observe/` | The bridge's home crate. | **REUSE.** Lib comment line 44 (`// Cinder, Sluice, Augur, Ray, Strata bridges follow XxxToPulseRecorder / XxxToOtlpJsonWriter naming.`) **explicitly anticipated** this addition. No new crate is created. |

**Conclusion of Reuse Analysis**: The bridge is a *new file in an
existing crate* implementing a *new trait* (Cinder's) using *existing
infrastructure* (Pulse's `MetricStore`, the `XxxToPulseRecorder`
shape). Zero new crates. Zero new external dependencies. One new
in-workspace dependency (`cinder = { path = "../cinder", version = "0.1.0" }`).

## In-wave decisions (DD = DESIGN Decision)

### DD1: Acceptance-test seam — drive Cinder, query Pulse (mirror the precedent)

**Options considered**:

1. **Drive bridge directly**: instantiate `CinderToPulseRecorder` and
   call `recorder.record_place(...)` / `record_migrate(...)` /
   `record_evaluate(...)` directly. Assert against `InMemoryMetricStore`.
2. **Drive Cinder's store, the bridge is the recorder**: instantiate
   `InMemoryTieringStore::new(Box::new(bridge))` and call
   `cinder.place(...)` / `migrate(...)` / `evaluate_at(...)`. Assert
   against `InMemoryMetricStore`. *(Mirror of the Lumen bridge test
   pattern: `InMemoryLogStore::new(Box::new(bridge))`.)*
3. **Use `CapturingRecorder`** as an additional assertion target. Verify
   what Cinder *intends* to emit, separately from what the bridge
   actually pushes to Pulse.

**Recommendation**: **Option 2**.

**Rationale**:
- Mirrors `crates/self-observe/tests/lumen_to_pulse.rs` exactly. The
  Lumen bridge tests wire `InMemoryLogStore::new(Box::new(bridge))` and
  query `pulse.query(...)`. The Cinder bridge tests wire
  `InMemoryTieringStore::new(Box::new(bridge))` and query
  `pulse.query(...)`. Convention consistency across the
  `self-observe` test suite has high informational value at review
  time.
- The dual-emission test in Slice 03 (`evaluate_at` produces both
  `cinder.migrate.count` *and* `cinder.evaluate.migrated.count` points)
  is **only meaningful when driven through Cinder**. Calling the
  bridge's `record_evaluate` directly would not exercise the
  `record_migrate` cascade inside `InMemoryTieringStore::evaluate_at`
  (`crates/cinder/src/store.rs:200-232`). Option 1 cannot express the
  dual-emission contract; it would falsely separate the assertions.
- Option 3 (`CapturingRecorder`) asserts the wrong thing. It verifies
  Cinder's own emission behaviour, which Cinder already tests in-tree.
  The bridge's contract is "Cinder emission becomes queryable Pulse
  points" — the assertion must terminate at Pulse, not at an
  intermediate.
- The bridge's `Send + Sync` compile-time check (US-01 AC 7) is a
  separate one-line assertion (`fn assert_send_sync<T: Send + Sync>(); assert_send_sync::<CinderToPulseRecorder>();`) and is shipped in
  Slice 01's test file. Mirrors `lumen_to_pulse.rs:204-212`.

**Trade-off accepted**: tests exercise both Cinder and the bridge in a
single assertion. A regression in Cinder's `evaluate_at` cascade would
make the bridge's evaluate-test red — but this is the correct
behaviour: Cinder's cascade *is* the contract the bridge inherits per
DISCUSS D3.

### DD2: Module file location — `crates/self-observe/src/cinder_bridge.rs` (file-flat)

**Options considered**:

1. **File-flat**: `crates/self-observe/src/cinder_bridge.rs` — sibling
   to the existing `lumen_bridge.rs` and `lumen_otlp_json.rs`.
2. **Subdirectory**: `crates/self-observe/src/bridges/cinder.rs` —
   group all bridges under `bridges/`, introducing a new `mod bridges;`
   in `lib.rs`.

**Recommendation**: **Option 1**.

**Rationale**:
- `lib.rs` lines 51-52 already declare bridges as siblings at the
  crate root: `mod lumen_bridge;` and `mod lumen_otlp_json;`. The
  established pattern is file-flat.
- Introducing `bridges/` for one additional file is over-organisation
  at N=3 modules (lumen-bridge, lumen-otlp-json, cinder-bridge). The
  threshold at which `bridges/` pays off is when the file count
  passes ~6 and the directory becomes an information unit by itself
  (rule of thumb from Rust ecosystem: `serde_json/src/` stays flat at
  ~10 files; `tokio/src/` introduces subdirectories at ~30). We are
  at the start of the curve.
- The lib.rs comment block at lines 44-47 anticipates "Cinder, Sluice,
  Augur, Ray, Strata bridges follow `XxxToPulseRecorder` /
  `XxxToOtlpJsonWriter` naming". If/when the file count grows to 6+,
  a single mechanical move into `bridges/` is a contained
  refactoring; the *current* feature should not pay that cost
  speculatively.

**Trade-off accepted**: when Sluice/Augur/Ray/Strata bridges land,
the `lib.rs` root will hold one `mod xxx_bridge;` declaration per
bridge plus one `mod xxx_otlp_json;`. At ~8-10 files this becomes
visually heavy. The refactor is *not* this feature's concern; it is
deferred to a future "refactor: group self-observe bridges under
bridges/" change that ships when the threshold is reached.

### DD3: Public surface shape — byte-equivalent to `LumenToPulseRecorder`

**Options considered**:

1. **Byte-equivalent clone of `LumenToPulseRecorder`**: `pub struct
   CinderToPulseRecorder { pulse: Arc<dyn MetricStore + Send + Sync> }`,
   `pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self`,
   private `fn emit(&self, tenant, metric_name, value, attributes)`
   helper plus a tier-lowercase helper. Three `impl
   cinder::MetricsRecorder` methods.
2. **Add an `emit_with_attributes` helper that takes a `BTreeMap`** so
   each `record_*` method passes its own attribute set in. (This is
   the principled extension of the Lumen pattern — Lumen's `emit` has
   no attribute parameter because Lumen events have no point
   attributes.)
3. **Generic over the metric kind / unit** to make the helper reusable
   across all bridge metrics (place/migrate/evaluate). Lumen's helper
   already hardcodes `MetricKind::Sum`, unit `"1"`, no attributes;
   keeping the cinder helper similarly hardcoded matches the
   precedent.

**Recommendation**: **Option 1 + the targeted attribute extension from
Option 2**. The struct + constructor are byte-equivalent to
`LumenToPulseRecorder`. The internal helper grows one parameter
(`attributes: BTreeMap<String, String>`) because Cinder events carry
point attributes where Lumen events do not. Specifically:

```rust
// Public surface (locked):
pub struct CinderToPulseRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
}

impl CinderToPulseRecorder {
    pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self;
}

impl cinder::MetricsRecorder for CinderToPulseRecorder {
    fn record_place(&self, tenant: &TenantId, tier: Tier);
    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier);
    fn record_evaluate(&self, tenant: &TenantId, migrated: usize);
}
```

Internal helpers (crate-private, the crafter chooses the exact body):

- `fn emit(&self, tenant: &TenantId, metric_name: &str, value: f64, attributes: BTreeMap<String, String>)`
  — single emission path, mirrors `lumen_bridge.rs:52-76` with the
  one added parameter.
- `fn tier_str(tier: Tier) -> &'static str` (or
  `fn tier_attr_value(tier: Tier) -> String`) — the single lowercase
  serialisation point. Locking the lowercase strings in *one* helper
  prevents drift across the three `record_*` implementations.

**Rationale**:
- The field name `pulse`, the constructor name `new`, the constructor
  argument name and type `pulse: Arc<dyn MetricStore + Send + Sync>`,
  and the internal helper name `emit` are **byte-equivalent** to
  `LumenToPulseRecorder` (`crates/self-observe/src/lumen_bridge.rs:42-76`).
  This is mandated by the task brief and serves the
  shared-artifacts-registry's HIGH-risk `pulse_store` invariant: the
  bridge's constructor signature is the contract the post-v0 CLI
  wires against.
- The attribute parameter on `emit` is the minimum-friction extension
  of the Lumen helper. The alternative (one specialised helper per
  metric name) would scatter the lowercase-serialisation choice across
  three methods and invite drift.
- A single `tier_str` helper is the only place lowercase serialisation
  happens. DISCUSS D4 fixes the convention; the helper enforces it
  mechanically. Slices 01 and 02 both assert the lowercase strings
  directly, locking the contract from both sides.
- The crafter retains flexibility on the helper's exact body —
  inline `&'static str` returning the lowercase literal, or
  `tier.to_string().to_ascii_lowercase()`, or a `match` with
  pre-allocated strings. All three produce identical observable
  behaviour. ADR-0038 locks the *contract* (lowercase strings as
  documented); the crafter picks the *expression*.

**Trade-off accepted**: the `emit` helper diverges by one parameter
from `lumen_bridge.rs`. The divergence is required by the domain
(Cinder events carry attributes, Lumen events do not). This is not a
violation of the byte-equivalence rule, which applies to the public
surface (`pub struct`, `pub fn new`) and not to internal helpers.

### DD4: ADR scope — write **one** new ADR

**Options considered**:

1. **Zero ADRs**: the existing precedent (`lumen_bridge.rs` + its
   tests) plus the DISCUSS-wave wave-decisions document together
   capture every design choice. No new decision exists.
2. **One ADR**: `adr-0038-cinder-to-pulse-bridge-public-api-and-crate-layout.md`,
   following the established Phase-1+ pattern (ADR-0011 spark,
   ADR-0018 sieve, ADR-0022 codex, ADR-0026 prism, ADR-0033 beacon).
3. **Three ADRs**: separately lock the bridge layout, the test-seam
   convention for cross-crate bridges (the
   InMemoryXxxStore + InMemoryMetricStore pattern), and the
   lowercase-tier-serialisation convention.

**Recommendation**: **Option 2 — one ADR**.

**Rationale**:
- The pattern in Phase 1+ is "one ADR per crate's public API + layout"
  (ADR-0011 spark, ADR-0018 sieve, ADR-0022 codex, ADR-0026 prism,
  ADR-0033 beacon). This bridge is the second public type in the
  `self-observe` crate; pinning its surface as a discrete artefact
  matches the convention. ADR style + length + section order taken
  from ADR-0033 as the most recent exemplar.
- The test-seam convention (Option 3a) is captured *operationally* by
  the precedent in `crates/self-observe/tests/lumen_to_pulse.rs` and
  by the convention adoption in DD1 above. It does not warrant a
  cross-bridge ADR until a third bridge ships and a divergence
  appears. Recording a "standardise the seam across future bridges"
  ADR today would speculatively pin a pattern that has only two
  exemplars (Lumen at v0, Cinder at v0). Defer.
- The lowercase-tier convention (Option 3b) is captured by DISCUSS D4
  + by the shared-artifacts-registry's `tier_value` row + by the
  acceptance-test string-equality asserts in Slices 01 and 02. The
  convention does not extend beyond the one helper inside
  `cinder_bridge.rs`. An ADR for a one-helper convention is
  ceremonial. Defer.

**ADR number**: `0038` (next after the existing `0037-beacon-evaluator-and-scheduler-seam.md`).
**ADR file**: `docs/product/architecture/adr-0038-cinder-to-pulse-bridge-public-api-and-crate-layout.md`.

## Architecture-rule enforcement (Principle 11)

The `self-observe` crate already enforces:
- `#![forbid(unsafe_code)]` (lib.rs:49).
- `cargo deny check` at workspace level (per ADR-0005 Gate 1).
- `cargo public-api` at workspace level (per ADR-0005 Gate 2) catches
  any change to `CinderToPulseRecorder`'s public surface at CI time.
- `cargo semver-checks` at workspace level (per ADR-0005 Gate 3)
  catches breaking changes to the same surface.
- `cargo clippy --workspace --all-targets` (Slice DoR for each slice).
- `cargo mutants` at 100% kill rate per ADR-0005 Gate 5.

No new enforcement tooling is required. The existing five-gate CI
contract covers this feature.

## Earned Trust (Principle 12)

The bridge is an in-process function from `(Tier, TenantId, count)` to
`pulse.ingest(...)`. It depends on the world only through the
runtime-supplied `Arc<dyn MetricStore + Send + Sync>`. The dependency's
contract is the `MetricStore` trait — there is no environment to lie
about (no filesystem, no clock semantics beyond `SystemTime::now()`, no
network, no subprocess, no vendor SDK).

The probe contract is the acceptance-test suite itself. Specifically:

1. **Five Slice-01 tests** demonstrate that `record_place` produces
   exactly one queryable point per call with the correct tier
   attribute and the correct per-tenant partition. This is the
   bridge demonstrating empirically that it can honour the
   `cinder::MetricsRecorder::record_place` contract against a real
   `pulse::InMemoryMetricStore`.
2. **Slice-02 tests** demonstrate the same for `record_migrate`,
   including the `from`/`to` direction attributes and the
   negative-case "failed migrate produces no point" (which probes
   that the bridge is *not* called by Cinder on the failure path —
   the contract is inherited from `crates/cinder/src/store.rs:174-188`).
3. **Slice-03 tests** demonstrate the *dual-emission* contract — the
   highest-information-density probe in the suite — that a single
   `cinder.evaluate_at` call produces both per-item
   `cinder.migrate.count` points *and* per-tenant
   `cinder.evaluate.migrated.count` points. This probes that the
   bridge honours both `record_migrate` and `record_evaluate`
   simultaneously when driven through Cinder's real
   `InMemoryTieringStore::evaluate_at`.
4. **The compile-time `Send + Sync` assertion** is the structural
   probe layer (Principle 12c): if either bound is lost, the test file
   fails to compile, not at runtime.
5. **The `cargo public-api` + `cargo semver-checks` gates** are the
   subtype-check probe layer: a change to the bridge's public surface
   fails CI before merge.

The three Earned-Trust layers (subtype / structural / behavioural)
reduce to two for a pure adapter with no external substrate: the
subtype layer is the public-API check, the behavioural layer is the
acceptance-test suite, and the structural layer is the compile-time
`Send + Sync` assertion. This is the minimum the principle permits for
a no-substrate adapter — the same reduction that ADR-0001's
`otlp-conformance-harness` documented for a pure-function leaf.

**Environments-known-to-lie**: none in scope. The bridge has no
filesystem, no network, no vendor SDK, no subprocess. The only
substrate touched is `SystemTime::now()` (for the
`time_unix_nano` field on each `MetricPoint`); acceptance tests use
`TimeRange::all()` and assert on count + value + attributes, not on
timestamps, so clock-skew lies in the runtime environment do not
affect the test outcome. This is the same posture the Lumen bridge
adopted and shipped.

## Quality gates (DESIGN-wave self-check, per agent definition)

- [x] Requirements traced to components (US-01/02/03 → bridge's three
  `record_*` methods).
- [x] Component boundaries with clear responsibilities (the bridge is
  one struct in one file with one constructor; responsibility is "turn
  Cinder events into Pulse points").
- [x] Technology choices in ADR with alternatives (ADR-0038 enumerates
  three options for each of: test seam, file layout, public-surface
  shape).
- [x] Quality attributes addressed (see "Quality attribute alignment"
  in ADR-0038 and the brief-side append).
- [x] Dependency-inversion compliance (the bridge holds
  `Arc<dyn MetricStore + Send + Sync>` — a port, not a concrete
  adapter).
- [x] C4 diagrams (L1 + L2 in `application-architecture.md` and in
  the brief append; L3 explicitly skipped — see below).
- [x] Integration patterns specified (synchronous in-process method
  calls; one `MetricBatch` per Cinder event; best-effort emission;
  no retry; no async).
- [x] OSS preference validated (no proprietary dependencies; all
  in-workspace).
- [x] AC behavioural, not implementation-coupled (DISCUSS-side AC
  already pass this gate; DESIGN does not re-specify AC).
- [x] External integrations annotated with contract test
  recommendation — **N/A**: no external integrations at runtime.
  Documented in the brief append's "External integrations" section.
- [x] Architectural enforcement tooling recommended — covered by the
  existing five-gate CI contract (ADR-0005).
- [ ] Peer review completed and approved — TO BE DONE before DEVOPS
  handoff (see Handoff section).

## C4 L3 (Component View) decision

**Explicitly skipped.** The bridge is one Rust source file (~90 LOC by
shape-equivalence with `lumen_bridge.rs:1-87`) implementing three
trait methods plus one struct, one constructor, and two private
helpers. Per the SA agent principle ("Component (L3) only for complex
subsystems"), a four-box component diagram for a one-file module is
ceremonial. L1 (System Context) + L2 (Container) are produced in
`application-architecture.md` and reproduced in the brief append.

## Handoff

Next agent: `nw-platform-architect` (DEVOPS wave).

Deliverables ready for handoff:

| Artefact | Path |
|----------|------|
| Feature-side DESIGN-wave decision log | `docs/feature/cinder-to-pulse-bridge-v0/design/wave-decisions.md` (this file) |
| Feature-side application-architecture document | `docs/feature/cinder-to-pulse-bridge-v0/design/application-architecture.md` |
| Brief append | `docs/product/architecture/brief.md > ## Application Architecture — cinder-to-pulse-bridge-v0` |
| Public-API + crate-layout ADR | `docs/product/architecture/adr-0038-cinder-to-pulse-bridge-public-api-and-crate-layout.md` |
| Reuse Analysis | "Reuse Analysis" section of this file |
| C4 diagrams | L1 + L2 in `application-architecture.md` and in the brief append (Mermaid) |

What DEVOPS receives (annotation for platform-architect):

- **Development paradigm**: Rust idiomatic per `CLAUDE.md` (data +
  free functions + traits where polymorphism is genuinely needed). No
  class hierarchies; no `dyn Trait` indirection where direct generic
  monomorphisation suffices, except at the runtime-supplied store
  boundary (`Arc<dyn MetricStore + Send + Sync>`) where the trait
  object is the right shape (per `LumenToPulseRecorder` precedent).
- **External integrations**: none. No contract-test recommendation
  applies.
- **CI gates**: inherits the existing five-gate workspace contract
  (ADR-0005) — `cargo test`, `cargo deny check`, `cargo public-api`,
  `cargo semver-checks`, `cargo mutants` at 100% kill rate. No new
  gate needed.
- **Workspace changes**: one new in-workspace dependency declaration
  (`cinder = { path = "../cinder", version = "0.1.0" }` in
  `crates/self-observe/Cargo.toml`) and one new `[[test]]` block
  (`name = "cinder_to_pulse"`). No workspace-root `Cargo.toml` edit;
  the `cinder` crate is already a workspace member.
- **Mutation-testing scope**: per `CLAUDE.md`'s per-feature MT
  strategy, the wave runs `cargo mutants` scoped to
  `crates/self-observe/src/cinder_bridge.rs` after the DELIVER wave
  refactor pass. 100% kill-rate gate per ADR-0005 Gate 5.

Peer review required before DEVOPS handoff: `solution-architect-reviewer`
(max 2 iterations; address all critical/high issues).
