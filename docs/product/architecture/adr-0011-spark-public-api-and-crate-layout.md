# ADR-0011 — `spark` public API surface and crate layout

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `spark` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

Spark v0 is the Apache-2.0 Rust SDK that wraps the upstream
`opentelemetry`, `opentelemetry_sdk`, and `opentelemetry-otlp` crates,
injects Kaleidoscope's house resource attributes on every emitted
signal, lints required attributes at `init` time, honours the
OTel-canonical environment-variable contract, and flushes pending
exports synchronously on guard drop.

DISCUSS (`docs/feature/spark/discuss/wave-decisions.md`) locks the
**contract**: one public entry point `spark::init`, the closed
`SparkError` variant set, the `SparkConfig` builder shape, the opaque
`SparkGuard` with synchronous flushing `Drop`. DISCUSS does **not**
lock:

1. The exact public surface (which items beyond `init`, `SparkConfig`,
   `SparkError`, `SparkGuard` are `pub`?).
2. The internal module split (`lib.rs` flat? per-concept files?).
3. Whether the OTLP wire types (`ExportTraceServiceRequest`, etc.) are
   re-exported from `spark` or imported by consumers from
   `opentelemetry-proto` directly.
4. The `Cargo.toml` skeleton (binary? examples? `[[test]]` declarations?).

Harness ADR-0001 ("Free `pub fn`s in `lib.rs`, with the crate split into
named internal modules from day one") is the precedent. Spark inherits
the same shape because the architectural drivers are the same: small
public surface + module-per-concept + idiomatic Rust + `cargo public-api`
catches drift.

The `shared-artifacts-registry.md > spark_init_function` and
`shared-artifacts-registry.md > spark_config_builder` entries classify
public-API drift as **HIGH integration risk**.

## Decision

### Public surface (final list)

```rust
// from lib.rs, alphabetised:

pub fn init(config: SparkConfig) -> Result<SparkGuard, SparkError>;

pub struct SparkConfig { /* fields private; constructor + builder methods are the API */ }
pub struct SparkGuard  { /* fields private; opaque per ADR-0016 */ }

#[non_exhaustive]
pub enum SparkError { /* variants per ADR-0012 */ }

// re-exports: NONE.
```

That is the entire public surface. `lib.rs` does **not** re-export
`opentelemetry_proto`, `opentelemetry`, `opentelemetry_sdk`, or
`opentelemetry-otlp`. Consumers depend on those crates directly.

The harness ADR-0001 rationale ("keep the dependency edge visible; do
not shadow upstream type paths") applies verbatim. Aperture's
integration tests, future Sieve / Pulse / Lumen / Ray code, and any
third-party consumer of Spark all import upstream OTel types from the
upstream crates — never from `spark::*`.

### `SparkConfig` API shape (locked here; method signatures locked in ADR-0016)

```rust
impl SparkConfig {
    pub fn for_service(name: impl Into<String>) -> SparkConfig;
    pub fn require_tenant_id(self) -> Self;
    pub fn with_tenant_id(self, tenant_id: impl Into<String>) -> Self;
    pub fn with_feature_flags<I, K, V>(self, flags: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>;
    pub fn with_experiment_id(self, experiment_id: impl Into<String>) -> Self;
    pub fn with_endpoint(self, endpoint: impl Into<String>) -> Self;
    pub fn with_flush_timeout(self, timeout: std::time::Duration) -> Self;
}
```

Every method takes `self` and returns `Self` (the value-consuming
builder pattern). Every method is `#[must_use]`. The struct itself
is `#[non_exhaustive]` — adding a future field (e.g. for Aegis Phase 2
TLS configuration) is a non-breaking change.

`with_feature_flags` accepts the most flexible shape:
`IntoIterator<Item = (impl Into<String>, impl Into<String>)>`. This
covers `HashMap<String, String>`, `BTreeMap<&str, &str>`, `Vec<(&str,
&str)>`, the `[("checkout-v2", "on")]` array literal in the journey,
and every future shape an application might use to read its
flag-state. The cost is generic instantiation per call site; for a
function called once at startup, this is invisible.

### Internal layout

```
crates/spark/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs        # public surface only: spark::init, re-exports of
│   │                 # SparkConfig / SparkError / SparkGuard from
│   │                 # the named internal modules. Crate-root
│   │                 # #![forbid(unsafe_code)] is here.
│   ├── config.rs     # pub struct SparkConfig + builder methods +
│   │                 # the resolution chain helpers used by init.rs.
│   ├── error.rs      # pub enum SparkError + thiserror derive +
│   │                 # impl From<url::ParseError> for SparkError +
│   │                 # impl From<opentelemetry::trace::TraceError>
│   │                 # for SparkError where applicable.
│   ├── guard.rs      # pub struct SparkGuard + Drop impl +
│   │                 # the per-provider force_flush helpers.
│   ├── init.rs       # pub(crate) fn init: the lint pass, then the
│   │                 # OTel SDK pipeline construction, then global-
│   │                 # provider set, then SparkGuard return.
│   └── observability.rs
│                     # pub(crate) helpers for tracing event emission:
│                     # spark::init succeeded, spark: shutdown
│                     # initiated, etc. Centralises target="spark"
│                     # vocabulary so renames are one-file edits.
├── examples/
│   ├── send_one_span_grpc.rs
│   │                 # Slice 01 demo. Runs against a developer's local
│   │                 # Aperture instance.
│   ├── send_one_span_with_house_attrs.rs
│   │                 # Slice 03 demo. Same shape with all four house
│   │                 # attrs set.
│   └── trigger_init_errors.rs
│                     # Slice 02 demo. Walks the SparkError variants.
└── tests/
    ├── slice_01_walking_skeleton.rs
    ├── slice_02_init_error_paths.rs
    ├── slice_03_feature_flags_and_experiment.rs
    ├── slice_04_env_var_precedence.rs
    ├── slice_05_logs_and_metrics.rs
    ├── slice_06_flush_deadline.rs
    ├── invariant_single_init.rs
    └── invariant_no_telemetry_on_telemetry.rs
```

The split-from-day-one decision is mechanical: file boundaries match
the conceptual boundaries the journey-spark already names (Configure,
Lint, Initialise SDK, Emit telemetry, Shutdown / flush) plus the cross-
cutting `target="spark"` tracing vocabulary. Splitting after the fact
would require touching every test in subsequent slices.

The `[[test]]` declarations in `Cargo.toml` enumerate each integration
test by name (the harness's and Aperture's pattern). One test per slice
plus two cross-cutting invariant tests (single-init, no-telemetry-on-
telemetry — these are CI invariants the registry names; they ride
their own `[[test]]` declarations so they appear as named CI-step
output rows).

### Cargo.toml skeleton

```toml
[package]
name = "spark"
version = "0.1.0"
edition.workspace = true
license = "Apache-2.0"   # SDK class per LICENSING.md
rust-version.workspace = true
description = "Kaleidoscope's Apache-2.0 Rust SDK: OTel SDK + OTLP exporter pre-configured with Kaleidoscope's house resource attributes, init-time required-attribute lint, OTel-canonical env-var precedence, bounded synchronous flush on guard drop."
repository = "https://github.com/andrealaforgia/kaleidoscope"
publish = false   # v0 is in-tree only; crates.io publication is post-v0

[lib]
path = "src/lib.rs"

[dependencies]
# Per ADR-0013 (dependency pinning).
opentelemetry = "=0.27"
opentelemetry_sdk = { version = "=0.27", features = ["trace", "logs", "metrics"] }
opentelemetry-otlp = { version = "=0.27", default-features = false, features = ["grpc-tonic", "trace", "logs", "metrics"] }
opentelemetry-semantic-conventions = "=0.27"
thiserror = "2"
tracing = "0.1"
url = "2"

[dev-dependencies]
# Per ADR-0011 §"Internal layout" + technology-choices.md.
aperture = { path = "../aperture", version = "0.1.0" }
tokio = { version = "1.40", features = ["full"] }
tracing-subscriber = { version = "0.3", default-features = false, features = ["fmt", "json", "env-filter", "registry"] }
serde_json = "1"
serial_test = "3"

[[example]]
name = "send_one_span_grpc"
path = "examples/send_one_span_grpc.rs"

[[example]]
name = "send_one_span_with_house_attrs"
path = "examples/send_one_span_with_house_attrs.rs"

[[example]]
name = "trigger_init_errors"
path = "examples/trigger_init_errors.rs"

[[test]]
name = "slice_01_walking_skeleton"
path = "tests/slice_01_walking_skeleton.rs"

[[test]]
name = "slice_02_init_error_paths"
path = "tests/slice_02_init_error_paths.rs"

[[test]]
name = "slice_03_feature_flags_and_experiment"
path = "tests/slice_03_feature_flags_and_experiment.rs"

[[test]]
name = "slice_04_env_var_precedence"
path = "tests/slice_04_env_var_precedence.rs"

[[test]]
name = "slice_05_logs_and_metrics"
path = "tests/slice_05_logs_and_metrics.rs"

[[test]]
name = "slice_06_flush_deadline"
path = "tests/slice_06_flush_deadline.rs"

[[test]]
name = "invariant_single_init"
path = "tests/invariant_single_init.rs"

[[test]]
name = "invariant_no_telemetry_on_telemetry"
path = "tests/invariant_no_telemetry_on_telemetry.rs"
```

The workspace `Cargo.toml` adds `crates/spark` to `[workspace] members`
and (optionally) declares `opentelemetry`, `opentelemetry_sdk`,
`opentelemetry-otlp`, `opentelemetry-semantic-conventions` at workspace
level for future crates (Codex, Sieve) to inherit. ADR-0013 §1 details.

### CI gates (mirrored from ADR-0005, scoped to `crates/spark/**`)

Five blocking gates, identical mechanism to the harness:

1. `cargo test --workspace --all-targets --locked` — runs Spark's unit
   tests, integration tests, doc-tests; the harness's and Aperture's
   tests still run too because the workspace target is the same.
2. `cargo public-api --diff-git-checkouts main HEAD -p spark` — locks
   the public surface above. Empty diff is the steady state; any change
   requires a version bump in the same commit.
3. `cargo semver-checks check-release -p spark --baseline-rev main` —
   SemVer-aware compatibility. Variants on `SparkError`, builder methods
   on `SparkConfig`, fields on the public structs are the load-bearing
   surface.
4. `cargo deny check` — licence policy + advisories + pin policy. The
   workspace's `deny.toml` (already authored for the harness, extended
   for Aperture) covers Spark's runtime closure verbatim.
5. `cargo mutants --package spark --in-diff` (DEVOPS workflow:
   `gate-5-mutants-spark`). 100% kill rate per ADR-0005 Gate 5.

The five gates execute in any order; they are independent. DEVOPS
chooses the runner specifics (`gate-5-mutants-spark.yml` mirrors
`gate-5-mutants-aperture.yml`).

## Alternatives Considered

### Option A — Free `pub fn init` in `lib.rs`, modules-from-day-one (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Smallest possible public surface for the contract DISCUSS locks (one function; three types — config, guard, error).
- Idiomatic for an init-and-return-RAII-guard library (`tracing-subscriber::fmt().init()`, `env_logger::init()`, `simplelog::TermLogger::init()` all share this shape).
- `cargo public-api` keeps the surface stable with one-line manifest assertions.
- Zero construction overhead at the call site.
- The internal-module split lets each concept (config, error, guard, init, observability) live in its own file from day one.

**Cons**:
- A future `spark::init_with_subscriber(config, subscriber)` parallel constructor would need either a parallel `pub fn` or a builder on `SparkConfig` (e.g. `SparkConfig::with_subscriber(...)`). Acceptable: the v0 contract is one entry point; configurability hangs off the builder.

### Option B — Trait-based plugin (`pub trait SparkProvider`)

```rust
let _guard = MyApp::init_telemetry()?;
```

**Pros**:
- Maximum flexibility for an application to swap out parts of the SDK.

**Cons**:
- Resume-driven over-engineering for a v0 SDK with one consumer pattern. DISCUSS `wave-decisions.md > D3 Rejected alternative 3` already rejects this shape: "the crafter agent's data + free functions + traits paradigm forbids this shape" for a struct-plus-builder-sufficient case.
- Forces every consumer to learn one Spark abstraction before touching the OTel SDK. Spark's value proposition is the *opposite*: minimum new surface so existing OTel SDK knowledge transfers.

**Rejected** for premature abstraction, per the same rationale harness ADR-0001 rejects its Option C (Harness builder).

### Option C — Re-export OTel types from `spark::*`

```rust
pub use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
// ... etc.
```

**Pros**:
- Application code can `use spark::*` and have everything it needs.

**Cons**:
- Shadows the upstream type-path. Every consumer that depends on Spark **and** another OTel crate (e.g. `tracing-opentelemetry`) sees two paths to the same type and the compiler's "type mismatch" errors become baffling.
- Breaks the harness's ADR-0003 §1 contract: "consumers depend on `opentelemetry-proto` directly to keep the dependency edge visible". Spark must obey the same rule.
- Bumping the OTel SDK family pin then forces Spark's own `pub use` lines to change in tandem. The current scheme (no re-exports) keeps the upstream dependency edge crisp.

**Rejected**.

### Option D — Single-file flat `lib.rs`

**Pros**:
- One file to read.

**Cons**:
- Six slices' worth of code in one file ≈ 600+ lines covering five distinct concerns. Splitting after the fact is mechanical but touches every test file's `use` paths. Same rationale that rejected this option in harness ADR-0001 §"Option D".

**Rejected**.

## Consequences

### Positive

- Public surface is exactly four items (`init`, `SparkConfig`, `SparkError`, `SparkGuard`). `cargo public-api` keeps it locked.
- Internal modules align with the journey's five backbone activities + cross-cutting `target="spark"` tracing vocabulary.
- The `[dev-dependencies]` posture for `aperture` is the canonical Apache-2.0-protecting idiom; Gate 4 (`cargo deny check`) prevents accidental promotion to runtime.
- The five `[[test]]`-declared slice tests + two invariant tests give DEVOPS clean per-slice CI step boundaries (one row per slice in the runner's UI).

### Negative

- The `init` module is a thin shim today (lint pass + SDK construction + global-set + guard return). The shim's existence is justified by keeping `lib.rs` documentation-only (a one-page front door explaining what Spark is).
- The internal-module split commits the crafter to per-concept boundaries before the implementation lands. Acceptable: harness ADR-0001 made the same trade-off and the harness's per-module tests landed cleanly.

### Trade-off ATAM

This decision is a **sensitivity point** for **Maintainability —
Modifiability** (positive: minimum surface, easy to evolve via additive
builder methods + non-exhaustive enums) and for **Functional Suitability
— Appropriateness** (positive: matches the call-site shape every Rust
OTel-instrumenting application wants).

It is a sensitivity point for **Compatibility — Interoperability** because
the no-re-exports rule forces consumers to depend on `opentelemetry`
directly, which is the right answer for upstream dependency hygiene
and the wrong answer for "I want one crate to import". The trade is
accepted because the upstream-hygiene benefit dominates: every other
Kaleidoscope crate (Aegis, Loom, Codex, Sieve) will also depend on
the OTel crates directly, and Spark must not become a redirection
layer.

## Self-Application of Earned Trust (principle 12)

The public-surface contract is enforced by three mechanisms (the
ArchUnit-style three-layer pattern from the agent's principle 12):

1. **Subtype check (compile-time)** — `cargo public-api` (Gate 2) reads
   the type-checked surface and fails the build on any drift not
   accompanied by a version bump.
2. **Structural check (CI)** — `cargo semver-checks` (Gate 3) walks
   the SemVer rules: removed variants, signature changes, narrowed trait
   bounds. A SemVer minor bump that should have been major fails Gate 3.
3. **Behavioural check (CI)** — the integration tests under `tests/`
   exercise the public surface via the OTel API and a real Aperture.
   A surface change that compiles and is semver-clean but breaks the
   contract (e.g. the `SparkConfig::for_service` constructor stops
   accepting the realistic test data the integration suite drives) is
   caught by Gate 1.

A change that bypasses one layer is caught by another. There is no
scenario where the public-surface contract erodes silently.

The dev-dep-not-runtime-dep contract for `aperture` is similarly
enforced: Gate 4 (`cargo deny`) refuses any commit that lists `aperture`
under `[dependencies]`. The mechanism is the licence list — `cargo deny`
already rejects `AGPL-3.0-or-later` in the runtime closure, so the
licence policy IS the structural enforcement of the dev-dep contract.
