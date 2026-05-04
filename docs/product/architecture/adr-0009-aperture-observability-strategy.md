# ADR-0009 — Aperture observability strategy: tracing-subscriber JSON to stderr, healthz/readyz on the OTLP HTTP listener, no metrics in v0

- **Status**: Accepted
- **Date**: 2026-05-04
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

DISCUSS Q6 locks Aperture's v0 observability surface:
- Structured JSON logs to stderr (levels error / warn / info / debug).
- HTTP `/healthz` (always 200 if up).
- HTTP `/readyz` (200 once both listeners bound, 503 during startup or shutdown drain).
- **No Prometheus or OTLP-out metrics in v0** — Pulse-shaped concern, deferred to Phase 4.
- **No telemetry-on-telemetry**: Aperture must NOT emit OTLP from itself. Verified by CI invariant `no_telemetry_on_telemetry`.

DISCUSS D1 locks the closed event-name vocabulary: 16 names at v0; renames are version-bump-able, additions are non-breaking. DISCUSS US-AP-01 through US-AP-09 specify the events emitted at each step; DISCUSS shared-artifacts-registry treats `log_event_vocabulary` as a contract.

DISCUSS US System Constraint 9: no panics on user input. A panic in a `/healthz` or `/readyz` handler is a fatal invariant violation (DISCUSS US-AP-05 failure_modes).

What DESIGN must lock:
1. The Rust logging library (the facade and the layer producing JSON-on-stderr).
2. The exact JSON shape (one event per line, level, timestamp, event-name field).
3. Where `/healthz` and `/readyz` live (separate admin port, OR co-located on the OTLP HTTP listener).
4. The four DESIGN-derived event names that crystallise the Earned-Trust probe contract.
5. The defensive panic-handler shape.
6. The `clippy::print_*` lint posture.

## Decision

- **Logging facade**: `tracing` (caret `^0.1`). Aperture and every later Kaleidoscope component will use this; consistency over the project lifetime.
- **JSON layer**: `tracing-subscriber` (caret `^0.3`) with `fmt::Layer::new().json().with_writer(std::io::stderr).flatten_event(true).with_current_span(false).with_span_list(false).with_target(false)`.
- **Event-name field**: every event names itself via the `event=<name>` field at the macro call site. Closed v0 vocabulary documented in `crates/aperture/src/observability/events.rs`.
- **Health/readiness endpoints**: co-located on the OTLP HTTP listener (port `:4318`). No separate admin port at v0. **Rationale**: DISCUSS US-AP-02 Solution explicitly says operators expect to probe one place. A separate admin port is a Phase-1+ option if security review demands it.
- **Closed v0 event vocabulary** (20 names: 16 from DISCUSS D1 + 4 DESIGN-derived for the Earned-Trust probe contract and config-load failure paths). The four additions (`health.startup.refused`, `config_validation_failed`, `internal_invariant_violation`, plus `request_received` already in D1's set) are listed in `component-design.md > Closed v0 event-name set`.
- **Panic handler**: `std::panic::set_hook` is set at startup to write a structured `event=internal_invariant_violation` line, then call `std::process::exit(70)` (EX_SOFTWARE). Default panic-unwind would leave the process limping; loud exit is the right shape for a service whose state machine has been compromised.
- **Clippy lints**: workspace `[lints.clippy]` sets `print_stdout = "deny"` and `print_stderr = "deny"`. The only stderr-writing path is `tracing`; this is structurally enforced.

## Alternatives Considered

### Option A — `tracing` + `tracing-subscriber` JSON layer to stderr (RECOMMENDED, accepted)

**Pros**:
- `tracing` is the de-facto Rust ecosystem logging facade since 2019; every modern Rust service uses it. Hyper, tonic, axum, reqwest all instrument via `tracing` internally — Aperture inherits structured spans for the libraries' internal events for free.
- `tracing-subscriber`'s JSON layer is the canonical implementation. Mature, fast, well-documented.
- One-event-per-line guarantee: the JSON formatter writes a complete line per `tracing::info!` etc. (subject to one known issue handled in Phase-1 below).
- Workspace-level `print_stdout`/`print_stderr` clippy denial structurally enforces "tracing is the only stderr-writing path".

**Cons**:
- `tracing-subscriber`'s JSON layer has a known issue where panic-during-format produces partial lines (a panic in a custom Display impl while formatting a field). DESIGN mitigates: every field that goes into a tracing event is a primitive (string, integer, bool); no `impl Display` evaluated lazily during format. Plus the panic handler exits the process before more partial lines can be written.
- `tracing-subscriber`'s `EnvFilter` (`APERTURE_LOG=debug`) interprets the filter at every event; a verbose filter is a performance hit. Acceptable: production filters are `info`, which is fast.

### Option B — `slog` (older alternative)

**Pros**:
- Mature.

**Cons**:
- `slog` is in maintenance mode since 2022; community momentum is on `tracing`.
- Less well-integrated with hyper/tonic/axum (those crates instrument via `tracing`).

**Rejected** for community-momentum reasons.

### Option C — `log` facade + custom stderr writer

**Pros**:
- The smallest-possible dep tree.

**Cons**:
- `log` does not carry structured fields; everything is free-text. Operator log aggregators (Loki, Elasticsearch) parse better against structured JSON; with `log` the parsers have to regex-extract.
- Re-implementing the JSON formatter is a non-trivial amount of code (timestamps in RFC 3339; correct escaping; one-line-per-event guarantees; level mapping). Existing solutions exist for a reason.

**Rejected** for the structured-log gap.

### Option D — `slog` to stderr + `prometheus` exporter for in-memory counters

**Cons**:
- DISCUSS Q6 explicitly forbids `/metrics` at v0.
- Two libraries (logging + metrics) where one (tracing) suffices for v0.

**Rejected** outright by DISCUSS Q6.

### Option E — Separate admin port for `/healthz` and `/readyz`

**Pros**:
- Cleaner separation of concerns (OTLP traffic vs operator probes on different listeners).
- Some security reviews prefer this shape (operator probes do not share a port with the public OTLP surface).

**Cons**:
- DISCUSS US-AP-02 Solution explicitly says "operators expect to probe one place". The shared-port shape matches the OTel Collector convention and the Kubernetes idiom.
- Three listeners (gRPC + HTTP-OTLP + HTTP-admin) is more operational surface than two.
- The argument "the public OTLP surface should not serve operator probes" assumes operator probes are sensitive; they are not (`/healthz` returns "ok\n", `/readyz` returns "ready\n" or "starting\n" or "draining\n"; none of this is sensitive).

**Rejected** for v0 in favour of operator-expected single-port shape. Phase-1+ revisit gate: if a security review surfaces a reason to separate, the change is a one-`Router`-build refactor.

## Consequences

### Positive
- Standard Rust observability stack; nothing exotic.
- Structured JSON to stderr matches every operator log aggregator (Loki, Fluentd, journald, ES).
- `/healthz` + `/readyz` on the OTLP HTTP listener match operator expectations and the Kubernetes idiom.
- No metrics endpoint at v0; no Prometheus dep; no telemetry-on-telemetry concern.
- The closed event vocabulary makes operator-side parsing deterministic; future Pulse instrumentation (Phase 4) can ingest the same vocabulary without renaming.
- Workspace-level `print_*` lint denial structurally enforces "tracing is the only stderr writer".

### Negative
- Aperture's stderr is the operator's only operational telemetry at v0. Operators on bare metal without a log aggregator see only `journalctl -u aperture` or similar; that is acceptable for v0 (Pulse Phase 4 will provide richer telemetry). Documented in the operator runbook DEVOPS will write.
- The closed event vocabulary requires every event-emitting site to use the constants in `observability/events.rs`. Discipline; structurally enforced by `xtask` AST walk asserting `tracing::info!(event = "..."` literal calls fail (must be `event = event::CONSTANT_NAME`).

### Earned-Trust probe events (DESIGN additions to DISCUSS D1)

The Earned-Trust probe contract (Principle 12; ADR-0007) introduces one new event:

- `health.startup.refused` — emitted when a sink probe fails at composition root; process exits 1 immediately. The dot-namespace name reflects the convention used by k8s probes (`livenessProbe.failureThreshold`, etc.) and gives operators a grep-friendly distinction from the regular `health` request stream.

DESIGN also adds three failure-path events:
- `config_validation_failed` — emitted before exit when config invariants fail.
- `internal_invariant_violation` — emitted by the panic handler before exit.
- `body_too_large` — listed in DISCUSS D1; mapped here to its concrete tonic / axum trigger.

These four are non-breaking additions under DISCUSS D1's evolution rules ("additions are non-breaking").

### Trade-off ATAM

**Sensitivity point** for **Maintainability — Analysability** (the closed event vocabulary IS the operator's parsing schema) and for **Operational Simplicity** (single-port operator probes match the Kubernetes idiom).

**Trade-off point**: Operational Simplicity vs Security boundary. The single-port shape exposes operator probes on the same listener as the public OTLP surface; a separate admin port would be safer for some security models. Bias toward Operational Simplicity is correct at v0 (the probes are not sensitive); revisit at Aegis if security review demands.

### Phase-1 revisit gates

- If `tracing-subscriber`'s JSON layer's known partial-line-on-panic issue surfaces in the panic-handler integration test, switch to a custom `MakeWriter` that buffers per-event and flushes line-atomically. Documented as `component-design.md > Open issues for DELIVER (4)`.
- If a Pulse-driven Phase-4 metrics endpoint lands, evaluate whether to keep the closed-event vocabulary OR migrate to span-derived metrics. The vocabulary is forward-compatible either way.
- If a security review at Aegis (Phase 2) demands `/healthz` + `/readyz` on a separate admin port, refactor `transport::http` into two routers and bind the admin one separately. One-day change.
