# Slice 01 — Walking skeleton

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-AP-01, US-AP-03 (gRPC arm), US-AP-04 (gRPC arm).
> **Companion slice files**: none upstream — this is the walking skeleton.

## Outcome added

A real OpenTelemetry Rust SDK 0.27 sends an `ExportLogsServiceRequest` over OTLP/gRPC to `localhost:4317`; Aperture binds the listener, calls the **real** `otlp_conformance_harness::validate_logs(bytes, Framing::GrpcProtobuf)`, hands the typed record to a `StubSink` implementation of the `OtlpSink` trait, the sink writes a single structured stderr JSON line, and the SDK receives gRPC OK.

## What it lights up (across the six backbone activities)

| Activity | Slice 01 coverage |
|---|---|
| Bind listeners | gRPC `:4317` only. (HTTP `:4318` deferred to Slice 02.) |
| Receive payload | gRPC `ExportLogsServiceRequest` only. |
| Validate via harness | Real `validate_logs(bytes, Framing::GrpcProtobuf)` call. **Not a stub.** Reject paths come for free with this call. |
| Hand off to sink | Real `OtlpSink` trait dispatch to a concrete `StubSink` implementation. |
| Observe self | stderr structured JSON for `startup`, `listener_bound`, `request_received`, `sink_accepted`. |
| Shut down gracefully | Process exits cleanly on SIGTERM (best-effort, no full drain — Slice 08 lands that). |

## Demo command

```bash
# Terminal 1: build and run Aperture with default config.
cargo run -p aperture -- --config examples/config-stub.toml

# Terminal 2: send a real OTLP/gRPC logs export from the OTel Rust SDK.
cargo run --example send_one_log_record_grpc

# Expected: the SDK prints "exported 1 record" with no error.
# Expected: Aperture's stderr (terminal 1) shows three JSON lines:
#   event=listener_bound transport=grpc addr=0.0.0.0:4317
#   event=request_received transport=grpc signal=logs
#   event=sink_accepted sink=stub signal=logs record_count=1 resource.service.name="payments-api"
```

## Acceptance summary (full UAT in user-stories.md and journey-aperture.feature)

- TCP listener accepts connections on `0.0.0.0:4317` after startup completes.
- One stderr JSON line per request with `event=request_received`.
- A real `ExportLogsServiceRequest` from OTel Rust SDK 0.27 round-trips to gRPC OK.
- One stderr JSON line per accepted record with `event=sink_accepted` naming `record_count` and `resource.service.name`.
- An empty body produces gRPC `INVALID_ARGUMENT` with `grpc-message` carrying the harness's `OtlpViolation::Display` output verbatim.

## Complexity drivers

- First integration of `tonic` Server with the harness library. The OTLP/gRPC service definition (`opentelemetry-proto`'s `LogsServiceServer`) is the surface that needs handling.
- First definition of the `OtlpSink` trait and `SinkRecord` enum. DESIGN-wave (Morgan) locks the exact signatures.
- First use of structured stderr JSON. The log-event vocabulary established here gets reused by every later slice.

## Known unknowns

- The exact mapping from the harness's `OtlpViolation` to the gRPC `Status` is straightforward in the happy and rejection cases (rule + message), but **status code choice for sink failure** is an open design question Morgan will resolve in DESIGN. DISCUSS specifies `UNAVAILABLE` for sink-side failures; DESIGN confirms.
- Whether `aperture --version` displays `aperture_version` only or also `OTLP_SPEC_VERSION` (re-exported from the harness) is a DESIGN-wave question.

## Out of scope for this slice

- HTTP/protobuf transport (Slice 02).
- `/healthz` and `/readyz` endpoints (Slice 02 — they live on the HTTP port).
- Traces and metrics signals (Slices 03, 04).
- Backpressure / concurrency cap (Slice 05).
- ForwardingSink (Slice 06).
- TLS / SPIFFE schema (Slice 07).
- Graceful shutdown drain (Slice 08).
