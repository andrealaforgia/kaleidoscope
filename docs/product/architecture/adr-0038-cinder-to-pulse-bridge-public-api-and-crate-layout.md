# ADR-0038 — `CinderToPulseRecorder` public API and crate layout

- **Status**: Accepted
- **Date**: 2026-05-18
- **Author**: `@nw-solution-architect` (Morgan)
- **Feature**: `cinder-to-pulse-bridge-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0011 (spark), ADR-0018 (sieve), ADR-0022 (codex),
  ADR-0026 (prism), ADR-0033 (beacon) — the chain of crate-public-API
  ADRs whose convention this ADR continues. ADR-0005 — the CI contract
  whose five gates this addition inherits without change.

## Context

`cinder-to-pulse-bridge-v0` introduces the second public type into the
`self-observe` crate: `CinderToPulseRecorder`, a struct that implements
`cinder::MetricsRecorder` and writes each Cinder tier event as a
single-point Pulse `MetricBatch`. The DISCUSS-wave wave-decisions
document (`docs/feature/cinder-to-pulse-bridge-v0/discuss/wave-decisions.md`,
D1-D7) locks the **contract**:

- Three separate metric names: `cinder.place.count`,
  `cinder.migrate.count`, `cinder.evaluate.migrated.count` (D1).
- `record_evaluate` value = `migrated_count`, not `value = 1` (D2).
- Dual emission (per-item migrate + per-tenant evaluate from a single
  `evaluate_at` call) preserved unchanged from Cinder's own behaviour
  (D3).
- Tier topology as point attribute, lowercased to
  `"hot"`/`"warm"`/`"cold"` (D4).
- Best-effort emission: `let _ = pulse.ingest(...)` (D5).
- No CLI surface in v0 (D6).
- SSOT artefacts unmodified (D7).

DISCUSS does **not** lock:

1. The acceptance-test seam shape (DD1 below).
2. The module file location within `self-observe/src/` (DD2 below).
3. The internal helper shape that supports the three trait methods
   (DD3 below).
4. Whether this design warrants an ADR (DD4 below — this ADR is the
   answer).

The precedent for the bridge's shape is
`crates/self-observe/src/lumen_bridge.rs:1-87` (the
`LumenToPulseRecorder` already shipped at v0 of the `self-observe`
crate). The `lib.rs` doc comment at lines 44-47 anticipates "Cinder,
Sluice, Augur, Ray, Strata bridges follow `XxxToPulseRecorder` /
`XxxToOtlpJsonWriter` naming". This ADR locks the Cinder bridge as the
second instance of that family.

`crates/cinder/src/metrics.rs:25-29` defines the trait the bridge
implements:

```rust
pub trait MetricsRecorder: Send + Sync {
    fn record_place(&self, tenant: &TenantId, tier: Tier);
    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier);
    fn record_evaluate(&self, tenant: &TenantId, migrated: usize);
}
```

`crates/pulse/src/store.rs:57-84` defines the sink trait. The bridge
holds an `Arc<dyn MetricStore + Send + Sync>` and calls `ingest` on it.

## Decision

### 1. Public surface (final, locked)

One new public item in the `self-observe` crate, re-exported through
`crates/self-observe/src/lib.rs`:

```rust
// from crates/self-observe/src/lib.rs:
pub use cinder_bridge::CinderToPulseRecorder;
```

Where the type's contract is:

```rust
// from crates/self-observe/src/cinder_bridge.rs:

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

**Locked**:

- The struct name `CinderToPulseRecorder`.
- The single field name `pulse` and its exact type
  `Arc<dyn MetricStore + Send + Sync>`.
- The constructor name `new`, taking the same `Arc` shape.
- The three trait-method dispatches.

**Rationale**: byte-equivalence with `LumenToPulseRecorder`
(`crates/self-observe/src/lumen_bridge.rs:42-50`). The
`shared-artifacts-registry.md > pulse_store` MEDIUM-risk invariant
requires the operator (or, at v0, the integration tests) to wire one
`Arc<dyn MetricStore + Send + Sync>` to both the bridge and the query
path. The constructor signature *is* the contract the post-v0 CLI
wiring will be written against; locking it byte-equivalently to the
Lumen bridge ensures the operator's mental model is one "wire an
`Arc<MetricStore>` to an `XxxToPulseRecorder::new(...)`" idiom,
shared across every future bridge.

### 2. Per-event emission contract (locked)

Each Cinder event becomes exactly one Pulse `MetricBatch` containing
exactly one `Metric` containing exactly one `MetricPoint`. The
contract per event:

| Cinder method | Metric name | Metric kind | Metric unit | Point value | Point attributes |
|---------------|-------------|-------------|-------------|-------------|------------------|
| `record_place(tenant, tier)` | `cinder.place.count` | `Sum` | `"1"` | `1.0` | `{"tier": lowercase(tier)}` |
| `record_migrate(tenant, from, to)` | `cinder.migrate.count` | `Sum` | `"1"` | `1.0` | `{"from": lowercase(from), "to": lowercase(to)}` |
| `record_evaluate(tenant, migrated)` | `cinder.evaluate.migrated.count` | `Sum` | `"1"` | `migrated as f64` | `{}` (empty) |

Where `lowercase(Tier::Hot) = "hot"`, `lowercase(Tier::Warm) = "warm"`,
`lowercase(Tier::Cold) = "cold"`. All point attribute values are
ASCII-lowercased strings.

The `MetricPoint.time_unix_nano` is `SystemTime::now()`-derived nanos
since Unix epoch (mirror of `lumen_bridge.rs:52-56`). The
`MetricPoint.start_time_unix_nano` is `0`. The
`MetricPoint.resource_attributes` is empty (tier topology is per-event,
not per-process — DISCUSS D4).

Emission is best-effort: `let _ = self.pulse.ingest(tenant, batch)`
(D5). The `pulse::MetricStoreError` is an empty enum at v0
(`crates/pulse/src/store.rs:35`); no error path actually exists. The
explicit discard is forward-compatible for v1+.

### 3. Acceptance-test seam (locked)

Acceptance tests under
`crates/self-observe/tests/cinder_to_pulse.rs` drive the bridge
through `cinder::InMemoryTieringStore`:

```rust
// sketch (the crafter writes the production tests during DELIVER):
let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
let bridge = CinderToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
let cinder = InMemoryTieringStore::new(Box::new(bridge));
cinder.place(&tenant("acme"), &item("trade-001"), Tier::Hot, SystemTime::now());
let points = pulse.query(&tenant("acme"), &MetricName::new("cinder.place.count"), TimeRange::all()).unwrap();
assert_eq!(points.len(), 1);
```

**Locked**: tests use `cinder::InMemoryTieringStore` as the driver and
`pulse::InMemoryMetricStore` as the assertion target. Direct
invocation of `bridge.record_place(...)` / `record_migrate(...)` /
`record_evaluate(...)` is **not** used because it cannot express the
dual-emission contract that one `cinder.evaluate_at` call produces
both per-item `record_migrate` invocations *and* per-tenant
`record_evaluate` invocations (`crates/cinder/src/store.rs:200-232`).
The dual-emission contract is the highest-information-density
assertion in the suite (Slice 03); the seam choice exists to support
it.

**Compile-time probe** (Slice 01 carries the assertion that covers all
slices):

```rust
#[test]
fn the_bridge_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToPulseRecorder>();
}
```

This mirrors `crates/self-observe/tests/lumen_to_pulse.rs:204-212`.

### 4. Module file location (locked)

The new file is `crates/self-observe/src/cinder_bridge.rs`, sibling to
the existing `lumen_bridge.rs` and `lumen_otlp_json.rs`.
`crates/self-observe/src/lib.rs` gains:

```rust
mod cinder_bridge;
pub use cinder_bridge::CinderToPulseRecorder;
```

inserted after the existing `mod lumen_bridge;` / `mod lumen_otlp_json;`
declarations (lib.rs:51-52). The re-export is appended to the existing
`pub use` block (lib.rs:54-55).

Rationale: matches the established file-flat sibling pattern in the
`self-observe` crate. A future `bridges/` subdirectory refactoring
becomes warranted at ~8-10 bridge files (when Sluice / Augur / Ray /
Strata bridges and their OTLP-JSON variants ship). The refactor is
deferred to that future change.

### 5. Internal module structure (recommended, not locked)

The crafter writes the production source during DELIVER. The
recommended internal shape mirrors the Lumen bridge's, extended by one
`attributes` parameter on the `emit` helper:

```text
crates/self-observe/src/cinder_bridge.rs
├── pub struct CinderToPulseRecorder { pulse: Arc<dyn MetricStore + Send + Sync> }
├── impl CinderToPulseRecorder {
│   ├── pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self
│   └── fn emit(&self, tenant: &TenantId, metric_name: &str, value: f64,
│              attributes: BTreeMap<String, String>)
│ }
├── impl cinder::MetricsRecorder for CinderToPulseRecorder {
│   ├── fn record_place(&self, tenant: &TenantId, tier: Tier)
│   ├── fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier)
│   └── fn record_evaluate(&self, tenant: &TenantId, migrated: usize)
│ }
└── (free or associated fn) tier_attr(tier: Tier) -> &'static str
```

**Recommended**, not locked. The crafter may:

- Rename `emit` to any equivalent internal name (`record` / `push` /
  `ingest_one`).
- Choose between a free function `tier_attr` and an associated
  `Tier::as_attr_str(self)` (the latter would require a trivial
  extension to `crates/cinder/src/tier.rs` and is non-trivial because
  it changes the `cinder` crate's surface — recommended to keep the
  helper local to `cinder_bridge.rs` for v0).
- Inline `emit` into each `record_*` method if the resulting code is
  shorter and more legible. The single-helper recommendation exists
  to centralise the Sum-kind + unit-"1" + best-effort convention; it
  is not a contract.
- Use `String::from("hot")` vs `"hot".to_string()` vs `&'static str`
  for the lowercase strings; all are behaviourally equivalent.

What **is** locked is the per-event emission contract in §2 above —
the metric name strings, the values, the attribute sets, and the
best-effort `let _ = pulse.ingest(...)` posture.

### 6. Cargo manifest additions (locked)

`crates/self-observe/Cargo.toml` gains:

```toml
[dependencies]
# existing dependencies preserved
cinder = { path = "../cinder", version = "0.1.0" }

[[test]]
name = "cinder_to_pulse"
path = "tests/cinder_to_pulse.rs"
```

The `cinder` crate is already a workspace member; no workspace-root
`Cargo.toml` edit is needed. No new external (non-workspace)
dependencies are introduced.

## Considered Alternatives

### Alternative 1 — One generic bridge over both Cinder's and Lumen's recorder traits

Pros: zero code duplication; one `XxxToPulseRecorder<R: SomeTrait>`
generic across both crates.

Cons: impossible without changing the upstream trait shapes. Cinder's
`MetricsRecorder` and Lumen's `MetricsRecorder` are distinct traits in
distinct crates with distinct method signatures (Cinder has three
methods; Lumen has two). Rust cannot unify them under one generic
constraint without an intermediate `pub trait CommonRecorder` either
in `self-observe` or in a new shared crate, which inverts the
dependency arrow (upstream crates would have to depend on the
shared-trait crate to implement it).

**Rejected**.

### Alternative 2 — Drive the bridge directly in acceptance tests, bypassing Cinder

Pros: smallest test surface; pure bridge-only assertions; no Cinder
behaviour entangled with bridge assertion.

Cons: cannot express the dual-emission contract (DISCUSS D3). Direct
`bridge.record_evaluate(&acme, 5)` does not cascade into five
`bridge.record_migrate(...)` calls — that cascade lives inside
Cinder's `InMemoryTieringStore::evaluate_at`. Tests under Alternative
2 would have to issue six direct bridge calls per dual-emission
scenario, simulating the cascade by hand. The simulation is brittle
(any change to Cinder's cascade order breaks the test in a way that
misrepresents the bridge's contract).

**Rejected** for the dual-emission test (Slice 03). The shape would
have been acceptable for Slices 01 and 02, but using a different seam
across slices in one test file is inconsistent.

### Alternative 3 — Use `cinder::CapturingRecorder` as an additional assertion target

Pros: three layers of independent assertion (Cinder intends → bridge
converts → Pulse stores).

Cons: Cinder's `CapturingRecorder` exists precisely to give Cinder's
own crate tests a way to assert what Cinder intends to emit
(`crates/cinder/src/metrics.rs:57-110`). Cinder ships its own
in-tree tests against `CapturingRecorder`. Adding `CapturingRecorder`
assertions to the bridge's tests would duplicate Cinder's coverage
without adding bridge-specific evidence. The bridge's contract is
"Cinder emission becomes Pulse points" — the assertion must terminate
at Pulse.

**Rejected**.

### Alternative 4 — `bridges/` subdirectory under `self-observe/src/`

Pros: anticipates the eventual file-count growth (Sluice + Augur +
Ray + Strata bridges and their OTLP-JSON variants will land
post-v0).

Cons: over-organisation at N=3 files. Forces a retrospective move of
`lumen_bridge.rs` and `lumen_otlp_json.rs` (or accepts an
inconsistency where Lumen bridges live at crate root and Cinder lives
under `bridges/`). Either path is worse than the file-flat status
quo.

**Rejected** for v0. A future "refactor: group self-observe bridges
under bridges/" change ships when the file count reaches ~8.

### Alternative 5 — Three ADRs (layout + cross-bridge test-seam convention + lowercase-tier convention)

Pros: maximum traceability; each cross-bridge convention earns its
own audit-trail artefact.

Cons: two of the three ADRs would speculatively pin patterns with
only two exemplars (Lumen at v0, Cinder at v0). Premature
formalisation. Both conventions are captured informally — the test
seam in this ADR's §3 and operationally in the test file's structure;
the lowercase-tier convention in DISCUSS D4 + the
`shared-artifacts-registry.md > tier_value` row + the
acceptance-test string-equality asserts in Slices 01 and 02.

**Rejected** for v0. Both deferred ADRs become warranted when a third
bridge (Sluice / Augur / Ray / Strata) is added and the conventions
hold across three exemplars.

### Alternative 6 — Zero ADRs

Pros: less paperwork.

Cons: inconsistent with the Phase-1+ convention that every crate's
public-API + layout decision earns an ADR (ADR-0011 spark, ADR-0018
sieve, ADR-0022 codex, ADR-0026 prism, ADR-0033 beacon). The
`self-observe` crate currently has no ADR because it shipped before
the convention crystallised; recording one ADR now establishes the
convention for the crate's second public type and pays the cost
forward for future bridge additions.

**Rejected**.

## Consequences

**Positive**:

- Public surface byte-equivalent (modulo trait identity) to
  `LumenToPulseRecorder`. The operator's mental model is one idiom
  shared across every bridge in `self-observe`: wire an `Arc<dyn
  MetricStore>` to `XxxToPulseRecorder::new(...)`.
- Public surface locked by `cargo public-api -p self-observe` (CI
  Gate 2, ADR-0005) and `cargo semver-checks` (Gate 3). Breaking
  changes to the constructor signature require a major-version bump
  on the `self-observe` crate.
- The acceptance-test seam (Cinder drives, Pulse asserts) is
  consistent across all slices in this feature and across both
  bridge tests in the `self-observe` crate.
- The dual-emission contract from DISCUSS D3 is naturally expressible
  in one Slice 03 test (one `cinder.evaluate_at` call produces both
  metric streams) without bridge-side simulation of the cascade.
- No new external dependencies. No new workspace members. No new CI
  gates. Inherits the existing five-gate workspace contract from
  ADR-0005.

**Negative**:

- The `emit` helper diverges from `lumen_bridge.rs`'s `emit` by one
  parameter (`attributes: BTreeMap<String, String>`). The divergence
  is required by the domain (Cinder events carry attributes, Lumen
  events do not). Future bridges that emit attributed events will
  copy Cinder's `emit` shape; future bridges emitting unattributed
  events will copy Lumen's. A unifying helper is not warranted at
  N=2 exemplars.
- A regression in Cinder's `InMemoryTieringStore::evaluate_at`
  cascade can break Slice 03's dual-emission test even if the bridge
  itself is correct. This is the desired behaviour per DISCUSS D3:
  Cinder's cascade *is* the contract the bridge inherits. The
  regression's actual location (Cinder, not the bridge) is
  diagnosable from Cinder's own in-tree tests, which would fail
  first.
- The recommended internal shape (§5) is non-binding; the crafter may
  diverge in helper naming, inlining, or string-allocation strategy.
  The non-binding shape is the right level of specificity — the
  contract (§2) is enough to pin observable behaviour; over-pinning
  internal helpers is back-seat driving.

**Trade-offs**:

- File-flat layout (DD2) vs `bridges/` subdirectory: optimised for
  *current* readability at N=3 sibling bridge files. The eventual
  growth to N=8-10 pays a refactoring cost in a future commit; this
  feature does not pre-pay.
- Test seam choice (DD1) entangles bridge tests with Cinder
  behaviour: chosen because the dual-emission contract requires it
  and consistency across slices is more valuable than test isolation
  from a stable upstream behaviour. The Lumen bridge tests made the
  same trade-off and have not regretted it.
- ADR scope (DD4) records only the public-surface ADR, deferring the
  cross-bridge test-seam ADR and the lowercase-tier ADR. Defers
  formalisation cost to the point where the conventions have three
  exemplars (Lumen + Cinder + first-of-Sluice/Augur/Ray/Strata).

## Quality attribute alignment

- **Functional Suitability**: the per-event contract in §2 is
  exhaustive (three Cinder methods × three locked attribute schemas);
  every BDD scenario in
  `discuss/journey-observe-cinder-tier-transitions.feature` resolves
  to a single per-event contract check.
- **Maintainability — Testability**: the acceptance-test seam in §3
  is one canonical shape across all three slices; mutation-testing
  scope is one file (`crates/self-observe/src/cinder_bridge.rs`) at
  100% kill rate per ADR-0005 Gate 5.
- **Maintainability — Modifiability**: future additive bridge methods
  (if Cinder grows a fourth event type) require one new
  `record_xxx` impl and one new per-event-contract row in §2;
  inheriting the `emit` helper is mechanical.
- **Compatibility — Interoperability**: the bridge consumes and
  produces upstream trait types unchanged; no shadowing, no
  wrapping. Adapter swap is mechanical for both ports.
- **Reliability — Maturity**: best-effort emission posture (D5)
  means a future non-empty `MetricStoreError` does not propagate to
  Cinder (whose trait methods return `()`). The bridge cannot crash
  Cinder.
- **Security — Integrity**: `tenant_id` is forwarded unchanged from
  Cinder's call to Pulse's ingest; two-tenant isolation is asserted
  in all three slices' tests, defending the
  `shared-artifacts-registry.md > tenant_id` HIGH-risk invariant.
- **Portability**: pure Rust, no `unsafe`, no platform-specific code.
  Inherits the crate's `#![forbid(unsafe_code)]` posture.

## Earned Trust (Principle 12) — adapter posture

The bridge has no external substrate (no filesystem, no network, no
vendor SDK, no subprocess). It depends on the world only through the
runtime-supplied `Arc<dyn MetricStore + Send + Sync>` (whose contract
is the `MetricStore` trait, tested in-tree by the `pulse` crate) and
through `SystemTime::now()` (whose nanos-since-epoch value is not
asserted by acceptance tests — they use `TimeRange::all()`).

The three Earned-Trust layers reduce to two for a no-substrate
adapter:

1. **Subtype-check layer**: `cargo public-api -p self-observe` (Gate
   2) catches any change to `CinderToPulseRecorder`'s public surface
   at CI time. The compile-time `assert_send_sync::<CinderToPulseRecorder>()`
   test catches any loss of the `Send + Sync` trait bound at compile
   time (not at runtime).
2. **Behavioural-check layer**: the acceptance-test suite under
   `crates/self-observe/tests/cinder_to_pulse.rs` exercises the
   per-event contract in §2 against a real
   `pulse::InMemoryMetricStore`. The dual-emission test in Slice 03
   exercises the cross-method contract end-to-end.

The structural-check layer is degenerate — there is no on-disk
source-of-truth schema to enforce drift against beyond the public
surface, which the subtype layer already covers. This is the minimum
the principle permits for a no-substrate adapter; same posture
as ADR-0001's `otlp-conformance-harness`.
