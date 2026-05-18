# ADR-0039 — `CinderToOtlpJsonWriter` public API and crate layout

- **Status**: Accepted
- **Date**: 2026-05-18
- **Author**: `@nw-solution-architect` (Morgan)
- **Feature**: `cinder-to-otlp-json-bridge-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0011 (spark), ADR-0018 (sieve), ADR-0022 (codex),
  ADR-0026 (prism), ADR-0033 (beacon), ADR-0038 (cinder-to-pulse) —
  the chain of crate-public-API ADRs whose convention this ADR
  continues. ADR-0005 — the CI contract whose five gates this
  addition inherits without change. ADR-0001 — the no-substrate-
  adapter Earned-Trust posture this writer also inherits.

## Context

`cinder-to-otlp-json-bridge-v0` introduces the second NDJSON writer
into the `self-observe` crate: `CinderToOtlpJsonWriter<W: Write +
Send + Sync>`, a struct that implements `cinder::MetricsRecorder` and
writes each Cinder tier event as a single OTLP-JSON `ResourceMetrics`
NDJSON line to a generic sink. It is the cross-process counterpart to
the `CinderToPulseRecorder` shipped by ADR-0038, and the Cinder
counterpart to the `LumenToOtlpJsonWriter` shipped at v0 of the
`self-observe` crate.

The DISCUSS-wave wave-decisions document
(`docs/feature/cinder-to-otlp-json-bridge-v0/discuss/wave-decisions.md`,
D1-D10) locks the **behaviour contract**:

- Three metric names re-used identically from ADR-0038 §2:
  `cinder.place.count`, `cinder.migrate.count`,
  `cinder.evaluate.migrated.count` (D1).
- Scope name `kaleidoscope.cinder` (D2).
- Tier serialisation as lowercase ASCII string `"hot"` / `"warm"` /
  `"cold"` (D3).
- `record_evaluate` value = `migrated.to_string()`, NOT `"1"` (D4).
- Best-effort emission, `let _ = ...`, no panic (D5).
- Mutex-guarded NDJSON-validity atomicity: `Mutex<W>` +
  `write_all(body) + write_all(b"\n") + flush` inside the critical
  section (D6).
- OTLP-JSON serde-struct duplication from `lumen_otlp_json.rs` at v0
  (rule-of-three deferral) (D7).
- One OTLP-JSON line per `MetricsRecorder` call; no batching, no
  compound metrics (D8).
- CLI wiring (the `--observe-otlp <path>` plumbing) explicitly OUT
  of scope (D9).
- SSOT journey and `jobs.yaml` not modified in this wave (D10).

DISCUSS does **not** lock:

1. The module file location within `self-observe/src/` (DD1 below).
2. The shape of the per-point attribute slot when the Cinder
   per-event attribute cardinality (1, 2, 3) diverges from the Lumen
   writer's uniform `[OtlpAttr; 1]` (DD2 below).
3. The acceptance-test seam shape (DD3 below).
4. The stub posture for un-implemented methods during the slice
   walking (DD4 below).
5. Whether this design warrants an ADR (DD5 below — this ADR is the
   answer).

The precedent for the writer's shape is
`crates/self-observe/src/lumen_otlp_json.rs:1-200` (the
`LumenToOtlpJsonWriter` already shipped at v0 of the `self-observe`
crate, used in production via the `kaleidoscope-cli --observe-otlp
<path>` flag, commits `c6b336c` and `3af7e82`). The `lib.rs` doc
comment at lines 44-47 anticipates the `CinderToOtlpJsonWriter`
addition as the fourth quadrant of the `{Source} × {sink}` writer
matrix (Lumen × Pulse, Lumen × OTLP-JSON, Cinder × Pulse,
Cinder × OTLP-JSON). This ADR locks the fourth quadrant.

`crates/cinder/src/metrics.rs:25-29` defines the trait the writer
implements:

```rust
pub trait MetricsRecorder: Send + Sync {
    fn record_place(&self, tenant: &TenantId, tier: Tier);
    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier);
    fn record_evaluate(&self, tenant: &TenantId, migrated: usize);
}
```

The writer holds a `Mutex<W>` over the runtime-supplied sink and
calls `write_all` on the mutex guard. There is no `Arc<dyn ...>`
indirection — the sink is generic over `W: Write + Send + Sync`,
mirroring the Lumen writer exactly.

## Decision

### 1. Public surface (final, locked)

One new public item in the `self-observe` crate, re-exported through
`crates/self-observe/src/lib.rs`:

```rust
// from crates/self-observe/src/lib.rs:
pub use cinder_otlp_json::CinderToOtlpJsonWriter;
```

Where the type's contract is:

```rust
// from crates/self-observe/src/cinder_otlp_json.rs:

pub struct CinderToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> CinderToOtlpJsonWriter<W> {
    pub fn new(inner: W) -> Self;
}

impl<W: Write + Send + Sync> cinder::MetricsRecorder for CinderToOtlpJsonWriter<W> {
    fn record_place(&self, tenant: &TenantId, tier: Tier);
    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier);
    fn record_evaluate(&self, tenant: &TenantId, migrated: usize);
}
```

**Locked**:

- The struct name `CinderToOtlpJsonWriter`.
- The generic parameter `W: Write + Send + Sync` and its exact
  bounds.
- The two field names `inner` and `scope_name` and their exact
  types `Mutex<W>` and `String` respectively. **Field name `inner`
  mirrors `lumen_otlp_json.rs:129` exactly; `scope_name` is a struct
  field (not a `const`) per Lumen precedent (`lumen_otlp_json.rs:130`).**
- The constructor name `new`, taking `W` by value (matches
  `lumen_otlp_json.rs:135`).
- The three trait-method dispatches against `cinder::MetricsRecorder`.

**Rationale**: byte-equivalence with `LumenToOtlpJsonWriter`
(`crates/self-observe/src/lumen_otlp_json.rs:128-140`) for every part
of the public surface that can be byte-equivalent. The differences
are forced by the upstream trait (`cinder::MetricsRecorder` has three
methods; `lumen::MetricsRecorder` has two) and by the per-event
attribute cardinality difference handled inside the private payload
shape (see §2). The operator's mental model is the same idiom shared
across every OTLP-JSON writer in `self-observe`: construct one
`XxxToOtlpJsonWriter::new(W)` wrapping the sink and pass it as
`Cinder`'s recorder.

### 2. Per-event emission contract (locked)

Each Cinder event becomes exactly one NDJSON line. The line is the
JSON encoding of one `OtlpResourceMetrics` value containing exactly
one `OtlpScopeMetrics`, containing exactly one `OtlpMetric`,
containing exactly one `OtlpSum` with exactly one
`OtlpNumberPoint`. The contract per event:

| Cinder method | Metric name | Metric kind | `asInt` value | Point attributes |
|---------------|-------------|-------------|---------------|------------------|
| `record_place(tenant, tier)` | `cinder.place.count` | `Sum` (cumulative, monotonic) | `"1"` | `[{tenant_id: tenant.0}, {tier: lowercase(tier)}]` |
| `record_migrate(tenant, from, to)` | `cinder.migrate.count` | `Sum` (cumulative, monotonic) | `"1"` | `[{tenant_id: tenant.0}, {from: lowercase(from)}, {to: lowercase(to)}]` |
| `record_evaluate(tenant, migrated)` | `cinder.evaluate.migrated.count` | `Sum` (cumulative, monotonic) | `migrated.to_string()` | `[{tenant_id: tenant.0}]` |

Where `lowercase(Tier::Hot) = "hot"`, `lowercase(Tier::Warm) = "warm"`,
`lowercase(Tier::Cold) = "cold"`. All point attribute values are
ASCII-lowercased strings. All `asInt` values are JSON strings (per
the OTLP-JSON encoding rule for `uint64`).

**Resource-attribute slot**: `[{tenant_id: tenant.0}]` — exactly one
attribute, mirroring `lumen_otlp_json.rs:71`. Cinder events are
per-tenant; the same `tenant_id` appears in BOTH the resource
attributes AND the point attributes (mirroring the Lumen writer's
"emit-both" interop posture documented at `lumen_otlp_json.rs:37-40`).

**Scope**: `OtlpScope { name: "kaleidoscope.cinder" }`. The
`scope_name` is stored as a `String` field on the writer (mirror of
`lumen_otlp_json.rs:130, 138`), populated by `new` to
`"kaleidoscope.cinder".to_string()`.

**Atomicity** (DISCUSS D6): the writer holds a `Mutex<W>`. Each
emission acquires the guard exactly once and performs the triple
`write_all(line.as_bytes()) + write_all(b"\n") + flush()` inside the
critical section. This is byte-equivalent to
`lumen_otlp_json.rs:182-189`. The triple is what defends the
NDJSON-validity invariant against concurrent emissions interleaving.

**Best-effort posture** (DISCUSS D5): serialisation failure is
silently swallowed (`if let Ok(line) = serde_json::to_string(...)`).
Write failure is silently swallowed (`let _ = writer.write_all(...)`).
`Mutex<W>` poisoning is silently swallowed (`if let Ok(mut writer) =
self.inner.lock()`). Same posture as `lumen_otlp_json.rs:182-189`.

### 3. Acceptance-test seam (locked)

Acceptance tests under
`crates/self-observe/tests/cinder_to_otlp_json.rs` drive the writer
through `cinder::InMemoryTieringStore` and capture the emitted bytes
through a `SharedBuf(Arc<Mutex<Vec<u8>>>)` sink:

```rust
// sketch (the crafter writes the production tests during DELIVER):
let buf = SharedBuf::new();
let writer = CinderToOtlpJsonWriter::new(buf.clone());
let cinder = InMemoryTieringStore::new(Box::new(writer));
cinder.place(&tenant("acme"), &item("trade-001"), Tier::Hot, SystemTime::now());

let lines: Vec<serde_json::Value> = collect_lines(&buf);
assert_eq!(lines.len(), 1);
assert_eq!(lines[0]["scopeMetrics"][0]["scope"]["name"], "kaleidoscope.cinder");
assert_eq!(lines[0]["scopeMetrics"][0]["metrics"][0]["name"], "cinder.place.count");
// ... etc.
```

**Locked**:

- Tests drive `cinder::InMemoryTieringStore`; the writer is the
  unit under test; `SharedBuf(Arc<Mutex<Vec<u8>>>)` is the capture
  substrate.
- `SharedBuf` is defined locally in `tests/cinder_to_otlp_json.rs`
  mirroring `tests/lumen_to_otlp_json.rs:54-64` (rule-of-three:
  extraction into a `tests/common.rs` becomes warranted when a
  third OTLP-JSON-writer test file lands).
- Captured bytes are parsed line-by-line as `serde_json::Value`;
  assertions are against the parsed JSON tree (robust to whitespace
  and field-ordering variations).
- Direct invocation of `writer.record_place(...)` is **not** used as
  the primary seam, because it cannot express the dual-emission
  contract from DISCUSS D8 (one `evaluate_at` produces N
  `record_migrate` calls AND 1 `record_evaluate` call inside
  Cinder's `InMemoryTieringStore::evaluate_at`, lines 200-232 of
  `crates/cinder/src/store.rs`).

**Compile-time probe** (Slice 01 carries the assertion that covers
all slices):

```rust
#[test]
fn the_writer_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>();
}
```

This mirrors `tests/lumen_to_pulse.rs:204-212` and the equivalent
probe in `tests/cinder_to_pulse.rs` (sibling ADR-0038 §3). It is the
subtype-check layer of the Earned Trust contract (Principle 12c).

### 4. Module file location (locked)

The new file is `crates/self-observe/src/cinder_otlp_json.rs`,
sibling to the existing `lumen_bridge.rs`, `lumen_otlp_json.rs`, and
`cinder_bridge.rs`. `crates/self-observe/src/lib.rs` gains:

```rust
mod cinder_otlp_json;
pub use cinder_otlp_json::CinderToOtlpJsonWriter;
```

inserted after the existing `mod cinder_bridge;` declaration and
appended to the existing `pub use` block.

**Rationale**: matches the established file-flat sibling pattern in
the `self-observe` crate. After this feature ships, the crate root
holds N=4 sibling writer files (`lumen_bridge.rs`,
`lumen_otlp_json.rs`, `cinder_bridge.rs`, `cinder_otlp_json.rs`),
which is comfortably below the ~8-10 file threshold at which a
`bridges/` subdirectory refactoring becomes warranted (when Sluice,
Augur, Ray, Strata bridges and their OTLP-JSON variants ship). The
refactor is deferred to that future change. Identical posture to
ADR-0038 §4.

### 5. Internal module structure (recommended, not locked)

The crafter writes the production source during DELIVER. The
recommended internal shape mirrors `lumen_otlp_json.rs` exactly,
modulo the two structural differences forced by Cinder's per-event
attribute cardinality:

```text
crates/self-observe/src/cinder_otlp_json.rs
├── (duplicated serde structs — DISCUSS D7)
│   ├── OtlpResourceMetrics<'a>      (same shape as lumen_otlp_json.rs:62-67)
│   ├── OtlpResource<'a>             (same shape; attributes: [OtlpAttr<'a>; 1])
│   ├── OtlpScopeMetrics<'a>         (same shape as lumen_otlp_json.rs:74-78)
│   ├── OtlpScope<'a>                (same shape)
│   ├── OtlpMetric<'a>               (same shape)
│   ├── OtlpSum<'a>                  (same shape; data_points: [OtlpNumberPoint<'a>; 1])
│   ├── OtlpNumberPoint<'a>          (DIVERGES from Lumen: attributes: Vec<OtlpAttr<'a>>, not [OtlpAttr<'a>; 1])
│   ├── OtlpAttr<'a>                 (same shape)
│   └── OtlpAttrValue<'a>            (same shape)
├── pub struct CinderToOtlpJsonWriter<W: Write + Send + Sync> {
│       inner: Mutex<W>,
│       scope_name: String,
│   }
├── impl<W: Write + Send + Sync> CinderToOtlpJsonWriter<W> {
│   ├── pub fn new(inner: W) -> Self
│   └── fn emit(&self, tenant: &TenantId, metric_name: &str, value: &str,
│              point_attrs: Vec<OtlpAttr<'_>>)
│ }
├── impl<W: Write + Send + Sync> cinder::MetricsRecorder for CinderToOtlpJsonWriter<W> {
│   ├── fn record_place(&self, tenant: &TenantId, tier: Tier)
│   ├── fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier)
│   └── fn record_evaluate(&self, tenant: &TenantId, migrated: usize)
│ }
└── (free fn) tier_lowercase(tier: Tier) -> &'static str
```

**Two structural divergences from `lumen_otlp_json.rs`** (and ONLY
these two):

1. **`OtlpNumberPoint.attributes` type**: `Vec<OtlpAttr<'a>>`
   instead of `[OtlpAttr<'a>; 1]`. This is the DD2 decision: Cinder's
   per-event point-attribute cardinality is non-uniform (place: 2,
   migrate: 3, evaluate: 1), so the uniform Lumen array shape cannot
   be reused. The DISCUSS D7 deferral applies to the OTHER eight
   serde structs (which ARE byte-identical) but NOT to
   `OtlpNumberPoint`, whose attribute slot type is the genuine
   structural difference between the two writers.
2. **`emit` helper signature**: gains two parameters relative to
   Lumen's (`value: &str` accepts the already-stringified count,
   because evaluate's value is `migrated.to_string()` not
   `1.to_string()`; `point_attrs: Vec<OtlpAttr<'_>>` accepts the
   per-method-shaped attributes vector). The Lumen `emit` took
   `value: u64` and synthesised the one `tenant_id` attribute
   internally; the Cinder `emit` is parameterised over both.

**Recommended**, not locked. The crafter may:

- Rename `emit` to any equivalent internal name (`record` / `push` /
  `ingest_one`).
- Inline `emit` into each `record_*` method if the resulting code is
  shorter and more legible. The single-helper recommendation exists
  to centralise the `Mutex<W>` acquisition + best-effort triple
  pattern; it is not a contract.
- Choose between a free function `tier_lowercase` and an associated
  `Tier::as_lowercase_str(self)` (the latter would require a trivial
  extension to `crates/cinder/src/tier.rs` and is non-trivial
  because it changes the `cinder` crate's surface — recommended to
  keep the helper local to `cinder_otlp_json.rs` for v0).
- Pass `point_attrs` as `&[OtlpAttr<'_>]` (borrowed slice) instead of
  `Vec<OtlpAttr<'_>>` (owned). The owned form is suggested because
  the per-method callers construct fresh vectors; the borrowed form
  is also legitimate.
- Use the unit string `"1"` as a `String` field on the emitted
  `OtlpSum` (Cinder events do not carry a unit; the Lumen writer
  omits the unit field entirely, which is acceptable per the
  OpenTelemetry collector's tolerance for omitted optional fields).

What **is** locked is the per-event emission contract in §2 above —
the metric name strings, the `asInt` values, the attribute sets, the
scope name, and the best-effort `let _ = writer.write_all(...)`
posture.

**Note on serde-struct duplication vs. attribute-array shape (DD1 +
DD2 disambiguation)**: DISCUSS D7 directed that the OTLP-JSON serde
structs be **duplicated from `lumen_otlp_json.rs`** rather than
extracted into a shared module at v0 (rule of three not reached: two
writers is too few exemplars to justify the extraction). DD2 above
adds the constraint that the duplicated structs are NOT byte-
identical — they share STYLE and SHAPE with the Lumen versions, but
`OtlpNumberPoint.attributes` is typed `Vec<OtlpAttr<'a>>` here
versus `[OtlpAttr<'a>; 1]` in Lumen. The DISCUSS D7 duplication is
SHAPE/STYLE duplication, NOT byte-equivalence duplication, and this
ADR locks both that semantics and the single divergence point.

### 6. Cargo manifest additions (locked)

`crates/self-observe/Cargo.toml` gains exactly one new entry:

```toml
[[test]]
name = "cinder_to_otlp_json"
path = "tests/cinder_to_otlp_json.rs"
```

The `cinder = { path = "../cinder", version = "0.1.0" }` dependency
line was already added by the Pulse-sink sibling feature (ADR-0038
§6); no further `[dependencies]` edits are required. The `serde` and
`serde_json` dependencies are already present (used by the Lumen
OTLP-JSON writer at v0). No new external (non-workspace) dependency
is introduced. No workspace-root `Cargo.toml` edit is needed.

## Considered Alternatives

### Alternative 1 — Share the OTLP-JSON serde structs between the Lumen and Cinder writers via a private `otlp_envelope` module

Pros: zero duplication of the eight envelope serde structs; single
source of truth for the OTLP-JSON wire-shape decisions
(`aggregationTemporality = 2`, `isMonotonic = true`, the JSON
encoding of `uint64` as a string, etc.); any future tightening of
the wire shape lands in one file.

Cons: rule of three. Two exemplars (Lumen, Cinder) is insufficient
evidence that the eight structs are the right shared abstraction —
the third exemplar (the next Source × OTLP-JSON writer, likely
Sluice or Augur) is what will tell us whether the abstraction holds
or whether each writer's envelope is subtly different (perhaps in
which fields are optional, which scope name format applies, or
whether some writers need a `Gauge` or `Histogram` variant
alongside `Sum`). Premature extraction at N=2 risks locking in a
shape that the third exemplar will want to break, costing a
refactor at that point AND costing the readability cost of indirect
imports now.

Additionally, the `OtlpNumberPoint.attributes` slot **already
diverges** between Lumen and Cinder (`[T; 1]` vs `Vec<T>` — DD2);
the shared module would have to either generic-parameterise that
slot (adding a generic to nine structs to accommodate one
divergence) or duplicate `OtlpNumberPoint` (giving up part of the
sharing the alternative was supposed to deliver). Neither is
attractive.

**Rejected** for v0. The extraction trigger is the third writer's
landing.

### Alternative 2 — Use per-method fixed-size attribute arrays (`[OtlpAttr; 2]` for place, `[OtlpAttr; 3]` for migrate, `[OtlpAttr; 1]` for evaluate) with the envelope structs generic over the payload type

Pros: zero heap allocation per event (matches the Lumen writer's
allocation profile exactly for the attributes slot); the compiler
enforces the per-method attribute cardinality at type-check time; a
test that constructs a per-method payload missing one attribute
fails to compile.

Cons: three near-duplicated `OtlpNumberPoint*` struct definitions
(one per method); per-method monomorphisation of the envelope
structs (which would have to become generic over the payload type
to accommodate the differing point shape); the per-event-type
rigidity prevents adding a fourth attribute to any event (e.g. a
future `cause: &str` on migrate) without a new struct variant and
new envelope monomorphisation. The performance argument does not
apply at this seam — the writer is on a best-effort observability
path, not a hot path, and the `serde_json::to_string` call that
follows dominates any allocation cost of the attribute slot. The
correctness argument is already covered by the acceptance tests
(Slice 01 asserts `tier`; Slice 02 asserts `from` and `to`); the
compile-time guard is duplicative.

**Rejected**. The chosen shape (DD2 Option 2 — `attributes:
Vec<OtlpAttr<'a>>` on one shared `OtlpNumberPoint` struct) ships
one struct, has comparable runtime cost (one small Vec allocation,
≤3 entries), and accommodates future fourth-attribute extensions
trivially.

### Alternative 3 — Use an `enum OtlpCinderPoint<'a> { Place(...), Migrate(...), Evaluate(...) }` with derived `Serialize`

Pros: type-system encodes the per-method attribute cardinality
without three duplicate struct definitions; no heap allocation; the
operator's only-asserted view is the JSON wire format, which the
enum decodes from cleanly with `#[serde(untagged)]`.

Cons: most code of all three options; the `#[serde(untagged)]`
attribute is the kind of small piece of serde cleverness that is
easy to forget when adding a new variant or to break by reordering
variants (untagged deserialisation tries variants in source order);
the enum's type-system value is local to the writer module — the
operator never sees an enum, only the JSON. The cost of the
cleverness exceeds its benefit at this scale.

**Rejected**. Same reason as Alternative 2 plus the additional
serde-trap surface.

### Alternative 4 — Drive the writer directly in acceptance tests, bypassing `cinder::InMemoryTieringStore`

Pros: smallest test surface; pure writer-only assertions; no
Cinder behaviour entangled with writer assertion; a regression in
Cinder's `evaluate_at` cascade cannot break the writer's tests.

Cons: cannot express the dual-emission contract from DISCUSS D8.
Direct `writer.record_evaluate(&acme, 5)` does not cascade into
five `writer.record_migrate(...)` calls — that cascade lives inside
Cinder's `InMemoryTieringStore::evaluate_at` (`crates/cinder/src/store.rs:200-232`).
Slice 03 would have to issue six direct writer calls per
dual-emission scenario, simulating the cascade by hand. The
simulation is brittle (any change to Cinder's cascade order breaks
the test in a way that misrepresents the writer's contract) and
inconsistent with both the Pulse-sink sibling (ADR-0038 §3) and the
already-shipped Lumen OTLP-JSON writer (`tests/lumen_to_otlp_json.rs`).

**Rejected** for the dual-emission test (Slice 03). Direct
invocation would have been acceptable for Slices 01 and 02
considered in isolation, but using a different seam across slices
in one test file is inconsistent and the consistency value
dominates.

### Alternative 5 — Use a real `tempfile::NamedTempFile` as the test sink instead of `SharedBuf`

Pros: exercises real `File::write_all` semantics including
`O_APPEND` (closer to the post-v0 CLI integration which writes to
a real file passed via `--observe-otlp <path>`); catches `File`-
specific failure modes (partial writes shorter than the line,
ENOSPC, EAGAIN) the v0 library would otherwise not exercise.

Cons: new `tempfile` dev-dependency in the workspace; v0's scope
is the LIBRARY contract on a generic `W: Write + Send + Sync`,
not the real-file integration (DISCUSS D9 + the shared-artifacts-
registry `file_handle` MEDIUM-risk note assign the real-file
semantics to the CLI follow-up feature's tests); adds substrate
complexity v0 does not need. The Lumen OTLP-JSON writer's
acceptance tests use `SharedBuf` for the same reasons; the real-
file behaviour is exercised by the CLI feature's tests (commits
`c6b336c` and `3af7e82`), which already validated that the writer
pattern holds against a real `File`.

**Rejected** for v0. The substrate-lie probe contract is already
discharged: the Lumen writer (which shares the `Mutex<W>` pattern
byte-for-byte) has been exercised against a real `File` by the CLI
follow-up and has not regressed. The Cinder writer inherits that
substrate confidence.

### Alternative 6 — `bridges/` subdirectory under `self-observe/src/`

Pros: anticipates the eventual file-count growth (Sluice + Augur +
Ray + Strata writers and their OTLP-JSON variants will land post-
v0).

Cons: over-organisation at N=4 sibling files (after this feature
ships). Forces a retrospective move of `lumen_bridge.rs`,
`lumen_otlp_json.rs`, and `cinder_bridge.rs` (or accepts an
inconsistency where some writers live at crate root and Cinder-
OTLP-JSON lives under `bridges/`). Either path is worse than the
file-flat status quo. ADR-0038 §4 made the identical decision for
the Pulse-sink sibling; consistency favours making it again here.

**Rejected** for v0. A future "refactor: group self-observe
writers under bridges/" change ships when the file count reaches
~8.

### Alternative 7 — Two ADRs (this one for the public surface + a separate ADR for the cross-bridge OTLP-JSON serde-struct duplication convention)

Pros: maximum traceability; the OTLP-JSON serde-struct duplication
choice (DISCUSS D7) earns its own audit-trail artefact, separate
from the per-crate public-API ADR.

Cons: premature formalisation. The duplication convention has only
two exemplars at this point (Lumen writer at v0, Cinder writer
here). The third exemplar (Sluice or Augur OTLP-JSON writer) is
what will let us see whether the duplicated structs really are
"the same shape" or whether each writer's envelope is subtly
different. ADR-0038 §5 Alternative 5 made the identical decision
to defer the cross-bridge test-seam ADR; the same rule-of-three
logic applies to the cross-bridge serde-struct ADR.

**Rejected** for v0. Both deferred ADRs become warranted when a
third bridge or writer (Sluice / Augur / Ray / Strata) lands and
the conventions hold across three exemplars.

### Alternative 8 — Zero ADRs

Pros: less paperwork.

Cons: inconsistent with the Phase-1+ convention that every crate's
public-API + layout decision earns an ADR (ADR-0011 spark, ADR-0018
sieve, ADR-0022 codex, ADR-0026 prism, ADR-0033 beacon, ADR-0038
cinder-to-pulse). The `self-observe` crate's *first* OTLP-JSON
writer (`LumenToOtlpJsonWriter`) shipped before the convention
crystallised, so it has no dedicated ADR; ADR-0038 retro-fitted the
convention for the Cinder Pulse-sink and this ADR continues the
convention for the Cinder OTLP-JSON sink. Skipping an ADR here
would leave a documentation gap exactly where the cross-bridge
NDJSON-validity invariant (DISCUSS D6) and the cross-bridge
metric-name contract (DISCUSS D1, cross-locked to ADR-0038 §2)
need a referenceable artefact.

**Rejected**.

## Consequences

**Positive**:

- Public surface byte-equivalent to `LumenToOtlpJsonWriter` for
  every part that can be (`pub struct ...<W: Write + Send + Sync>`,
  `inner: Mutex<W>`, `scope_name: String`, `pub fn new(inner: W) ->
  Self`). The operator's mental model is one idiom shared across
  every OTLP-JSON writer in `self-observe`: construct one
  `XxxToOtlpJsonWriter::new(W)` wrapping the sink and pass it as
  the upstream crate's recorder.
- Public surface locked by `cargo public-api -p self-observe` (CI
  Gate 2, ADR-0005) and `cargo semver-checks` (Gate 3). Breaking
  changes to the constructor signature or the impl-block dispatch
  set require a major-version bump on the `self-observe` crate.
- The acceptance-test seam (Cinder drives, `SharedBuf` captures,
  `serde_json::Value` asserts) is consistent across all four
  writer test files in the `self-observe` crate (`lumen_to_pulse.rs`,
  `lumen_to_otlp_json.rs`, `cinder_to_pulse.rs`,
  `cinder_to_otlp_json.rs`).
- The dual-emission contract from DISCUSS D8 is naturally
  expressible in one Slice 03 test (one `cinder.evaluate_at` call
  produces both per-item migrate lines and the per-tenant evaluate
  line in the same byte stream) without writer-side simulation of
  the cascade.
- The cross-bridge metric-name parity invariant (DISCUSS D1, cross-
  locked to ADR-0038 §2) is auditable by `diff`-ing the metric-name
  string literals between `cinder_bridge.rs` and
  `cinder_otlp_json.rs`; both files emit the exact same three
  strings.
- No new external dependencies. No new workspace members. No new
  CI gates. Inherits the existing five-gate workspace contract from
  ADR-0005.

**Negative**:

- The OTLP-JSON envelope serde structs are duplicated between
  `lumen_otlp_json.rs` and `cinder_otlp_json.rs` (DISCUSS D7). The
  duplication is acknowledged and accepted at v0; the extraction
  trigger is the third writer's landing. A regression that tightens
  the OTLP-JSON wire shape (e.g. requires an `aggregationTemporality`
  value change from `2` to `1`) requires two edits in v0 — one in
  each writer file. The acceptance tests on both writers catch any
  divergence at CI time (each asserts the wire shape independently).
- The `OtlpNumberPoint.attributes` slot diverges from Lumen's
  (`Vec<T>` vs `[T; 1]`). This is the single structural difference
  between the two writers; it is forced by the Cinder per-event
  attribute cardinality. A reader cross-referencing the two files
  must notice the divergence. The `emit` helper's signature also
  diverges by two parameters (`value: &str` and `point_attrs:
  Vec<OtlpAttr<'_>>`).
- The recommended internal shape (§5) is non-binding; the crafter
  may diverge in helper naming, inlining, attribute-vector
  ownership, or the unit-string handling. The non-binding shape is
  the right level of specificity — the contract (§2) is enough to
  pin observable behaviour; over-pinning internal helpers is
  back-seat driving.
- A regression in Cinder's `InMemoryTieringStore::evaluate_at`
  cascade can break Slice 03's dual-emission test even if the
  writer itself is correct. This is the desired behaviour per
  DISCUSS D8: Cinder's cascade *is* the contract the writer
  inherits. The regression's actual location (Cinder, not the
  writer) is diagnosable from Cinder's own in-tree tests, which
  would fail first.

**Trade-offs**:

- File-flat layout (DD1) vs `bridges/` subdirectory: optimised for
  *current* readability at N=4 sibling writer files. The eventual
  growth to N=8-10 pays a refactoring cost in a future commit; this
  feature does not pre-pay.
- Attribute-array shape (DD2 — `Vec<OtlpAttr<'a>>`) vs per-method
  fixed-size arrays: one small `Vec` allocation per event accepted
  in exchange for one struct rather than three near-clones and for
  forward-extensibility to a fourth attribute on any event.
- Test seam choice (DD3) entangles writer tests with Cinder
  behaviour: chosen because the dual-emission contract requires it
  and consistency across slices is more valuable than test
  isolation from a stable upstream behaviour. The Lumen OTLP-JSON
  writer's tests made the same trade-off and have not regretted
  it.
- Stub posture (DD4 — empty no-op): Slice 01 ships with the two
  un-implemented methods as `{}` instead of `todo!()`. The
  loudness gain of `todo!()` is theoretical in the Slice-01-only
  window because Slice 02 and Slice 03's RED-phase tests are the
  loudness mechanism for any missing implementation; the build
  noise of `todo!()` is real.
- ADR scope (DD5) records only the public-surface ADR, deferring
  the cross-bridge serde-struct duplication ADR and the
  cross-bridge test-seam ADR. Defers formalisation cost to the
  point where the conventions have three exemplars (Lumen +
  Cinder + first-of-Sluice/Augur/Ray/Strata).

## Quality attribute alignment

- **Functional Suitability — Correctness**: the per-event contract
  in §2 is exhaustive (three Cinder methods × one locked attribute
  schema each). Every BDD scenario in
  `discuss/journey-observe-cinder-via-otlp-json.feature` resolves
  to a single per-event contract check. The cross-bridge metric-
  name parity invariant (DISCUSS D1, cross-locked to ADR-0038 §2)
  is enforced by string-equality asserts in the tests on BOTH
  sides (Pulse sink + OTLP-JSON sink).
- **Performance Efficiency**: one small `Vec<OtlpAttr>` allocation
  per event (≤3 entries, fits the smallest allocator size class);
  one `serde_json::to_string` call (linear in line size); one
  `Mutex<W>` acquisition; one to three `write_all` calls inside
  the critical section. No async, no I/O beyond `W`'s semantics,
  no network. Cost basis matches the established
  `CinderToPulseRecorder` per-event cost (`BTreeMap<String, String>`
  allocation per event).
- **Compatibility — Interoperability**: consumes
  `cinder::MetricsRecorder` (upstream port, unchanged) and produces
  OTLP-JSON `ResourceMetrics` NDJSON lines (downstream wire
  protocol, defined by the OpenTelemetry specification). The
  generic `W: Write + Send + Sync` is the technology-neutral seam
  at the sink side; the operator's sidecar chooses any
  `W`-compatible substrate (`File`, `BufWriter<File>`, a memory
  buffer for tests, a `socket2::Socket` writer, etc.).
- **Reliability — Maturity**: best-effort emission posture
  (DISCUSS D5) prevents serialisation, write, or mutex-poisoning
  failures from propagating to Cinder (whose trait methods return
  `()`). The writer cannot crash Cinder. NDJSON validity (D6) is
  defended by the `Mutex<W>` + `write_all + write_all + flush`
  triple inside the critical section, byte-equivalent to the Lumen
  writer's pattern that has been exercised in production via the
  CLI follow-up (commits `c6b336c`, `3af7e82`).
- **Security — Integrity**: `tenant_id` is forwarded unchanged from
  Cinder's call to the OTLP-JSON output (DISCUSS D3 / shared-
  artifacts-registry `tenant_id` HIGH-risk row). Two-tenant
  isolation is asserted in every slice's tests, defending against
  silent transforms (trim, case-fold, intern). Tier serialisation
  is locked to lowercase ASCII by the `tier_lowercase` helper from
  one source location.
- **Maintainability — Modularity, Testability**: one new file,
  three trait method bodies + one `emit` helper + one
  `tier_lowercase` helper. Acceptance tests are per-slice with
  explicit per-tenant isolation, NDJSON validity, and dual-
  emission tests. Mutation-testing scope is one file at 100% kill
  rate per ADR-0005 Gate 5 (per `CLAUDE.md`'s per-feature mutation
  testing strategy).
- **Maintainability — Modifiability**: public surface locked by
  `cargo public-api -p self-observe` (Gate 2) and
  `cargo semver-checks` (Gate 3); breaking changes require a
  major-version bump on the `self-observe` crate. The
  `attributes: Vec<OtlpAttr<'a>>` choice (DD2) makes adding a
  fourth attribute to any event a one-line change in the calling
  `record_*` method.
- **Portability**: pure Rust, no `unsafe`, no platform-specific
  code. Inherits the crate's `#![forbid(unsafe_code)]` posture.

**ATAM sensitivity points**:

1. The `migrated.to_string()` rendering on `record_evaluate`
   (DISCUSS D4). Exact for any `usize`: OTLP-JSON encodes `uint64`
   as a string, with no precision loss; Slice 03's tests assert
   the exact string (e.g. "5" for acme, "2" for globex).
2. The lowercase serialisation of `Tier` (DISCUSS D3). One helper
   function (`tier_lowercase`) drives both the `place` event's
   `tier` attribute and the `migrate` event's `from`/`to`
   attributes; a regression that capitalises one but not the
   others would be a one-helper edit. Slice 01's three-tier test
   pins the convention with string-equality asserts.
3. The NDJSON-validity invariant (DISCUSS D6, OK5). One assertion
   in Slice 01 (the "buffer ends with `\n`" and "exactly one line
   per event" checks); a regression that emits without a trailing
   newline or interleaves bytes across two threads' calls breaks
   the assertion directly.

**ATAM trade-off points**:

1. Best-effort emission (DISCUSS D5) sacrifices error visibility to
   Cinder for forward compatibility with future non-empty error
   conditions. Same trade-off the Pulse-sink sibling already
   accepted (ADR-0038 trade-offs).
2. Test seam choice (DD3) entangles the writer's tests with
   Cinder's `InMemoryTieringStore` behaviour. Chosen because the
   dual-emission contract (DISCUSS D8) requires it; consistency
   across all four writer test files outweighs the entanglement
   risk (which is the same one ADR-0038 §3 already accepted).
3. Cross-bridge serde-struct duplication (DISCUSS D7) sacrifices
   DRY for the rule of three. The eight envelope serde structs are
   duplicated between `lumen_otlp_json.rs` and
   `cinder_otlp_json.rs` (with the single
   `OtlpNumberPoint.attributes` divergence noted above); the
   extraction trigger is the third OTLP-JSON writer sibling
   (Sluice/Augur/Ray/Strata).

## Earned Trust (Principle 12) — adapter posture

The writer has no external substrate of its own (no filesystem
ownership, no network, no vendor SDK, no subprocess). It depends on
the world only through:

1. The runtime-supplied `W: Write + Send + Sync` (whose contract is
   `std::io::Write` — well-tested upstream; in v0 acceptance tests
   `W = Arc<Mutex<Vec<u8>>>`, in the post-v0 CLI integration
   `W = std::fs::File` opened with `O_APPEND`).
2. `SystemTime::now()` (whose nanos-since-epoch value is rendered
   into `timeUnixNano` but is NOT asserted by acceptance tests
   beyond "parses as `u64`"; mirrors the Lumen writer's posture at
   `lumen_otlp_json.rs:142-146`).
3. `serde_json::to_string` (whose failure mode for these hand-
   rolled structs is "impossible in practice", because the structs
   derive `Serialize` and contain only strings and integers).
4. `Mutex<W>::lock` (whose failure mode is poisoning under a
   previous panic; handled silently to keep the best-effort
   contract).

The three Earned-Trust layers (Principle 12c):

1. **Subtype-check layer**: `cargo public-api -p self-observe` (CI
   Gate 2 per ADR-0005) catches any change to
   `CinderToOtlpJsonWriter`'s public surface at CI time. The
   compile-time `assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>()`
   test catches any loss of the `Send + Sync` trait bound at
   compile time (not at runtime). The
   `impl cinder::MetricsRecorder for CinderToOtlpJsonWriter<W>`
   block is subtype-checked by the compiler against
   `cinder::MetricsRecorder`'s trait definition.
2. **Behavioural-check layer**: the acceptance-test suite under
   `crates/self-observe/tests/cinder_to_otlp_json.rs` exercises
   the per-event contract in §2 against a `SharedBuf` byte sink
   that is then parsed and re-asserted as `serde_json::Value`.
   The dual-emission test in Slice 03 exercises the cross-method
   contract end-to-end (one `evaluate_at` call → N migrate lines
   + 1 evaluate line in the same byte stream). The "buffer ends
   with `\n`" assertion (Slice 01) defends the NDJSON-validity
   invariant (DISCUSS D6, OK5) — the substrate-lie probe for the
   `Mutex<W>` + `write_all + write_all + flush` triple.
3. **Structural-check layer**: degenerate for a no-substrate
   adapter — there is no on-disk source-of-truth schema to
   enforce drift against beyond the public surface, which the
   subtype layer already covers. This is the minimum the
   principle permits for a no-substrate adapter; same posture as
   ADR-0001's `otlp-conformance-harness` and ADR-0038's
   `CinderToPulseRecorder`.

**Environments-known-to-lie**:

The writer's only substrate-adjacent dependency is `Mutex<W>::lock`
plus the `write_all` calls on the inner `W`. The Rust standard-
library `Mutex` implementation is well-tested. The generic `W` is
exercised against an in-memory `Arc<Mutex<Vec<u8>>>` in the v0
acceptance tests; the real `File` (with its `O_APPEND` atomicity
guarantees on POSIX) is the CLI follow-up feature's substrate. The
acceptance tests do not need to exercise `File` lies because:

- The Lumen OTLP-JSON writer's identical `Mutex<W>` pattern is
  already exercised against a real `File` by the CLI follow-up
  (commits `c6b336c` "feat(self-observe): LumenToOtlpJsonWriter —
  cross-process OTLP-JSON sink" and `3af7e82`
  "feat(kaleidoscope-cli): --observe-otlp <path> flag for
  ingest"). Both passed; the substrate has not lied.
- A future `File`-specific failure (ENOSPC, EAGAIN, partial
  writes shorter than the line) would degrade emission in the
  same way for both writers, and the documented best-effort
  posture (DISCUSS D5) accepts this.

The probe contract for THIS writer is the acceptance-test suite.
The Slice 01 NDJSON-line-termination test
(`output_is_ndjson_one_line_per_event_with_trailing_newline`,
or whatever the DISTILL wave names it) is the substrate-lie probe:
it asserts that even when the writer is invoked three times in
succession, the byte sequence in the sink is exactly three lines
each terminated by `\n`, with no interleaving, truncation, or
missing terminators. This is the "demonstrate empirically that it
can honor its contract in the real environment where it will run"
requirement of Principle 12, discharged at the v0 substrate
(in-memory) and inherited at the post-v0 substrate (real `File`)
from the Lumen sibling's already-validated proof.

## §7 — Post-v0 Cross-Writer NDJSON-Validity Handoff

The `CinderToOtlpJsonWriter` inherits the same atomic triple
(`write_all(body) + write_all(b"\n") + flush`) pattern as
`LumenToOtlpJsonWriter` (§2). This ensures per-writer NDJSON
validity (OK5), verified in this feature's Slice 01 tests against
an in-memory `Write`.

When the post-v0 CLI feature wires both writers to the same
`std::fs::File` (via `--observe-otlp <path>`), an additional
**cross-writer NDJSON-validity invariant** becomes relevant: the
byte stream must remain valid even if Lumen and Cinder record
events concurrently. Each writer's internal `Mutex<W>` guarantees
within-writer atomicity, but the `File` is a shared resource at
the OS level — the kernel guarantees `O_APPEND` writes are atomic
up to `PIPE_BUF` (typically 4096 bytes on Linux), which exceeds
the size of any single OTLP-JSON line this writer emits.

**The CLI follow-up feature's DEVOPS wave MUST**:

1. Define a new outcome KPI (e.g. **OK6-CLI-cross-writer-ndjson**):
   "100% of captured NDJSON lines are independently parseable as
   JSON and the stream ends with `\n`, even when Lumen and Cinder
   writers emit concurrently to the same `File`."

2. Measure this KPI via acceptance tests that spawn Lumen and
   Cinder record threads simultaneously against a real `File`
   (`std::fs::OpenOptions::new().create(true).append(true).open(path)`)
   and assert the captured stream's per-line JSON validity.

3. Include a "concurrent random pause" scenario (sleep jitter
   between writes) that forces scheduling variations capable of
   exposing interleaving bugs.

**The within-writer contract** locked by this ADR §2 is a
**prerequisite but not sufficient** for cross-writer safety. The
CLI feature owns the cross-writer test surface. Recorded here so
the CLI feature's DISCUSS/DESIGN waves can read the requirement
during their Prior Wave Consultation step.

This forward-compatibility note was added during Forge's peer
review of this DEVOPS wave (2026-05-18) as Issue 3 (HIGH,
non-blocking).

## §8 — CLI wiring: cross-writer sink-sharing mechanism

Added 2026-05-18 during the DESIGN wave of the follow-up feature
`cli-cinder-otlp-wiring-v0`. Discharges the mandate in §7.

### Context

The CLI feature `cli-cinder-otlp-wiring-v0` wires both
`LumenToOtlpJsonWriter` and `CinderToOtlpJsonWriter` against the same
operator-supplied file path passed via `--observe-otlp <path>`.
ADR-0039 §1 locks both writer constructors to `new(W: Write + Send +
Sync) -> Self`, taking ownership of the sink by value. The CLI
therefore needs to produce two distinct `W` values that, between
them, append to the same file with cross-writer NDJSON-validity
(OK6 in `docs/feature/cli-cinder-otlp-wiring-v0/discuss/outcome-kpis.md`,
inheriting the within-writer guarantee from §2 and lifting it to
cross-writer per §7).

### Decision

The CLI opens the operator-supplied path **exactly once** with
`std::fs::OpenOptions::new().create(true).append(true).open(path)`,
then obtains a second `File` handle via `file.try_clone()?`. The
original `File` is passed into `LumenToOtlpJsonWriter::new(file)`;
the cloned `File` is passed into `CinderToOtlpJsonWriter::new(file_clone)`.
Each writer continues to own its own `Mutex<File>` per §1 and §2.
Cross-writer atomicity is the POSIX `O_APPEND` kernel guarantee:
each `write(2)` against an `O_APPEND` descriptor atomically appends
relative to other `O_APPEND` writes on the same underlying file
description, up to `PIPE_BUF` (4096 bytes on Linux and macOS). The
worst-case OTLP-JSON line either writer emits is the
`cinder.migrate.count` line at approximately 540 bytes, well below
`PIPE_BUF`.

Concretely (illustrative, the crafter writes the final form during
DELIVER):

```rust
// inside the Some(path) => { … } arm of the otlp_log_path match in
// crates/kaleidoscope-cli/src/lib.rs (currently lines 147-160):
let file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(path)?;
let file_clone = file.try_clone()?;
let lumen_recorder = Box::new(LumenToOtlpJsonWriter::new(file));
// … and at the parallel Cinder construction site (currently line 163):
let cinder_recorder: Box<dyn cinder::MetricsRecorder + Send + Sync> =
    Box::new(CinderToOtlpJsonWriter::new(file_clone));
```

The Cinder recorder construction at line 163 becomes a parallel
`match otlp_log_path { Some(_) => …, None => Box::new(CinderRecorder) }`
mirror of the existing Lumen-side match at lines 147-160.

### Considered Alternatives

**Alternative 1 — Two separate `OpenOptions::new().create(true).append(true).open(path)` calls.**
Same kernel-level atomicity guarantee as `try_clone` (each `open`
produces an independent file description, each with `O_APPEND`).
Marginally more error-prone: second `open` failure after the first
succeeded leaves a half-constructed state to unwind; with
`try_clone`, the failure point is collapsed into a single sequence
of two consecutive operations on one already-validated descriptor.
Marginally less idiomatic per the std-lib documentation
(`File::try_clone` is documented as "creates a new independently
owned handle to the underlying file" — the exact idiom for this
use case). **Rejected on idiomatic posture**; acceptable as a
fallback if a portability surprise ever made `try_clone` unusable
(none anticipated on the deployment targets per ADR-0005's CI
matrix).

**Alternative 2 — `Arc<Mutex<File>>` shared via a `SharedFile(Arc<Mutex<File>>)` adapter implementing `Write`.**
Wraps a single `Mutex<File>` in an `Arc`; the adapter implements
`Write` by locking the inner mutex on each `write_all`. Each
writer's outer `Mutex<W>` (per §2) wraps an `Arc<Mutex<File>>`,
producing a double-mutex shape: writer's outer mutex for its emit-
triple atomicity, adapter's inner mutex for cross-writer
serialisation. Pros: genuine cross-writer single-point-of-
serialisation at userspace, independent of any kernel atomicity
claim. Cons:

1. Introduces a new public-ish type in `kaleidoscope-cli`, which
   would warrant a separate ADR-0040.
2. Two mutex acquisitions per emission compounded with the existing
   writer mutex (lock-graph depth doubles).
3. Defeats the `O_APPEND` atomicity by serialising at userspace
   what the kernel was already going to serialise for free.
4. **Paints future-Andrea into a corner**: when multi-process
   scenarios surface post-v0 (the DISCUSS D7 deferral), the
   userspace mutex protects nothing across processes; the adapter
   would have to be torn out and the design re-done around
   `O_APPEND` anyway.
5. Code footprint: +25-35 lines in `lib.rs` for the new type and
   its `Write` impl, plus two `Arc::clone()` calls at the
   construction site, versus +5 lines for `try_clone`.

**Rejected on abstraction cost, double-mutex contention, multi-
process forward-incompatibility, and ADR scope.**

**Alternative 3 — `fs::write` via a shared buffer drained periodically through `parking_lot::Mutex<File>` outside both writers.**
Each writer would write to a userspace buffer; a separate
draining mechanism would flush the buffer to the file mutex
periodically. Defeats per-line atomicity by coalescing writes
across lines into a single buffer flush; introduces a "did the
buffer drain before the process exited?" failure mode requiring
explicit shutdown logic; introduces a new external dependency
(`parking_lot`) not currently in the workspace. **Rejected as
the wrong shape** for an NDJSON sink where per-line atomicity is
the contract.

**Alternative 4 — `OwnedFd::try_clone_to_owned()` then re-wrap as `File`.**
Equivalent to `File::try_clone` on Unix; less portable (Windows
requires `as_handle().try_clone_to_owned()` shimming).
`File::try_clone` already wraps the platform-appropriate primitive
internally per the std-lib source. **Rejected as redundant
indirection.**

### Consequences

**Positive**:

- OK6 (cross-writer NDJSON validity under concurrent emission) is
  guaranteed by a kernel-level mechanism (`O_APPEND` atomicity)
  rather than by a userspace abstraction. The acceptance test
  `cross_writer_ndjson_validity_under_concurrent_random_pauses`
  mandated by §7 item 3 is the empirical substrate-lie probe.
- No new public type, no new abstraction, no new module in any
  crate. The wiring change is approximately five lines inside the
  existing `Some(path)` arm of the `otlp_log_path` match in
  `crates/kaleidoscope-cli/src/lib.rs`.
- Idiomatic Rust per `CLAUDE.md`: `File::try_clone` is the std-lib's
  exact answer to "I have two structs that each want to own a `Write`
  over the same file". No `dyn Trait` indirection beyond what already
  exists (the `Box<dyn cinder::MetricsRecorder + Send + Sync>` at
  line 163, which is forced by the conditional construction over two
  concrete recorder types, not a design preference).
- Forward-compatible with the post-v0 multi-process scenario
  (DISCUSS D7): `O_APPEND` IS the multi-process atomicity mechanism
  for sub-`PIPE_BUF` writes. The DD1 design extends transparently to
  the cross-process case if a future feature lifts the deferral.
- Writer public surfaces (`CinderToOtlpJsonWriter::new`,
  `LumenToOtlpJsonWriter::new`) are consumed unchanged. ADR-0039 §1
  remains locked; no `new_shared` variant, no `Arc<Mutex<W>>`
  parameter, no constructor overload.
- One additional FD per `ingest` invocation (two total). FD reference
  counting is the kernel's responsibility; `Drop` on each `File` is
  independent. No double-close hazard.

**Negative**:

- The cross-writer guarantee is asserted by the test for the
  in-process two-thread case at line sizes up to ~540 bytes. If a
  future change ever made a single OTLP-JSON line exceed `PIPE_BUF`
  (4 KiB), the `O_APPEND` atomicity would no longer apply across
  the line and interleaving would become possible. The current line
  sizes are well under `PIPE_BUF`; a regression that quadrupled the
  point-attribute count or added kilobyte-scale fields would need
  to revisit this decision. The acceptance test would catch the
  regression in practice (the concurrent-random-pause scenario
  would start producing interleaved lines), but the failure mode
  would be opaque ("test fails sometimes") rather than obvious
  ("static analysis flags a line bigger than 4 KiB").
- The mechanism's correctness depends on the OS providing the
  `O_APPEND` guarantee at the kernel level. Linux and macOS (the
  CI matrix per ADR-0005) both honour it; Windows honours
  `FILE_APPEND_DATA` equivalently. Exotic filesystems mounted via
  FUSE may not honour `O_APPEND` correctly (this is the
  CLAUDE.md "environments-known-to-lie" residue inherited from
  Principle 12). The acceptance test exercises the deployment
  substrate the operator actually runs on; an exotic filesystem
  would be a future operator's responsibility to validate.

**Trade-offs**:

- Single-line atomicity vs. abstraction cost: chosen the single-line
  atomicity at zero abstraction cost (kernel `O_APPEND`) over the
  userspace serialisation alternative (Arc<Mutex<File>> adapter)
  that would have added a new type and double the mutex acquisitions
  per emission. The trade-off is paid in increased dependence on the
  OS's substrate guarantee, which is well-characterised on the
  deployment targets and probed by the acceptance test.

### Quality attribute alignment with OK6

- **Functional Suitability — Correctness**: OK6 is asserted directly
  by the `cross_writer_ndjson_validity_under_concurrent_random_pauses`
  acceptance test mandated by §7 item 3. The test reads back the
  shared file post-join and asserts every non-empty line parses as
  `serde_json::Value`, the file ends with `\n`, and the per-writer
  line counts (100 each) match the spawned thread workloads.
- **Reliability — Fault Tolerance**: `O_APPEND` is a hard kernel
  guarantee on the deployment targets (Linux, macOS). Within-writer
  triple atomicity (per §2) handles serialisation, write, and mutex-
  poisoning failures with the best-effort `let _ = …` pattern;
  cross-writer atomicity is independent of that pattern (the kernel
  handles it).
- **Performance Efficiency — Resource Utilisation**: two FDs for the
  lifetime of the `ingest` call; one `write(2)` syscall per OTLP-JSON
  line; no cross-writer userspace lock contention.
- **Maintainability — Analysability**: a reader following the
  Lumen-side wiring at line 153 sees the Cinder-side wiring as the
  obvious parallel; the `try_clone` call and the cross-writer
  rationale are documented in the wiring's source comments and in
  this §8 extension.
- **Portability**: `File::try_clone` is cross-platform; `O_APPEND`
  atomicity holds on Linux, macOS, and (under the equivalent
  `FILE_APPEND_DATA` semantics) Windows.

### Earned Trust (Principle 12) — adapter posture

The CLI wiring introduces no new substrate-adjacent dependency
beyond the existing `std::fs::OpenOptions::open(path)` call (already
in production at `crates/kaleidoscope-cli/src/lib.rs:148-152`). The
addition is `file.try_clone()`, a `dup(2)` syscall whose failure
modes (`EMFILE`, `ENFILE`) are well-characterised by POSIX and lift
cleanly through the existing `From<std::io::Error> for Error` impl
into `Error::Io`. No new probe contract is needed at the wiring
seam; the substrate-lie probe is the acceptance test mandated by
§7 item 3 (the concurrent-random-pause scenario), which exercises
the `O_APPEND` substrate claim against a real `File` on the
deployment filesystem.

The three Earned-Trust layers (Principle 12c) for the wiring change:

1. **Subtype-check layer**: the existing `cargo public-api -p
   kaleidoscope-cli` (Gate 2 per ADR-0005) catches any change to
   `ingest`'s signature, which is `pub fn ingest(tenant: &TenantId,
   data_dir: &Path, batch_size: usize, reader: impl BufRead,
   otlp_log_path: Option<&Path>) -> Result<IngestStats, Error>` and
   does NOT change. The compile-time `Box<dyn cinder::MetricsRecorder
   + Send + Sync>` type assertion at line 163 catches any loss of the
   `Send + Sync` trait bound on `CinderToOtlpJsonWriter<File>`.
2. **Behavioural-check layer**: the new acceptance test file
   `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs`
   exercises the cross-writer contract end-to-end against a real
   `File` substrate (per §7 item 2), including the random-pause
   scenario (per §7 item 3). The existing
   `observe_otlp_flag.rs` test file continues to pass byte-
   equivalently (OK8 guardrail).
3. **Structural-check layer**: degenerate for a no-new-substrate
   wiring change. The wiring depends only on the std-lib
   `File::try_clone` primitive (whose source-of-truth schema is
   defined by POSIX `dup(2)`) and on the writer constructors
   (locked by ADR-0039 §1, defended by Gate 2). Same minimum
   posture as the within-writer Earned-Trust footer above.

**Environments-known-to-lie**: the `O_APPEND` kernel guarantee
holds on the CI matrix (Linux + macOS per ADR-0005) and on the
operator's deployment target (Docker Linux per the recent
`Dockerfile` work in commit `0c5d91c`). The acceptance test
exercises the substrate the operator runs on. Exotic filesystems
(FUSE mounts that do not honour `O_APPEND`) are out of scope at
v0; if a future operator deploys on such a substrate, that
operator's own probe (running the acceptance test on their
substrate) is the empirical answer.

The within-writer triple (`write_all(body) + write_all(b"\n") +
flush` inside the `Mutex<W>` critical section per §2) is the
prerequisite for the cross-writer guarantee. The §8 extension does
not modify the triple; it composes two instances of it via the
kernel's `O_APPEND` atomicity on a shared file description.
