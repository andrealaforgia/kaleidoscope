# Wave Decisions — `aperture` v0 (DISCUSS)

> **Wave**: DISCUSS (`nw-product-owner` / Luna).
> **Date**: 2026-05-04.
> **Author**: Luna.
> **Companion documents**: `user-stories.md`, `journey-aperture.yaml`, `journey-aperture-visual.md`, `story-map.md`, `prioritization.md`, `outcome-kpis.md`, `shared-artifacts-registry.md`.

This file is the load-bearing artefact for the DESIGN wave. Morgan (`nw-solution-architect`) reads this to know which decisions are locked at DISCUSS and not to be re-litigated in DESIGN.

---

## Inherited decisions (from architecture and roadmap, recorded for posterity)

The platform-level architecture is laid in `docs/architecture/kaleidoscope-architecture.md`; the implementation roadmap is in `docs/roadmap/kaleidoscope-implementation-roadmap.md`. The following are inherited:

- **Aperture is a service** in the integration plane (architecture View 2). Long-lived process, network-facing, no library framing.
- **Phase 1 deliverable** alongside Prism v0 (roadmap phase 1: Months 2-4). v0 ships paired with any OTel-compatible backend the operator already runs.
- **Substrate**: Tokio, Tonic, Hyper, Prost, OTLP via `opentelemetry-proto` (architecture stratum diagram). Apache-Foundation governance, exempt from port-and-adapter discipline.
- **Consumes the OTLP conformance harness**: `crates/otlp-conformance-harness/` (Phase 0 deliverable, locked public API per the harness's `design/wave-decisions.md`).
- **License**: CC0-1.0 (Kaleidoscope-wide).
- **No telemetry from telemetry**: the project-wide commitment that Aperture must honour by NOT exposing `/metrics` or OTLP-out from itself at v0; Pulse owns this concern in Phase 4.
- **British English**, **no human-effort estimation**, **trunk-based development**, **CI is feedback not gate** (project conventions).

DESIGN does not re-derive any of the above.

---

## Andrea's locked scope decisions (the six Q&A items, verbatim)

These are recorded VERBATIM from the kickoff conversation (2026-05-04) so that DESIGN, DEVOPS, DELIVER, and any subsequent wave can read them as the ground truth without ambiguity.

### Q1 — Transport coverage at v0

**Decision**: gRPC (port 4317) AND HTTP/protobuf (port 4318), both day one.

### Q2 — Async runtime

**Decision**: Tokio.

### Q3 — Boundary with Sieve

**Decision**: trait `OtlpSink`. Aperture's job ends when it has called `sink.accept(record)` and the sink has acknowledged. v0 ships with `StubSink` (logs to stderr) and `ForwardingSink` (writes OTLP downstream to an external OTel-compatible backend per Phase-1 roadmap). Sieve, when it lands, will be `impl OtlpSink`.

### Q4 — Backpressure / overload

**Decision**: configurable max-concurrent-requests limit per transport. Once reached, reject with HTTP 503 (Retry-After header) or gRPC `RESOURCE_EXHAUSTED`. NO internal queue (Sluice's job in Phase 7), NO block (violates OTel SDK contract), NO silent drop (anti-pattern listed in roadmap).

### Q5 — Auth / TLS at v0

**Decision**: plaintext, no auth. BUT a configuration knob (TLS yes/no, SPIFFE yes/no) MUST be present in the v0 config schema, defaulting off. This avoids breaking the schema in Phase 2 when Aegis ships. Surface this as a v0 design constraint Morgan will need in DESIGN.

### Q6 — Aperture's own observability

**Decision**: structured JSON logs to stderr (levels error/warn/info/debug), no telemetry-on-telemetry (emphatically not back through Aperture itself); HTTP `/healthz` (liveness, always 200 if process up) and `/readyz` (200 once both listeners bound, 503 during startup or shutdown drain); NO Prometheus or OTLP-out metrics in v0 (Pulse-shaped concern, deferred to Phase 4).

---

## Slice 01 — walking-skeleton shape (verbatim)

An OpenTelemetry Rust SDK sends a real OTLP/gRPC `ExportLogsServiceRequest` to `localhost:4317`. Aperture binds the listener, receives the request, calls `otlp_conformance_harness::validate_logs(bytes, Framing::Grpc)` (the REAL harness, not a stub), and on `Ok(record)` hands the record to a `StubSink` that logs `received {count} log records from resource {service.name}` to stderr. Returns gRPC `OK` to the SDK. One transport (gRPC), one signal (logs), real harness integration, real sink trait. HTTP/protobuf and traces/metrics arrive in subsequent slices. Andrea explicitly chose this over the smaller "hard-coded reject" version because the harness is the load-bearing dependency and integration risk should land at Slice 01.

(Note on the `Framing::Grpc` reference in Andrea's text: the harness's enum variant is `Framing::GrpcProtobuf`; the corresponding HTTP variant is `Framing::HttpProtobuf`, per `crates/otlp-conformance-harness/src/framing.rs`. Aperture's call site uses `Framing::GrpcProtobuf` for the gRPC arm. This is a naming-clarification, not a behavioural deviation from Andrea's lock.)

---

## Decisions made in DISCUSS (additive, derived from the locked scope)

These are the requirements-level decisions Luna made by deriving them from Andrea's locked scope. They are the substrate Morgan starts from in DESIGN. None of them is a "DESIGN decision" — DESIGN locks the *technology* and *internal structure*; DISCUSS locks the *contract* and *user-observable behaviour*.

### D1 — Stderr-event vocabulary (closed set at v0)

The closed set of structured event names Aperture writes to stderr in v0:

```
startup
listener_bound
listener_closing
listener_bind_failed
ready
readiness_changed
request_received
sink_accepted
sink_failed
shutdown_initiated
shutdown_complete
in_flight_drained
drain_deadline_exceeded
unsupported_media_type
body_too_large
concurrency_cap_hit
tls_not_supported_in_v0
```

Renames are version-bump-able; additions are non-breaking. DESIGN locks the implementation mechanism (`pub enum LogEvent`, or `&'static str` constants, or an internal trait); DISCUSS locks the names.

### D2 — `OtlpSink` trait contract

Required at the contract level:

- `Send + Sync`.
- An async method `accept(record) -> Result<(), SinkError>`.
- The `record` parameter is a `SinkRecord` enum with exactly three variants at v0: `Logs(ExportLogsServiceRequest)`, `Traces(ExportTraceServiceRequest)`, `Metrics(ExportMetricsServiceRequest)` — each carrying the upstream `opentelemetry_proto` type unwrapped (no harness-local wrapper, no Aperture-local wrapper).
- The `SinkError` type names the failure shape (downstream unavailable, downstream timeout, etc.) — DESIGN locks the variants.

DESIGN locks the exact trait signature, the `SinkError` variant set, and the `#[non_exhaustive]` posture. Two impls in v0: `StubSink` and `ForwardingSink`.

#### Rejected alternatives (recorded for posterity so DESIGN does not re-derive)

Three alternative shapes were considered before settling on the async-trait contract above:

1. **Synchronous trait** (`fn accept(&self, record: SinkRecord) -> Result<(), SinkError>`) — rejected because `ForwardingSink` does network I/O to a downstream backend; a synchronous trait would force every async I/O call inside the sink to block a Tokio runtime thread, defeating Q2's locked Tokio choice. The harness itself is synchronous because it does no I/O; the sink is asynchronous because it does.
2. **Channel-based sink** (`fn handle(&self) -> Sender<SinkRecord>`, where Aperture sends records into a channel and the sink consumes them on its own task) — rejected because backpressure semantics get fuzzy across the channel boundary. With a channel, "the sink has acknowledged" becomes "the channel has accepted the record", which is not the same thing — Andrea's locked Q3 explicitly says Aperture's job ends when the sink has acknowledged. A direct trait-method call preserves the acknowledgement chain end-to-end.
3. **Callback-based sink** (`fn accept(&self, record: SinkRecord, on_done: Box<dyn FnOnce(Result<(), SinkError>)>)`) — rejected because the trait shape is what Sieve will plug into in Phase 1 (Q3), and a trait method returning `impl Future` is the standard Rust shape for an async boundary. A callback shape would make Sieve's eventual integration awkward; an async trait is what a Sieve maintainer would expect.

DESIGN locks the async-trait flavour (`async-trait` crate, `#[async_trait]` attribute, vs nightly's `async fn` in trait, vs hand-rolled associated-`Future` types) — but the contract above is fixed.

### D3 — Single validation gate (CI invariant)

Each accepted byte sequence flows through exactly one `otlp_conformance_harness::validate_*` call. No alternative validator, no wrapper. Enforced by a static check (grep + AST walk) in Aperture's CI named `single_validator_per_signal`.

### D4 — No telemetry-on-telemetry (CI invariant)

Aperture's outbound network footprint = ForwardingSink-only. Verified by an integration test in a constrained network namespace named `no_telemetry_on_telemetry`. No `/metrics` endpoint at v0; no OTLP-out from Aperture itself.

### D5 — Refusal-not-drop (`@property` UAT)

Backpressure refusals are deterministic and observable. The `@property`-tagged UAT scenario in `journey-aperture.feature` defends this invariant: for every request that exceeds the concurrency cap, the client receives a deterministic refusal status (gRPC `RESOURCE_EXHAUSTED` or HTTP 503), and every refusal is a structured stderr line. Zero silent drops.

### D6 — Reject-message identity

The harness's `OtlpViolation::Display` output is what consumers see on rejection: gRPC `grpc-message` header verbatim, HTTP response body (text/plain) verbatim. Aperture does NOT reformat, truncate, or replace this string. Enforced by a unit test on the rejection path.

### D7 — Per-transport concurrency cap as a unit semaphore

The mechanism is a per-transport semaphore. Default capacity 1024 per transport. Permit acquired on connection accept (gRPC) or request begin (HTTP); released on response sent. The sink hand-off-and-await counts as in-flight.

**Default-value rationale**: 1024 chosen as a placeholder large enough to absorb realistic burst traffic from a 50-pod application cluster (50 pods × 16 concurrent OTel exporters per pod / 1 Aperture replica = 800 concurrent in flight, rounded up to a power of two). Operators with smaller fleets should lower the cap; operators with larger fleets should run more Aperture replicas before raising the cap. DESIGN may calibrate against measured production traffic; the contract (per-transport, semaphore-based, deterministic refusal) is fixed.

**Memory-bound NFR (derived)**: Worst-case in-flight memory footprint for inbound buffers is bounded by `max_concurrent_requests × max_recv_msg_size × number_of_transports`. With v0 defaults (1024 × 4 MiB × 2 transports) this is 8 GiB; operators are expected to lower one or both bounds to fit their pod memory limit. This is a documentation contract, not a runtime check — Aperture does not introspect its own memory at v0. Operators sizing a pod for Aperture must compute this product before setting `resources.limits.memory`.

### D8 — Drain-respecting shutdown order

On SIGTERM/SIGINT, the shutdown order is **fixed**: `/readyz` flips to 503 `"draining"` first; listeners stop accepting new connections second; in-flight requests drain to a deadline (default 30 s) third; on clean drain, exit 0; on deadline expiry, exit 1 with a stderr warn line naming the dropped count. SIGKILL is acknowledged as un-graceful by definition.

**Default-value rationale**: 30 s default chosen to match Kubernetes' default `terminationGracePeriodSeconds` (30 s); k8s sends SIGKILL after that period regardless, so deadlines longer than 30 s have no effect under k8s anyway. Operators running outside k8s (systemd, bare metal) may set a longer deadline if their orchestrator allows it.

---

## Out-of-scope (intentional, with rationale)

| Item | Rationale | Whose job, when |
|---|---|---|
| Internal request queue | Andrea's locked Q4 explicitly excludes it. Sluice owns durable queueing. | Sluice, Phase 7 |
| Blocking on the producer | Violates OTel SDK contract; never. | n/a |
| Silent drop | Anti-pattern listed in roadmap; never. | n/a |
| TLS termination | Andrea's locked Q5: schema present, behaviour off. | Aegis, Phase 2 |
| SPIFFE / SVID validation | Same as TLS. | Aegis, Phase 2 |
| `/metrics` endpoint | Andrea's locked Q6: telemetry-on-telemetry deferred. | Pulse, Phase 4 |
| OTLP-out from Aperture itself | Same as `/metrics`. | Pulse, Phase 4 |
| Multi-tenancy / per-tenant cap | Multi-tenancy is Aegis's domain. | Aegis, Phase 2 |
| Adaptive caps based on system load | Out of scope for v0; potentially Pulse-driven. | Pulse, Phase 4 (post-v0) |
| OTLP/JSON encoding | Not stable in OTel spec at the harness's pinned version (v1.5.0). | Future release; harness adds `Framing::OtlpJson` first. |
| OTLP Profiles signal | Not stable in OTel spec at the harness's pinned version. | Future release. |
| Live config reload | v0 ships restart-as-process-exit. | Future, when an operator demands it. |
| Outbound retry / circuit-breaker | The SDK retries; Aperture refuses fast. | n/a (unless KPI 7 fails in production) |
| Sampling | Sieve's domain. | Sieve, Phase 1+ |

---

## Risks Morgan needs to know about (DESIGN risk register input)

| Risk | Probability | Impact | Surface to Morgan |
|---|---|---|---|
| Sink trait shape locked here is wrong for Sieve in Phase 1 | Low | High | DISCUSS specifies the contract loosely (`Send + Sync`, async `accept`, `Result<(), SinkError>`); DESIGN locks the signature. The trait is `#[non_exhaustive]`-friendly so additive evolution is safe. |
| Concurrency-cap default (1024 per transport) wrong for production | Medium | Low | Default is operator-tunable. DESIGN may revisit the literal value; the contract (per-transport, semaphore-based, deterministic refusal) is locked. |
| Stderr event vocabulary (D1) too restrictive | Low | Medium | The set is closed by intent. Adding events is a non-breaking change; renames are version-bump-able. |
| Drain-deadline default (30 s) too short for slow sinks | Medium | Medium | Tunable. UAT scenarios cover both clean drain and deadline-exceeded paths. |
| `validate_*` performance becomes a bottleneck under high concurrency | Low | Low | The harness is synchronous and CPU-bound; Tokio's blocking-task pool is the standard mitigation. DESIGN owns this. |
| `opentelemetry-proto` upstream cuts a breaking change before Phase 1 | Low | Medium | Aperture pins the same version as the harness (`=0.27.0`). |
| OTLP/HTTP/protobuf and gRPC servers compete for the same Tokio runtime under burst load | Medium | Low | DESIGN may co-locate them on one runtime or split into two. The contract (caps independent per transport) is what matters. |
| Aperture's listener bind ordering races against the readiness signal | Low | Medium | Slice 02's `/readyz` state machine: 200 only after BOTH listeners bound. DESIGN locks the ordering check. |

---

## Discoverability — references for Morgan

Files Morgan should read before starting DESIGN:

| File | Why |
|---|---|
| `journey-aperture.yaml` | The structured contract Morgan's ADRs must defend. |
| `journey-aperture-visual.md` | The wire-level picture; mockups of every endpoint and stderr line. |
| `journey-aperture.feature` | The full Gherkin scenario set. |
| `user-stories.md` | The 9 user stories with embedded acceptance criteria. |
| `outcome-kpis.md` | The 8 KPIs Morgan must keep measurable in DESIGN's choices. |
| `shared-artifacts-registry.md` | Every `${variable}`'s source of truth and consumer list. |
| `slices/slice-*.md` | The 8 thin end-to-end slices, each with its own demo command and acceptance summary. |
| `crates/otlp-conformance-harness/src/lib.rs` | The locked public API Aperture consumes. |
| `crates/otlp-conformance-harness/src/{framing,violation}.rs` | The exact types Aperture passes / receives. |
| `docs/feature/otlp-conformance-harness-v0/design/wave-decisions.md` | The harness's locked DESIGN decisions, for Morgan's reuse / consistency reasoning. |

---

## What Morgan owns next (DESIGN)

The DESIGN wave's job is to lock the **technology** and **internal crate structure**, not to re-litigate the contract. Concretely:

1. The exact `OtlpSink` trait signature and `SinkError` variant set.
2. The HTTP server library (probably `hyper` directly or `axum`) and the gRPC server (probably `tonic`).
3. The internal module split of the `aperture` crate.
4. The configuration parser (probably `figment` or `config-rs`); the schema is locked here, the parser choice is DESIGN's.
5. The stderr serialiser (probably `serde_json` direct or `tracing-subscriber` with a JSON layer).
6. The shutdown handler implementation (probably `tokio::signal` plus a `JoinSet` of listener tasks).
7. The semaphore implementation (probably `tokio::sync::Semaphore`).
8. ADRs for any of the above whose alternatives have material trade-offs.

Anything DESIGN decides that requires changing a DISCUSS contract (a story, an AC, a KPI, an event name) flows back via `design/upstream-changes.md` (only if needed). DISCUSS contracts are otherwise frozen.
