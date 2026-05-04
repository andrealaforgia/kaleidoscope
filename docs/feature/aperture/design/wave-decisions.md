# Wave Decisions — `aperture` v0 (DESIGN)

> **Wave**: DESIGN (`nw-solution-architect` / Morgan).
> **Date**: 2026-05-04.
> **Mode**: Propose (orchestrator-decided in autonomous mode; Andrea is at dinner and trusts orchestrator decisions).
> **Author**: Morgan.
> **Companion documents**: `architecture-overview.md`, `component-design.md`, `aperture-port-and-adapter-diagram.md`, `workspace-layout.md`, ADR-0006 through ADR-0010 in [`../../../product/architecture/`](../../../product/architecture/).

This file is the load-bearing artefact for DISTILL. Atlas (`nw-acceptance-designer`) reads this to know which decisions are locked at DESIGN and not to be re-litigated.

---

## Inherited decisions (recorded for posterity, not re-derived)

The platform-level architecture is laid in `docs/architecture/kaleidoscope-architecture.md`; the implementation roadmap in `docs/roadmap/kaleidoscope-implementation-roadmap.md`. DISCUSS lives in `../discuss/`. The harness DESIGN precedent is at `../../otlp-conformance-harness-v0/design/wave-decisions.md`. The following are inherited:

- **Aperture is a service** (DISCUSS US System Constraint 1; long-lived process; binary in `crates/aperture/`).
- **Phase 1 deliverable**, ships paired with any OTel-compatible backend (roadmap Phase 1).
- **Substrate**: Tokio, Tonic, Hyper, Prost, OTLP via `opentelemetry-proto` — substrate-exempt from port-and-adapter discipline (architecture stratum diagram).
- **Consumes the OTLP conformance harness** as a substrate library (DISCUSS US System Constraint 4).
- **Licence**: CC0-1.0 (Kaleidoscope-wide).
- **`opentelemetry-proto` exact pin** (`=0.27.0`) inherited from harness ADR-0003.
- **MSRV 1.85**, **edition 2021**, **British English**, **no human-effort estimation**, **trunk-based development** (project conventions).
- **No telemetry-on-telemetry** (DISCUSS Q6 + roadmap A.2). CI invariant `no_telemetry_on_telemetry`.
- **Single validator per signal** (DISCUSS D3). CI invariant `single_validator_per_signal`.
- **Both transports at v0** (DISCUSS Q1: gRPC :4317 AND HTTP/protobuf :4318).
- **Tokio runtime** (DISCUSS Q2).
- **`OtlpSink` trait as the boundary with future Sieve** (DISCUSS Q3).
- **Configurable max-concurrent-requests per transport, deterministic refusal, no queue** (DISCUSS Q4).
- **Plaintext + no auth at v0, schema-forward-compatible TLS/SPIFFE knobs** (DISCUSS Q5).
- **Structured JSON stderr + /healthz + /readyz + no /metrics in v0** (DISCUSS Q6).
- **Closed v0 event-name vocabulary** (DISCUSS D1; renames version-bump-able, additions non-breaking).
- **Worst-case in-flight memory NFR**: `cap × max_recv_msg_size × transports` product; operators compute and set pod limits accordingly (DISCUSS D7).

DESIGN does not re-litigate any of the above.

---

## Load-bearing decisions made in DESIGN

### D1. Architectural style — hexagonal (ports-and-adapters), idiomatic Rust shape

**Decision**: ports-and-adapters with the Rust-idiomatic data + free functions + traits shape. `OtlpSink` and `Probe` are the output ports. Transport listeners (tonic gRPC, axum HTTP) are driving adapters. The application core is pure async functions calling the harness directly. The harness and `opentelemetry-proto` are substrate-exempt from port discipline (per the architecture document's stratum diagram).

**Rationale and rejected alternatives**: see `architecture-overview.md > Architectural style`. The hexagonal shape was chosen because the future Sieve seam (`OtlpSink`) is a real second-implementation boundary. Microservices (multiple processes) was rejected because two transports + one validator + one sink in one process is the simplest viable shape; event-driven (internal queue) was rejected because DISCUSS Q4 explicitly excludes one; DDD was deliberately skipped (transport-and-validation pipeline, not a rich domain).

**ADR**: this decision sits behind every other ADR; not separately ADR'd.

### D2. Transport stack — tonic for gRPC, axum for HTTP

**Decision**: `tonic ^0.12` (gRPC server, port 4317) + `axum ^0.7` on `hyper ^1.4` (HTTP server, port 4318). Both run on a single Tokio multi-threaded runtime. The harness is called synchronously on the runtime thread that received the body (no `spawn_blocking` indirection at v0).

**Alternatives considered**:
- (A) tonic + axum on single runtime — **recommended and accepted**.
- (B) tonic + hand-rolled hyper for HTTP — rejected (reinvents axum's value for marginal dep saving).
- (C) tonic + actix-web — rejected (mixing async runtimes is a footgun).
- (D) hyper-util + manual gRPC framing — rejected (resume-driven dev; complexity disproportionate).

**ADR**: [ADR-0006](../../../product/architecture/adr-0006-aperture-transport-stack.md).

### D3. `OtlpSink` trait shape — async-trait, separate `Probe`, `Arc<dyn OtlpSink>` storage

**Decision**: `OtlpSink` is an `async_trait::async_trait` trait with `Send + Sync + 'static`; `accept(&self, record: SinkRecord) -> Result<(), SinkError>`. `Probe` is a separate trait with `probe(&self) -> Result<(), ProbeError>`; every concrete sink implements both. Storage is `Arc<dyn OtlpSink>` at the composition root, cloned into each transport adapter. `SinkRecord` is `#[non_exhaustive]` with three variants (`Logs`, `Traces`, `Metrics`); `SinkError` is `#[non_exhaustive]` with three variants (`DownstreamUnavailable`, `DownstreamTimeout`, `Internal`).

**Alternatives considered**:
- (A) `async-trait` crate + separate Probe + `Arc<dyn>` — **recommended and accepted**.
- (B) Native `async fn in trait` — rejected (no `dyn` dispatch in stable 1.85).
- (C) Hand-rolled associated `Future` type — rejected (GAT verbosity; no `dyn` dispatch).
- (D) Single combined trait with default `probe()` — rejected (defeats Earned-Trust structural enforcement).
- (E) Generic-only, no `dyn` — rejected (requires duplicating the startup graph per concrete sink).

**ADR**: [ADR-0007](../../../product/architecture/adr-0007-otlpsink-trait-design.md).

### D4. Configuration — TOML + figment with deny-unknown-fields, forward-compat TLS/SPIFFE knobs

**Decision**: TOML configuration loaded via `figment ^0.10` (file then env-var overlay; `APERTURE__` prefix, `__` separator). `serde(deny_unknown_fields)` on every nested struct so misspelled keys produce loud `event=config_validation_failed` errors. TLS/SPIFFE keys present in v0 schema (defaulting off); setting `tls.enabled=true` or `auth.spiffe.enabled=true` on v0 emits one warn-level `event=tls_not_supported_in_v0` line and continues plaintext.

**Alternatives considered**:
- (A) TOML + figment + deny-unknown-fields — **recommended and accepted**.
- (B) TOML + plain serde + toml (no env layer) — rejected (env-var override is operator-table-stakes).
- (C) TOML + config-rs — rejected (less ergonomic than figment; smaller community momentum).
- (D) YAML — rejected (whitespace-and-anchors footguns; diverges from Rust ecosystem).
- (E) JSON — rejected (no comments).
- (F) In-binary config + SIGHUP reload — rejected at DISCUSS level (Q4 implicit).

**ADR**: [ADR-0008](../../../product/architecture/adr-0008-aperture-configuration-schema.md).

### D5. Observability — `tracing` + `tracing-subscriber` JSON to stderr; healthz/readyz on OTLP HTTP listener; no metrics in v0

**Decision**: `tracing ^0.1` as the logging facade, `tracing-subscriber ^0.3` JSON layer to stderr. One JSON event per line. Closed v0 event vocabulary in `crates/aperture/src/observability/events.rs` (20 names total: 16 from DISCUSS D1 + 4 DESIGN-derived). `/healthz` and `/readyz` co-located on the OTLP HTTP listener (port 4318), no separate admin port. Workspace-level `clippy::print_stdout = "deny"` and `print_stderr = "deny"` to structurally enforce "tracing is the only stderr-writing path". Panic handler set at startup to emit `event=internal_invariant_violation` and exit 70 (EX_SOFTWARE).

**Alternatives considered**:
- (A) `tracing` + JSON layer to stderr — **recommended and accepted**.
- (B) `slog` — rejected (maintenance mode; weaker hyper/tonic/axum integration).
- (C) `log` facade + custom stderr writer — rejected (no structured fields).
- (D) `slog` + Prometheus — rejected (DISCUSS Q6 forbids /metrics at v0).
- (E) Separate admin port for /healthz + /readyz — rejected at v0 (DISCUSS US-AP-02 says operators expect one place; revisit at Aegis if security review demands).

**ADR**: [ADR-0009](../../../product/architecture/adr-0009-aperture-observability-strategy.md).

### D6. Backpressure — per-transport `tokio::sync::Semaphore`, deterministic refusal, no queue

**Decision**: One `Arc<tokio::sync::Semaphore>` per transport. `Semaphore::try_acquire_owned()` (non-blocking). On `Err`: gRPC transport returns `tonic::Status::resource_exhausted("aperture: gRPC concurrency cap of {cap} reached on transport=grpc")`; HTTP transport returns 503 with `Retry-After: 1` header and a body naming the cap. Both emit one `event=concurrency_cap_hit` warn stderr line per refusal. Permit lifetime: middleware-acquired (request begin) until response sent. The drain orchestrator reads `Semaphore::available_permits()` per transport to compute in-flight count for `event=in_flight_drained drained_count=N` (clean) or `event=drain_deadline_exceeded dropped_count=N` (deadline hit).

**Alternatives considered**:
- (A) Per-transport `tokio::sync::Semaphore` with try_acquire_owned — **recommended and accepted**.
- (B) `tower::limit::ConcurrencyLimitLayer` — rejected (waits by default; wrong shape for "no queue").
- (C) Hand-rolled `arc-swap` counter — rejected (re-implements a semaphore; introduces races).
- (D) `dashmap` per-peer counter — rejected (per-peer is Aegis Phase 2).
- (E) Single global semaphore — rejected (DISCUSS Q4 says per-transport).

**ADR**: [ADR-0010](../../../product/architecture/adr-0010-aperture-backpressure-policy.md).

### D7. Error type — single top-level `ApertureError` enum via `thiserror`

**Decision**: One `pub enum ApertureError` in `crates/aperture/src/error.rs` derived via `thiserror ^1`. Variants: `ConfigInvalid`, `ConfigUnreadable`, `ListenerBindFailed`, `SinkProbeFailed` (`#[from] ProbeError`), `DrainDeadlineExceeded`, `Internal`. `#[non_exhaustive]` for additive evolution. The `main()` entry maps each variant to a specific exit code (1 for runtime failures, 2 for config errors, 70 for internal invariants).

**Alternatives considered**:
- (A) Single `ApertureError` enum via `thiserror` — **recommended and accepted**.
- (B) Per-module error types with `From` impls — rejected (cross-module failure paths multiply; main()'s match becomes unwieldy).
- (C) `Box<dyn Error>` — rejected (loses pattern-matching; loses exit-code mapping).
- (D) `anyhow::Error` — rejected (consumer-application convention; libraries / services should expose typed errors per Rust idiom).

**ADR**: not separately ADR'd; documented in `component-design.md > error::ApertureError`.

### D8. Outbound HTTP client for `ForwardingSink` — `reqwest` with `rustls-tls`

**Decision**: `reqwest ^0.12` with `default-features = false, features = ["rustls-tls"]`. Custom `User-Agent: aperture/{version}`. Configurable timeout via `ClientBuilder::timeout` (default 5000 ms; DISCUSS US-AP-08 timeout rationale).

**Alternatives considered**:
- (A) `reqwest` with rustls-tls — **recommended and accepted**.
- (B) Hand-rolled `hyper::Client` — rejected (hyper 1.x's hyper-util `Client` is lower-level than v0 needs; reqwest is the convenience layer).
- (C) `tonic` outbound (gRPC) — rejected at DISCUSS level (US-AP-08 technical notes: HTTP-out only at v0).
- (D) `ureq` — rejected (synchronous; would need spawn_blocking).

**ADR**: not separately ADR'd; documented in `component-design.md > Sinks > ForwardingSink`. Listed as a substrate choice in ADR-0006 transitively.

### D9. Earned-Trust probe contract — three semantically orthogonal enforcement layers

**Decision**: Every concrete `OtlpSink` MUST also implement `Probe`. The composition root invariant is **wire → probe → use**: probe runs at startup, before listeners bind; on failure, exits with `event=health.startup.refused`. Three enforcement layers:

1. **Subtype** — composition root signature `wire_then_probe_then_use<T: OtlpSink + Probe>(...)`; non-Probe sinks fail to compile.
2. **Structural** — `xtask` AST-walking pre-commit hook walks `crates/aperture/src/sinks/`; asserts every `impl OtlpSink for T` is matched by `impl Probe for T`. (`import-linter` was investigated and rejected — Python-only; import-graph contracts have no API for method-presence enforcement.)
3. **Behavioural** — `tests/probe_gold_runner.rs` CI gold-test starts Aperture against a fixture downstream that lies (200 OK to OPTIONS but 503 to POST); asserts startup refusal with `event=health.startup.refused`.

For `ForwardingSink`, the probe is two-stage: OPTIONS request to the configured downstream first; on 404/405, fall back to a known-empty `POST /v1/logs`. Catalogued substrate lie: a downstream that returns 200 to OPTIONS but 503 to POST; the degraded probe catches it at startup.

**Self-application**: the gold-test IS the probe-that-probes-actually-probe. Layer 2 additionally asserts every `Probe` impl contains at least one network call (to catch a maintainer who shorts probe to `Ok(())`).

**ADR**: documented in ADR-0007's "Earned-Trust enforcement" section and `architecture-overview.md > Earned Trust`.

### D10. Architectural-rule enforcement — language-appropriate automated tooling

**Decision**: Every architectural rule has automated enforcement (Principle 11). Mechanisms:

| Rule | Mechanism |
|---|---|
| Hexagonal layer dependency direction | `xtask` AST walk over `use` statements |
| Single validator per signal (DISCUSS D3) | `xtask` AST counts `validate_*` call sites per signal |
| No telemetry from telemetry (DISCUSS D4) | network-namespace integration test `tests/no_telemetry_on_telemetry.rs` |
| No `println!` / direct `eprintln!` | `clippy::print_stdout` + `print_stderr` workspace-level deny |
| OTLP protobuf decode is harness's job | `xtask` AST asserts no `prost::Message::decode` / `Export*ServiceRequest::decode` in `crates/aperture/src/` |
| Public surface stability | `cargo public-api` (workspace-wide; aperture's library surface is `aperture::testing` only) |
| Licence policy | `cargo deny check` (workspace-level; harness ADR-0005 already mandates) |
| Probe contract | three-layer enforcement per D9 above |

**Rationale**: `import-linter` (Python) was investigated as the closest tool to ArchUnit-shape contracts; rejected because it is Python-only. `dependency-cruiser` is JS-only. `cargo-arch` is unmaintained. The `xtask` pattern is the language-appropriate solution for Rust today; documented as DEVOPS-owned tooling in `architecture-overview.md > Architectural rule enforcement`.

**ADR**: documented in `architecture-overview.md > Architectural rule enforcement` and ADR-0007 Earned-Trust section.

---

## ADR index

| ADR | Title | Status |
|---|---|---|
| ADR-0006 | Aperture transport stack: tonic for gRPC, axum for HTTP | Accepted |
| ADR-0007 | `OtlpSink` trait design: async trait, three-variant SinkRecord, structured SinkError | Accepted |
| ADR-0008 | Aperture configuration schema and loader: TOML + figment with forward-compatible TLS/SPIFFE knobs | Accepted |
| ADR-0009 | Aperture observability strategy: tracing-subscriber JSON to stderr, healthz/readyz on the OTLP HTTP listener, no metrics in v0 | Accepted |
| ADR-0010 | Aperture backpressure policy: per-transport semaphore, deterministic refusal, no queue | Accepted |

ADRs 0001–0005 belong to the harness (Phase 0) and are not re-litigated.

---

## Reuse Analysis (HARD GATE)

| Existing component | Path | Overlap | Decision | Justification |
|---|---|---|---|---|
| `otlp-conformance-harness` | `crates/otlp-conformance-harness/` | 100% — Aperture is the first consumer of the harness's public API | **Reuse via direct dependency** (`path = "../otlp-conformance-harness"`) | The harness was built specifically for this use. Aperture calls `validate_logs/traces/metrics` from `app::ingest_*`. No wrapping (DISCUSS D3). |
| (none other) | — | — | — | The Kaleidoscope repository contains one prior crate (the harness, Phase 0). Aperture is the second crate, the first one that is a service. There is no other in-house code to extend. |

The Reuse Analysis table is intentionally short. The harness IS the reuse.

---

## Substrate dependencies (not Kaleidoscope-component reuse — listed for completeness)

| Crate | Version | Licence | Role | ADR |
|---|---|---|---|---|
| `tokio` | `^1.40` | MIT | Async runtime | ADR-0006 |
| `tonic` | `^0.12` | MIT | gRPC server + client | ADR-0006 |
| `axum` | `^0.7` | MIT | HTTP server | ADR-0006 |
| `hyper` | `^1.4` | MIT | HTTP foundation | ADR-0006 (transitive) |
| `tower` + `tower-http` | `^0.5` / `^0.6` | MIT | Middleware | ADR-0006 |
| `tracing` | `^0.1` | MIT | Logging facade | ADR-0009 |
| `tracing-subscriber` | `^0.3` | MIT | JSON layer | ADR-0009 |
| `serde` | `^1` | MIT/Apache-2.0 | Config deserialise | ADR-0008 |
| `figment` | `^0.10` | MIT/Apache-2.0 | Layered config | ADR-0008 |
| `async-trait` | `^0.1` | MIT/Apache-2.0 | Async trait dyn dispatch | ADR-0007 |
| `thiserror` | `^1` | MIT/Apache-2.0 | Error derive | D7 (component-design.md) |
| `reqwest` | `^0.12` | MIT/Apache-2.0 | Outbound HTTP for ForwardingSink | D8 (component-design.md) |
| `prost` | `^0.13` | Apache-2.0 | Protobuf encoding for ForwardingSink | inherited (workspace dep) |
| `opentelemetry-proto` | `=0.27.0` | Apache-2.0 | OTLP types | inherited (ADR-0003) |

All open-source, all MIT or Apache-2.0 (or CC0 for the in-tree harness). No proprietary dependencies. All dependencies are mature (≥ 0.7 or ≥ 1.0; community-momentum positive).

CI tooling (not crate dependencies) inherited from harness ADR-0005:
- `cargo public-api`
- `cargo semver-checks`
- `cargo deny`
- `cargo mutants`

---

## Quality Attribute Coverage (ISO 25010)

Summary; full discussion in `architecture-overview.md > Quality attributes addressed`.

| Attribute | Mechanism |
|---|---|
| Functional Suitability — Correctness | Single-validation-gate (DISCUSS D3); CI invariant `single_validator_per_signal`. |
| Functional Suitability — Completeness | Both transports × all three signals lit by Slice 04 (DISCUSS Q1 + slices 01–04). |
| Performance Efficiency — Time behaviour | North-Star KPI ≤50 ms p99; harness called synchronously on runtime threads (sensitivity-point flag in ADR-0006). |
| Performance Efficiency — Resource utilisation | Per-transport semaphore caps in-flight memory (DISCUSS D7; ADR-0010). |
| Compatibility — Interoperability | Standard ports + Content-Types + verbatim violation Display (DISCUSS Q1, D3, D6). |
| Reliability — Maturity | Canonical Rust ecosystem libraries (tonic, axum, hyper, tokio); all 1.x or post-1.0. |
| Reliability — Fault tolerance | Deterministic refusal-on-overload; no queue, no block, no silent drop (DISCUSS Q4; ADR-0010). |
| Reliability — Recoverability | Drain-respecting shutdown (DISCUSS D8; US-AP-09). |
| Security — at v0 | Plaintext by deliberate scope; TLS/SPIFFE schema knob present, defaulting off (DISCUSS Q5; ADR-0008). |
| Security — Integrity at v0 | Single-validation-gate shields the sink and downstream from confused-deputy / cross-signal pollution. |
| Maintainability — Modularity | Hexagonal style; module boundaries enforced by `xtask` AST walk. |
| Maintainability — Modifiability | `OtlpSink` trait IS the Sieve seam; `#[non_exhaustive]` on every public enum. |
| Maintainability — Testability | `RecordingSink` test double; transport adapters can be unit-tested behind it. |
| Maintainability — Analysability | Closed event vocabulary; structured JSON; one event per line. |
| Portability | Pure Rust, no `unsafe`; Tokio-supported platforms (all major). |
| Operational simplicity | No /metrics, no agent-of-an-agent setup; operator's existing log aggregator IS the observability backend. |

---

## ATAM trade-off summary

| Decision | Sensitivity to | Trade-off | Bias |
|---|---|---|---|
| Harness called synchronously on Tokio runtime threads | Performance Efficiency vs Operational Simplicity | If harness CPU cost grows, runtime starvation is plausible | v0 ships without `spawn_blocking` indirection; ADR-0006 sensitivity flag; revisit on profiling. |
| `async-trait` crate vs native `async fn in trait` | Modifiability vs Performance | Per-call `Box<dyn Future>` allocation | v0 uses async-trait for `dyn` dispatch ergonomics; revisit when stable Rust supports `dyn` on native async-fn-in-trait. ADR-0007. |
| Per-transport concurrency cap default 1024 | Operational Simplicity vs Memory Use | 1024 × 4 MiB × 2 = 8 GiB pod ceiling at v0 defaults | Operators tune; default documented; ADR-0010. |
| Plaintext at v0 with TLS schema knob present | Security vs Time-to-Market | Real TLS is Aegis (Phase 2) | DISCUSS Q5 explicit; the schema knob avoids a Phase-2 break. ADR-0008. |
| `axum` for HTTP rather than hand-rolled hyper | Maintainability vs Dependency Footprint | adds tower + tower-http to dep tree | Routing ergonomics + middleware composition justify the modest cost. ADR-0006. |
| Single Tokio runtime for both transports | Performance Efficiency vs Operational Simplicity | Hot HTTP path could starve gRPC under burst | v0 ships single runtime; ADR-0006 sensitivity flag; load test (KPI 5) decides. |
| Health/readiness on OTLP HTTP listener (no admin port) | Operational Simplicity vs Security boundary | Operator probes share a port with public OTLP surface | DISCUSS US-AP-02 explicit; revisit at Aegis if security review demands. ADR-0009. |

No trade-off is a critical risk; each is documented and has a Phase-1+ revisit gate.

---

## Conway's Law check

Aperture v0 is built by a single AI agent (`nw-software-crafter`) under Andrea's direction. Single team, single binary. Conway's Law check passes trivially: the architecture is a single hexagon with one boundary that matters (the `OtlpSink` trait, which is the future Sieve seam).

Forward-looking: when Sieve lands in Phase 1, the team boundary becomes "Aperture maintainers + Sieve maintainers"; the architecture boundary is the trait. **Inverse Conway Maneuver applied prophylactically** — the trait is designed so the team boundary CAN exist when it needs to.

---

## Earned Trust (Principle 12) — full discussion

See `architecture-overview.md > Earned Trust (Principle 12) — adapter probe contracts` for the full discussion. Summary:

- Three driven adapters depend on something external: `ForwardingSink` (operator-supplied downstream), gRPC listener (OS network stack), HTTP listener (OS network stack).
- Each has a probe contract.
- The composition root invariant is **wire → probe → use**.
- The probe contract is enforced by three semantically orthogonal layers (subtype, structural, behavioural).
- Catalogued substrate lie at v0: a downstream that returns 200 to OPTIONS but 503 to POST; the degraded-probe path catches it at startup.
- Self-application: `tests/probe_gold_runner.rs` IS the probe-that-probes-actually-probe.

For environments-known-to-lie that DO NOT apply to Aperture v0:
- Docker overlayfs `fsync` no-op — irrelevant; Aperture does no disk writes.
- WSL2 DrvFs — same.
- tmpfs — same.

For environments that DO apply:
- Misconfigured downstream — covered by `ForwardingSink::probe()`.
- Misbound listener (port in use, permission denied) — covered by `bind()` failure; DISCUSS US-AP-01 UAT.

---

## Risks (DESIGN-wave register)

Carried forward from `architecture-overview.md > Risks` for completeness:

| # | Risk | Probability | Impact | Mitigation |
|---|---|---|---|---|
| R1 | `OtlpSink` shape locked here is wrong for Sieve in Phase 1 | Low | High | async-trait + non-exhaustive `SinkRecord`/`SinkError`; additive evolution non-breaking. |
| R2 | Concurrency-cap default (1024) wrong for production | Medium | Low | Operator-tunable; default documented; ADR-0010. |
| R3 | tonic + axum on the same Tokio runtime contend under burst | Medium | Low | Both built on hyper; shared runtime is the standard shape; ADR-0006 sensitivity flag. |
| R4 | `validate_*` performance becomes a bottleneck under high concurrency | Low | Low | If profile shows it, move to spawn_blocking; ADR-0006 sensitivity flag. |
| R5 | `axum` major-version bump (0.x → 1.0) before Phase 1 | Medium | Low | Caret pin; track upstream; ADR-0006. |
| R6 | `figment` config edge-case allows half-validated startup | Low | Medium | `serde(deny_unknown_fields)` + post-validate function; ADR-0008. |
| R7 | Listener bind-ordering races against `/readyz` 200 | Low | Medium | Atomic ReadinessState flips to Ready only after both bind() succeed; UAT defends. |
| R8 | `tracing-subscriber` JSON layer partial-line on panic | Low | Low | Custom MakeWriter buffering per-event + flush-line-atomically; integration test asserts no partial lines on forced panic; ADR-0009. |
| R9 | `reqwest` brings native-tls + rustls + default UA conflicts | Low | Low | Build with default-features = false, only rustls-tls + custom UA; D8. |
| R10 | Future maintainer adds a second validator without noticing | Low | High | `single_validator_per_signal` static check; D3. |
| R11 | `no_telemetry_on_telemetry` invariant regresses | Low | High | Network-namespace integration test; DISCUSS D4. |

R10 and R11 are the system-level Earned-Trust probes; both have language-appropriate enforcement (Principle 11). Sink-level probes are documented in ADR-0007 and `component-design.md`.

---

## External integrations (contract-test annotation)

Aperture has ONE external integration at v0: the operator-supplied downstream OTel-compatible backend, consumed by `ForwardingSink` over OTLP/HTTP/protobuf.

**Annotation for platform-architect (DEVOPS handoff)**:

> **External Integrations Requiring Contract Tests**:
> - **Operator-supplied downstream OTLP backend** (HTTP/protobuf): Aperture's `ForwardingSink` POSTs `application/x-protobuf` to `${endpoint}/v1/{logs,traces,metrics}`. The contract is the OTLP/HTTP/protobuf wire spec at OTel spec version 1.5.0 (the same version the harness pins).
>   **Recommended**: consumer-driven contracts via Pact (polyglot tool with widest language support; some downstreams will be Go-implemented OTel Collectors, some Java-implemented Tempo/Loki/Mimir, etc.). The Aperture-side Pact consumer test asserts: `Content-Type: application/x-protobuf`, request body decodes as `ExportLogs/Trace/MetricsServiceRequest`, expected response is HTTP 2xx. Run in CI acceptance stage.
>   **Why**: the OTLP wire spec is stable, but downstream backends sometimes drift on auxiliary behaviour (Content-Type strictness, OPTIONS support, rate-limiting headers, redirect handling). Contract tests detect drift before production.
>   **Probe contract**: `ForwardingSink::probe()` is the runtime defence; Pact is the build-time defence.

The `Probe` and Pact-style contract test are complementary, not redundant: `Probe` catches "the downstream named in operator config does not honour the contract right now" at startup; Pact catches "a new downstream version deployed in CI breaks the contract" at build time.

---

## Out-of-scope (DESIGN-explicit)

Carries forward DISCUSS's out-of-scope list:

| Item | Why out of scope at v0 |
|---|---|
| Internal request queue | Sluice's job, Phase 7; DISCUSS Q4 explicit. |
| Adaptive concurrency caps | Pulse Phase 4+. |
| Live config reload (SIGHUP) | restart-as-process-exit at v0. |
| Outbound retry / circuit-breaker on ForwardingSink | SDK retries; double-retry is anti-pattern. |
| `/metrics` endpoint | DISCUSS Q6 explicit; Pulse Phase 4. |
| Sampling | Sieve's domain. |
| Multi-tenancy | Aegis Phase 2. |
| OTLP/JSON encoding / OTLP Profiles signal | Not stable in OTel spec at the harness's pinned version. |
| Outbound gRPC from ForwardingSink | DISCUSS US-AP-08; HTTP-out only. |
| Real TLS termination + SPIFFE | DISCUSS Q5; Aegis Phase 2. |
| Connection pooling tuning beyond reqwest defaults | Operator can set via env if it ever matters. |
| Custom resource-attribute redaction on sink_accepted lines | Out of scope; field is informational only. |
| Separate admin port for /healthz + /readyz | DISCUSS US-AP-02 says one port; revisit at Aegis if security review demands. |

---

## Back-propagation to DISCUSS

**None required.** Every load-bearing DESIGN decision sits within the latitude DISCUSS explicitly granted. No story, AC, KPI, or event name needs to change.

A `design/upstream-changes.md` file is therefore **not** created.

DESIGN added four event names (`health.startup.refused`, `config_validation_failed`, `internal_invariant_violation`; plus `request_received` already in DISCUSS D1's set). Per DISCUSS D1: "additions are non-breaking". This is documented in `component-design.md > Closed v0 event-name set` and ADR-0009.

---

## Handoff to DISTILL

Recipient: `nw-acceptance-designer` (Atlas).

**Required inputs**:
1. This file (`design/wave-decisions.md`).
2. `architecture-overview.md` — the C4 diagrams, quality attributes, Earned-Trust contract.
3. `component-design.md` — every type signature, error variant, module path, configuration key, observability event.
4. `aperture-port-and-adapter-diagram.md` — the dependency-direction picture.
5. `workspace-layout.md` — the `crates/aperture/` layout DELIVER will create.
6. ADRs 0006–0010 in `docs/product/architecture/`.
7. The DISCUSS artefacts (locked, read-only): `../discuss/wave-decisions.md`, `../discuss/user-stories.md`, `../discuss/journey-aperture.yaml`, `../discuss/story-map.md`, `../discuss/outcome-kpis.md`, `../slices/slice-*.md`.

**What DISTILL turns this into**:
The DISTILL wave turns the BDD scenarios in `discuss/user-stories.md` and `discuss/journey-aperture.yaml` into executable Rust acceptance tests against the public surface defined in `component-design.md`. The integration tests under `crates/aperture/tests/slice_*.rs` are RED at DISTILL completion; DELIVER's `nw-software-crafter` drives them green.

**What DISTILL does NOT need**:
- Workspace-level CI wiring (DEVOPS owns that).
- `Cargo.toml` files (DELIVER owns those, guided by `workspace-layout.md`).
- Source files under `crates/aperture/src/` (DELIVER owns those).

---

## Handoff to DEVOPS

Recipient: `nw-platform-architect`.

**Required inputs**:
1. `outcome-kpis.md` (DISCUSS) — the eight KPIs and measurement plans.
2. ADR-0006 — transport choices (tonic, axum); the substrate-dep manifest.
3. ADR-0010 — backpressure policy; the load-test (KPI 5, KPI 6) shape.
4. ADR-0009 — observability (closed event vocabulary; alerting thresholds).
5. ADR-0008 — config schema (the env-var override convention).
6. This file's "Architectural-rule enforcement" / D10 section — the three new CI gates Aperture introduces.

**New CI gates Aperture introduces**:
1. `single_validator_per_signal` — `xtask` AST-walking check; counts call sites of `validate_logs/traces/metrics` in `crates/aperture/src/**`; asserts ≤ 1 per signal. DEVOPS implements as a workspace-level `xtask` binary invoked from CI (and from the pre-commit hook).
2. `no_telemetry_on_telemetry` — network-namespace integration test (`crates/aperture/tests/no_telemetry_on_telemetry.rs`); asserts zero outbound packets except listener acks and ForwardingSink-to-downstream. DEVOPS provides the network-namespace fixture (Linux only at v0; the test is `#[cfg(target_os = "linux")]`).
3. `probe_gold_runner` — behavioural-layer probe test (`crates/aperture/tests/probe_gold_runner.rs`); uses `wiremock` to stand up a fixture downstream that returns 200 to OPTIONS but 503 to POST; asserts Aperture refuses to start with `event=health.startup.refused`.

**Existing CI gates from harness ADR-0005, scope-extended to `crates/aperture/`**:
1. `cargo test --all-targets --locked`
2. `cargo public-api` (Aperture's library surface is `aperture::testing` only; binary surface is empty)
3. `cargo semver-checks`
4. `cargo deny check`
5. `cargo mutants` (per-feature, scoped to changed files; per root `CLAUDE.md`)

**External integrations annotation** (per `nw-architecture-patterns` skill's contract-testing guidance):

> **External Integrations Requiring Contract Tests**:
> - **Operator-supplied downstream OTLP backend** (HTTP/protobuf): consumer-driven contracts via Pact in CI acceptance stage. See "External integrations" section above for the full annotation.

DEVOPS chooses the workflow runner (GitHub Actions, Gitea Actions, etc.); writes the runner-specific YAML; configures caching. The contract above is runner-agnostic.

---

## Handoff to DELIVER

Recipient: `nw-software-crafter`.

**Required inputs**:
1. `component-design.md` — the binding contract for module structure, every type signature, error variant, configuration key, and tracing macro path.
2. `workspace-layout.md` — the exact diff to root `Cargo.toml` and the exact `crates/aperture/Cargo.toml` to create.
3. ADRs 0006–0010 — the rationale for every load-bearing choice (so DELIVER does not re-derive).
4. The RED integration tests at `crates/aperture/tests/slice_*.rs` (produced by DISTILL after this handoff lands).
5. The `nw-software-crafter` agent's TDD methodology (RED → GREEN → REFACTOR; mutation testing per `CLAUDE.md` policy).

**What DELIVER does NOT need**:
- Architectural decisions that DESIGN already locked (would be re-derivation).
- The CI workflow YAML (DEVOPS owns).

---

## DESIGN-wave summary

- 10 load-bearing decisions, 5 with their own ADR (D2/ADR-0006, D3/ADR-0007, D4/ADR-0008, D5/ADR-0009, D6/ADR-0010); D1 (style) sits behind every other ADR; D7-D10 documented in `component-design.md` and `architecture-overview.md`.
- 0 platform-level decisions re-litigated.
- 0 changes back-propagated to DISCUSS (locked scope honoured).
- 11 substrate dependencies (all MIT/Apache-2.0 or CC0); 0 proprietary.
- 1 sibling Kaleidoscope crate reused (`otlp-conformance-harness`).
- 1 external integration (operator-supplied downstream); 1 contract-test recommendation (Pact).
- 3 new CI gates introduced (single_validator_per_signal, no_telemetry_on_telemetry, probe_gold_runner); 5 inherited from harness ADR-0005 (scope-extended).
- 11 risks named with mitigations.
- 7 ATAM trade-off points with documented bias.

DESIGN was a single pass. The DISCUSS wave's contract was tight; the harness DESIGN precedent (ADR-0001 to ADR-0005) gave a stylistic template; the DISCUSS-locked scope answered every "what" question, leaving DESIGN to lock "with which library, in which file, with which signature" — the questions DELIVER actually needs answered.

Vai.
