# Workspace Layout — adding `crates/aperture/` (DESIGN proposal)

> **Wave**: DESIGN (`nw-solution-architect` / Morgan).
> **Date**: 2026-05-04.
> **Author**: Morgan.
> **Companion documents**: `architecture-overview.md`, `component-design.md`.

This document specifies the workspace-layout change DELIVER must make. DESIGN does NOT touch `Cargo.toml` files; this document IS the proposal that DELIVER reads.

---

## Root `Cargo.toml` — diff

```diff
 [workspace]
 resolver = "2"
-members = ["crates/otlp-conformance-harness"]
+members = [
+    "crates/otlp-conformance-harness",
+    "crates/aperture",
+]
```

Add to `[workspace.dependencies]` (so future Phase-1 crates can adopt without re-deriving versions):

```toml
[workspace.dependencies]
# (existing entries kept)
opentelemetry-proto = { version = "=0.27.0", default-features = false, features = ["gen-tonic-messages", "logs", "trace", "metrics"] }
prost = "0.13"
sha2 = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Aperture additions:
tokio = { version = "1.40", features = ["full"] }
tonic = { version = "0.12", default-features = false, features = ["transport", "codegen", "prost"] }
axum = { version = "0.7" }
hyper = { version = "1.4" }
tower = { version = "0.5" }
tower-http = { version = "0.6", features = ["limit", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter", "fmt"] }
figment = { version = "0.10", features = ["toml", "env"] }
async-trait = "0.1"
thiserror = "1"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
```

Add (or extend) `[workspace.lints]`:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
print_stdout = "deny"
print_stderr = "deny"
unwrap_used = "warn"
expect_used = "warn"
```

(Forbid on `unsafe_code` already aligns with the harness's `#![forbid(unsafe_code)]` declaration; lifting it to workspace level removes the per-crate repetition for future crates.)

---

## `crates/aperture/Cargo.toml` — full file

```toml
[package]
name = "aperture"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "OTLP gateway. Validates byte sequences via otlp-conformance-harness and hands accepted records to a configurable sink."
repository = "https://github.com/andrealaforgia/kaleidoscope"
readme = "README.md"
publish = false

[lints]
workspace = true

[[bin]]
name = "aperture"
path = "src/main.rs"

[lib]
# Library surface is the test-doubles namespace only (`aperture::testing`).
# Integration tests under `tests/` consume both the binary's `compose::run`
# entry point and the test doubles.
path = "src/lib.rs"

[dependencies]
# Substrate (workspace-aligned)
opentelemetry-proto = { workspace = true }
prost = { workspace = true }

# Async runtime + HTTP/gRPC servers
tokio = { workspace = true }
tonic = { workspace = true }
axum = { workspace = true }
hyper = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }

# Observability
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

# Configuration
serde = { workspace = true }
figment = { workspace = true }

# Trait machinery
async-trait = { workspace = true }
thiserror = { workspace = true }

# Outbound HTTP for ForwardingSink
reqwest = { workspace = true }

# Sibling crate
otlp-conformance-harness = { path = "../otlp-conformance-harness" }

[dev-dependencies]
# Real OTel SDK for integration tests
opentelemetry = "0.27"
opentelemetry_sdk = { version = "0.27", features = ["rt-tokio", "logs", "trace", "metrics"] }
opentelemetry-otlp = { version = "0.27", features = ["grpc-tonic", "http-proto", "logs", "trace", "metrics", "reqwest-client"] }

# Mocking + raw HTTP/gRPC clients for adversarial UATs
wiremock = "0.6"
http = "1"
tokio-test = "0.4"

[[test]]
name = "slice_01_walking_skeleton"
path = "tests/slice_01_walking_skeleton.rs"

[[test]]
name = "slice_02_http_and_readiness"
path = "tests/slice_02_http_and_readiness.rs"

[[test]]
name = "slice_03_traces"
path = "tests/slice_03_traces.rs"

[[test]]
name = "slice_04_metrics"
path = "tests/slice_04_metrics.rs"

[[test]]
name = "slice_05_backpressure"
path = "tests/slice_05_backpressure.rs"

[[test]]
name = "slice_06_forwarding_sink"
path = "tests/slice_06_forwarding_sink.rs"

[[test]]
name = "slice_07_tls_schema_knob"
path = "tests/slice_07_tls_schema_knob.rs"

[[test]]
name = "slice_08_graceful_shutdown"
path = "tests/slice_08_graceful_shutdown.rs"

[[test]]
name = "no_telemetry_on_telemetry"
path = "tests/no_telemetry_on_telemetry.rs"

[[test]]
name = "probe_gold_runner"
path = "tests/probe_gold_runner.rs"

[[example]]
name = "send_one_log_record_grpc"
path = "examples/send_one_log_record_grpc.rs"
```

---

## Directory tree DELIVER will create

```
crates/aperture/
├── Cargo.toml                     # as above
├── README.md                      # operator-facing one-pager
├── examples/
│   ├── send_one_log_record_grpc.rs
│   └── config-stub.toml
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── compose.rs
│   ├── error.rs
│   ├── ports/
│   │   └── mod.rs
│   ├── app/
│   │   ├── mod.rs
│   │   ├── readiness.rs
│   │   ├── responses.rs
│   │   └── summary.rs
│   ├── transport/
│   │   ├── mod.rs
│   │   ├── grpc.rs
│   │   └── http.rs
│   ├── sinks/
│   │   ├── mod.rs
│   │   ├── stub.rs
│   │   └── forwarding.rs
│   ├── config/
│   │   ├── mod.rs
│   │   └── schema.rs
│   ├── observability/
│   │   ├── mod.rs
│   │   └── events.rs
│   └── shutdown/
│       └── mod.rs
└── tests/
    ├── slice_01_walking_skeleton.rs
    ├── slice_02_http_and_readiness.rs
    ├── slice_03_traces.rs
    ├── slice_04_metrics.rs
    ├── slice_05_backpressure.rs
    ├── slice_06_forwarding_sink.rs
    ├── slice_07_tls_schema_knob.rs
    ├── slice_08_graceful_shutdown.rs
    ├── no_telemetry_on_telemetry.rs
    └── probe_gold_runner.rs
```

`crates/aperture/CLAUDE.md` is recommended (project-level note, optional, mirrors what the root `CLAUDE.md` already says — the Rust-idiomatic paradigm declaration). The root `CLAUDE.md` already names Aperture by name, so a per-crate `CLAUDE.md` is non-essential.

---

## Workspace-level lockfile and `cargo deny`

The root `deny.toml` (added by DEVOPS in the harness's ADR-0005 work) already covers:
- license allowlist (CC0-1.0, MIT, Apache-2.0, MPL-2.0)
- `unmaintained = "deny"`
- `unsound = "deny"`
- `yanked = "deny"`

Aperture's dependency tree should pass without further `deny.toml` changes. If `reqwest`'s rustls feature pulls a copyleft transitive crate (it should not at v0.12), DELIVER updates `deny.toml`.

---

## CI — no new gate, just scope expansion

The harness's ADR-0005 lists five CI gates. They apply to `crates/aperture/` mechanically once it is added to the workspace:

1. `cargo test --all-targets --locked` — runs Aperture's slice integration tests + the unit tests under `src/`
2. `cargo public-api` — Aperture's binary surface is empty; library surface is `aperture::testing` only; no policy issue
3. `cargo semver-checks` — same; binary changes are out-of-scope for this gate
4. `cargo deny check` — license + maintenance check on Aperture's transitive deps
5. `cargo mutants` — scoped to `crates/aperture/src/**` per the per-feature mutation strategy in root `CLAUDE.md`

**New CI gates Aperture introduces** (DEVOPS owns the wiring):
- `single_validator_per_signal` — AST-walking xtask check; counts call sites of `validate_logs/traces/metrics` in `crates/aperture/src/**`; asserts ≤ 1 per signal.
- `no_telemetry_on_telemetry` — network-namespace integration test in `tests/no_telemetry_on_telemetry.rs`; asserts zero outbound packets except listener acks and `ForwardingSink`-to-downstream.
- `probe_gold_runner` — behavioural-layer probe test in `tests/probe_gold_runner.rs`; asserts a misconfigured downstream causes startup refusal with `event=health.startup.refused`.

These three are documented in `architecture-overview.md > Architectural rule enforcement` and surface for DEVOPS in the handoff annotations in `wave-decisions.md > Handoff to DEVOPS`.

---

## What this document is NOT

It is **not** the workspace-layout change itself. DESIGN proposes; DELIVER makes the change. The diff above is exactly the patch DELIVER applies; no architectural decision is hidden inside it.
