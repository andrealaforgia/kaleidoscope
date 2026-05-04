# Walking Skeleton — `aperture` v0 (DISTILL notes)

> **Wave**: DISTILL.
> **Author**: Quinn (Scholar).
> **Date**: 2026-05-04.
> **Companion documents**: `wave-decisions.md`,
> `acceptance-test-coverage-matrix.md`, `gherkin-scenarios.feature`,
> `crates/aperture/tests/slice_01_walking_skeleton.rs` (the
> executable form is the SSOT).

This file is a *notes* document — the executable walking skeleton
lives in the Rust integration test
`crates/aperture/tests/slice_01_walking_skeleton.rs`. The notes here
record the user-centric framing, the strategy rationale, and the
litmus-test pass.

---

## The skeleton — one paragraph

A real OpenTelemetry SDK client (a Rust integration test using a
`tonic`-generated `LogsServiceClient` against the
`opentelemetry-proto` service definition) sends a real
`ExportLogsServiceRequest` body — a single log record carrying
`resource.service.name = "payments-api"` — over OTLP/gRPC to a
freshly-launched Aperture instance bound to an ephemeral loopback
port. Aperture binds the listener, calls the real
`otlp_conformance_harness::validate_logs(bytes,
Framing::GrpcProtobuf)` (no validator stub), hands the typed record
to an `OtlpSink` implementation (the test substitutes
`aperture::testing::RecordingSink` at the trait seam — production
runs `StubSink` or `ForwardingSink`), the sink writes a single
structured stderr JSON line `{event:"sink_accepted", sink:"stub",
signal:"logs", record_count:1, "resource.service.name":"payments-api"}`,
and the SDK receives gRPC `OK`.

That is the value proposition. Everything else in v0 (HTTP transport,
traces, metrics, backpressure, forwarding, drain) is incremental
slices on this substrate.

---

## Walking Skeleton Strategy — C (Real local)

The orchestrator pre-authorised **Strategy C — Real local** on
Andrea's behalf:
- Real `tonic` Server bound to ephemeral loopback ports.
- Real `axum` Server bound to ephemeral loopback ports.
- Real `otlp-conformance-harness` invocation (no validator double).
- Real loopback TCP between the test-side `tonic` client and the
  Aperture-side `tonic` Server.
- For Slice 06: real `wiremock` downstream (in-process axum stub on
  loopback).

The sink at the trait seam is `aperture::testing::RecordingSink` (a
test double that records records into a `Mutex<Vec<SinkRecord>>`).
The `OtlpSink` trait IS the hexagonal seam (DESIGN ADR-0007); the
trait's hexagonal-correct test pattern is "real adapter, test sink"
exactly as Strategy C names.

**No InMemory transports. No costly external deps. No container.**

---

## Tags applied to walking-skeleton scenarios

Per the orchestrator brief: walking-skeleton scenarios in
`gherkin-scenarios.feature` are tagged:

```gherkin
@walking_skeleton @real-io @driving_adapter
```

The `@real-io` tag signals that the scenario exercises real
loopback transports (Strategy C compliance); `@driving_adapter`
signals it enters Aperture through a driving port over real
protocol (RCA P1 compliance).

---

## What the skeleton lights up across the six backbone activities

(Excerpt from `slices/slice-01-walking-skeleton.md`.)

| Activity | Slice 01 coverage |
|---|---|
| Bind listeners | gRPC `:4317` only. (HTTP `:4318` arrives in Slice 02.) |
| Receive payload | gRPC `ExportLogsServiceRequest` only. |
| Validate via harness | Real `validate_logs(bytes, Framing::GrpcProtobuf)` call. |
| Hand off to sink | Real `OtlpSink` trait dispatch to a concrete impl. |
| Observe self | stderr structured JSON for `startup`, `listener_bound`, `request_received`, `sink_accepted`. |
| Shut down gracefully | Process exits cleanly on SIGTERM (full drain in Slice 08). |

Slice 01 trades width for depth: only the gRPC arm and only the logs
signal land at this slice, but the harness boundary, the sink trait,
and the observability vocabulary all land *together*. Andrea's locked
choice (DISCUSS `wave-decisions.md > Slice 01 — walking-skeleton
shape`).

---

## Walking-skeleton litmus test (CM-C)

| Litmus | Verdict |
|---|---|
| Title describes user goal? | YES — "customer exports one log record and receives gRPC OK" |
| Given/When describe user actions? | YES — "an OTel SDK is configured with the local Aperture endpoint and emits one log record"; "the SDK calls its OTLP/gRPC log exporter once" |
| Then describe user observations? | YES — "the SDK receives gRPC status 0 (OK)"; "stderr contains a JSON line naming the accepted record" |
| Stakeholder confirmable? | YES — Andrea (and any operator) can read the test name and confirm "yes, that is what an SDK client wants" |
| Real protocol entry? | YES — real `tonic` client against real `tonic` Server over real loopback TCP |
| Real harness? | YES — `otlp_conformance_harness::validate_logs` is called for real |
| `@real-io @driving_adapter` tagged? | YES — see `gherkin-scenarios.feature` |

All seven points pass. The walking skeleton is user-centric, real-I/O,
and demo-able to a stakeholder.

---

## Why this skeleton, not a smaller one

DISCUSS `wave-decisions.md > Slice 01 — walking-skeleton shape`
records Andrea's explicit choice:

> Andrea explicitly chose this thicker walking skeleton over a
> smaller "hard-coded reject" version because the harness is the
> load-bearing dependency and integration risk should land at Slice
> 01.

A smaller skeleton (e.g. "binary starts and exits cleanly", or "gRPC
listener accepts a connection that immediately closes") would prove
the wiring without proving the value proposition. The harness is
the load-bearing dependency for Aperture's value; the
walking-skeleton scenarios test the harness boundary AND the sink
hand-off AND the stderr observability surface — three integration
risks landing together in Slice 01 so subsequent slices add capability
without re-litigating any of them.

---

## DELIVER hand-off

DELIVER's first task is to take the RED scaffold at
`crates/aperture/src/{lib.rs, main.rs, ...}` and grow it until
`tests/slice_01_walking_skeleton.rs` goes GREEN. The test functions
to drive green, in order of difficulty:

1. `customer_exports_one_log_record_and_receives_grpc_ok`
2. `customer_exports_one_log_record_and_record_reaches_sink`
3. `customer_exports_one_log_record_and_record_carries_logs_variant`
4. `customer_exports_three_log_records_and_record_count_matches`
5. `startup_emits_listener_bound_stderr_line_for_grpc_transport`
6. `customer_exports_one_log_record_and_request_received_line_is_emitted`
7. `customer_exports_one_log_record_and_sink_accepted_line_names_record_count`
8. `customer_exports_one_log_record_and_sink_accepted_line_names_service_name`
9. `customer_sends_empty_body_and_receives_invalid_argument`
10. `customer_sends_empty_body_and_grpc_message_names_empty_input_rule`
11. `customer_sends_empty_body_and_grpc_message_names_logs_signal`
12. `customer_sends_empty_body_and_grpc_message_names_grpc_protobuf_framing`
13. `customer_sends_empty_body_and_no_record_reaches_sink`

Tests 1-4 prove the gRPC arm of the validate-and-route path.
Tests 5-8 prove the stderr observability surface.
Tests 9-13 prove the harness-violation-verbatim contract on the
reject path.

Once Slice 01 is GREEN, the walking skeleton has been proven and
DELIVER moves to Slice 02 (HTTP transport + healthz/readyz).
