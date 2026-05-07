# ADR-0022 — `codex` public API surface and crate layout

- **Status**: Accepted
- **Date**: 2026-05-07
- **Author**: `@nw-solution-architect` (Morgan)
- **Feature**: `codex` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

Codex v0 is the AGPL-3.0-or-later schema-authority library codifying the
pinned OTel semconv version (`opentelemetry-semantic-conventions
=0.27.x`) plus the three Kaleidoscope-house attributes (`tenant.id`,
`feature_flag.{key}`, `experiment.id`). DISCUSS
(`docs/feature/codex/discuss/wave-decisions.md`) locks the **contract**:

- Library, not service (Q1).
- Hand-written Rust constants generated from upstream (Q2).
- Single pinned semconv version (Q3).
- No per-tenant overlays (Q4).
- `LintReport` carrying one or more `LintViolation`s, each with `name`,
  `kind: ViolationKind`, `nearest_blessed_match: Option<String>` (Q5).
- Spark consumes via direct `SchemaCatalogue::validate(...)` API call
  (Q6).
- Five public types only: `SchemaCatalogue`, `BlessedAttribute`,
  `LintReport`, `LintViolation`, `ViolationKind`
  (`shared-artifacts-registry.md`).

DISCUSS does **not** lock:

1. The exact shape of `BlessedAttribute` (D1: enum vs struct).
2. The exact `validate` argument type (D2: `&[(&str, &str)]` vs
   `IntoIterator` vs Spark's typed Resource).
3. Multi-violation collection mechanism (D3: collect-all `Vec`
   accumulator vs iterator chain).
4. Internal module split.
5. Catalogue storage shape (`&'static [_]` vs `phf` vs `BTreeMap`).
6. Whether the catalogue is built lazily once per process or
   reconstructed per `new()` call.

Harness ADR-0001 ("free `pub fn`s and free types in `lib.rs`, crate
split into named internal modules from day one") and ADR-0011 (Spark's
public API) are the precedents. Codex inherits the same shape because
the architectural drivers are identical: small public surface +
module-per-concept + idiomatic Rust + `cargo public-api` catches
drift.

The `shared-artifacts-registry.md > SchemaCatalogue public type` and
`> LintReport and LintViolation types` entries classify public-API
drift as **HIGH integration risk**.

## Decision

### 1. Public surface (final list, locked)

Five items in `crates/codex/src/lib.rs`, alphabetised below. No
re-exports of upstream types. The no-re-exports rule keeps the
dependency edge visible (harness ADR-0001 precedent, Spark ADR-0011
precedent).

```rust
// from crates/codex/src/lib.rs:

pub use crate::catalogue::{BlessedAttribute, SchemaCatalogue};
pub use crate::report::{LintReport, LintViolation, ViolationKind};
```

`SchemaCatalogue::new() -> Self` is the constructor; `validate(...)` is
the single behavioural method. The crafter chooses whether to keep
`BlessedAttribute` directly constructible from the public surface or
hide construction behind `pub(crate)` constructors and expose only the
catalogue-iteration view; both are consistent with the "consumers do
not extend the catalogue at v0" contract from Q4.

### 2. `BlessedAttribute` shape (D1) — enum with two variants

```rust
#[non_exhaustive]
pub enum BlessedAttribute {
    /// Exact-name match. The full attribute name must equal `&'static str`.
    Exact(&'static str),
    /// Prefix-and-non-empty-suffix match. The attribute name must start
    /// with `&'static str` and continue with at least one further
    /// character.
    Prefix(&'static str),
}
```

Rationale: the catalogue's lookup loop expresses cleanly as a `match`
over the variant; the static-slice form is zero-cost; a future third
match-kind (regex, glob, version-pattern) lands as a third variant
without breaking matchers thanks to `#[non_exhaustive]`. The struct-
plus-enum-field alternative was considered (a `BlessedAttribute {
name: &'static str, kind: MatchKind }` shape); rejected because it
forces a separate `MatchKind` enum into the public surface, growing
the type count from five to six without expressive benefit.

### 3. `validate` argument shape (D2) — `&[(&str, &str)]`

```rust
impl SchemaCatalogue {
    pub fn new() -> Self;

    pub fn validate(
        &self,
        attributes: &[(&str, &str)],
    ) -> Result<(), LintReport>;
}
```

Rationale: smallest shape Spark can call without per-call allocation.
Spark's `init.rs` `build_resource` already accumulates attributes as
`Vec<KeyValue>`; converting to `&[(&str, &str)]` is one cheap borrow
loop using the existing `KeyValue { key, value }` fields, no heap
traffic. The `IntoIterator<Item = (&str, &str)>` alternative was
considered; rejected because (a) no v0 caller benefits — Spark's
single call site can build a slice trivially; (b) the slice form is
the simplest type signature for a public API; (c) generic `IntoIterator`
inflates monomorphisation across the consumer's call graph for no
runtime benefit at the corpus size in play. The "borrow Spark's typed
`Resource`" alternative was rejected outright: it would couple Codex
to a specific OTel SDK version (the `opentelemetry_sdk::Resource`
shape is not stable), defeating the schema-authority abstraction.

The lifetime ergonomics: `&[(&str, &str)]` is the same shape that
`std::collections::HashMap` exposes to `HashMap::from_iter` callers
and that Rust's standard library uses for slice-of-pairs APIs; it is
familiar to Rust consumers and ergonomic at the call site.

### 4. Multi-violation collection (D3) — `Vec` accumulator

```rust
// internal sketch; the crafter writes the production implementation:

fn validate(&self, attributes: &[(&str, &str)]) -> Result<(), LintReport> {
    let mut violations = Vec::new();
    for (name, _value) in attributes {
        if !self.matches(name) {
            violations.push(LintViolation {
                attribute_name: (*name).to_owned(),
                kind: ViolationKind::Unknown,
                nearest_blessed_match: self.suggest(name),
            });
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(LintReport::from_violations(violations))
    }
}
```

Rationale: the `Vec` accumulator is the simplest expression of the
"collect all violations, return one report" contract from DISCUSS Q5.
The iterator-chain alternative
(`attributes.iter().filter_map(...).collect::<Vec<_>>()` with
`Result::Err` wrapping) was considered; rejected because the
accumulator is more legible at review time, and the corpus size means
the allocation pattern is identical (one `Vec<LintViolation>` either
way). Per US-CO-04 the violations list mirrors input order; the
accumulator naturally preserves order, the iterator chain would too,
but the accumulator's intent is clearer.

The clean path (every attribute blessed) returns `Ok(())` without
allocating a `Vec` if the implementation places the `Vec::new()` call
inside a lazy initialiser; the crafter chooses the optimisation level.
Either way the v0 budget (KPI 6: under 1 ms for ~10 attributes,
under 10 ms for the full corpus) is met comfortably.

### 5. Internal module split (locked)

```
crates/codex/
├── Cargo.toml
└── src/
    ├── lib.rs           # public re-exports only; no logic
    ├── catalogue.rs     # SchemaCatalogue + BlessedAttribute + lookup loop
    ├── report.rs        # LintReport + LintViolation + ViolationKind
    │                    # + Display + Error impls
    ├── fuzzy.rs         # pub(crate) fn levenshtein(...) — Slice 05
    └── generated/
        ├── mod.rs       # re-exports the corpus const
        └── semconv_0_27.rs  # &'static [BlessedAttribute] — Slice 02
```

Rationale: one module per public type plus one `fuzzy` module for the
Levenshtein helper plus one `generated` directory for the regenerated
artefact. The `generated` directory naming makes the maintainer
ritual visible at the file-tree level (Q7 lock).

### 6. Catalogue lifecycle — owned-per-call at the API surface, lazy `OnceLock` internally

```rust
// public API: SchemaCatalogue::new() returns an owned value.
// internally, the static corpus is shared via a `&'static [BlessedAttribute]`.

pub struct SchemaCatalogue {
    // The crafter chooses whether this field references the static
    // corpus directly or holds an Arc, etc. For v0 the simplest shape
    // is a unit struct that uses the static slice via a free function.
}
```

Rationale: the public surface's `new() -> Self` shape allows future
catalogue extensions (multi-version, tenant overlays at v1+) without a
breaking change. Internally at v0, the corpus is `&'static
[BlessedAttribute]` populated once at module init from the generated
file and the four house attributes; no per-`new()` allocation is
needed. Spark's `spark::init` callers can construct a `SchemaCatalogue`
once at process boot and reuse it via `OnceLock<SchemaCatalogue>` if
they wish; this is documented in ADR-0025 as the recommended
integration shape.

### 7. Crate skeleton

```toml
# crates/codex/Cargo.toml (sketch)
[package]
name = "codex"
version = "0.1.0"
edition = "2021"
license = "AGPL-3.0-or-later"
rust-version.workspace = true

[lints]
workspace = true

[dependencies]
opentelemetry-semantic-conventions = "=0.27.0"  # exact-patch pin per ADR-0024

# Note: no other runtime deps. The in-tree Levenshtein is per ADR-0024.

[dev-dependencies]
insta = { workspace = true }                    # Display snapshot tests (Slice 04)
```

`crates/codex/src/lib.rs` carries `#![forbid(unsafe_code)]` mirroring
Spark's posture (ADR-0011) and Sieve's (ADR-0018).

## Considered Alternatives

### Alternative 1 — `BlessedAttribute` as `struct { name: &'static str, kind: MatchKind }`

Pros: easier to add fields per attribute (deprecation marker, version
range, `Display` description) without growing the variant count.

Cons: forces `MatchKind` into the public type count (six types,
breaking the Q5 / shared-artefacts-registry lock at five). Requires a
separate enum for `MatchKind` whose variants are the same shape as the
proposed `BlessedAttribute` enum's variants — a layer of indirection
without expressive benefit. Future fields (deprecation marker, version
range) at v1+ are additive on the variant payload (`Exact { name,
deprecated_in: Option<Version> }`) anyway; the struct-plus-enum-field
shape buys nothing.

**Rejected**.

### Alternative 2 — `validate(&self, attributes: impl IntoIterator<Item = (&str, &str)>)`

Pros: ergonomic for callers iterating over a `HashMap`, `BTreeMap`, or
custom collection.

Cons: monomorphisation cost across consumer call graphs without
runtime benefit at the corpus size in play; v0 caller is exactly one
(Spark's `init.rs` `build_resource` output), and Spark builds a slice
trivially. The slice form is the simplest possible type signature —
"a borrowed list of name-value pairs" reads naturally on first
encounter; the `IntoIterator` form invites bikeshedding on item type
(`(&str, &str)` vs `(&K, &V)` vs `(impl AsRef<str>, impl AsRef<str>)`
…). Generic-on-shape APIs are the right call when the API is the
boundary between many consumers; v0 has one consumer.

**Rejected** for v0. If a future consumer needs the `IntoIterator`
form, an additional method `validate_iter` is the cheap additive
extension.

### Alternative 3 — `validate(&self, resource: &opentelemetry_sdk::Resource)`

Pros: Spark already has a `Resource`; calling `validate(&resource)`
would be the most direct call shape.

Cons: couples Codex to a specific OTel SDK version. The
`opentelemetry_sdk::Resource` type is not stable across minor versions
(its iteration API changed between 0.26 and 0.27), and Codex's whole
purpose is to be the schema authority *across* SDK versions. Pulling
the SDK as a runtime dep into Codex inverts the dependency arrow
(Codex would now depend on the SDK, defeating the abstraction).

**Rejected**.

### Alternative 4 — short-circuit on first violation

Pros: simpler implementation; faster on corpus-spanning errors
(though those are not in the v0 fixture set).

Cons: violates DISCUSS Q5 lock ("collect all violations into one
LintReport"). Operators want one round-trip per init failure to know
all the problems; short-circuit would force iterative discovery.

**Rejected**.

## Consequences

**Positive**:
- Five public types, surface stable, locked by `cargo public-api -p
  codex` (CI Gate 2) and `cargo semver-checks` (Gate 3).
- The enum shape of `BlessedAttribute` admits future match kinds
  without breaking; the `#[non_exhaustive]` annotation on each public
  type ensures additive evolution is non-breaking.
- The `&[(&str, &str)]` argument shape is the smallest call-site cost
  for Spark; no allocations per validate call.
- The `Vec` accumulator implementation is one-screen of code, easy to
  read and easy to mutation-test (Gate 5, 100% kill rate).
- Internal module split mirrors Spark's and Sieve's; the contributor
  ritual is consistent across the workspace.

**Negative**:
- The `&[(&str, &str)]` shape forces Spark to build a slice from its
  `Vec<KeyValue>`. This is one short loop and one allocation per
  `init` call; the cost is microseconds, well inside KPI 6's 1 ms
  budget.
- The two-variant `BlessedAttribute` enum means future additive variants
  (Deprecated, Misnamed) require pattern-match consumers in
  `pub(crate)` code to handle the new variants; the public surface
  exposes the enum, so consumers must already be `match`-ing it.
  Mitigation: `#[non_exhaustive]` forces wildcard arms; new variants
  are non-breaking.

**Trade-offs**:
- Allocation-per-call (Spark side): yes, one short-vector. The
  alternative (push the typed `Resource` in directly) costs a
  cross-crate dep on the OTel SDK version Codex would otherwise be
  agnostic about.
- API surface vs implementation flexibility: locking five public
  types means the `BlessedAttribute` enum cannot grow into a struct
  with metadata fields without a new public type. Mitigation: the
  variants' payloads are the natural growth axis.

## Quality attribute alignment

- **Functional suitability**: each public type is named in
  shared-artefacts-registry as a HIGH-integration-risk artefact; the
  enum-with-variants shape lets every BDD scenario in user-stories
  resolve to a single `match` arm.
- **Maintainability**: module-per-concept; mutation testing target
  100% kill rate per ADR-0005 Gate 5 covers the lookup loop, the
  Levenshtein, and the report-building.
- **Testability**: the `validate(&[(&str, &str)])` shape lets every
  test be a synchronous `cargo test` invocation against literal
  fixtures. No async, no I/O, no test seams beyond standard Rust
  visibility.
- **Compatibility**: `#[non_exhaustive]` on every public type means
  additive evolution is non-breaking; `cargo semver-checks` enforces.
