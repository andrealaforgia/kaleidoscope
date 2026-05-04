# ADR-0003 — `opentelemetry-proto` dependency pinning policy

- **Status**: Accepted
- **Date**: 2026-05-03
- **Author**: `nw-solution-architect` (Morgan)
- **Supersedes**: none
- **Superseded by**: none

## Context

The harness depends on the upstream `opentelemetry-proto` crate (Apache-2.0) as the source of OTLP message types and `prost`-generated decoders. The shared-artefacts registry (`otlp_wire_format`) classifies version drift between the harness and its consumers as **HIGH integration risk**. The wave-decisions risk table flags upstream breaking changes between Phase 0 and Phase 1 as a real risk too.

`opentelemetry-proto` is a Rust crate published to crates.io that mirrors the protobuf schemas at `github.com/open-telemetry/opentelemetry-proto`. Its version progression follows OpenTelemetry's spec versioning loosely: minor versions of the crate may correspond to spec MINOR bumps, but the crate is a 0.x series, which means **MINOR bumps are breaking changes under Cargo's SemVer rules** (`0.27 -> 0.28` is a major-equivalent bump for Cargo).

Three things must be decided:

1. **What pin range** does the harness use for `opentelemetry-proto`?
2. **Where** is the spec version of record stored?
3. **What is the path to v1** if the chosen pin policy turns out to be unsuitable?

## Decision

### 1. Pin range — exact-version pin in v0

```toml
# crates/otlp-conformance-harness/Cargo.toml
[dependencies]
opentelemetry-proto = { version = "=0.27.0", default-features = false, features = ["gen-tonic-messages"] }
```

The `=` operator is an **exact-version pin** in Cargo: only `0.27.0` is acceptable, not `0.27.1` or `0.28.0`. Cargo's lockfile already pins the exact version transitively, but the manifest constraint makes the requirement explicit and impossible to relax accidentally via `cargo update`.

The `default-features = false` plus the explicit `gen-tonic-messages` feature gate is deliberate: the harness uses only the prost-generated message types, not the gRPC client/server code. This keeps the harness's compile time small and avoids pulling in `tonic` (and thereby `tokio`, `hyper`) as a build dependency just for type definitions.

### 2. Spec version of record

The OTLP spec version is recorded in the crate's metadata table:

```toml
[package.metadata.kaleidoscope.otlp]
spec_version = "1.5.0"
proto_crate_version = "0.27.0"
```

The spec version is re-exported from `lib.rs` as a `pub const`:

```rust
pub const OTLP_SPEC_VERSION: &str = "1.5.0";
```

The corpus runner (US-07) reads this constant and refuses to run vectors whose declared `spec_version` in `.expected.json` differs (per `shared-artifacts-registry.md > otlp_spec_version`).

### 3. Path to v1 if the policy proves wrong

The exact-version pin is **explicitly a v0 choice**. If maintaining the pin proves painful in practice (e.g. the upstream cuts releases with critical bug fixes faster than the harness can absorb them; security advisories appear), v0.1 may relax to a caret pin (`^0.27`) — but only after:

- The corpus is mature enough to catch silent semantic changes (i.e. an upstream MINOR bump that subtly changes accept-path behaviour). The corpus (US-07) is what makes this regression-detectable.
- A workspace-level `cargo metadata` consistency check exists ensuring every Kaleidoscope crate using `opentelemetry-proto` pins the same version. (`shared-artifacts-registry.md > otlp_wire_format` flags this requirement; in v0 the harness is the only consumer so the check is a no-op.)

If exact-version pinning proves *too lax* (e.g. upstream re-publishes `0.27.0` with different behaviour — the yanked-and-reissued scenario), v0.2 may switch to **vendoring**: the protobuf schemas are checked into the repository, and a build script generates the prost types in-tree. This is the heaviest, most stable option and is held in reserve.

The full progression is:

| Phase | Pin policy | Trigger to escalate |
|---|---|---|
| v0 | `=0.27.0` exact pin | (current) |
| v0.x | `^0.27.0` caret pin | Upstream cuts security or correctness fixes the harness needs faster than exact-pin allows |
| v1+ | Vendored protos, in-tree codegen | Yanked-and-reissued scenario, or upstream maintenance lapses, or operator-controlled spec version is required |

Each escalation is a new ADR superseding this one.

### 4. CI enforcement

`cargo deny` (recommended dependency in ADR-0005) verifies:

- `opentelemetry-proto` is pinned via `=` (no caret, no tilde, no wildcard) in v0.
- The Apache-2.0 licence is acceptable.
- No transitive dependency is on the disqualified-licence list (BSL/SSPL/FSL/RSAL per roadmap section A.1).

## Alternatives Considered

### Option A — Exact-version pin (`=0.27.0`) (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Maximally predictable: every commit builds against exactly one upstream version, no implicit drift.
- Cargo's lockfile already does this; the manifest pin makes it explicit and impossible to relax via `cargo update --aggressive`.
- The corpus is the regression check. With the corpus's hash invariants and the rule-coverage check, any silent semantic change in upstream produces an observable test failure.
- Aligns with the roadmap's stated approach for the `opentelemetry-proto` dependency: a deliberately-bumped pin.

**Cons**:
- Manual upgrade overhead: the harness maintainer must explicitly bump the pin and re-run the corpus. Acceptable for a Phase-0 leaf dependency that does not need rapid upstream uptake.
- A CVE in the pinned version forces an immediate bump-and-corpus-revalidation cycle. Acceptable: the corpus revalidation is a `cargo test` invocation, not a multi-day human-engineering campaign.

### Option B — Caret pin tracking the upstream MINOR version (`^0.27`)

```toml
opentelemetry-proto = { version = "^0.27", ... }
```

**Pros**:
- Free upstream patch bumps. Bug fixes in `0.27.1` flow in without manual action.

**Cons**:
- For a 0.x crate, **caret with no patch number is equivalent to `=0.27` because Cargo treats 0.x MINOR as breaking**. So the apparent looseness of `^0.27` is illusory for 0.x crates and does not buy the supposed flexibility.
- For `^0.27.0` (with the patch number), Cargo allows `0.27.x` for any `x`, which **does** allow patch bumps. But this is precisely where silent regressions could enter — and the corpus does not exist until US-07, so for slices 01..06 there is no regression net.
- For a 1.x crate the caret would be (`^1.0`) and would allow MINOR bumps automatically; the harness cannot reach 1.x because upstream `opentelemetry-proto` itself is on 0.x.

**Rejected for v0** but documented as the natural v0.x escalation once the corpus is in place to catch silent regressions. The v0 corpus arrives only at slice 07, so for the duration of slices 01–06 a caret pin would be unsafe; by the time the corpus exists, the pin is effectively locked anyway. Switching is a future ADR.

### Option C — Vendored protos with in-tree codegen

```
crates/otlp-conformance-harness/
├── proto/
│   ├── opentelemetry/proto/common/v1/common.proto    # vendored copy
│   ├── opentelemetry/proto/resource/v1/resource.proto
│   └── opentelemetry/proto/{logs,trace,metrics}/v1/*.proto
├── build.rs            # uses prost-build to generate types into OUT_DIR
└── src/lib.rs          # includes the generated code via include!(concat!(env!("OUT_DIR"), "/..."))
```

**Pros**:
- Maximally stable: the harness controls the proto schema source-of-truth byte-for-byte.
- Immune to upstream yank-and-reissue scenarios.
- Operator-controlled spec version (the `proto/` directory is the source).

**Cons**:
- **Violates US-04 AC 2.** US-04 requires the accept-path return type to be exactly `opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest` — the *upstream* type-path. Vendoring generates types under a harness-local path (e.g. `otlp_conformance_harness::generated::ExportLogsServiceRequest`), which is exactly what US-04 AC 2 forbids.
- Ships a heavy build step (`prost-build` runs `protoc`) that complicates the `cargo build` story — and `protoc` is a system dependency that not every CI environment has.
- Adds a non-trivial governance burden: who decides when to re-vendor? Re-vendoring is an opportunity for human error.
- Doubles the maintenance surface: when upstream OpenTelemetry releases a new spec, the vendored protos must be re-vendored *and* the version-of-record bumped.

**Rejected for v0** because it directly contradicts the locked US-04 AC 2 contract. Reserved as a v1+ option if the upstream relationship deteriorates badly enough to justify breaking the type-path identity contract. The break would require a major version bump of the harness and an explicit migration story for every consumer; this is a heavy lever, kept in reserve only.

### Option D — No pin, depend on whatever Cargo resolves

**Pros**:
- Zero maintenance.

**Cons**:
- Builds are non-reproducible across machines and times. Catastrophic.
- KPI 1 (zero false positives) is unenforceable if the build resolves a different upstream on different runs.
- No known professional Rust project does this for any non-trivial dependency.

**Rejected** outright.

## Consequences

### Positive

- Builds are reproducible: every commit builds against `opentelemetry-proto = 0.27.0` exactly.
- The integration risk identified in `shared-artifacts-registry.md > otlp_wire_format` is mitigated for v0: there is no version drift possible.
- The escalation path to v0.x and v1+ is documented; future maintainers see why exact pinning was chosen and what would justify changing it.
- `cargo deny` catches accidental relaxation of the pin in PR review.

### Negative

- Manual upgrade ceremony: when upstream cuts a new version, a maintainer must update the pin, the spec-version constant, and the corpus's `.expected.json` `spec_version` fields. Mitigated by the rarity of OTLP spec bumps.
- New consumers adopting the harness must coordinate their own `opentelemetry-proto` pin to match. Mitigated because the harness re-exports the relevant types' paths via documentation; a future workspace-level `cargo metadata` check (deferred per `shared-artifacts-registry.md`) would automate verification.

### Trade-off ATAM

This decision is a sensitivity point for **Reliability — Maturity** (positive: pinned dependencies make accidents impossible) and **Compatibility — Interoperability** (positive: every consumer of the harness sees a consistent OTLP type version; conditional on consumers themselves pinning compatibly, which the future workspace check addresses).

It is a trade-off point against **Performance Efficiency — Time Behaviour for Maintainers** (negative: manual upgrade ceremony) — accepted because this attribute is inherently subordinate to KPI 1 (zero false positives), which the pinning guarantees.
