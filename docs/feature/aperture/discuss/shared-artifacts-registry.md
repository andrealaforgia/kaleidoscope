# Shared Artefacts Registry — Aperture v0

> **Wave**: DISCUSS — Phase 2 (Journey Visualisation, integration check).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-04.
> **Companion documents**: `journey-aperture.yaml`, `journey-aperture-visual.md`, `journey-aperture.feature`, `user-stories.md`.

Every `${variable}` referenced in any of Aperture's discuss artefacts has a single source of truth recorded in this file. Drift between source and consumer is the primary failure mode this registry exists to prevent.

The registry is grouped by integration risk. **HIGH** items break the wire contract or the contract Aperture has with the harness; **MEDIUM** items break operator expectations or schema forward-compatibility; **LOW** items are cosmetic but worth tracking.

---

## HIGH-risk artefacts (wire / harness / sink contracts)

### `harness_function_logs`

| Field | Value |
|---|---|
| Source of truth | `crates/otlp-conformance-harness/src/lib.rs` :: `pub fn validate_logs(bytes: &[u8], framing: Framing) -> Result<ExportLogsServiceRequest, OtlpViolation>` |
| Displayed as | `otlp_conformance_harness::validate_logs` |
| Consumers | Aperture's logs validate-and-route module (Slice 01); `journey-aperture.yaml` step 3; `journey-aperture.feature` validate scenarios; `user-stories.md` US-AP-03, US-AP-08, US-AP-09 |
| Owner | `otlp-conformance-harness-v0` (locked by ADR-0001 of that feature) |
| Integration risk | HIGH — Aperture has exactly one validation gate; any duplicate or alternative validator is a contract violation |
| Validation | `cargo public-api` over the harness crate (Gate 2 of harness ADR-0005) catches signature drift. Aperture's CI grep gate `single_validator_per_signal` (named in `journey-aperture.yaml > integration_validation > ci_invariants`) catches duplicate-call-site drift. |

### `harness_function_traces`

| Field | Value |
|---|---|
| Source of truth | `crates/otlp-conformance-harness/src/lib.rs` :: `pub fn validate_traces` |
| Displayed as | `otlp_conformance_harness::validate_traces` |
| Consumers | Aperture's traces validate-and-route module (Slice 03); journey step 3; `user-stories.md` US-AP-08 |
| Owner | `otlp-conformance-harness-v0` |
| Integration risk | HIGH (same as logs) |
| Validation | Same as logs. |

### `harness_function_metrics`

| Field | Value |
|---|---|
| Source of truth | `crates/otlp-conformance-harness/src/lib.rs` :: `pub fn validate_metrics` |
| Displayed as | `otlp_conformance_harness::validate_metrics` |
| Consumers | Aperture's metrics validate-and-route module (Slice 04); journey step 3; `user-stories.md` US-AP-09 |
| Owner | `otlp-conformance-harness-v0` |
| Integration risk | HIGH (same as logs) |
| Validation | Same as logs. |

### `framing_enum`

| Field | Value |
|---|---|
| Source of truth | `crates/otlp-conformance-harness/src/framing.rs` :: `pub enum Framing { HttpProtobuf, GrpcProtobuf }` |
| Displayed as | `Framing::GrpcProtobuf`, `Framing::HttpProtobuf` |
| Consumers | Every Aperture call into the harness must pass the variant matching the inbound transport |
| Owner | `otlp-conformance-harness-v0` |
| Integration risk | HIGH — passing the wrong framing causes false positives or false negatives that the harness cannot detect |
| Validation | Aperture's transport-to-framing mapping has a unit test enumerating both transports against the matching enum variant. |

### `violation_display`

| Field | Value |
|---|---|
| Source of truth | `crates/otlp-conformance-harness/src/violation.rs` :: `impl Display for OtlpViolation` |
| Displayed as | Single-line: `otlp violation: rule=... signal=... framing=... locus=... expected=... observed=...` |
| Consumers | gRPC `grpc-message` header on reject; HTTP response body on reject (text/plain) |
| Owner | `otlp-conformance-harness-v0` |
| Integration risk | HIGH — Aperture must not reformat, truncate, or replace this string. Doing so would force consumers to maintain two parsers. |
| Validation | UAT scenarios in `journey-aperture.feature` assert the response body / grpc-message contains `rule=...`, `signal=...`, etc. directly; the assertions match the harness's Display contract verbatim. |

### `grpc_port`

| Field | Value |
|---|---|
| Source of truth | `aperture/config.toml` :: `aperture.transport.grpc.bind_addr` (default `0.0.0.0:4317`) |
| Displayed as | `${grpc_port}` (default `4317`) |
| Consumers | TCP listener bind in Slice 01; `stderr listener_bound` event; OTLP/gRPC client connection target documented to operators; `user-stories.md` US-AP-01 |
| Owner | Aperture |
| Integration risk | HIGH — port 4317 is the OTel-canonical gRPC port. Drift breaks every standard SDK out of the box. |
| Validation | UAT scenario "Both OTLP listeners bind on configured ports" asserts the listener is on 4317 by default. Config-schema test (DESIGN-owned) asserts the default matches. |

### `http_port`

| Field | Value |
|---|---|
| Source of truth | `aperture/config.toml` :: `aperture.transport.http.bind_addr` (default `0.0.0.0:4318`) |
| Displayed as | `${http_port}` (default `4318`) |
| Consumers | TCP listener bind; `POST /v1/{logs,traces,metrics}` target; `/healthz` and `/readyz` (multiplexed on same port); `user-stories.md` US-AP-02 |
| Owner | Aperture |
| Integration risk | HIGH — port 4318 is the OTel-canonical HTTP port. Same drift hazard as `grpc_port`. |
| Validation | Same as `grpc_port`. |

### `sink_trait`

| Field | Value |
|---|---|
| Source of truth | DESIGN-wave ADR (Morgan locks the exact signature). DISCUSS specifies the contract: a `Send + Sync` trait with an async `accept(record) -> Result<(), SinkError>` method. |
| Displayed as | `OtlpSink` |
| Consumers | `StubSink` (Slice 01); `ForwardingSink` (Slice 06); future Sieve `impl OtlpSink` (Phase 1) |
| Owner | Aperture |
| Integration risk | HIGH — this trait IS the Aperture/Sieve boundary the Phase-1 component will plug into. Any later refactor that changes the trait shape is a breaking integration. |
| Validation | DESIGN-wave ADR captures the exact signature. Once locked, `cargo public-api` over the Aperture crate catches drift. |

### `sink_record_enum`

| Field | Value |
|---|---|
| Source of truth | DESIGN-wave ADR. DISCUSS specifies: a sum type with one variant per OTLP signal, each variant carrying the upstream `opentelemetry_proto` record type unwrapped. |
| Displayed as | `SinkRecord::{Logs,Traces,Metrics}` |
| Consumers | Every sink impl |
| Owner | Aperture |
| Integration risk | HIGH — wrapping the harness's typed return in anything other than this enum would force every sink to convert types at the hot path, defeating the type-path identity guarantee from harness US-04 AC 2. |
| Validation | Static check: `SinkRecord` variants reference `opentelemetry_proto::tonic::collector::{logs,trace,metrics}::v1::Export*ServiceRequest` directly, no harness-local wrappers. |

---

## MEDIUM-risk artefacts (operator-facing schema and observability)

### `aperture_version`

| Field | Value |
|---|---|
| Source of truth | `crates/aperture/Cargo.toml` :: `package.version` |
| Displayed as | `${aperture_version}` |
| Consumers | stderr `startup` event; `User-Agent` string on `ForwardingSink` outbound (Slice 06); operator inventory tools |
| Owner | Aperture |
| Integration risk | MEDIUM — drift between Cargo metadata and the runtime version string makes incident triage harder |
| Validation | Aperture reads `env!("CARGO_PKG_VERSION")` once at startup. Hand-written test asserts the runtime value matches the build-time value. |

### `max_recv_msg_size`

| Field | Value |
|---|---|
| Source of truth | `aperture/config.toml` :: `aperture.transport.{grpc,http}.max_recv_msg_size` (default `4 MiB` per transport) |
| Displayed as | `${max_recv_msg_size}` |
| Consumers | tonic `Server` builder configuration; hyper body limit; stderr `body_too_large` events; rejection responses (gRPC `RESOURCE_EXHAUSTED` or HTTP 413) |
| Owner | Aperture |
| Integration risk | MEDIUM — too small breaks legitimate large batches; too large invites memory pressure. The default is operator-tunable. |
| Validation | UAT scenario "A body exceeding max_recv_msg_size is refused" asserts behaviour at the configured boundary. |

### `max_concurrent_requests` (per transport)

| Field | Value |
|---|---|
| Source of truth | `aperture/config.toml` :: `aperture.transport.{grpc,http}.max_concurrent_requests` (default `1024` per transport — placeholder, DESIGN may revisit) |
| Displayed as | `${max_concurrent_requests}` |
| Consumers | per-transport semaphore in Slice 05; gRPC `RESOURCE_EXHAUSTED` `grpc-message`; HTTP 503 response body; stderr `concurrency_cap_hit` events |
| Owner | Aperture |
| Integration risk | MEDIUM — bound is the load contract. Tunability without re-deploy is desirable; the default value's correctness is a v0 calibration problem, not a contract break. |
| Validation | UAT scenarios "gRPC concurrency cap reached" and "HTTP concurrency cap reached". The `@property` UAT asserts non-silent-drop across all caps. |

### `drain_deadline_ms`

| Field | Value |
|---|---|
| Source of truth | `aperture/config.toml` :: `aperture.shutdown.drain_deadline_ms` (default `30000`) |
| Displayed as | `${drain_deadline_ms}` |
| Consumers | shutdown handler; `stderr shutdown_initiated`; `stderr drain_deadline_exceeded` |
| Owner | Aperture |
| Integration risk | MEDIUM — too short drops in-flight on every restart; too long blocks orchestrator-level rolling restarts |
| Validation | UAT scenarios "Graceful shutdown drains in-flight requests" and "Drain deadline exceeded is observable, never silent". |

### `downstream_endpoint`

| Field | Value |
|---|---|
| Source of truth | `aperture/config.toml` :: `aperture.sink.forwarding.endpoint` (Slice 06; required when sink=forwarding) |
| Displayed as | `${downstream_endpoint}` |
| Consumers | `ForwardingSink` HTTP/gRPC client target; stderr `sink_accepted` event |
| Owner | Aperture |
| Integration risk | MEDIUM — misconfigured endpoint sends valid records into a black hole. ForwardingSink failures are loud (sink_failed events), so the failure mode is observable. |
| Validation | UAT scenarios "ForwardingSink writes downstream and propagates success" and "ForwardingSink refusal becomes UNAVAILABLE upstream". |

### `readyz_state_machine`

| Field | Value |
|---|---|
| Source of truth | `journey-aperture.yaml` step 5 + step 6; DESIGN-wave ADR captures the state-machine implementation |
| Displayed as | `starting -> ready -> draining` |
| Consumers | `/readyz` handler; stderr `readiness_changed` events; k8s readiness probe (operator-provided) |
| Owner | Aperture |
| Integration risk | MEDIUM — state-machine drift breaks the orchestrator's expectation that a 503 `/readyz` means "do not route" |
| Validation | UAT scenarios "Readiness probe is 503 during startup" and "Graceful shutdown drains in-flight requests" together cover the three-state transition. |

### `tls_config_schema`

| Field | Value |
|---|---|
| Source of truth | `aperture/config.toml` :: `aperture.security.tls.{enabled,cert_path,key_path}` and `aperture.security.spiffe.{enabled,trust_domain}` |
| Displayed as | TOML keys above |
| Consumers | v0: no-op except for the warn-line when `tls.enabled=true`. Phase 2 (Aegis): the actual TLS / SPIFFE integration. |
| Owner | Aperture (schema); Aegis (Phase 2 behaviour) |
| Integration risk | MEDIUM — the schema's job is forward compatibility. If v0 ships without these keys, Phase 2 has to break the schema, which is the exact failure this registry exists to prevent. |
| Validation | Config-schema test asserts the keys are accepted (and ignored, with a warn for `tls.enabled=true`) at v0. UAT scenario "TLS knob set true on v0 emits a warning and continues plaintext". |

---

## LOW-risk artefacts (vocabulary and convention)

### `log_event_vocabulary`

| Field | Value |
|---|---|
| Source of truth | `journey-aperture.yaml` step-by-step `tui_mockup` blocks; `journey-aperture-visual.md` |
| Displayed as | The closed set: `{startup, listener_bound, listener_closing, listener_bind_failed, ready, readiness_changed, request_received, sink_accepted, sink_failed, shutdown_initiated, shutdown_complete, in_flight_drained, drain_deadline_exceeded, unsupported_media_type, body_too_large, concurrency_cap_hit, tls_not_supported_in_v0}` |
| Consumers | every stderr line in v0; operator's log-aggregator parsers; Pulse instrumentation (Phase 4) for naming consistency |
| Owner | Aperture |
| Integration risk | LOW — adding a new event name is additive. The risk is renaming an existing one, which breaks operator queries. |
| Validation | DESIGN-wave: a static `pub enum LogEvent` (or `&'static str` constants) in the Aperture crate locks the vocabulary. Renames require a version bump documented in the changelog. |

### `request_received_event_schema`

| Field | Value |
|---|---|
| Source of truth | `journey-aperture.yaml` step 2 mockup |
| Displayed as | `{event:"request_received", transport, signal, bytes, peer}` |
| Consumers | every step-2 stderr line; operator's log-aggregator parsers |
| Owner | Aperture |
| Integration risk | LOW — schema additions are non-breaking; field renames are. |
| Validation | DESIGN-wave: a single Rust struct with `serde::Serialize` + an integration test asserting the JSON shape. |

### `otlp_spec_version`

| Field | Value |
|---|---|
| Source of truth | `crates/otlp-conformance-harness/src/lib.rs` :: `pub const OTLP_SPEC_VERSION: &str = "1.5.0";` |
| Displayed as | `OTLP_SPEC_VERSION` |
| Consumers | Aperture's `User-Agent` string and `aperture --version` output (informational); operators' compatibility matrices |
| Owner | `otlp-conformance-harness-v0` |
| Integration risk | LOW — informational only; Aperture does not behave differently at runtime based on this value. |
| Validation | Aperture re-exports the constant verbatim; CI gate ensures the value is not shadowed. |

---

## CI invariants enforced by this registry

The registry is not just a document — it names two CI-enforced invariants. Both are reiterated in `journey-aperture.yaml > integration_validation > ci_invariants` for the DEVOPS wave to pick up:

| Invariant | Mechanism | Owner |
|---|---|---|
| `no_telemetry_on_telemetry` | Integration test in a constrained network namespace asserts Aperture opens no outbound connections to ports 4317/4318 except via `ForwardingSink` to the operator-configured endpoint. | DEVOPS workflow YAML; named here as a contract. |
| `single_validator_per_signal` | Static (grep + AST) check in Aperture's CI: exactly one call site per `validate_logs`, `validate_traces`, `validate_metrics`. No fallback validator, no wrapper. | Aperture crate's CI; named here as a contract. |

---

## How to add a shared artefact to this registry

When a new `${variable}` enters any DISCUSS artefact:

1. Add a section above with all six fields (Source of truth, Displayed as, Consumers, Owner, Integration risk, Validation).
2. Cross-reference any UAT scenario or CI invariant that defends the artefact.
3. If the artefact is HIGH-risk, surface the dependency to Morgan (DESIGN) explicitly in `wave-decisions.md`.

When an existing `${variable}` is renamed or its source moves:

1. Update this registry first.
2. Walk the consumers list and update each.
3. Update the corresponding UAT scenarios.
4. Re-run peer review before handoff to DESIGN.
