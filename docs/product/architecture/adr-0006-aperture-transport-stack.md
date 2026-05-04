# ADR-0006 — Aperture transport stack: tonic for gRPC, axum for HTTP

- **Status**: Accepted
- **Date**: 2026-05-04
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

Aperture is the integration-plane OTLP gateway. DISCUSS Q1 locks both transports at v0:

- **gRPC** on `:4317` — the OTLP `LogsService` / `TracesService` / `MetricsService` defined in `opentelemetry-proto` (the same crate the harness consumes).
- **HTTP/protobuf** on `:4318` — `POST /v1/{logs,traces,metrics}` with `Content-Type: application/x-protobuf`, plus `/healthz` and `/readyz` co-located on the same listener (US-AP-02).

DISCUSS Q2 locks **Tokio** as the runtime; DISCUSS US System Constraint 2 forbids alternatives. DISCUSS Q5 leaves TLS schema present but behaviour off at v0; the chosen libraries must support TLS configuration in a forward-compatible way for Aegis (Phase 2). DISCUSS US System Constraint 4 mandates that every accepted byte sequence flows through `otlp_conformance_harness::validate_*`; the transport layer's job is to assemble the byte slice and the framing variant, then call into the application core.

The architecture document's stratum diagram lists `tonic` and `hyper` as substrate-exempt. DESIGN must lock the concrete library choices and the concurrency-cap mechanism per transport.

## Decision

- **gRPC server**: `tonic` (caret `^0.12`).
- **HTTP server**: `axum` (caret `^0.7`), built on `hyper` `^1.4` and `tower` `^0.5`.
- Both run on the **same** Tokio runtime (multi-threaded scheduler, default worker count).
- Per-transport concurrency cap is implemented as a Tokio **semaphore** (`tokio::sync::Semaphore`); each transport holds one. Permit acquisition fails fast (no wait) on saturation; the failure becomes the deterministic refusal (gRPC `RESOURCE_EXHAUSTED` / HTTP 503) per DISCUSS Q4.
- The harness (`otlp_conformance_harness::validate_*`) is called **synchronously** on the runtime thread that received the body, not via `tokio::task::spawn_blocking`. Rationale: the harness is CPU-bound but fast; the indirection cost outweighs the benefit at v0. A profiling exercise during Phase 1 may revisit (sensitivity-point flag).

## Alternatives Considered

### Option A — `tonic` + `axum` on a single Tokio runtime (RECOMMENDED, accepted)

**Pros**:
- `tonic` is the canonical Rust gRPC server. It is already a transitive dependency of `opentelemetry-proto`'s `gen-tonic-messages` feature (which the harness uses to generate the Rust types). Adopting it as Aperture's gRPC server adds zero net dependency — `tonic` itself was previously a transitive-only dep.
- `axum` is the canonical Rust HTTP server. Its `Router` API gives a clean way to multiplex `/v1/{logs,traces,metrics}` + `/healthz` + `/readyz` on the same port (DISCUSS US-AP-02 Solution).
- Both built on `hyper` 1.x; no protocol-stack divergence.
- Both support TLS via rustls; forward-compatibility for DISCUSS Q5's TLS schema knob is straightforward (Aegis Phase 2 turns it on).
- One Tokio runtime, two listener tasks; minimal operational surface.

**Cons**:
- Two transport stacks to maintain. Acceptable: every OTLP gateway has this property; DISCUSS Q1 made it a v0 hard requirement.
- Shared runtime means a hot HTTP path could in principle starve gRPC (or vice versa). Mitigation: semaphore caps are independent per transport; the runtime work-steals between transports; a documented sensitivity-point in `architecture-overview.md > Risks (R3)` revisits this if a load test surfaces contention.

**Why this is the right answer**: tonic + axum is the de-facto Rust ecosystem stack for "gRPC server + HTTP server in one Tokio process" in 2025/2026. It is what `opentelemetry-collector-contrib` (the de-facto OTel Collector implementation in Go) inspires the Rust-side parallel of, and what `apollo-router`, `linkerd2-proxy`, and `tonic-web`-using services already use. No exotic choice is justified for a Phase-1 v0 gateway.

### Option B — `tonic` for gRPC, hand-rolled `hyper` for HTTP

**Pros**:
- One fewer dependency in the tree (`axum` + `tower-http` ≈ 30 KB compiled).
- Total control over the HTTP routing layer.

**Cons**:
- Hand-rolled hyper handlers for path-prefix routing reinvent the parts of `axum` that are 90% of `axum`'s value. Routing `/v1/logs` vs `/v1/traces` vs `/healthz` vs `/readyz` is not zero work without a router.
- `tower-http`'s middleware (e.g. `RequestBodyLimitLayer` for `body_too_large`) would still be needed; running `tower-http` on hand-rolled hyper requires more glue than `axum::Router::layer(...)`.
- Maintenance: every new endpoint or middleware (`X-Request-ID`, OpenTelemetry-spec `Retry-After` header on 503, etc.) is bespoke code.

**Rejected** because the dependency cost of `axum` + `tower-http` is small and the productivity cost of hand-rolling is real. The architecture document's Substrate stratum already exempts these from port discipline.

### Option C — `tonic` + `actix-web` (the older alternative HTTP server)

**Pros**:
- Mature; production-proven.

**Cons**:
- Built on its own actor system; not a `tokio::spawn` task model. Mixing actix's runtime with tonic's Tokio runtime is the kind of thing that can be done but should not be done (Tokio + actix can be made to coexist with `actix-rt`'s LocalSet shenanigans, but it's a known footgun).
- Library momentum has shifted to `axum` since 2022; new Rust HTTP-server work is overwhelmingly axum.
- Less idiomatic with `tower` middleware; integration with `tracing` is more bespoke.

**Rejected** because mixing async runtimes is a class of footgun nobody should choose deliberately at v0.

### Option D — `hyper-util` lower-level + manual gRPC framing

**Pros**:
- Maximum control, minimum dependency surface.

**Cons**:
- Reimplementing gRPC framing on top of HTTP/2 means reimplementing what `tonic` already gives for free. The `opentelemetry-proto`-generated service stubs only target `tonic`; without `tonic`, all the service-method dispatch is hand-rolled.
- Resume-driven development pattern: complexity disproportionate to the benefit.

**Rejected** outright.

## Consequences

### Positive
- The two canonical, production-proven Rust libraries for the two server protocols. Zero novelty risk.
- `tonic` was already in the dep tree; net addition is `axum` + `tower-http` only (and the latter is small).
- Forward-compatible with TLS (DISCUSS Q5): both `tonic` and `axum` accept rustls configurations identically.
- Forward-compatible with the readiness state machine (US-AP-02): `axum` Router state-extraction + the `ReadinessState` Arc make `/readyz` straightforward.
- Forward-compatible with the per-transport semaphore (DISCUSS D7): `tower::limit::ConcurrencyLimitLayer` is the standard pattern for HTTP; `tonic`'s tower-aware `Server::builder().layer(...)` is the symmetric pattern for gRPC.

### Negative
- Two libraries, two release cadences. Mitigated by caret pinning at the workspace level and by `cargo deny check`'s "advisories" gate (already present).
- `axum` is at major-version 0.x (currently 0.7); a 1.0 release before Phase 1 is non-trivial to handle (the 0.6→0.7 migration was a half-day exercise for many consumers). `tonic` is similarly at 0.12. Caret pinning protects us from accidental adoption; deliberate upgrades are a Phase-1+ task.

### Trade-off ATAM

This decision is a **sensitivity point** for **Performance Efficiency — Time behaviour** (the choice of running both servers on one Tokio runtime might introduce contention under burst load) and for **Reliability — Maturity** (the choice optimises for ecosystem-canonical libraries, accepting their 0.x cadence as a maturity caveat). It is **not a trade-off point** in the strict ATAM sense because no quality attribute is degraded by the choice — it strictly improves Functional Suitability, Compatibility, and Maintainability over the alternatives.
