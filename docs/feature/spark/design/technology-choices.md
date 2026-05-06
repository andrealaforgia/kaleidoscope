# Technology Choices — `spark` v0

> **Wave**: DESIGN.
> **Author**: Morgan (`nw-solution-architect`).
> **Date**: 2026-05-06.
> **Companion documents**: `wave-decisions.md`, `c4-context.md`, `c4-container.md`,
> `c4-component.md`, `slice-mapping.md`,
> `../../../product/architecture/adr-0011..0016`.

This file is the dependency table. Every runtime, dev, and tooling
dependency Spark v0 takes is listed below with version, licence, role,
and rejected alternatives. The table is the working evidence for ADR-0013
(dep pinning) and `wave-decisions.md > Constraints`.

---

## Constraint envelope (recap)

Spark is **Apache-2.0** per `LICENSING.md`. Every runtime dependency must
be Apache-2.0 / MIT / BSD-* / Apache-2.0-OR-MIT dual / similar permissive.
**No copyleft in the runtime path.** AGPL-3.0-or-later (Aperture's
licence) is allowed only as a `[dev-dependencies]` integration-test
target — that is the only way Spark can prove end-to-end behaviour
against a real Aperture without making Aperture's AGPL viral on
application code linking Spark.

The OTLP wire pin is `opentelemetry-proto = "=0.27.0"` (harness ADR-0003,
inherited workspace-wide). The OTel SDK family that pairs with that pin
is the **0.27.x** family.

`Cargo.lock` already carries `opentelemetry 0.27.1` and `tonic 0.12.3`
transitively (via Aperture and the harness); Spark's pins co-resolve
without a lockfile churn.

---

## Runtime dependencies (Apache-2.0 / MIT / BSD)

| Crate | Version | Licence | Role | Why this version |
|---|---|---|---|---|
| `opentelemetry` | `=0.27` | Apache-2.0 | The upstream OTel API surface. Spark wraps this; consumers import it directly to call `opentelemetry::global::tracer(...)`. | Pairs with `opentelemetry-proto =0.27.0` (harness ADR-0003); already in `Cargo.lock`. |
| `opentelemetry_sdk` | `=0.27` | Apache-2.0 | The OTel SDK runtime: `Resource`, `TracerProvider`, `LoggerProvider`, `MeterProvider`, batch processors. Spark composes a `Resource` and wires the three providers. | Same minor as `opentelemetry`. The SDK and the API are shipped in lockstep upstream. |
| `opentelemetry-otlp` | `=0.27` | Apache-2.0 | The OTLP exporter. Spark constructs an exporter targeting the resolved endpoint over gRPC (default). | Same minor as `opentelemetry`/`opentelemetry_sdk`; pairs with `opentelemetry-proto =0.27.0`. |
| `opentelemetry-semantic-conventions` | `=0.27` | Apache-2.0 | Canonical attribute name constants (`SERVICE_NAME` etc.) so Spark does not stringify `"service.name"` by hand. | Same family. See ADR-0013 §2 "semconv version verification" for the resolved-attribute decision. |
| `tonic` | `=0.12` | MIT | gRPC transport for the OTLP exporter. Already a transitive dep of `opentelemetry-otlp` 0.27. Spark does NOT name `tonic` directly in `Cargo.toml` (it rides the `opentelemetry-otlp` `["grpc-tonic"]` feature). Listed here for traceability. | Locked by `opentelemetry-otlp` 0.27; co-resolves with Aperture's `tonic 0.12`. |
| `thiserror` | `^2` | Apache-2.0 OR MIT | `derive(Error)` on `SparkError`. Eliminates 30+ lines of hand-written `Display` and `source` impls per the variant set locked in ADR-0012. | The ecosystem-canonical error-derive crate. Caret pin at 2.x; Aperture and harness already use compatible idioms. |
| `tracing` | `^0.1` | MIT | Spark's diagnostic channel. Per D5, Spark's own events go through `tracing` and **must not** flow through the OTel pipeline Spark configured. | Aperture already pins `tracing = "0.1"`; sharing the workspace's resolved version is the path-of-least-friction. |
| `url` | `^2` | Apache-2.0 OR MIT | Endpoint URI parsing for `SparkError::InvalidEndpoint` (Slice 02). `with_endpoint("htp://...")` produces a parse failure whose `reason` field names the failure precisely. | Idiomatic Rust URL parser; small, well-maintained, no transitive surprises. |

### Rejected runtime alternatives

- **`anyhow` for `SparkError`** — rejected. `anyhow` is for application code, not for library error types. Library consumers need a concrete enum to pattern-match (US-SP-02). `thiserror` is the library half of the pair.
- **`http` for URI parsing** — `http::uri::Uri` is also viable, but `url::Url` produces richer parse-error messages and Spark's `InvalidEndpoint::reason` field is the value-add. Both are Apache-2.0/MIT.
- **`reqwest` (or any HTTP client)** — Spark does not make HTTP calls. The OTLP exporter inside `opentelemetry-otlp` owns the network surface. Adding a client crate would be dead weight.
- **`tracing-subscriber`** — Spark **does not depend on** a subscriber. Spark *emits* `tracing` events; the application chooses (or omits) the subscriber. Adding `tracing-subscriber` would couple every Spark consumer to a particular subscriber implementation.
- **`tokio`** — Spark does not own the runtime. Spark's `init` is sync; the OTel SDK's batch processor runs on whatever runtime the application has. The exporter pins `tokio` transitively but Spark itself does not name it.

---

## Dev dependencies (integration test substrate; allowed to be AGPL)

| Crate | Version | Licence | Role |
|---|---|---|---|
| `aperture` | path `../aperture`, `version = "0.1.0"` | AGPL-3.0-or-later | The integration-test target. Spark's tests under `crates/spark/tests/` spawn a real Aperture instance via `aperture::spawn`, plug in `aperture::testing::RecordingSink`, point Spark at the bound port, and assert the round-trip. **Dev-only**; never appears in the runtime dependency tree. |
| `tokio` | `^1.40` (`features = ["full"]`) | MIT | Test runtime. The Aperture test fixture is Tokio-based; Spark's integration tests need a runtime to drive the fixture and the OTel exporter's batch processor. |
| `tracing-subscriber` | `^0.3` (`features = ["fmt", "json", "env-filter", "registry"]`) | MIT | Captures `tracing` events from `target="spark"` so the integration tests assert the resolved-config / shutdown-complete / flush-deadline-exceeded vocabulary. The Aperture crate's `testing::stderr_capture` shape is the precedent. |
| `serde_json` | `^1` | Apache-2.0 OR MIT | Field-level interrogation of captured `tracing` events in tests. |
| `serial_test` | `^3` | MIT | Single-threaded execution of integration tests that mutate `OTEL_*` env vars (Slice 04 cases A–C) or the OTel global tracer provider (Slice 02 `GlobalAlreadyInitialised`). `std::env::set_var` is process-global; without `serial_test`, tests race. |

The `aperture` dev-dep is the licensing-critical edge: it is path-resolved
inside the workspace (Cargo prefers the path over the registry when both
are specified) and the explicit `version` satisfies `cargo deny check`'s
`bans.wildcards = "deny"` (the same idiom Aperture itself uses for the
harness; see `crates/aperture/Cargo.toml` lines 86-96).

The crate-level `Cargo.toml` MUST NOT list `aperture` under
`[dependencies]`. ADR-0011 and ADR-0013 make this an enforceable
invariant; Gate 4 (`cargo deny check`) catches any future PR that
accidentally promotes the dev-dep into the runtime tree.

### Rejected dev alternatives

- **InMemoryExporter / InMemorySpanExporter** — `opentelemetry_sdk::testing::InMemorySpanExporter` exists. Rejected because Bea explicitly chose **Strategy C "real local"** in `discuss/wave-decisions.md > Slice 01`: the value proposition is "OTel→OTLP→Aperture round-trip", and an InMemoryExporter would short-circuit the load-bearing OTLP/gRPC transport. The walking skeleton must exercise the wire.
- **`testcontainers` (Aperture-in-a-container)** — heavyweight relative to `aperture::spawn(Config::for_test())`. Aperture already exposes a public `spawn` API + `RecordingSink` that gives the same evidence at zero container overhead.
- **`assert_cmd` + spawn `aperture` binary** — `aperture::spawn` is the higher-fidelity seam; the binary path adds process boundaries the test does not need.

---

## Build / CI tooling

These are not crate dependencies; they run in CI per ADR-0011 §CI gates
(which mirror harness ADR-0005 + Aperture's gate-5-mutants-aperture).

| Tool | Licence | Role |
|---|---|---|
| `cargo test` | Apache-2.0 OR MIT | Gate 1 — workspace test suite. |
| `cargo public-api` | Apache-2.0 OR MIT | Gate 2 — public-API surface diff vs `main`. ADR-0011 names `spark::init`, `SparkConfig`, `SparkError`, `SparkGuard` as the locked surface. |
| `cargo semver-checks` | Apache-2.0 OR MIT | Gate 3 — SemVer-aware compatibility analysis (variant removals, signature changes). |
| `cargo deny` | Apache-2.0 OR MIT | Gate 4 — licence policy + advisory + pin-policy enforcement. The `licenses.allow` list MUST include Apache-2.0 and MIT (and the BSD variants for `tonic`'s transitive surface). The `bans.deny` list MUST include `aperture` for the runtime tree (the `[target.'cfg(not(test))'.dependencies]` discrimination is the mechanism). |
| `cargo mutants` | Apache-2.0 OR MIT | Gate 5 — 100% kill rate per ADR-0005 of the harness, inherited by Spark. The `cargo-mutants --in-diff` approach Aperture uses (`gate-5-mutants-aperture` workflow) is the template DEVOPS will adapt for `gate-5-mutants-spark`. |

---

## Licence audit

Every runtime dependency listed in the first table is one of the
permissive licences (`Apache-2.0`, `MIT`, `Apache-2.0 OR MIT`,
`BSD-3-Clause`, `BSD-2-Clause`, `ISC`). The transitive closure of
`opentelemetry-otlp 0.27` (which brings `tonic`, `prost`, `tower`,
`tower-http`, `hyper`, `tokio`, `pin-project-lite`, etc.) is dominated
by `Apache-2.0 OR MIT` and `MIT`-only crates. The harness's own
`deny.toml` policy (ADR-0005 Gate 4) is sufficient verbatim for Spark.

The dev-dep `aperture` is `AGPL-3.0-or-later`. AGPL is **not** in the
permissive list and **must not** be promoted to runtime. The mechanism
(path-resolved dev-dep, no `[dependencies]` entry, `cargo deny`
verification) is documented in ADR-0013 §3 and is enforced by Gate 4
on every commit.

---

## Why these choices satisfy the quality attributes

| ISO 25010 attribute | How the choice satisfies it |
|---|---|
| Functional Suitability — Completeness | All four house attributes (`service.name`, `tenant.id`, `feature_flag.*`, `experiment.id`) ride on the OTel SDK's `Resource`; the SDK propagates them to traces, logs, and metrics without per-signal code. |
| Functional Suitability — Correctness | `=0.27` exact-minor pin on the OTel SDK family co-resolves with the harness's `opentelemetry-proto =0.27.0`. Wire bytes Spark emits are decodable by the harness Aperture runs. |
| Reliability — Maturity | Each OTel crate is downstream of >5k GitHub stars and weekly release cadence. `tonic` is the de-facto Rust gRPC server. |
| Reliability — Fault Tolerance | The OTel SDK's batch processor handles transient export failures; `SparkGuard::Drop` bounds the worst-case shutdown via the configured deadline (ADR-0014). |
| Maintainability — Modularity | Spark's internal modules (per ADR-0011: `config.rs`, `error.rs`, `guard.rs`, `init.rs`, `lib.rs`) match the conceptual decomposition the user stories already imply. |
| Maintainability — Modifiability | `#[non_exhaustive]` on `SparkError` and `SparkConfig` (ADR-0012, ADR-0011) makes additive changes non-breaking. Caret pins on `thiserror`, `tracing`, `url` accept upstream patch fixes. |
| Maintainability — Testability | Aperture's `testing::RecordingSink` is the integration seam; Spark's tests assert real wire bytes, not mocked behaviour. The dev-dep on Aperture is the load-bearing test substrate. |
| Performance Efficiency — Time Behaviour | The default 5 s flush deadline matches the OTel SDK's recommended exporter timeout. Sequential per-provider flush (ADR-0014) avoids cross-provider contention. |
| Compatibility — Interoperability | Spark honours the OTel-canonical `OTEL_EXPORTER_OTLP_*` env-var contract (D6); operators redirecting traffic between regions need no rebuild. |
| Security — Confidentiality / Integrity | TLS / SPIFFE deferred to Aegis Phase 2 (matches Aperture v0's plaintext default). Spark is forward-compatible (the upstream `opentelemetry-otlp` exposes the TLS hooks Aegis will turn on). |
| Portability — Adaptability | `#![forbid(unsafe_code)]` at crate root (constraint 4); MSRV 1.88 (workspace floor); no platform-specific deps. |

---

## What this file does NOT decide

- The exact MSRV for Spark — declared via `rust-version.workspace = true` so Spark inherits the workspace floor (`1.88`). See ADR-0013 §4.
- The internal module split — locked in ADR-0011.
- The `SparkError` variant set + `#[non_exhaustive]` posture — locked in ADR-0012 (DISCUSS D2 specifies the variants; DESIGN adds derive macros and traits).
- The flush-timeout per-provider mechanism — locked in ADR-0014.
- The single-init test mechanism — locked in ADR-0015.
- The `SparkGuard` posture (`#[must_use]`, opaque) — locked in ADR-0016.
