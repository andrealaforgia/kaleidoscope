# Upstream issue stub — `opentelemetry-proto`: split `messages-only` from SDK-bringing features

> **Purpose**: a paste-ready issue body for the upstream
> OpenTelemetry Rust project, requesting a feature-gate split so that
> consumers who want only the prost-generated message types can avoid
> the transitive `opentelemetry_sdk` build-graph dependency.
>
> **Where to file**: the canonical home for the `opentelemetry-proto`
> Rust crate at the time of writing is the
> [`open-telemetry/opentelemetry-rust`](https://github.com/open-telemetry/opentelemetry-rust)
> repository (the `opentelemetry-proto` crate sits under
> `opentelemetry-proto/` in that monorepo for v0.27.x). If upstream
> has since moved the crate to
> [`open-telemetry/opentelemetry-rust-contrib`](https://github.com/open-telemetry/opentelemetry-rust-contrib)
> or another location, file there instead.
>
> **Status**: not blocking on the harness's DEVOPS wave. File when
> convenient.

---

## Title

> Feature-gate split: separate `messages-{logs,trace,metrics}` from `{logs,trace,metrics}` to avoid transitive SDK dependency for prost-only consumers

## Body (paste-ready)

### Summary

The current feature schema for `opentelemetry-proto` v0.27.x (and
likely later) ties the `logs`, `trace`, and `metrics` features to
both:

1. The prost-generated message types (e.g.
   `ExportLogsServiceRequest`), which is the part most consumers
   want; and
2. Re-exports / dependencies on `opentelemetry` and
   `opentelemetry_sdk`, which most consumers do not want when they
   are validating, decoding, or transforming wire bytes outside the
   SDK pipeline.

Concretely, in `opentelemetry-proto/Cargo.toml` v0.27.0:

```toml
[features]
gen-tonic-messages = ["tonic", "prost"]
logs    = ["opentelemetry/logs",    "opentelemetry_sdk/logs"]
trace   = ["opentelemetry/trace",   "opentelemetry_sdk/trace"]
metrics = ["opentelemetry/metrics", "opentelemetry_sdk/metrics"]
```

Enabling any of `logs`, `trace`, `metrics` is required to expose the
corresponding `Export*ServiceRequest` types (they are gated behind
`#[cfg(feature = "logs")]` etc. in `src/proto.rs`), but doing so
also pulls `opentelemetry_sdk` into the consumer's build graph.

### Why this matters

Several legitimate use cases need only the message types:

- Wire-format validation harnesses and conformance test rigs.
- Telemetry routers and gateways that decode-then-forward without
  invoking the SDK pipeline.
- Backends that accept OTLP and re-encode into a different format.
- FFI shims and language-binding crates that expose the message
  types to non-Rust callers.

For these consumers, the `opentelemetry_sdk` transitive bring-in is
pure build-time cost (extra crates compiled, extra dependency
surface, extra lock-file pinning concerns) with zero runtime
benefit. Dead-code elimination at link time removes the SDK code
from the final binary — but the build-time cost is real and the
licence audit surface (every transitive crate must be reviewed)
grows unnecessarily.

### Proposed change

Split each of `logs`, `trace`, `metrics` into a `messages-*` and
an `sdk-*` feature pair, with the existing umbrella feature
re-defined as the union of both:

```toml
[features]
gen-tonic-messages = ["tonic", "prost"]

# New: messages-only feature gates. Expose the prost-generated
# Export*ServiceRequest types without pulling the SDK.
messages-logs    = ["gen-tonic-messages"]
messages-trace   = ["gen-tonic-messages"]
messages-metrics = ["gen-tonic-messages"]

# New: SDK-only feature gates (or kept as-is, as preferred upstream).
sdk-logs    = ["opentelemetry/logs",    "opentelemetry_sdk/logs"]
sdk-trace   = ["opentelemetry/trace",   "opentelemetry_sdk/trace"]
sdk-metrics = ["opentelemetry/metrics", "opentelemetry_sdk/metrics"]

# Existing umbrella features, redefined as the union (preserves
# backwards compatibility for current consumers).
logs    = ["messages-logs",    "sdk-logs"]
trace   = ["messages-trace",   "sdk-trace"]
metrics = ["messages-metrics", "sdk-metrics"]
```

The corresponding `#[cfg(feature = "logs")]` gates in `src/proto.rs`
would loosen to `#[cfg(any(feature = "messages-logs", feature = "sdk-logs"))]`
or, equivalently, the umbrella feature continues to gate the same
modules and the new `messages-*` features just imply the umbrella
without the SDK side.

This change is **additive and non-breaking**: every consumer that
currently writes `features = ["logs"]` continues to get the same
build graph and the same message types. New consumers who want only
the messages can write `default-features = false, features = ["messages-logs"]`
and avoid `opentelemetry_sdk`.

### Concrete consumer affected

The CC0-licensed
[`otlp-conformance-harness`](https://github.com/andrealaforgia/kaleidoscope/tree/main/crates/otlp-conformance-harness)
crate, part of the Kaleidoscope project, validates OTLP byte
sequences against the OTLP wire spec. It is a leaf dependency for
the rest of Kaleidoscope and (the project hopes) a small
contribution to the wider OTel ecosystem: third-party
implementations can use it to verify their own emitter conformance
without reading the OTLP specification themselves.

The harness's `Cargo.toml` declares:

```toml
opentelemetry-proto = {
  version = "=0.27.0",
  default-features = false,
  features = ["gen-tonic-messages", "logs", "trace", "metrics"]
}
```

Per the harness's
[ADR-0003](https://github.com/andrealaforgia/kaleidoscope/blob/main/docs/product/architecture/adr-0003-opentelemetry-proto-pinning-policy.md),
the stated intent of `default-features = false` plus the explicit
feature list is to "avoid pulling in tonic / tokio / hyper as a
build dependency just for type definitions". `tokio` and `hyper`
indeed stay out, but `opentelemetry_sdk` does not, because the
`logs` / `trace` / `metrics` features pull it transitively. This
issue's proposal would let the harness drop the SDK from its build
graph entirely.

### Backwards compatibility

The proposal preserves the existing umbrella features (`logs`,
`trace`, `metrics`) as the union of the new pair. Every existing
consumer's `features = ["logs"]` declaration continues to work
unchanged. The change can ship in a minor version (0.x.y → 0.x.(y+1))
under SemVer because nothing breaks.

### Willingness to contribute

If the maintainers find the proposal acceptable, the Kaleidoscope
project is willing to submit a draft pull request. The harness's
licence (CC0-1.0) is more permissive than `opentelemetry-proto`'s
(Apache-2.0), so any contribution can be made under whatever
licensing posture the maintainers prefer.

### Cross-references

- ADR-0003 (Kaleidoscope project, public): pinning policy and stated
  feature-set intent.
- DELIVER wave decisions, Q1 (Kaleidoscope project, public):
  documents Crafty's investigation that surfaced the constraint.

Thank you for considering the request.

---

## Internal notes (not part of the issue body)

- The harness currently accepts the SDK transitive bring-in (Crafty's
  Q1 in DELIVER wave-decisions). Crate compiles green, all 73 tests
  pass, `cargo deny check` passes (`deny.toml` allows the duplicate
  versions the SDK introduces). This issue is a courtesy contribution,
  not a blocker.
- If upstream rejects the proposal, no action is needed on the
  Kaleidoscope side. The harness's contract holds either way.
- If upstream accepts the proposal and ships it in a future
  minor / patch release, the harness's `Cargo.toml` will be updated
  to use the new `messages-*` features and `deny.toml`'s
  `multiple-versions = "allow"` relaxation may be tightenable. That
  would be a separate small PR in this repository, gated by the same
  five ADR-0005 gates.
