# Slice 03 — Traces signal end-to-end

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-AP-05.
> **Depends on**: Slice 02.

## Outcome added

OpenTelemetry SDKs that emit traces get first-class treatment on both transports. A real `ExportTraceServiceRequest` arriving on `localhost:4317` (gRPC) or `POST localhost:4318/v1/traces` (HTTP/protobuf) round-trips to gRPC OK / HTTP 200; the StubSink writes a structured stderr line naming `signal=traces` and the span count.

## What it lights up

| Activity | Slice 03 coverage |
|---|---|
| Bind listeners | (Reuse — no new listeners.) |
| Receive payload | gRPC `ExportTraceServiceRequest` and `POST /v1/traces`. |
| Validate via harness | New call site: `otlp_conformance_harness::validate_traces(bytes, framing)`. |
| Hand off to sink | `StubSink::accept(SinkRecord::Traces(record))`. The `SinkRecord` enum gains its second variant. |
| Observe self | New stderr fields: `signal=traces`, `span_count`. |
| Shut down gracefully | (Reuse.) |

## Demo command

```bash
# Terminal 1: Aperture with the demo config from earlier slices.
cargo run -p aperture -- --config examples/config-stub.toml

# Terminal 2: send a real ExportTraceServiceRequest over gRPC (captured from OTel Rust SDK).
cargo run --example send_one_span_grpc

# And over HTTP/protobuf:
curl -fsS \
  -H 'Content-Type: application/x-protobuf' \
  --data-binary @examples/fixtures/traces-minimal.bin \
  http://localhost:4318/v1/traces

# Expected stderr (terminal 1) for each: event=sink_accepted sink=stub signal=traces span_count=1
```

## Acceptance summary

- Real `ExportTraceServiceRequest` on gRPC -> gRPC OK + sink_accepted stderr line with `signal=traces`.
- Real `ExportTraceServiceRequest` on HTTP -> HTTP 200 + sink_accepted stderr line with `signal=traces`.
- `POST /v1/traces` with logs bytes -> HTTP 400, body contains `rule=WireType::SignalMismatch`, `observed=Logs`, `asserted=Traces` (verbatim from the harness's Display).
- `validate_traces` is invoked exactly once per traces request (CI invariant `single_validator_per_signal`).

## Complexity drivers

- Adding the second variant to `SinkRecord` exercises the trait's polymorphism for the first time. `StubSink` and the (future) `ForwardingSink` must handle all three variants.
- Counting spans for the stderr line requires walking the `ResourceSpans` -> `ScopeSpans` -> `Span` tree. Documenting this counting convention here means later slices and Pulse (Phase 4) can reuse it.

## Known unknowns

- Whether `span_count` includes spans inside scope spans only, or also rolls up resource-level metadata. DISCUSS picks "spans only" for simplicity; DESIGN may refine.

## Out of scope

- Metrics (Slice 04).
- Concurrency cap (Slice 05).
- ForwardingSink (Slice 06) — though the typed traces records will flow through it once it lands.
