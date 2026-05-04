<!-- markdownlint-disable MD024 -->

# User Stories — `aperture` v0

> **Wave**: DISCUSS — Phase 3 (Requirements crafting).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-04.
> **Companion documents**: `journey-aperture.yaml`, `journey-aperture.feature`, `story-map.md`, `prioritization.md`, `outcome-kpis.md`, `dor-validation.md`, `wave-decisions.md`.

> Persona note: Aperture's consumers are OpenTelemetry SDK clients (machine-to-service), the future Sieve component (component-to-component via the `OtlpSink` trait), third-party engineers operating Kaleidoscope in production, and Kaleidoscope CI. House style: British English, no human-effort estimation.

---

## System Constraints

These constraints apply to every story below and are not repeated in each. They are the load-bearing decisions Andrea locked before this DISCUSS round.

1. **Service, not library**: Aperture is a long-lived Rust process. It binds two TCP listeners (gRPC `:4317`, HTTP/protobuf `:4318`), accepts traffic, and only exits on signal or fatal error.
2. **Tokio runtime**: all asynchronous I/O runs on Tokio; no alternative runtime, no mixing.
3. **Both transports at v0**: gRPC and HTTP/protobuf ship together at v0. Neither is deferred.
4. **Single validation gate**: every accepted byte sequence flows through exactly one `otlp_conformance_harness::validate_*` call. No alternative validator, no wrapper. The harness's `OtlpViolation::Display` output IS the wire-error message — gRPC `grpc-message` header on reject, HTTP response body on reject.
5. **Sink boundary**: a trait `OtlpSink` with method `async fn accept(&self, record: SinkRecord) -> Result<(), SinkError>` is the Aperture/Sieve boundary. v0 ships two implementations: `StubSink` (logs to stderr) and `ForwardingSink` (writes OTLP to a configured downstream). When Sieve lands in Phase 1 it will be `impl OtlpSink`.
6. **Deterministic backpressure**: each transport carries a configurable `max_concurrent_requests`. Once reached: gRPC `RESOURCE_EXHAUSTED`, HTTP 503 with `Retry-After`. **No internal queue, no block, no silent drop.**
7. **Plaintext at v0, schema-forward-compatible**: TLS / SPIFFE keys exist in the v0 config schema, default off; setting `tls.enabled=true` on v0 emits a single warn-level stderr line and continues plaintext. Schema is forward-compatible with Aegis (Phase 2).
8. **Observability is stderr + healthz + readyz, nothing else**: structured JSON logs to stderr (levels error/warn/info/debug); `GET /healthz` (200 always while up); `GET /readyz` (200 once both listeners bound and not draining). **No `/metrics`, no OTLP-out from Aperture itself** — telemetry-on-telemetry is Pulse's job in Phase 4.
9. **No panics on user input**: every error path returns a structured response. Panicking is reserved for true invariant violations of Aperture's own internal logic.
10. **British English**, **no human-effort estimation**, **trunk-based development**.

---

## US-AP-01 — Bind both OTLP listeners at startup

### Elevator Pitch

- **Before**: An operator deploying Aperture has no way to know whether the process is actually listening on the OTLP-canonical ports until they try to point an SDK at it. A misconfiguration silently fails open, with the SDK's connection-refused error being the first symptom.
- **After**: Aperture starts, binds gRPC on `0.0.0.0:4317` and HTTP/protobuf on `0.0.0.0:4318`, and writes a structured stderr line per listener: `{"event":"listener_bound","transport":"grpc","addr":"0.0.0.0:4317"}`. The operator confirms the listeners are up by `curl -fsS http://localhost:4318/readyz`, which returns `ready` once both bindings completed.
- **Decision enabled**: The operator decides whether to flip the load balancer to route traffic to this Aperture instance. They have a definitive readiness signal (`/readyz` 200) and a per-listener confirmation on stderr.

### Problem

A multi-listener service has multiple ways to fail at startup: one port can be in use, the other not; one can be permission-denied, the other not. Without per-listener stderr lines and a readiness probe that flips only after BOTH listeners are bound, an operator's first signal of trouble is whatever their SDK or load balancer reports. That signal is too far from the cause for fast triage.

### Who

- **Operator deploying Aperture** (third-party engineer running Kaleidoscope): needs a definitive "this instance is up and listening" signal so the orchestrator's readiness probe can flip the instance into rotation.
- **OpenTelemetry SDK client**: needs the gRPC :4317 and HTTP :4318 ports to be standard — anything else breaks SDK auto-configuration.
- **Kaleidoscope CI**: runs an OTel SDK against a freshly-built Aperture; the SDK's connection success IS the integration test for Slice 01.

### Solution

Aperture's startup binds both listeners and emits a structured stderr line per success. `/readyz` returns 200 only when both listeners have bound. Bind failure (port in use, permission denied) is loud — stderr error line, exit code non-zero, `/readyz` never reaches 200.

### Domain Examples

#### 1: A clean startup on a fresh host

`acme-observability` deploys Aperture v0 on a Kubernetes pod with no other process listening on 4317 or 4318. The pod's k8s readiness probe is `GET /readyz`. Aperture writes three stderr lines (startup, listener_bound x2, ready), the readiness probe goes 200, k8s adds the pod to the Service's endpoints, and the cluster's OTel Collector starts forwarding traffic to it. Time from process exec to ready: under 100 ms.

#### 2: Port collision on a misconfigured host

A second Aperture instance is mistakenly scheduled to the same host with `hostNetwork: true`. The first instance has bound 4317. The second instance's startup writes `{"level":"error","event":"listener_bind_failed","transport":"grpc","addr":"0.0.0.0:4317","reason":"address already in use"}` to stderr, exits 1, and `/readyz` never returned 200 in its lifetime. The k8s scheduler restarts the pod; the operator's runbook points them at the stderr line.

#### 3: TLS knob set true on v0

An operator porting their config from a v1+ Aperture to v0 leaves `tls.enabled = true` set. v0 ignores TLS but does not silently swallow the misconfiguration: stderr gets one warn line (`event=tls_not_supported_in_v0`), the listeners still bind plaintext, `/readyz` reaches 200. The operator notices the warn during their post-deploy log review and corrects the config.

### UAT Scenarios (BDD)

#### Scenario: Both OTLP listeners bind on configured ports

Given Aperture is started with bind addrs grpc=0.0.0.0:4317 and http=0.0.0.0:4318
When the process completes startup
Then a TCP listener is accepting connections on 4317
And a TCP listener is accepting connections on 4318
And stderr contains a JSON line with event=listener_bound transport=grpc addr=0.0.0.0:4317
And stderr contains a JSON line with event=listener_bound transport=http_protobuf addr=0.0.0.0:4318
And GET /readyz returns 200 with body "ready"

#### Scenario: Port already in use produces a structured failure

Given another process is already listening on 0.0.0.0:4317
When Aperture is started with the default config
Then Aperture exits with a non-zero status code
And stderr contains a JSON line with level=error, event=listener_bind_failed, transport=grpc, addr=0.0.0.0:4317
And /readyz never returned 200 during the process lifetime

#### Scenario: TLS knob set true on v0 emits a warning and continues plaintext

Given the configuration sets aperture.security.tls.enabled = true
When Aperture is started
Then stderr contains a JSON line with level=warn, event=tls_not_supported_in_v0
And the listeners bind in plaintext mode
And GET /readyz returns 200 with body "ready"

#### Scenario: Identical bind addresses for grpc and http are rejected at config validation

Given the configuration sets aperture.transport.grpc.bind_addr = aperture.transport.http.bind_addr
When Aperture is started
Then Aperture exits with a non-zero status code
And stderr contains a JSON line with level=error, event=config_validation_failed, reason="grpc and http bind addresses must differ"
And no listener attempts to bind

### Acceptance Criteria

- [ ] gRPC listener accepts connections on 0.0.0.0:4317 after startup completes.
- [ ] HTTP listener accepts connections on 0.0.0.0:4318 after startup completes.
- [ ] One `event=listener_bound` stderr JSON line is emitted per listener after binding.
- [ ] `GET /readyz` returns 503 with body `starting` until both listeners are bound.
- [ ] `GET /readyz` returns 200 with body `ready` after both listeners are bound.
- [ ] Failure to bind either port produces an `event=listener_bind_failed` error stderr line and exits the process non-zero.
- [ ] `tls.enabled=true` on v0 produces exactly one `event=tls_not_supported_in_v0` warn stderr line and listeners still bind plaintext.
- [ ] Configuration with identical `grpc.bind_addr` and `http.bind_addr` is rejected at startup with an `event=config_validation_failed` error line; no listener attempts to bind.

### Outcome KPIs

- **Who**: operators deploying Aperture v0.
- **Does what**: rely on `/readyz` as the single-source readiness signal, instead of probing both listeners individually.
- **By how much**: 100% of v0 deployments use `/readyz` for readiness gating (target: post-launch survey of three pilot operators returns "yes, /readyz is what we use").
- **Measured by**: pilot operator interviews 30 days post-Phase-1 launch.
- **Baseline**: greenfield (zero operators today; the v0 launch establishes the practice).

### Technical Notes

- Slice 01 lands the gRPC listener; Slice 02 lands the HTTP listener and `/readyz` / `/healthz`. Until Slice 02, `/readyz` is unreachable (the HTTP port is not listening); `/readyz`'s contract therefore lands fully in Slice 02 even though this story names it.
- `aperture.transport.grpc.bind_addr` and `aperture.transport.http.bind_addr` are config keys with default `0.0.0.0:4317` and `0.0.0.0:4318` respectively.
- DESIGN-wave decision (Morgan): which Tokio Server library handles HTTP (likely `hyper`); which gRPC server (likely `tonic`); how the two share the runtime.

### Dependencies

None at the story level. Slice 01 depends on the harness crate's locked public API (Phase 0 deliverable, already shipped per `crates/otlp-conformance-harness/src/lib.rs`).

---

## US-AP-02 — Serve health, readiness, and HTTP/protobuf on `:4318`

### Elevator Pitch

- **Before**: After US-AP-01 lands the gRPC listener, Aperture has no HTTP surface. Operators with Kubernetes-style deployments cannot probe liveness or readiness, and SDKs that prefer or require OTLP/HTTP/protobuf cannot send anything at all.
- **After**: `GET http://localhost:4318/healthz` returns 200 `"ok"` while the process is up; `GET http://localhost:4318/readyz` returns 200 `"ready"` once both listeners are bound, 503 `"starting"` before that. `POST http://localhost:4318/v1/logs` with `Content-Type: application/x-protobuf` and a real `ExportLogsServiceRequest` body returns HTTP 200, exercising the same harness call site as the gRPC arm.
- **Decision enabled**: The k8s operator decides whether to add a readiness/liveness probe on `:4318`; the SDK author decides whether to use the gRPC or HTTP exporter — both are first-class.

### Problem

The HTTP listener carries three concerns: OTLP `/v1/{logs,traces,metrics}`, liveness probe, readiness probe. They share a port deliberately — operators expect to probe one place — but they need disjoint routing. Until this story lands, the HTTP listener does not exist, so neither probe nor HTTP exporter has a target.

### Who

- **Operator deploying Aperture in Kubernetes**: needs `livenessProbe` and `readinessProbe` HTTP endpoints to keep the pod's lifecycle healthy.
- **OpenTelemetry SDK author** picking between gRPC and HTTP exporters: many SDKs default to HTTP/protobuf because gRPC is heavier; both must be first-class at v0.
- **OpenTelemetry SDK client** (e.g. `opentelemetry-rust`'s HTTP exporter): the path is `/v1/{logs,traces,metrics}`, the Content-Type is `application/x-protobuf`, the response is HTTP 200 on accept.

### Solution

The HTTP listener routes by path-prefix: `/v1/{signal}` -> harness validation -> sink; `/healthz` -> always 200 `"ok"`; `/readyz` -> readiness state machine (`starting` -> `ready` in this story; `draining` added in Slice 08).

### Domain Examples

#### 1: k8s readiness probe in a rolling update

`acme-observability`'s Aperture Deployment specifies `readinessProbe: { httpGet: { path: /readyz, port: 4318 }, periodSeconds: 1 }`. During a rolling update, a new pod's startup goes through `starting` -> `ready` in under a second; k8s adds it to the Service endpoints; traffic routes to it; the old pod is terminated.

#### 2: An OTel Python SDK using the HTTP exporter

A Python application uses `OTLPLogExporter(endpoint="http://aperture:4318/v1/logs")`. The exporter POSTs `application/x-protobuf` bodies. Aperture validates each via `validate_logs(bytes, Framing::HttpProtobuf)` and returns HTTP 200 on accept. The Python app sees no errors; logs flow through.

#### 3: A misconfigured client POSTs JSON

A developer at `acme-observability` writes a custom integration that posts JSON bodies to `/v1/logs` (a misunderstanding — OTLP/HTTP is protobuf-only at v1.5.0). Aperture returns HTTP 415 `Unsupported Media Type` with a stderr `event=unsupported_media_type` line. The developer reads the stderr line in their Aperture instance's logs and corrects the integration.

### UAT Scenarios (BDD)

#### Scenario: Liveness probe always succeeds while the process is up

Given Aperture's HTTP listener is bound on port 4318
When a client GETs /healthz
Then the response status is 200
And the body is "ok"

#### Scenario: Readiness probe is 503 during startup, 200 once listeners are bound

Given Aperture has just been launched
When a client GETs /readyz before listeners have bound
Then the response status is 503
And the body is "starting"
When both listeners have bound
And the client GETs /readyz again
Then the response status is 200
And the body is "ready"

#### Scenario: HTTP/protobuf accepts a valid logs export

Given Aperture's HTTP listener is bound on port 4318
And a real ExportLogsServiceRequest body has been captured from the OpenTelemetry Rust SDK
When the client POSTs that body to /v1/logs with Content-Type application/x-protobuf
Then the response status is 200
And stderr contains a JSON line with event=sink_accepted signal=logs

#### Scenario: HTTP/protobuf refuses the wrong content type

Given Aperture's HTTP listener is bound on port 4318
When a client POSTs /v1/logs with Content-Type application/json
Then the response status is 415
And stderr contains a JSON line with level=warn event=unsupported_media_type

#### Scenario: HTTP/protobuf returns 404 for an unknown OTLP path

Given Aperture's HTTP listener is bound on port 4318
When a client POSTs /v1/profile with Content-Type application/x-protobuf
Then the response status is 404

### Acceptance Criteria

- [ ] HTTP listener accepts on 0.0.0.0:4318.
- [ ] `GET /healthz` -> 200 `"ok"` always while process up.
- [ ] `GET /readyz` -> 503 `"starting"` before listeners bind.
- [ ] `GET /readyz` -> 200 `"ready"` after both listeners bind.
- [ ] `POST /v1/logs` with valid body and correct Content-Type -> HTTP 200, sink_accepted stderr line emitted.
- [ ] `POST /v1/logs` with `application/json` -> HTTP 415 + warn stderr.
- [ ] `POST /v1/profile` -> HTTP 404.

### Outcome KPIs

- **Who**: SDK clients using the HTTP/protobuf exporter.
- **Does what**: send valid OTLP exports without protocol-level errors.
- **By how much**: HTTP-protobuf success ratio of at least 99.9% (HTTP 200 / total HTTP /v1/* requests with valid body and correct Content-Type).
- **Measured by**: operator's log aggregator counts stderr `sink_accepted` events with `transport=http_protobuf` divided by stderr `request_received` with `transport=http_protobuf`.
- **Baseline**: greenfield.

### Technical Notes

- DESIGN locks the HTTP routing mechanism (likely `axum` or hand-rolled hyper handlers).
- `/healthz` and `/readyz` share the OTLP listener port intentionally — operators expect one port. DESIGN may revisit if security review surfaces a reason to separate them onto an admin port.

### Dependencies

US-AP-01.

---

## US-AP-03 — Accept a valid logs export and acknowledge the sink

### Elevator Pitch

- **Before**: After US-AP-01 and US-AP-02, the listeners are up but the validation pipeline is not wired through. A real OTel SDK pointing at Aperture would get a connection but no acknowledgement.
- **After**: A real OpenTelemetry Rust SDK 0.27 invokes its OTLP/gRPC exporter against `grpc://localhost:4317` (or its HTTP exporter against `POST http://localhost:4318/v1/logs`) with one log record carrying `resource.service.name="payments-api"`. Aperture validates via `otlp_conformance_harness::validate_logs(bytes, framing)`, hands the typed record to `StubSink::accept`, the sink writes one stderr JSON line `{"event":"sink_accepted","sink":"stub","signal":"logs","record_count":1,"resource.service.name":"payments-api"}`, and the SDK receives gRPC OK / HTTP 200.
- **Decision enabled**: The SDK author decides their integration is working — the SDK reports the export succeeded, no errors, and they can grep their Aperture instance's stderr for the service name they expect to see.

### Problem

The whole point of Aperture is to accept OTLP. Until one valid export round-trips successfully, every other capability — backpressure, forwarding, drain — is theoretical. This story is the one that proves the value proposition.

### Who

- **OpenTelemetry SDK client** (the SDK process in a real application): exports logs via OTLP/gRPC or OTLP/HTTP and expects acknowledgement.
- **Operator deploying Aperture**: greps stderr for `sink_accepted` events to verify a service is producing telemetry.
- **Future Sieve component**: will replace `StubSink` with `impl OtlpSink`; this story locks the trait contract Sieve will plug into.

### Solution

The validate-and-route module routes inbound bytes to `validate_logs` with the correct `Framing` variant, then on `Ok(record)` invokes `sink.accept(SinkRecord::Logs(record)).await`, then responds gRPC OK / HTTP 200 to the caller. The harness's typed return value is passed through unchanged.

### Domain Examples

#### 1: A real Python Django app

`acme-observability`'s Django app has `OTEL_EXPORTER_OTLP_ENDPOINT=http://aperture:4318` and emits logs via the OTel Python SDK's HTTP exporter. The first log record reaches Aperture; stderr shows `event=sink_accepted sink=stub signal=logs record_count=1 resource.service.name="orders-api"`; the Django app sees no errors. Integration confirmed in 60 seconds.

#### 2: A Rust integration test with the OTel Rust SDK

Kaleidoscope CI runs `cargo run --example send_one_log_record_grpc` against a freshly-built Aperture. The example uses `opentelemetry-rust` 0.27's gRPC exporter, sends one log record with `resource.service.name="payments-api"`, and asserts no error. The CI run greps Aperture's captured stderr for `resource.service.name="payments-api"`; the assertion passes; the build is green.

#### 3: A multi-record batch from a high-volume service

`acme-observability`'s checkout service emits batches of 100 log records at a time. Aperture's stderr shows `event=sink_accepted sink=stub signal=logs record_count=100 resource.service.name="checkout-api"` per batch. The SDK's exporter reports 100 records exported per call.

### UAT Scenarios (BDD)

#### Scenario: A real OTel Rust SDK exports a logs batch over gRPC and receives OK

Given an OpenTelemetry Rust SDK 0.27 configured with endpoint http://localhost:4317
And the SDK is emitting one log record with resource.service.name="payments-api"
When the SDK calls its OTLP/gRPC log exporter once
Then the SDK receives gRPC status 0 (OK)
And stderr contains a JSON line with event=request_received transport=grpc signal=logs
And stderr contains a JSON line with event=sink_accepted sink=stub signal=logs record_count=1 resource.service.name="payments-api"

#### Scenario: HTTP/protobuf accepts a valid logs export

Given Aperture's HTTP listener is bound on port 4318
And a real ExportLogsServiceRequest body has been captured from the OpenTelemetry Rust SDK
When the client POSTs that body to /v1/logs with Content-Type application/x-protobuf
Then the response status is 200
And stderr contains a JSON line with event=sink_accepted sink=stub signal=logs record_count=1

#### Scenario: A multi-record logs batch is acknowledged with the correct count

Given an OTel SDK is emitting a batch of 3 log records
When the SDK calls the OTLP/gRPC log exporter once with that batch
Then the SDK receives gRPC status 0 (OK)
And stderr contains a JSON line with event=sink_accepted record_count=3

#### Scenario: A custom OtlpSink implementation (Sieve-shaped) plugs in without crate-level changes

Given a test sink implementing the OtlpSink trait whose accept method records every received SinkRecord into an in-memory vector
And Aperture is configured to use that sink instead of StubSink
When a valid ExportLogsServiceRequest is received over gRPC
Then sink.accept is invoked exactly once on the test sink
And the SinkRecord variant passed is SinkRecord::Logs
And the SDK receives gRPC status 0 (OK)
And the test sink's recorded vector contains the expected ExportLogsServiceRequest unchanged

### Acceptance Criteria

- [ ] Aperture invokes `otlp_conformance_harness::validate_logs(bytes, framing)` exactly once per logs request (CI invariant `single_validator_per_signal`).
- [ ] On `Ok(record)`: `sink.accept(SinkRecord::Logs(record))` is invoked; on `Ok(())` from the sink, the SDK receives gRPC `OK` / HTTP 200.
- [ ] One `event=sink_accepted` stderr line per accepted record batch, with `signal=logs`, `record_count`, and `resource.service.name` (extracted from the first ResourceLogs entry).
- [ ] `Framing::GrpcProtobuf` is used for gRPC requests; `Framing::HttpProtobuf` for HTTP requests; mapping is unit-tested.
- [ ] A custom `impl OtlpSink` (representing Sieve's future shape in Phase 1) plugs into Aperture as a drop-in replacement for `StubSink` without requiring crate-level changes; the trait is the only integration surface. Verified by the test-sink scenario above.
- [ ] The Slice-01 demo command sequence in `slices/slice-01-walking-skeleton.md` runs end-to-end in CI without manual intervention; this is the structural test that defends KPI 1.

### Outcome KPIs

- **Who**: OpenTelemetry SDK clients exporting logs to Aperture.
- **Does what**: receive successful acknowledgements (gRPC OK / HTTP 200) for valid exports.
- **By how much**: ratio of `sink_accepted` to `request_received` events for the logs signal is at least 99% under non-overload conditions.
- **Measured by**: stderr-event ratio in the operator's log aggregator over a 5-minute rolling window.
- **Baseline**: greenfield.

### Technical Notes

- The exact `OtlpSink` trait signature is locked in the DESIGN wave by Morgan. DISCUSS specifies the contract: `Send + Sync`, async `accept`, returns `Result<(), SinkError>`.
- Resource attribute extraction (`resource.service.name`) is a small helper inside the StubSink impl; the field is informational and may be omitted if absent.

### Dependencies

US-AP-01, US-AP-02.

---

## US-AP-04 — Reject malformed input with the harness's named violation rule

### Elevator Pitch

- **Before**: Without explicit validation, a malformed body could either crash the gateway, silently corrupt downstream pipelines, or surface a generic 400 with no diagnostic.
- **After**: An empty body sent to `grpc://localhost:4317` returns `grpc-status: 3` with `grpc-message: otlp violation: rule=EmptyInput signal=Logs framing=GrpcProtobuf locus=byte 0 expected="non-empty OTLP body" observed="0 bytes"`. A traces body sent to `POST http://localhost:4318/v1/logs` returns HTTP 400 with the response body containing `rule=WireType::SignalMismatch observed=Traces asserted=Logs`. Every rejection is the harness's `OtlpViolation::Display` output, verbatim, so consumers parse one format.
- **Decision enabled**: The SDK author whose export was rejected decides exactly how to fix their emitter — the rule name (`EmptyInput`, `ProtobufDecode`, `SignalMismatch`) maps directly to a class of bug they introduced. The operator triaging a misrouted client decides which client to talk to (the `signal_asserted` and `observed` fields name both ends of the mistake).

### Problem

A receiver that rejects malformed input but provides no diagnostic is barely better than a receiver that accepts malformed input. The diagnostic must be (a) named (so log searches work), (b) precise (so the bug is locatable), (c) consistent across transports (so consumers maintain one parser).

### Who

- **OpenTelemetry SDK client** with a serialiser bug: needs a precise diagnostic to locate the bug.
- **Operator triaging telemetry plumbing**: needs to grep stderr for "which clients are sending malformed input?" and get answers without parsing free-text.
- **Third-party engineer building a custom OTLP emitter**: uses Aperture (and the harness corpus) as the conformance gate during emitter development.

### Solution

Aperture's reject path returns the harness's `OtlpViolation::Display` output unchanged: gRPC `grpc-message` header on rejection, HTTP response body (Content-Type text/plain) on rejection. The status code is fixed by transport: gRPC `INVALID_ARGUMENT` (3) for any rule, HTTP 400 for any rule.

### Domain Examples

#### 1: An empty gRPC body from a misconfigured client

A flaky network drops the entire OTLP body before it reaches Aperture; the gRPC frame is zero-length. Aperture returns gRPC status 3 with grpc-message naming `rule=EmptyInput`. The SDK's exporter logs the gRPC message; the operator greps for `EmptyInput` and finds the offending client.

#### 2: A truncated logs body

A 1.2 KB OTLP body is truncated at byte 50 by an HTTP/2 reset. Aperture returns HTTP 400 with body `otlp violation: rule=WireType::ProtobufDecode signal=Logs framing=HttpProtobuf locus=byte 50 expected="valid protobuf wire bytes per opentelemetry-proto descriptor" observed="unexpected EOF in length-delimited field"`. The operator triages by reading the locus.

#### 3: A traces body misrouted to /v1/logs

`acme-observability`'s Python app has a typo: `OTLPLogExporter(endpoint="http://aperture:4318/v1/logs")` is wired up to send the traces serialiser's output. Aperture returns HTTP 400 with body `rule=WireType::SignalMismatch observed=Traces asserted=Logs`. The dev fixes the wiring.

### UAT Scenarios (BDD)

#### Scenario: An empty gRPC body is rejected with INVALID_ARGUMENT

Given Aperture's gRPC listener is accepting connections on port 4317
When a client opens a gRPC stream and sends a zero-length ExportLogsServiceRequest body
Then the response gRPC status is 3 (INVALID_ARGUMENT)
And the grpc-message contains "rule=EmptyInput"
And the grpc-message contains "signal=Logs"
And the grpc-message contains "framing=GrpcProtobuf"

#### Scenario: An HTTP POST with traces bytes to /v1/logs returns SignalMismatch

Given Aperture's HTTP listener is accepting connections on port 4318
And a real ExportTraceServiceRequest body has been captured from the OpenTelemetry Rust SDK
When the client POSTs that body to /v1/logs with Content-Type application/x-protobuf
Then the response status is 400
And the response body contains "rule=WireType::SignalMismatch"
And the response body contains "observed=Traces"
And the response body contains "asserted=Logs"

#### Scenario: A truncated logs body is rejected with ProtobufDecode

Given a real ExportLogsServiceRequest body has been captured and truncated at byte 50
When the client POSTs that truncated body to /v1/logs with Content-Type application/x-protobuf
Then the response status is 400
And the response body contains "rule=WireType::ProtobufDecode"

### Acceptance Criteria

- [ ] On gRPC rejection: response is `grpc-status: 3` (`INVALID_ARGUMENT`), `grpc-message` contains the harness's `OtlpViolation::Display` output verbatim.
- [ ] On HTTP rejection: response is HTTP 400 with `Content-Type: text/plain; charset=utf-8`, body is the `OtlpViolation::Display` output verbatim.
- [ ] All three v0 rules (`EmptyInput`, `WireType::ProtobufDecode`, `WireType::SignalMismatch`) produce the same shape: status code by transport, message body the Display string.
- [ ] No additional message reformatting, truncation, or field removal occurs in Aperture.
- [ ] One stderr `event=request_received` line is emitted before validation; no `sink_accepted` line on rejection.

### Outcome KPIs

- **Who**: SDK clients and operators whose telemetry pipelines produce malformed input.
- **Does what**: receive a named, locatable diagnostic per rejection, instead of a generic 400.
- **By how much**: 100% of rejections carry a non-empty `rule=...` substring in the response body / grpc-message (target: every reject UAT scenario above passes; no transport produces a generic 400 on this path).
- **Measured by**: integration test sweep covering all three rules across both transports.
- **Baseline**: greenfield.

### Technical Notes

- Aperture must NOT translate, summarise, or rephrase the harness's Display output — that breaks consumer parsers and forces the rule taxonomy to live in two places. DESIGN ADR enforces this with a unit test on the rejection path.

### Dependencies

US-AP-03.

---

## US-AP-05 — Accept a valid traces export

### Elevator Pitch

- **Before**: After US-AP-03 and US-AP-04, logs round-trip but traces do not. Half the OTLP three-signal contract is unproven.
- **After**: An OTel SDK exports a span batch over gRPC `localhost:4317` or `POST http://localhost:4318/v1/traces`. Aperture validates via `validate_traces(bytes, framing)`, the StubSink writes one stderr line `{"event":"sink_accepted","sink":"stub","signal":"traces","span_count":1,"resource.service.name":"payments-api"}`, and the SDK receives gRPC OK / HTTP 200.
- **Decision enabled**: The SDK author confirms traces work end-to-end; the operator confirms span counts on stderr; the future Ray component author (Phase 5) sees the boundary they will eventually consume.

### Problem

Symmetry. The validate-and-route module must handle traces with the same shape as logs; if the abstraction is wrong, the harness boundary leaks asymmetry into every later signal.

### Who

- **OpenTelemetry SDK client emitting spans**: the dominant signal type for application observability.
- **Operator triaging trace volume**: greps stderr for `signal=traces span_count=N` to estimate volume.
- **Future Ray component author** (Phase 5): the trace ingest path established here is what Ray will consume.

### Solution

A second route within the validate-and-route module: `validate_traces(bytes, framing)`. Same call shape, same rejection shape, same sink boundary. The `SinkRecord::Traces` variant is added to the enum.

### Domain Examples

#### 1: A real Python Django app instrumented with auto-instrumentation

`acme-observability` enables auto-instrumentation; their Django app sends spans for every HTTP request. Aperture's stderr shows `event=sink_accepted signal=traces span_count=4 resource.service.name="orders-api"` for a typical multi-span request.

#### 2: A custom Rust SDK with one root span

A custom service emits one root span per long-running task. Aperture's stderr shows `span_count=1` per export.

#### 3: A misrouted metrics body to /v1/traces

The same `acme-observability` engineer who hit US-AP-04's logs-vs-traces mistake makes the same mistake with metrics. They get HTTP 400 with `rule=WireType::SignalMismatch observed=Metrics asserted=Traces`. The diagnostic is symmetric — the same shape, the same parser works.

### UAT Scenarios (BDD)

#### Scenario: A valid traces export over gRPC is acknowledged

Given an OpenTelemetry Rust SDK 0.27 configured with endpoint http://localhost:4317
And the SDK is emitting one span with resource.service.name="payments-api"
When the SDK calls its OTLP/gRPC trace exporter once
Then the SDK receives gRPC status 0 (OK)
And stderr contains a JSON line with event=sink_accepted sink=stub signal=traces span_count=1 resource.service.name="payments-api"

#### Scenario: A valid traces export over HTTP/protobuf is acknowledged

Given a real ExportTraceServiceRequest body has been captured from the OpenTelemetry Rust SDK
When the client POSTs that body to /v1/traces with Content-Type application/x-protobuf
Then the response status is 200
And stderr contains a JSON line with event=sink_accepted signal=traces

#### Scenario: A logs body sent to /v1/traces returns SignalMismatch

Given a real ExportLogsServiceRequest body has been captured
When the client POSTs that body to /v1/traces with Content-Type application/x-protobuf
Then the response status is 400
And the response body contains "rule=WireType::SignalMismatch observed=Logs asserted=Traces"

### Acceptance Criteria

- [ ] Aperture invokes `otlp_conformance_harness::validate_traces(bytes, framing)` exactly once per traces request.
- [ ] On accept: `sink.accept(SinkRecord::Traces(record))`; on success, gRPC OK / HTTP 200.
- [ ] Stderr line on accept includes `signal=traces`, `span_count` (sum of `Span` entries across all `ResourceSpans` -> `ScopeSpans`), `resource.service.name`.
- [ ] Reject paths produce the same shape as logs: gRPC `INVALID_ARGUMENT` / HTTP 400 with `OtlpViolation::Display` verbatim.

### Outcome KPIs

- **Who**: SDK clients exporting traces.
- **Does what**: receive successful acknowledgements for valid exports across both transports.
- **By how much**: ratio of `sink_accepted` to `request_received` events for traces is at least 99% under non-overload conditions.
- **Measured by**: stderr-event ratio in the operator's log aggregator.
- **Baseline**: greenfield.

### Technical Notes

- Span counting walks `ResourceSpans` -> `ScopeSpans` -> `Span`. DESIGN locks the helper.

### Dependencies

US-AP-04.

---

## US-AP-06 — Accept a valid metrics export

### Elevator Pitch

- **Before**: After US-AP-05, two of three OTLP signals work end-to-end. Metrics is the most complex signal and the one most likely to surface harness-boundary surprises.
- **After**: An OTel SDK exports a metrics batch (one gauge data point and one sum data point) to `grpc://localhost:4317` or `POST http://localhost:4318/v1/metrics`. Aperture validates via `validate_metrics(bytes, framing)`, the StubSink writes `{"event":"sink_accepted","sink":"stub","signal":"metrics","data_point_count":2,"resource.service.name":"payments-api"}`, and the SDK receives gRPC OK / HTTP 200. After this story, all three OTLP stable signals work on both transports.
- **Decision enabled**: The SDK author confirms the integration is complete for their full telemetry signal set; the operator's volume estimation now covers all three signals; the future Pulse component author (Phase 4) sees the metrics ingest path they will eventually consume.

### Problem

Metrics is the harness's stress test (per `otlp-conformance-harness-v0` US-06). If the validate-and-route abstraction holds for metrics — including across the five point types (gauge, sum, histogram, exponential histogram, summary) — it holds for everything in OTLP scope.

### Who

- **OpenTelemetry SDK client emitting metrics**: needs the third leg of the OTLP three-signal contract.
- **Operator estimating metrics volume**: greps stderr for `signal=metrics data_point_count=N`.
- **Future Pulse component author** (Phase 4): consumes the metrics ingest path established here.

### Solution

A third route: `validate_metrics(bytes, framing)`. Same call shape, same rejection shape, same sink boundary. `SinkRecord::Metrics` completes the enum.

### Domain Examples

#### 1: A Prometheus scrape adapter

`acme-observability` runs an OTel Collector with a Prometheus receiver that converts Prometheus exposition into OTLP metrics. Aperture's stderr shows `event=sink_accepted signal=metrics data_point_count=147` per minute.

#### 2: A histogram-heavy workload

A latency-instrumented service emits histograms with 12 buckets each, 4 histograms per export. Aperture's stderr shows `data_point_count=4` (one per histogram, not per bucket) — DISCUSS picks histogram-as-one-data-point because that is the unit downstream backends count.

#### 3: A traces body misrouted to /v1/metrics

Same operator-misconfiguration class as US-AP-05's misroute. Aperture returns HTTP 400 with `rule=WireType::SignalMismatch observed=Traces asserted=Metrics`. Symmetric across all three signals.

### UAT Scenarios (BDD)

#### Scenario: A valid metrics export over gRPC is acknowledged

Given an OpenTelemetry Rust SDK 0.27 configured with endpoint http://localhost:4317
And the SDK is emitting one gauge data point and one sum data point
When the SDK calls its OTLP/gRPC metrics exporter once
Then the SDK receives gRPC status 0 (OK)
And stderr contains a JSON line with event=sink_accepted sink=stub signal=metrics data_point_count=2

#### Scenario: A valid metrics export over HTTP/protobuf is acknowledged

Given a real ExportMetricsServiceRequest body has been captured from the OpenTelemetry Rust SDK
When the client POSTs that body to /v1/metrics with Content-Type application/x-protobuf
Then the response status is 200
And stderr contains a JSON line with event=sink_accepted signal=metrics

#### Scenario: SinkRecord enum is exhaustive

Given the SinkRecord enum
When the test enumerates its variants
Then exactly three variants exist: Logs, Traces, Metrics
And each variant carries the upstream opentelemetry_proto type unwrapped

### Acceptance Criteria

- [ ] `validate_metrics(bytes, framing)` invoked exactly once per metrics request.
- [ ] `sink.accept(SinkRecord::Metrics(record))` on accept; gRPC OK / HTTP 200 on sink success.
- [ ] Stderr line on accept names `signal=metrics`, `data_point_count`, `resource.service.name`.
- [ ] `SinkRecord` enum has exactly three variants; unit test asserts variant exhaustiveness.

### Outcome KPIs

- **Who**: SDK clients exporting metrics.
- **Does what**: receive successful acknowledgements for valid exports across both transports.
- **By how much**: ratio of `sink_accepted` to `request_received` for metrics is at least 99% under non-overload.
- **Measured by**: stderr-event ratio.
- **Baseline**: greenfield.

### Technical Notes

- Data-point counting walks `ResourceMetrics` -> `ScopeMetrics` -> `Metric` -> `data` oneOf. Histograms count as one data point each. DESIGN locks the helper.

### Dependencies

US-AP-05.

---

## US-AP-07 — Refuse beyond the per-transport concurrency cap, never silently drop

### Elevator Pitch

- **Before**: After US-AP-06, Aperture handles every valid request — but if traffic exceeds capacity, behaviour is undefined: it may queue (memory blow-up), block (violates OTel SDK contract), or drop silently (no operator visibility).
- **After**: With `max_concurrent_requests=4` configured per transport, a 5th simultaneous gRPC export receives `grpc-status: 8` (`RESOURCE_EXHAUSTED`) with `grpc-message` naming the cap. A 5th simultaneous HTTP POST to `http://localhost:4318/v1/logs` receives HTTP 503 with header `Retry-After: 1` and body `aperture: gRPC concurrency cap of 4 reached on transport=grpc`. Every refusal writes one stderr line `event=concurrency_cap_hit transport=grpc cap=4`.
- **Decision enabled**: The SDK's retry policy (built into every OTel exporter) decides when to retry. The operator decides whether to scale up Aperture replicas based on the rate of `concurrency_cap_hit` events.

### Problem

Load behaviour is the riskiest unvalidated assumption in the integration plane. Andrea's locked Q4: cap, refuse, never block, never drop silently. An internal queue (Sluice's job, Phase 7) is explicitly out of scope; blocking violates OTel SDK contracts; silent drop is an anti-pattern listed in the roadmap.

### Who

- **OpenTelemetry SDK client**: needs deterministic refusal status so its retry logic engages correctly.
- **Operator deploying Aperture**: needs an observable saturation signal to drive horizontal-scaling decisions.
- **Kaleidoscope CI**: load-test scenario verifies determinism under overload.

### Solution

Each transport gets a Tokio semaphore with capacity `max_concurrent_requests`. Permits are acquired on connection accept (or HTTP request begin) and released on response sent. Failed acquire -> immediate refusal (no wait, no queue). gRPC: `RESOURCE_EXHAUSTED`; HTTP: 503 + `Retry-After: 1`.

### Domain Examples

#### 1: A traffic spike during incident-induced log volume

`acme-observability`'s payments-api hits an incident; log volume 10x normal. Aperture's gRPC cap (1024 default) is briefly saturated. Stderr shows a burst of `concurrency_cap_hit` events. The OTel SDK's retry policy backs off and re-sends. Operator scales Aperture from 3 to 6 replicas; events stop.

#### 2: A misbehaving client opening 100 concurrent streams

A single misconfigured client (an internal CI runner with too-aggressive parallelism) opens 100 concurrent gRPC streams to one Aperture instance. Cap=1024 absorbs it; cap=64 (a deliberately smaller fleet) refuses 36 of them. The 36 retry; the misbehaving client is found via stderr `peer=` field on the cap-hit lines.

#### 3: A load-test scenario in Kaleidoscope CI

Kaleidoscope CI runs a load-test scenario: 100 concurrent gRPC clients with cap=4. Asserts that exactly 4 in-flight at any time, exactly the rest receive `RESOURCE_EXHAUSTED`, exactly 100 `request_received` lines and (100 - 4N where N is the test's batch count) `concurrency_cap_hit` lines. No silent drops.

### UAT Scenarios (BDD)

#### Scenario: gRPC concurrency cap reached returns RESOURCE_EXHAUSTED

Given Aperture's gRPC transport is configured with max_concurrent_requests=4
And 4 requests are currently in-flight on the gRPC listener
When a 5th client opens a gRPC stream and begins an Export call
Then the 5th request receives gRPC status 8 (RESOURCE_EXHAUSTED)
And the grpc-message names the configured concurrency cap
And stderr contains a JSON line with level=warn event=concurrency_cap_hit transport=grpc cap=4

#### Scenario: HTTP concurrency cap reached returns 503 with Retry-After

Given Aperture's HTTP transport is configured with max_concurrent_requests=4
And 4 requests are currently in-flight on the HTTP listener
When a 5th client POSTs /v1/logs
Then the 5th request receives HTTP 503
And the response includes a "Retry-After: 1" header
And the response body names the configured concurrency cap
And stderr contains a JSON line with level=warn event=concurrency_cap_hit transport=http_protobuf cap=4

#### Scenario: Caps are independent per transport

Given Aperture's gRPC transport is saturated with 4 in-flight requests at cap=4
When a client POSTs /v1/logs to the HTTP listener (cap=4, currently 0 in-flight)
Then the HTTP request receives HTTP 200
And the gRPC saturation does not affect the HTTP request

### Acceptance Criteria

- [ ] Per-transport `max_concurrent_requests` config key, default 1024.
- [ ] gRPC refusal: `grpc-status: 8`, message names cap.
- [ ] HTTP refusal: HTTP 503, `Retry-After: 1` header, body names cap.
- [ ] One `event=concurrency_cap_hit` warn stderr line per refusal, with `transport` and `cap`.
- [ ] No internal queue; no block; no silent drop. The `@property` UAT in `journey-aperture.feature` defends this invariant.
- [ ] Caps are independent per transport.

### Outcome KPIs

- **Who**: Aperture under overload conditions.
- **Does what**: refuses excess traffic deterministically (refusal-rate equals exceeded-cap-rate).
- **By how much**: zero silent drops over a 1-hour load test where offered load is 2x the configured cap.
- **Measured by**: load-test integration scenario in CI; counts `request_received` minus `sink_accepted` minus `concurrency_cap_hit` minus reject-rule events; the residual must be zero.
- **Baseline**: greenfield; Aperture has no load behaviour before this story.

### Technical Notes

- DESIGN locks the semaphore mechanism (Tokio `Semaphore`, `BoundedSemaphore`, or hand-rolled).
- Permit lifetime: from connection accept (gRPC) or request begin (HTTP) until response sent. The sink's hand-off-and-await counts as "in-flight" and holds a permit.

### Dependencies

US-AP-06.

---

## US-AP-08 — Forward accepted records to a downstream OTel-compatible backend

### Elevator Pitch

- **Before**: After US-AP-07, Aperture handles traffic correctly under all conditions but the only sink is `StubSink` (logs to stderr). In production this is useless — accepted records vanish.
- **After**: An OTel SDK exports to `grpc://localhost:4317`; Aperture validates, hands the typed record to `ForwardingSink::accept`, which POSTs the typed record to `POST http://otel-backend:4318/v1/logs`. On downstream success, the SDK receives gRPC OK and Aperture's stderr shows `{"event":"sink_accepted","sink":"forwarding","downstream":"http://otel-backend:4318","signal":"logs","record_count":1,"downstream_latency_ms":17}`. On downstream 5xx, the SDK receives gRPC status 14 (`UNAVAILABLE`) and stderr shows `{"event":"sink_failed","sink":"forwarding"}`.
- **Decision enabled**: The operator decides Aperture is production-ready for their environment — accepted records reach the existing telemetry stack. The Phase-1 roadmap promise (operate over any existing OTel backend) is met.

### Problem

A receiver that accepts but does not durably forward is a benchmark, not a production component. ForwardingSink is what makes Aperture meet the Phase-1 roadmap promise: integrate with the operator's existing OTel-compatible storage stack without requiring Kaleidoscope-native engines.

### Who

- **Operator running Aperture in production**: needs accepted records to land in their existing Loki / Tempo / Mimir / OTel Collector.
- **OpenTelemetry SDK client**: cares only that it gets gRPC OK / HTTP 200 — the downstream is invisible.
- **Future Sieve component author** (Phase 1+): will replace `ForwardingSink` with a Sieve-driven sampling sink; the trait boundary and the success/failure semantics are locked here.

### Solution

`ForwardingSink::accept` POSTs the typed `Export*ServiceRequest` to a configured downstream endpoint. On success: returns `Ok(())`, Aperture responds OK upstream. On failure (5xx, connection refused, timeout): returns `Err(SinkError::DownstreamUnavailable)`, Aperture maps to gRPC `UNAVAILABLE` / HTTP 503 upstream.

### Domain Examples

#### 1: ForwardingSink to a co-located OTel Collector

`acme-observability` deploys Aperture as a sidecar to their OTel Collector. ForwardingSink endpoint is `http://localhost:14318`. The Collector handles routing to Loki / Tempo / Mimir downstream. Aperture's role is the conformance gate plus deterministic refusal-on-overload; the Collector handles the multiplexing.

#### 2: A downstream backend hits its own overload

The downstream Loki returns HTTP 503 during a brief incident. Aperture's stderr shows a burst of `event=sink_failed sink=forwarding` lines. The OTel SDK's retry policy engages; once Loki recovers, the retried records flow through. No data lost (all retries land), no silent failures.

#### 3: A misconfigured downstream endpoint

`acme-observability`'s engineer typos the endpoint as `http://otelbackend:4318` (missing hyphen). DNS resolution fails. Aperture's first received request returns HTTP 503 to the SDK; stderr shows `event=sink_failed sink=forwarding downstream=http://otelbackend:4318 reason="dns_resolve_failed"`. The engineer fixes the config, restart, traffic flows.

### UAT Scenarios (BDD)

#### Scenario: ForwardingSink writes downstream and propagates success

Given Aperture is configured with sink=forwarding, endpoint=http://otel-backend:4318
And the configured downstream backend is healthy
When a valid ExportLogsServiceRequest is received over gRPC
Then ForwardingSink POSTs the typed record to http://otel-backend:4318/v1/logs
And sink.accept returns Ok(())
And the SDK receives gRPC status 0 (OK)
And stderr contains a JSON line with event=sink_accepted sink=forwarding downstream=http://otel-backend:4318 downstream_latency_ms exists

#### Scenario: ForwardingSink refusal becomes UNAVAILABLE upstream

Given Aperture is configured with sink=forwarding, endpoint=http://otel-backend:4318
And the configured downstream backend is returning HTTP 503
When a valid ExportLogsServiceRequest is received over gRPC
Then sink.accept returns Err(SinkError::DownstreamUnavailable)
And the SDK receives gRPC status 14 (UNAVAILABLE)
And stderr contains a JSON line with level=error event=sink_failed sink=forwarding

#### Scenario: ForwardingSink connection refused becomes UNAVAILABLE upstream

Given Aperture is configured with sink=forwarding, endpoint=http://nowhere:4318
And no process is listening at that endpoint
When a valid ExportLogsServiceRequest is received
Then sink.accept returns Err(SinkError::DownstreamUnavailable)
And the SDK receives gRPC status 14 (UNAVAILABLE)
And stderr contains a JSON line with event=sink_failed reason="connection refused"

### Acceptance Criteria

- [ ] Config keys `aperture.sink.kind` (values: `stub`, `forwarding`) and `aperture.sink.forwarding.endpoint` (URL).
- [ ] On healthy downstream: typed record POSTed verbatim, `sink_accepted` stderr line includes `downstream` and `downstream_latency_ms`.
- [ ] On downstream 5xx: SDK receives gRPC `UNAVAILABLE` / HTTP 503; `sink_failed` error stderr line.
- [ ] On connection refused, DNS failure, timeout (default 5 s): same response shape; reason field on stderr line.
- [ ] ForwardingSink is the only outbound network Aperture originates (CI invariant `no_telemetry_on_telemetry`).

### Outcome KPIs

- **Who**: operators running Aperture with `sink=forwarding`.
- **Does what**: see accepted records arrive at the configured downstream.
- **By how much**: downstream-acceptance ratio (Aperture `sink_accepted` count / downstream-confirmed-receive count) of at least 99% under healthy-downstream conditions.
- **Measured by**: integration test scenario asserts every Aperture-accepted record produces a corresponding downstream `request_received`-equivalent on the downstream Collector.
- **Baseline**: greenfield.

### Technical Notes

- DESIGN locks the outbound HTTP client (likely `reqwest` or `hyper-client` directly).
- DISCUSS specifies: no retries from Aperture (the SDK retries; double-retry is anti-pattern). Configurable timeout with default 5 s.
- **Default-timeout rationale**: 5 s chosen because the OTel SDK's default exporter timeout is 10 s; Aperture's `ForwardingSink` should fail before the SDK times out so the SDK's retry budget is not consumed by a hung Aperture-to-downstream call. (Citation: `OTEL_EXPORTER_OTLP_TIMEOUT` default per OTel spec.)
- DESIGN may revisit whether to support gRPC outbound as well as HTTP outbound. DISCUSS picks HTTP-out only for v0 (simpler debugging, more downstreams accept it).

### Dependencies

US-AP-07.

---

## US-AP-09 — Drain in-flight requests on SIGTERM, never silently drop

### Elevator Pitch

- **Before**: After US-AP-08, Aperture handles all traffic correctly while running but a process restart (rolling deploy, k8s pod replacement) drops in-flight requests with no signal to the SDK or the operator.
- **After**: SIGTERM flips `/readyz` to 503 `"draining"` within 100 ms; listeners stop accepting new connections; in-flight requests drain to a configurable deadline (default 30 s); on clean drain, exit 0 with stderr `{"event":"in_flight_drained","drained_count":7}`; on deadline expiry, exit 1 with stderr `{"event":"drain_deadline_exceeded","dropped_count":3}` (warn level).
- **Decision enabled**: The k8s operator with a rolling-deploy strategy decides Aperture is fit for production — the readiness flip happens before the listeners stop, so the load balancer stops sending new traffic before Aperture stops accepting it. The operator triaging a deadline-exceeded event sees the dropped count on stderr; the drop is loud, never silent.

### Problem

The most operationally load-bearing slice. A service that drops in-flight requests on every restart is unfit for any production deployment. The drain order matters: readiness must flip first (so orchestrator-level routing stops), then listeners stop accepting (so no new requests arrive), then in-flight drains. Any drop on deadline expiry must be observable.

### Who

- **k8s operator with a rolling-deploy strategy**: needs `/readyz` to flip BEFORE the process refuses new traffic.
- **OpenTelemetry SDK client whose export was in flight at SIGTERM**: receives gRPC OK / HTTP 200 if drain completes, gRPC `UNAVAILABLE` / HTTP 503 if deadline hits — never a connection drop or undefined behaviour.
- **Operator triaging a missed deadline**: greps stderr for `event=drain_deadline_exceeded` and gets a count and a list.

### Solution

A shutdown handler hooks SIGTERM and SIGINT. The handler:

1. Writes `event=shutdown_initiated` stderr line.
2. Flips `/readyz` to `draining` (it returns 503 from this point).
3. Stops listeners from accepting new connections.
4. Awaits all in-flight permits to be released (uses the per-transport semaphores from US-AP-07).
5. On clean drain within deadline: exits 0 with `event=in_flight_drained` and `event=shutdown_complete exit_code=0`.
6. On deadline: exits 1 with `event=drain_deadline_exceeded dropped_count=N` (warn) and `event=shutdown_complete exit_code=1`.

### Domain Examples

#### 1: A k8s rolling deploy in production

`acme-observability` updates Aperture from v0.1.0 to v0.1.1. k8s sends SIGTERM to old pods one at a time. Each old pod's `/readyz` flips to 503 within 100 ms; the Service's endpoints exclude that pod within one readiness-probe period; old pod's listeners stop accepting; in-flight requests (typically <50 ms each) drain in <1 s; pod exits 0. SDK clients see no errors during the rollout.

#### 2: A drain-deadline-exceeded incident

A downstream Loki incident causes Aperture's ForwardingSink to back up. SIGTERM arrives during the incident; 12 in-flight requests are stuck waiting on the slow downstream. Drain deadline (30 s) expires with 3 still in-flight. Stderr: `event=drain_deadline_exceeded dropped_count=3`; pod exits 1; the 3 SDK clients receive gRPC `UNAVAILABLE`. Operator sees the warn line and knows to investigate Loki.

#### 3: SIGINT during a CI run

Kaleidoscope CI's integration test sends SIGINT to a background Aperture instance after the test scenarios complete. Aperture drains in <100 ms, exits 0. The CI run is clean.

### UAT Scenarios (BDD)

#### Scenario: Graceful shutdown drains in-flight requests

Given Aperture is running and has 7 in-flight requests
When the process receives SIGTERM
Then /readyz returns 503 "draining" within 100 ms
And stderr contains a JSON line with event=shutdown_initiated signal=SIGTERM
And no new connections are accepted on either listener after the readiness flip
And the 7 in-flight requests complete (sink-acknowledged and responded to client)
And stderr contains a JSON line with event=in_flight_drained drained_count=7
And the process exits with status code 0

#### Scenario: Drain deadline exceeded is observable, never silent

Given Aperture is running with drain_deadline_ms=1000
And 3 in-flight requests are blocked on a slow sink
When the process receives SIGTERM
And the drain deadline elapses with the requests still in-flight
Then stderr contains a JSON line with level=warn event=drain_deadline_exceeded dropped_count=3
And the process exits with status code 1

#### Scenario: SIGINT and SIGTERM behave identically

Given Aperture is running with 0 in-flight requests
When the process receives SIGINT
Then the shutdown follows the same sequence as SIGTERM (readiness flip, listener close, drain, exit 0)

### Acceptance Criteria

- [ ] Config key `aperture.shutdown.drain_deadline_ms`, default 30000.
- [ ] On SIGTERM/SIGINT: `/readyz` -> 503 `"draining"` within 100 ms.
- [ ] Listeners stop accepting new connections after readiness flip.
- [ ] In-flight requests complete the full validate-sink-respond cycle if drain finishes within deadline.
- [ ] Clean drain: exit 0, `event=in_flight_drained drained_count=N` info line.
- [ ] Deadline exceeded: exit 1, `event=drain_deadline_exceeded dropped_count=N` warn line.
- [ ] SIGINT and SIGTERM behave identically.
- [ ] A request whose body is being read at the moment of SIGTERM either (a) completes normally if the body is fully received before the listener-close grace window, or (b) is reset at the TCP level if the listener has already closed. Either way, the SDK observes a deterministic outcome (gRPC `UNAVAILABLE` / TCP reset on connection close), never a half-acknowledged response.

### Outcome KPIs

- **Who**: operators running Aperture under any orchestrator-driven restart workflow.
- **Does what**: experience zero silent drops during graceful restarts.
- **By how much**: in a 1000-restart load test (with offered load below capacity and downstream healthy), zero requests are lost without an observable stderr line. Lost-and-observed is acceptable; lost-and-silent is the failure.
- **Measured by**: integration scenario asserts request_received_count = sink_accepted_count + reject_count + drain_deadline_exceeded_dropped_count over the full test.
- **Baseline**: greenfield.

### Technical Notes

- The drain logic reads the per-transport semaphore's permit deficit to compute "in-flight count". This couples US-AP-09 to US-AP-07's semaphore design; DESIGN names the coupling.
- DESIGN may add a "wait one readiness-probe period after readiness flip before closing listeners" step to harden against orchestrator polling latency. DISCUSS leaves this open.

### Dependencies

US-AP-08, US-AP-02 (`/readyz` state machine).

---

## Out-of-scope (forward-compat infrastructure, no story)

### TLS / SPIFFE config-schema knob

Andrea's locked Q5: TLS and SPIFFE keys exist in the v0 config schema, default off; setting `tls.enabled = true` on v0 emits a single warn stderr line and continues plaintext. This is captured in `slice-07-tls-schema-knob.md` and in the System Constraints (item 7) above.

It is **not** a user story because no user can demonstrate value from it at v0 — its value lands at Phase 2 when Aegis ships and would-have-needed-to-break-the-schema otherwise. It rides as an `@infrastructure` technical task in Slice 07, alongside the user-facing Slices 06 and 08.

The acceptance is in `slice-07-tls-schema-knob.md`'s Acceptance Summary section: schema accepts the keys, defaults work, `tls.enabled=true` produces exactly one `event=tls_not_supported_in_v0` warn stderr line.
