# ADR-0013 — `spark` dependency pinning policy

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `spark` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

Spark v0's dependency tree is the load-bearing engineering decision for
its forward-compatibility story. The harness's exact-pin policy
(ADR-0003: `opentelemetry-proto = "=0.27.0"`) is the substrate the
entire integration plane relies on; Spark's runtime crates **must
co-resolve** with that pin or Spark's wire bytes will not be decodable
by the harness Aperture runs.

DISCUSS `wave-decisions.md > Risks` flags three concerns:

1. `opentelemetry-rust` API breaking change before Phase 1
   (`opentelemetry-otlp` is not yet 1.0; minor versions can break).
   Mitigation: pin to a single minor version compatible with
   `opentelemetry-proto =0.27.0`.
2. `opentelemetry-rust` and `opentelemetry-otlp` MSRV diverge from
   Kaleidoscope's `rust-version = "1.88"`. Mitigation: workspace MSRV
   is already 1.88; `opentelemetry-otlp` 0.27 supports MSRV 1.75. No
   conflict at v0 lock-time.
3. The runtime tree must be Apache-2.0 / MIT / BSD only; AGPL is
   forbidden in runtime. The dev-dep `aperture` is the only AGPL crate
   Spark touches, and it must remain dev-only.

Sentinel's peer-review (`discuss/peer-review.md > Suggestions for
Morgan §2`) explicitly directs DESIGN to "Lock `opentelemetry-otlp`
minor version in a DESIGN ADR mirroring harness ADR-0003. Name the
migration path if a future minor version breaks compatibility."

This ADR is that mirror.

## Decision

### 1. OTel SDK family pin — exact-minor pin in v0

```toml
# crates/spark/Cargo.toml
[dependencies]
opentelemetry = "=0.27"
opentelemetry_sdk = { version = "=0.27", features = ["trace", "logs", "metrics"] }
opentelemetry-otlp = { version = "=0.27", default-features = false, features = ["grpc-tonic", "trace", "logs", "metrics"] }
opentelemetry-semantic-conventions = "=0.27"
```

The `=0.27` operator pins all four crates to **the 0.27 minor series**
(any 0.27.x patch is acceptable; 0.28.0 is not). Cargo's lockfile
already pins the exact patch transitively (`Cargo.lock` carries
`opentelemetry 0.27.1` at the time of this ADR via Aperture's and the
harness's transitive resolution).

The `=0.27` (rather than `=0.27.0`) deliberately diverges from harness
ADR-0003's `=0.27.0` exact-patch pin. The reason: Spark depends on the
OTel SDK family which the upstream releases as a coordinated minor
bump (all four crates ship together at the same minor version). A
patch-level upstream fix to `opentelemetry-otlp 0.27.x` is exactly the
sort of bug fix Spark wants to absorb without ceremony, and `Cargo.lock`
provides the reproducibility guarantee at the workspace level. The
harness's exact-patch pin is right for the harness because the harness
defends the wire format byte-for-byte; Spark's job is to *use* the
SDK, and the SDK's patches are pure bug fixes.

### Why 0.27 specifically — the transitive lock with the harness

The OTel Rust SDK ecosystem ships in a single minor cadence: a 0.27
release across `opentelemetry`, `opentelemetry_sdk`,
`opentelemetry-otlp`, `opentelemetry-proto`,
`opentelemetry-semantic-conventions`, and `opentelemetry-stdout` all
land together. The harness pins `opentelemetry-proto =0.27.0`; the
co-resolved family is therefore 0.27.

`Cargo.lock` evidence at the time of this ADR:
- `opentelemetry 0.27.1` (transitive via Aperture's `opentelemetry-proto`
  consumption already pulled this into the lock)
- `tonic 0.12.3` (transitive via `opentelemetry-otlp 0.27` and
  `opentelemetry-proto 0.27.0`)

Spark's pins co-resolve without a lockfile churn.

### Feature flags — explicit and minimal

```toml
opentelemetry_sdk = { version = "=0.27", features = ["trace", "logs", "metrics"] }
opentelemetry-otlp = { version = "=0.27", default-features = false, features = ["grpc-tonic", "trace", "logs", "metrics"] }
```

- `opentelemetry_sdk` features `trace`, `logs`, `metrics` enable the
  three signal-type providers Slice 05 lights up. `default-features`
  for `opentelemetry_sdk` 0.27 enables `trace` only; explicit feature
  list gives logs+metrics from day one (Slice 05 needs them; Slices
  01–04 do not exercise them but the providers are wired in init).
- `opentelemetry-otlp` `default-features = false` strips the HTTP/JSON
  feature set (`http-json`, `http-proto`) and the synchronous client
  feature set. The explicit `grpc-tonic` is the v0 default transport
  per DISCUSS Q1; if a future application wants HTTP/protobuf, it
  passes `OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf` and Spark
  re-enables the `http-proto` feature in v0.1+ (additive feature flag
  is non-breaking).

The minimum-feature posture matches harness ADR-0003's
`default-features = false, features = ["gen-tonic-messages"]` rationale:
keep the compile time small, avoid pulling in code that v0 does not
exercise.

### 2. Semantic-conventions version verification

Sentinel's review flagged: "Confirm exact attribute names at the
harness's `opentelemetry-proto = 0.27.0` pin. If semconv diverges from
Spark's `feature_flag.*` choice, document the migration path."

The verification:

- **`service.name`** — `opentelemetry-semantic-conventions 0.27` exposes
  `SERVICE_NAME` as the constant for `"service.name"`. Spark uses this
  constant via `use opentelemetry_semantic_conventions::resource::SERVICE_NAME;`
  rather than stringifying the literal `"service.name"`. ✅ Aligned.
- **`tenant.id`** — NOT in OTel semconv 0.27. Spark v0 uses the literal
  `"tenant.id"` (Kaleidoscope-specific resource attribute, not in OTel
  semconv). The roadmap C.2 names `tenant.id` as a Kaleidoscope-house
  attribute; future OTel semconv may stabilise on a different name (e.g.
  `tenant.identifier`), in which case Codex Phase 0+ documents the
  migration path. v0's contract is the literal `"tenant.id"` matching
  the architecture document.
- **`feature_flag.*`** — OTel semconv 1.27 introduced
  `feature_flag.key`, `feature_flag.provider_name`, `feature_flag.variant`
  as **span-attribute** names for feature-flag evaluation events. Spark
  v0 uses `feature_flag.{key}` as **resource-attribute** names (one
  attribute per flag, prefix-namespaced with the developer's flag name).
  These two uses do **not** collide: OTel's feature-flag attributes
  describe a single evaluation event (one attribute per evaluation
  fact), Spark's feature-flag resource attributes describe the
  emitting-process-wide flag state (one attribute per flag).

  Rationale for keeping `feature_flag.{key}` at v0 (rather than
  switching to `feature_flag.key="checkout-v2"` per OTel semconv):
  resource-attribute prefixed naming is the cross-Kaleidoscope query
  contract (Loom Phase 2 reads `feature_flag.checkout-v2` directly as
  a resource-attribute filter). Switching to the semconv span-event
  model would force the operator to JOIN events to spans for every
  query, which is exactly the cross-attribute correlation the resource
  model is designed to avoid.

  **Migration path** (documented for Codex Phase 0+): if OTel semconv
  stabilises on a resource-level naming convention for feature flags
  (none exists at the harness's pinned 0.27.0 spec version), Spark
  v0.1+ adds an alias mode that emits both the v0 names (for backwards
  compatibility) and the OTel-semconv-aligned names. The alias is
  controlled by a `SparkConfig::with_semconv_compatibility(true)` builder
  method — additive, non-breaking. Codex's lint then chooses which
  vocabulary the corpus enforces.

- **`experiment.id`** — NOT in OTel semconv 0.27. Same posture as
  `tenant.id`: Kaleidoscope-house, literal name from the architecture
  document, future semconv alignment is Codex's domain.

The verification result: **Spark v0's `feature_flag.*`,
`tenant.id`, and `experiment.id` choices DO NOT diverge from OTel
semconv 0.27 because OTel semconv 0.27 has no resource-level naming
for these concepts**. Spark's choices are forward-compatible: a future
semconv version that introduces resource-level conventions for tenancy,
feature flags, or experiments can ship as an additive
`with_semconv_compatibility` mode in Spark v0.1+ without breaking the
v0 contract.

The `service.name` use is verified to use
`opentelemetry_semantic_conventions::resource::SERVICE_NAME` (the
constant, not the literal string) so any future spec rename in the
upstream crate flows through without a Spark code change.

### 3. Dev-dependency posture (the AGPL containment edge)

```toml
# crates/spark/Cargo.toml
[dev-dependencies]
aperture = { path = "../aperture", version = "0.1.0" }
```

`aperture` is `AGPL-3.0-or-later`. Listing it under `[dev-dependencies]`
keeps it out of the `[dependencies]` graph that ships with the crate.
`cargo deny check` (Gate 4 of ADR-0011) refuses Spark's
`AGPL-3.0-or-later` in the runtime closure; the licence policy IS the
containment mechanism.

The path-resolved + version-tagged dependency declaration is the
canonical Rust idiom for sibling-crate dev-deps that may eventually be
published. Aperture itself uses the same idiom for the harness
(`crates/aperture/Cargo.toml` lines 86-96). The `version = "0.1.0"`
satisfies `cargo deny`'s `bans.wildcards = "deny"` rule that
path-only declarations would violate.

**Forbidden**: any `[dependencies]` entry naming `aperture`, any
`[target.'cfg(...)'.dependencies]` smuggle of `aperture` into the
runtime tree, any feature flag that conditionally pulls `aperture` as
a runtime dep. ADR-0011 §"CI gates" (specifically Gate 4 with the
licence policy) makes this enforceable.

### 4. MSRV — workspace floor

```toml
[package]
rust-version.workspace = true   # = 1.88
```

The workspace MSRV is 1.88 (per workspace `Cargo.toml`). Spark inherits
the floor; Spark itself does not pin a stricter MSRV.

Verification: `opentelemetry-otlp 0.27` declares MSRV 1.75 (per
upstream crate metadata). `tonic 0.12` declares MSRV 1.74. `thiserror
2.x` declares MSRV 1.61. `url 2.x` declares MSRV 1.63. `tracing 0.1`
declares MSRV 1.65. `serial_test 3` declares MSRV 1.74. None of Spark's
runtime or dev deps push the floor above 1.88; Spark's MSRV is
workspace-driven, not Spark-driven.

If a future runtime dep raises Spark's MSRV above the workspace floor,
the response (per the project memory note `feedback_msrv_creep_is_ecosystem_reality`)
is to bump the workspace floor, not to pin around it.

### 5. Migration path if the policy proves wrong

The exact-minor pin is **explicitly a v0 choice**. The full progression
is:

| Phase | Pin policy | Trigger to escalate |
|---|---|---|
| v0 | `=0.27` exact-minor pin (all four OTel crates) | (current) |
| v0.x | Bump to `=0.28` (etc.) when upstream releases | Upstream cuts a 0.28 with a feature Kaleidoscope needs (e.g. OTLP/HTTP/JSON stabilises). New ADR. |
| v1 | Caret pin `^1.0` once OTel hits 1.0 | OTel publishes a stable 1.0. The harness's `opentelemetry-proto` follows. New ADR. |

Each escalation is a new ADR superseding this one. Each escalation
must verify:

1. The new family co-resolves with the harness's `opentelemetry-proto`
   pin at that time.
2. The integration tests under `crates/spark/tests/` pass against the
   new family without any `unsafe`-attribute or feature-flag
   gymnastics.
3. `cargo deny check` accepts the transitive closure (no new licences
   the policy disallows).
4. Aperture's matching upgrade is either complete or scheduled (the
   wire bytes Spark emits must remain decodable by the Aperture
   running the same harness).

### 6. CI enforcement

`cargo deny check` (Gate 4 of ADR-0011) verifies on every commit:

- The OTel-family pins are exact-minor (`=0.27`) — `bans.wildcards = "deny"`
  + `bans.multiple-versions = "deny"`.
- `aperture` does NOT appear in the runtime closure — the licence
  policy refuses `AGPL-3.0-or-later` for any non-`[dev-dependencies]`
  edge.
- All runtime licences are in the allow-list (Apache-2.0, MIT,
  BSD-2/3-Clause, ISC, Apache-2.0 OR MIT, etc.).

The workspace's `deny.toml` is the same one already authored for the
harness (ADR-0005 Gate 4); it covers Spark's runtime closure verbatim.

## Alternatives Considered

### Option A — Exact-minor pin `=0.27` for the OTel family (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Co-resolves with harness ADR-0003's `opentelemetry-proto =0.27.0`.
- Patch fixes flow in via `cargo update` without manifest changes.
- Lockfile guarantees reproducibility regardless of patch level.
- Matches the upstream OTel ecosystem's release cadence (the four
  crates ship as a coordinated minor bump).

**Cons**:
- Manual upgrade ceremony when upstream cuts 0.28: the maintainer must
  bump four pins in lockstep. Acceptable: this is the Rust idiom for
  dependency family pinning.

### Option B — Exact-patch pin `=0.27.1` (mirror harness ADR-0003 exactly)

**Pros**:
- Maximally predictable: every commit builds against exactly one upstream
  patch version, no implicit drift.

**Cons**:
- A CVE in `opentelemetry-otlp 0.27.1` forces an immediate manifest
  bump even for a one-line patch fix that the lockfile would already
  pick up via `cargo update`. The lockfile already provides the
  reproducibility guarantee; the manifest pin would be redundant
  ceremony.
- Harness's exact-patch is right because the harness *defends* the
  wire format byte-for-byte; Spark's job is to *use* the SDK, where
  upstream patches are pure bug fixes. The two crates have different
  load-bearing properties.

**Rejected for v0** in favour of Option A. The lockfile gives the
reproducibility; the manifest pin's job is to refuse a 0.28 (which
would be a breaking change for Cargo's 0.x SemVer), not to refuse
0.27.2 (which is a patch).

### Option C — Caret pin `^0.27` for the OTel family

```toml
opentelemetry = "^0.27"
```

**Pros**:
- Zero maintenance until 0.28 is needed.

**Cons**:
- For a 0.x crate, `^0.27` is **equivalent to `=0.27`** under Cargo's
  SemVer rules (Cargo treats 0.x MINOR as breaking). So the apparent
  looseness is illusory and adds documentation confusion.

**Rejected** because the explicit `=0.27` is clearer about intent.

### Option D — Vendored OTel SDK / hand-rolled exporter

**Pros**:
- Maximum control.

**Cons**:
- Recreates a tens-of-thousands-of-lines upstream codebase that the
  Rust OTel community already maintains. Catastrophic engineering
  burden for zero quality-attribute benefit.
- The Apache-2.0 licence on the upstream is permissive; vendoring
  brings no licensing advantage.

**Rejected** outright.

## Consequences

### Positive

- Builds are reproducible: `cargo build --locked` resolves the same
  OTel family on every machine and every CI run.
- The `opentelemetry-proto =0.27.0` invariant the harness defends
  flows to Spark transparently; Spark's wire bytes are decodable by
  Aperture's harness because they share the same SDK family.
- `cargo deny check` enforces the AGPL containment; the dev-dep
  `aperture` cannot accidentally leak into the runtime tree.
- Patch fixes from upstream OTel land via `cargo update` without
  manifest churn.

### Negative

- Upgrading to OTel 0.28 (whenever upstream cuts it) is a four-pin
  manifest change. Acceptable: a new ADR documents the upgrade and
  the integration tests prove the new family works.
- The semconv divergence (Spark's `tenant.id`, `feature_flag.*`,
  `experiment.id` are not OTel-semconv vocabulary) is a documented
  forward-compat risk; Codex Phase 0+ owns the alignment.

### Trade-off ATAM

This decision is a **sensitivity point** for **Reliability — Maturity**
(positive: pinned upstream family makes accidents impossible at the
manifest level; lockfile makes them impossible at the build level)
and for **Compatibility — Interoperability** (positive: every consumer
of Spark sees a consistent OTel SDK version; downstream Aegis, Loom,
Codex, Sieve will inherit the same family).

It is a trade-off point against **Performance Efficiency — Time
Behaviour for Maintainers** (negative: manual upgrade ceremony when
upstream cuts a new minor) — accepted because this attribute is
inherently subordinate to wire-byte reliability, which the family pin
defends.

## Self-Application of Earned Trust (principle 12)

The pin policy is enforced by three orthogonal layers:

1. **Subtype check (compile-time)** — Cargo's resolver. `=0.27` in the
   manifest plus the lockfile pinning a specific patch means a wrong
   family member literally cannot link.
2. **Structural check (CI)** — `cargo deny check` (Gate 4) reads
   `Cargo.toml` and refuses any wildcard or non-exact pin. A future
   PR that loosens to `^0.27` or `*` is rejected at PR review.
3. **Behavioural check (CI)** — Slice 01's integration test runs a real
   `ExportTraceServiceRequest` through Spark + the OTel exporter +
   Aperture + the harness. A patch-level upstream that breaks wire
   compatibility would fail this test — the `opentelemetry-proto =0.27.0`
   invariant is defended end-to-end.

The AGPL containment is enforced by:

1. **Subtype check** — Rust's compilation model: a `[dev-dependencies]`
   crate is unavailable at `cargo build` time; only `cargo build --tests`
   pulls it in. A `use aperture::*` in `src/` would fail to compile in
   a non-test build.
2. **Structural check** — `cargo deny check` rejects any `AGPL-3.0-or-later`
   in the runtime closure. The licence policy IS the structural rule.
3. **Behavioural check** — none needed at v0; the licence policy + the
   compilation model are sufficient. If a behavioural check were ever
   required (e.g. a confused crate-name collision), the test would be
   "resolve `aperture` from `cargo metadata --filter-platform host`
   for the published artefact and assert it does not appear".
