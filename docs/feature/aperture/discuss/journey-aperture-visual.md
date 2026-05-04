# Journey Visual — Aperture v0 (the OTLP gateway)

> **Wave**: DISCUSS — Phase 2 (Journey Visualisation).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-04.
> **Companion documents**: `journey-aperture.yaml`, `journey-aperture.feature`, `shared-artifacts-registry.md`.

---

## What this journey actually is

Aperture is **not a CLI**. It is a long-lived Rust service that binds two TCP listeners — gRPC on `:4317` and HTTP/protobuf on `:4318` — accepts OTLP bodies from OpenTelemetry SDKs, validates each body using the **`otlp-conformance-harness` library** (the load-bearing leaf shipped in Phase 0), and hands accepted records to a **sink** trait whose two v0 implementations are `StubSink` (logs to stderr) and `ForwardingSink` (writes OTLP to an external OTel-compatible backend, per the Phase-1 roadmap).

The "user" in this journey is therefore not a human. It is an **OpenTelemetry SDK**, and the journey we are designing is the integration handshake between that SDK and Aperture, viewed from the SDK author's seat. The "emotional arc" is an **integration-confidence arc**: does my export return the right gRPC status, does the receiver tell me whether it is healthy, does it behave under load.

The four personas the journey serves:

| Persona | Touch point | Confidence question |
|---|---|---|
| **OpenTelemetry SDK client** | exports OTLP via gRPC :4317 or HTTP/protobuf :4318 | "Did the receiver acknowledge my export, and on rejection, did it tell me why?" |
| **Future Sieve component** | will land as `impl OtlpSink` in Phase 1, replacing `StubSink`/`ForwardingSink` | "Is the boundary the harness draws (`accept(record) -> ack`) the boundary I want to receive?" |
| **Third-party engineer operating Kaleidoscope** | reads stderr JSON logs, polls `/healthz` and `/readyz`, runs OTel SDKs against Aperture | "Can I tell whether Aperture is up, ready, and not silently dropping data?" |
| **Kaleidoscope CI** | runs an OTel Rust SDK against a freshly-built Aperture | "Does the walking skeleton round-trip a real `ExportLogsServiceRequest` and produce the expected stderr line?" |

---

## Backbone — the six activities

```
+----------+    +----------+    +-----------+    +-----------+    +-----------+    +-----------+
|  BIND    | -> | RECEIVE  | -> | VALIDATE  | -> |  HAND OFF | -> |  OBSERVE  | -> |  SHUT     |
| listeners|    | payload  |    | via       |    | to sink   |    | self      |    |  DOWN     |
|          |    |          |    | harness   |    |           |    |           |    | gracefully|
+----------+    +----------+    +-----------+    +-----------+    +-----------+    +-----------+
   :4317           gRPC          harness::         OtlpSink         /healthz         drain in
   :4318           or HTTP       validate_*        ::accept         /readyz          flight,
                                                                    stderr           refuse new,
                                                                    JSON logs        flush
```

Six activities, left to right, each owned by a discrete concern. The walking skeleton (Slice 01) lights up exactly one path through this backbone: gRPC + logs + `StubSink`. Subsequent slices extend each station horizontally.

---

## Confidence arc, station by station

The arc maps each activity to the SDK author's question and the design lever that answers it.

| Station | SDK author's question | Design lever (v0) |
|---|---|---|
| **Bind listeners** | "Are you actually listening on the OTLP ports?" | Structured stderr JSON line `{"event":"listener_bound","transport":"grpc","addr":"0.0.0.0:4317"}` and `/readyz` flips to 200 once both listeners bound. |
| **Receive payload** | "Did you receive my bytes?" | gRPC: HTTP/2 stream accepted. HTTP/protobuf: TCP connection accepted, body read with documented `max-recv-msg-size`. No silent buffering. |
| **Validate via harness** | "Are my bytes wire-conformant?" | `otlp_conformance_harness::validate_logs(bytes, Framing::Grpc)` — the **real** harness, not a stub. Reject path returns gRPC `INVALID_ARGUMENT` / HTTP 400 with the violation rule echoed in the message. |
| **Hand off to sink** | "Did somebody downstream actually take responsibility for my data?" | `OtlpSink::accept(record).await` — Aperture's job ends only when the sink acknowledges. v0 sinks: `StubSink` (log + Ok), `ForwardingSink` (downstream OTLP write + Ok/Err). |
| **Observe self** | "How do I tell whether you are healthy without sending me telemetry-on-telemetry?" | Structured stderr JSON logs (no metrics, no OTLP-out), `/healthz` (liveness), `/readyz` (readiness, drain-aware). |
| **Shut down gracefully** | "If you are restarted while my export is in flight, do you drop me or finish the handshake?" | On SIGTERM: `/readyz` flips to 503, listeners stop accepting new connections, in-flight requests drain to a configurable deadline, then process exits. |

The arc starts at "is the receiver even there?" (resolved by `/readyz` and the stderr bind line) and ends at "can I trust this receiver under stress?" (resolved by deterministic backpressure: 503 with `Retry-After` or gRPC `RESOURCE_EXHAUSTED` once the per-transport concurrency cap is hit).

---

## Wire-level mockups

### Activity 1 — Bind listeners (process startup)

What the operator sees on stderr (the only telemetry channel Aperture has by design):

```
{"ts":"2026-05-04T09:12:01.004Z","level":"info","event":"startup","version":"0.1.0","config_path":"/etc/aperture/config.toml"}
{"ts":"2026-05-04T09:12:01.022Z","level":"info","event":"listener_bound","transport":"grpc","addr":"0.0.0.0:4317"}
{"ts":"2026-05-04T09:12:01.027Z","level":"info","event":"listener_bound","transport":"http_protobuf","addr":"0.0.0.0:4318"}
{"ts":"2026-05-04T09:12:01.027Z","level":"info","event":"ready","listeners":["grpc:4317","http_protobuf:4318"]}
```

What `curl` sees:

```
$ curl -fsS http://localhost:4318/healthz
ok

$ curl -fsS http://localhost:4318/readyz
ready
```

`/readyz` is **drain-aware**: it returns 200 only when both listeners are bound AND the process is not in the shutdown drain window.

### Activity 2 — Receive payload (gRPC happy path, Slice 01)

What the OpenTelemetry Rust SDK sends:

```
POST / HTTP/2
:authority: localhost:4317
content-type: application/grpc
te: trailers
grpc-encoding: identity
user-agent: opentelemetry-rust/0.27.0 grpc-rust/0.12

<gRPC frame: ExportLogsServiceRequest with 1 ResourceLogs containing
              service.name="payments-api", 3 LogRecords>
```

What Aperture writes to stderr **before** validation (one line per request, info level):

```
{"ts":"2026-05-04T09:14:33.221Z","level":"info","event":"request_received","transport":"grpc","signal":"logs","bytes":487,"peer":"127.0.0.1:54002"}
```

### Activity 3 — Validate via harness (the load-bearing call)

The function call. **No stub.** This is the whole point of Slice 01 — integration risk lands now, not later.

```rust
use otlp_conformance_harness::{validate_logs, Framing, OtlpViolation};

let outcome = validate_logs(&body_bytes, Framing::GrpcProtobuf);
match outcome {
    Ok(record) => sink.accept(SinkRecord::Logs(record)).await,
    Err(violation) => translate_violation_to_grpc_status(violation),
}
```

What the SDK sees on the **accept** path:

```
HTTP/2 200
grpc-status: 0
grpc-message:
```

What the SDK sees on the **reject** path (e.g. empty body to gRPC):

```
HTTP/2 200
grpc-status: 3                        # INVALID_ARGUMENT
grpc-message: otlp violation: rule=EmptyInput signal=Logs framing=GrpcProtobuf locus=byte 0 expected="non-empty OTLP body" observed="0 bytes"
```

The `grpc-message` value is the harness's `OtlpViolation::Display` impl (verified above in `crates/otlp-conformance-harness/src/violation.rs`). Nothing reformatted, nothing lost.

For HTTP/protobuf the equivalent rejection is:

```
HTTP/1.1 400 Bad Request
content-type: text/plain; charset=utf-8
content-length: 145

otlp violation: rule=EmptyInput signal=Logs framing=HttpProtobuf locus=byte 0 expected="non-empty OTLP body" observed="0 bytes"
```

### Activity 4 — Hand off to sink

The trait that Slice 01 introduces (sketched here at requirements granularity; the exact signature is a DESIGN decision for Morgan):

```rust
#[async_trait]
pub trait OtlpSink: Send + Sync {
    async fn accept(&self, record: SinkRecord) -> Result<(), SinkError>;
}

pub enum SinkRecord {
    Logs(ExportLogsServiceRequest),
    Traces(ExportTraceServiceRequest),
    Metrics(ExportMetricsServiceRequest),
}
```

Aperture's job ends when `accept()` returns `Ok(())`. On `Err(SinkError)`, the request becomes a 5xx (HTTP) or `UNAVAILABLE` (gRPC).

`StubSink` (Slice 01) writes a single stderr JSON line and returns `Ok`:

```
{"ts":"2026-05-04T09:14:33.234Z","level":"info","event":"sink_accepted","sink":"stub","signal":"logs","resource_count":1,"record_count":3,"resource.service.name":"payments-api"}
```

`ForwardingSink` (Slice 06) writes the typed `ExportLogsServiceRequest` to a downstream OTel-compatible backend (URL configured at startup) and propagates that backend's success/failure back up.

### Activity 5 — Observe self

Three observability surfaces, no fourth:

```
+-------------------------+----------------------------------------+
| stderr JSON logs        | levels: error | warn | info | debug    |
|                         | one event per line                     |
|                         | consumed by operator's log aggregator  |
+-------------------------+----------------------------------------+
| GET /healthz            | 200 always (process up)                |
+-------------------------+----------------------------------------+
| GET /readyz             | 200 once both listeners bound          |
|                         | 503 during startup or shutdown drain   |
+-------------------------+----------------------------------------+
```

There is no `/metrics`, no OTLP-out from Aperture itself. Telemetry-on-telemetry would point Aperture at Aperture (or at a parallel pipeline that has its own outage modes); both are anti-patterns in the roadmap. Pulse will own this concern in Phase 4.

### Activity 6 — Shut down gracefully

What the operator sees on `kill -TERM <pid>`:

```
{"ts":"2026-05-04T11:42:10.001Z","level":"info","event":"shutdown_initiated","signal":"SIGTERM","drain_deadline_ms":30000}
{"ts":"2026-05-04T11:42:10.002Z","level":"info","event":"readiness_changed","ready":false,"reason":"shutdown_drain"}
{"ts":"2026-05-04T11:42:10.005Z","level":"info","event":"listener_closing","transport":"grpc"}
{"ts":"2026-05-04T11:42:10.007Z","level":"info","event":"listener_closing","transport":"http_protobuf"}
{"ts":"2026-05-04T11:42:11.450Z","level":"info","event":"in_flight_drained","drained_count":7}
{"ts":"2026-05-04T11:42:11.451Z","level":"info","event":"shutdown_complete","exit_code":0}
```

`/readyz` flips to 503 first, so upstream load balancers stop sending new requests; in-flight requests drain; listeners close; process exits.

If the deadline is hit before in-flight drains, Aperture exits non-zero with a final stderr line naming the count of dropped requests. **Drop-on-deadline is observable, never silent.**

---

## Backpressure shape (cross-cutting, lands in Slice 05)

Every transport has a configurable `max_concurrent_requests` cap. Once the cap is reached:

- **gRPC**: respond with `RESOURCE_EXHAUSTED` (`grpc-status: 8`) and a `grpc-message` naming the cap.
- **HTTP/protobuf**: respond with `503 Service Unavailable`, header `Retry-After: 1`, body naming the cap.

No internal queue. No block. No silent drop. Three things explicitly *not* in v0 because each is somebody else's job:

| Anti-pattern in v0 | Whose job, when |
|---|---|
| Build an internal queue | Sluice, Phase 7 |
| Block on the producer | Violates OTel SDK contract — never |
| Drop silently | Roadmap-listed anti-pattern — never |

The configurable knob is `aperture.transport.{grpc,http}.max_concurrent_requests`, default value `1024` per transport (a placeholder Morgan can revisit; the value belongs to DESIGN).

---

## TLS / auth knob (cross-cutting, lands in Slice 07)

Plaintext, no auth at v0 — but the schema must already carry the knob, defaulting off, so Phase 2's Aegis arrival does not break the config schema. The shape:

```toml
[aperture.security]
tls.enabled = false                # default; v0 ignores even if true is set, but warns
tls.cert_path = ""                 # populated only when tls.enabled = true
tls.key_path = ""
spiffe.enabled = false             # reserved for Phase 2 (Aegis)
spiffe.trust_domain = ""           # reserved
```

If an operator sets `tls.enabled = true` on Aperture v0, Aperture emits a single warn-level stderr line at startup and continues plaintext. The schema is forward-compatible; the behaviour is plaintext-only.

---

## Slice 01 — the walking skeleton (visualised)

What the OTel Rust SDK sees, end to end:

```
SDK                     Aperture                      StubSink
 |                         |                              |
 |--ExportLogsServiceReq--->| (gRPC :4317)                 |
 |                         |                              |
 |                         |--validate_logs(bytes, Grpc)->|
 |                         |   [otlp-conformance-harness] |
 |                         |<-----Ok(ExportLogsService...)|
 |                         |                              |
 |                         |--accept(SinkRecord::Logs)--->|
 |                         |                              |--+
 |                         |                              |  | log to stderr:
 |                         |                              |  | {"event":"sink_accepted",...,
 |                         |                              |  |  "resource.service.name":"payments-api",
 |                         |                              |  |  "record_count":3}
 |                         |                              |<-+
 |                         |<-----Ok(())------------------|
 |                         |                              |
 |<--gRPC OK---------------|                              |
 |                         |                              |
```

Five integration edges in one slice:

1. SDK -> Aperture (real OTLP/gRPC over the wire, real port :4317).
2. Aperture -> harness (real `validate_logs` call, real `opentelemetry-proto` types).
3. Aperture -> sink (real trait dispatch, real `StubSink` impl).
4. Aperture -> stderr (real structured JSON line).
5. Aperture -> SDK (real gRPC OK status).

Andrea explicitly chose this thicker walking skeleton over a hard-coded reject because the harness is the load-bearing dependency: **integration risk has to land at Slice 01**.

---

## Failure paths surfaced in this journey

The journey YAML's `failure_modes` field for each step (see `journey-aperture.yaml`) catalogs every failure path at requirements granularity. In summary:

| Activity | Failure | What the user sees |
|---|---|---|
| Bind | Port already in use | stderr error line, exit code 1, `/readyz` never goes 200. |
| Bind | TLS knob set true on v0 | stderr warn line, plaintext continues. |
| Receive | gRPC frame size > limit | gRPC `RESOURCE_EXHAUSTED` (or `INVALID_ARGUMENT` per OTLP norm). |
| Receive | TCP connection drops mid-body | request abandoned, no half-validated record reaches the sink. |
| Validate | `EmptyInput` | gRPC `INVALID_ARGUMENT` / HTTP 400 with the violation Display string. |
| Validate | `WireType::ProtobufDecode` | same shape, different rule. |
| Validate | `WireType::SignalMismatch` | same shape, names asserted vs observed signal. |
| Hand off | sink returns `Err` | gRPC `UNAVAILABLE` / HTTP 503 with sink error message. |
| Observe | `/readyz` 503 during startup | upstream LB does not route to this instance yet — by design. |
| Shut down | drain deadline hit | non-zero exit, dropped-request count on stderr. |
| Backpressure | concurrency cap hit | gRPC `RESOURCE_EXHAUSTED` / HTTP 503 with `Retry-After`. |

Every failure is observable on stderr as a single structured JSON line. Every failure produces a status the SDK can act on. **No silent failures.**

---

## How this visual maps to subsequent artefacts

| Artefact | What it carries forward |
|---|---|
| `journey-aperture.yaml` | The same six activities, same TUI/wire mockups, with embedded Gherkin per step and machine-readable shared-artifact tracking. |
| `journey-aperture.feature` | A standalone feature file consolidating the embedded Gherkin scenarios for the DISTILL wave. |
| `shared-artifacts-registry.md` | Every `${variable}` in the mockups (port numbers, the harness function name, the `OtlpSink` trait name, `OTLP_SPEC_VERSION`, etc.) with its single source of truth. |
| `story-map.md` | The same backbone, with thin slices stacked underneath. |
| `user-stories.md` | One LeanUX story per concrete behaviour visualised here. |

The visual is the contract. Everything downstream defends it.

---

## Changelog

- 2026-05-04 — Luna — Initial journey visual derived from Andrea's locked scope (Q1–Q6 + Slice 01 shape). Walking skeleton: real OTel Rust SDK -> Aperture gRPC :4317 -> real `otlp_conformance_harness::validate_logs` -> `StubSink` -> stderr JSON -> gRPC OK back to SDK.
