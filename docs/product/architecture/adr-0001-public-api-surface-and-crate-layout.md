# ADR-0001 — Public API surface and crate layout for `otlp-conformance-harness`

- **Status**: Accepted
- **Date**: 2026-05-03
- **Author**: `nw-solution-architect` (Morgan)
- **Supersedes**: none
- **Superseded by**: none

## Context

The DISCUSS wave's user stories lock the function names and signatures of the three `validate_*` operations (US-06 AC 5, line 583 of `user-stories.md`):

```rust
validate_logs(bytes: &[u8], framing: Framing) -> Result<ExportLogsServiceRequest, OtlpViolation>
validate_traces(bytes: &[u8], framing: Framing) -> Result<ExportTraceServiceRequest, OtlpViolation>
validate_metrics(bytes: &[u8], framing: Framing) -> Result<ExportMetricsServiceRequest, OtlpViolation>
```

What DISCUSS does **not** lock is whether these operations are:

- standalone `pub fn`s in `lib.rs`, called directly;
- methods on a `Validator` struct constructed at call site;
- methods on a `&Harness` constructed once and reused.

DISCUSS also leaves open whether the crate's source is one flat `lib.rs` or split into modules from day one, and which (if any) items beyond the three functions and their associated types are part of the public surface.

The shared-artefacts registry (`crate_public_surface`) classifies any change to the public API as HIGH integration risk. The smaller and more conventional the surface, the safer.

## Decision

**Free `pub fn`s in `lib.rs`, with the crate split into named internal modules from day one.** No `Validator` struct, no `Harness` struct, no method dispatch.

### Public surface (final list)

```rust
// from lib.rs, alphabetised by item kind:

pub const OTLP_SPEC_VERSION: &str;

pub enum ByteOffset { Known(usize), Unknown }                         // #[non_exhaustive]
pub enum Framing { HttpProtobuf, GrpcProtobuf }                       // #[non_exhaustive]
pub enum Rule { EmptyInput, WireType(WireTypeRule) }                  // #[non_exhaustive]
pub enum SignalType { Logs, Traces, Metrics }                         // #[non_exhaustive]
pub enum WireTypeRule {                                               // #[non_exhaustive]
    ProtobufDecode,
    SignalMismatch { observed: SignalType, asserted: SignalType },
}

pub struct OtlpViolation { /* fields per ADR-0002 */ }

pub fn validate_logs(bytes: &[u8], framing: Framing) -> Result<...ExportLogsServiceRequest, OtlpViolation>;
pub fn validate_traces(bytes: &[u8], framing: Framing) -> Result<...ExportTraceServiceRequest, OtlpViolation>;
pub fn validate_metrics(bytes: &[u8], framing: Framing) -> Result<...ExportMetricsServiceRequest, OtlpViolation>;
```

That is the entire public surface. `lib.rs` does **not** re-export `opentelemetry_proto` or any of its modules — consumers depend on `opentelemetry-proto` directly to keep the dependency edge visible (and to satisfy US-04 AC 2's "no shadowing" rule).

### Internal layout

```
src/
├── lib.rs        # public surface only: re-exports the items above; thin pub fn delegates to validate.rs
├── framing.rs    # Framing enum + impls
├── signal.rs     # SignalType enum + impls
├── violation.rs  # OtlpViolation, Rule, WireTypeRule, ByteOffset
├── decode.rs     # pub(crate) fn decode_as_logs/traces/metrics; signal-mismatch fallback
└── validate.rs   # pub(crate) fn validate_logs/traces/metrics impl; lib.rs's fns are one-line wrappers
```

The split-from-day-one decision is mechanical: the file boundaries match the conceptual boundaries identified by the user stories (framing, signal, violation, decode, validation). Splitting after the fact would require touching every test in subsequent slices.

## Alternatives Considered

### Option A — Free `pub fn`s in `lib.rs`, split into internal modules (RECOMMENDED, accepted)

**Pros**:
- Smallest possible public surface for the contract the user stories require.
- Idiomatic Rust for a stateless validation library (matches `serde_json::from_slice`, `regex::Regex::is_match`, `prost::Message::decode`).
- Zero construction overhead at the call site — Aperture, Sluice, every storage engine just calls the function.
- No state to misconfigure; the function is referentially transparent in the framing argument.
- The internal-module split lets each concept (framing, signal, violation, decode) live in its own file from day one without leaking into the public surface.

**Cons**:
- Future configurability (e.g. "validate but cap allocation at 1 MiB", "validate with a custom decode-error mapper") would force adding either function arguments or a separate constructor-style API. Acceptable for v0; no such configurability is named in the user stories.
- Cannot inject a mock decoder for the harness's own tests. Not actually a problem because the harness's tests use real bytes (corpus vectors), not mocked decoders.

### Option B — Methods on a `Validator` struct constructed per call

```rust
let v = Validator::new();
let result = v.logs(bytes, Framing::HttpProtobuf);
```

**Pros**:
- Future configurability hangs off the constructor (e.g. `Validator::with_locus_strategy(...)`).
- Object-style API is familiar to consumers coming from non-Rust languages.

**Cons**:
- The `new()` call is dead code in v0 — the struct has no state.
- Adds a tier of indirection at the call site that conveys no information.
- Diverges from the locked function signatures in US-06 AC 5 unless those become methods, which renames the call from `validate_logs` to `Validator::new().logs` — a strict regression in clarity.
- Resume-driven structuring: it adds an OO veneer over a stateless function for no quality-attribute benefit.

**Rejected** because the construction step is dead weight today and the signatures locked by US-06 AC 5 read most naturally as free functions.

### Option C — Methods on a long-lived `&Harness` configured at construction time

```rust
let h = Harness::builder().spec_version("1.5.0").build();
h.logs(bytes, Framing::HttpProtobuf);
```

**Pros**:
- Allows per-instance configuration (e.g. spec-version override, custom locus mapper, future plugin points).
- One construction cost amortised over many calls.

**Cons**:
- Configurability the user stories do not require. The spec version is a compile-time constant; per-instance override is a feature without a customer.
- Builder pattern is overkill for a function with two arguments.
- Even worse divergence from the locked function signatures than Option B.
- Threads through every consumer's call site as either a global `static` (which is a service locator anti-pattern in Rust) or a parameter (which expands every consumer's signature).

**Rejected** for premature abstraction. If v1 grows real configurability, Option A's free functions can wrap a `pub fn validate_logs_with_options(bytes, framing, options)` form without breaking existing callers.

### Option D — Single-file flat `lib.rs` (modules deferred)

**Pros**:
- One file to read.
- Zero up-front module-split decisions.

**Cons**:
- Every later slice (US-02..US-07) would add code to the same file; by US-07 the file would be ~600 lines covering five distinct concerns.
- Splitting after the fact is mechanical but touches every test file's `use` statements.
- Module boundaries are useful as documentation; flat `lib.rs` denies the reader that signal.

**Rejected** because the modules-from-day-one cost is a 30-second decision now versus a refactor later.

## Consequences

### Positive

- The public surface is exactly the seven types, three functions, and one constant the user stories require — no more.
- The internal modules align with the conceptual decomposition the user stories already imply (framing, signal, violation, decode).
- `cargo public-api` (per ADR-0005) can lock the surface with a one-line manifest assertion.
- Consumers can call `validate_logs(bytes, framing)` directly from any code site; no `mut`, no thread-local, no construction step.
- Signature drift is impossible without bumping the crate's minor version (mechanism: `cargo public-api` + `cargo semver-checks` per ADR-0005).

### Negative

- Future configurability (if any) requires an `_with_options` parallel function rather than an option setter on a `Harness`. Acceptable: the design treats configurability as a future problem and the user stories name no v0 configurability.
- The internal `validate.rs` module is a thin shim today — `lib.rs::validate_logs` delegates to `validate::validate_logs`. The shim is a one-line function whose presence is justified by keeping `lib.rs` documentation-only.

### Trade-off ATAM

This decision is a sensitivity point for **Maintainability — Modifiability** (positive: minimum surface, easy to evolve via additive `_with_options` if needed) and for **Functional Suitability — Appropriateness** (positive: matches the call-site shape every consumer wants). It is not a trade-off point because no quality attribute is degraded by the choice.
