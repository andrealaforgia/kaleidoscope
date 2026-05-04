# Slice 06 — ForwardingSink (downstream OTLP write)

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-AP-08.
> **Depends on**: Slice 05.

## Outcome added

`ForwardingSink` writes accepted OTLP records to a configured downstream OTel-compatible backend (Loki, Tempo, Mimir, an OTel Collector, or any other OTLP receiver). Per the Phase-1 roadmap promise: Aperture integrates with the operator's existing telemetry stack without requiring Kaleidoscope-native storage.

After this slice, Aperture is **production-useful**. Slice 01–05 produce a service that accepts traffic and logs it to stderr; Slice 06 makes that traffic land somewhere durable.

## What it lights up

| Activity | Slice 06 coverage |
|---|---|
| Bind listeners | (Reuse.) |
| Receive payload | (Reuse.) |
| Validate via harness | (Reuse.) |
| Hand off to sink | New `ForwardingSink` impl of `OtlpSink`. Same trait, second concrete impl. |
| Observe self | New stderr fields on `sink_accepted`: `sink=forwarding`, `downstream`, `downstream_latency_ms`. New stderr event: `sink_failed` with `sink=forwarding` and downstream error message. |
| Shut down gracefully | (Reuse — drain in Slice 08 will respect the sink's deadline.) |

## Demo command

```bash
# Terminal 1: a downstream OTel-compatible receiver. Easiest is a local OTel Collector.
docker run --rm -p 14318:4318 otel/opentelemetry-collector \
  --config /path/to/collector-stdout-exporter.yaml

# Terminal 2: Aperture configured to forward.
APERTURE_SINK=forwarding APERTURE_DOWNSTREAM=http://localhost:14318 \
  cargo run -p aperture -- --config examples/config-forwarding.toml

# Terminal 3: drive a real OTLP/gRPC export.
cargo run --example send_one_log_record_grpc

# Expected: the OTel Collector (terminal 1) prints the log record.
# Expected: Aperture's stderr (terminal 2):
#   event=sink_accepted sink=forwarding downstream=http://localhost:14318 signal=logs record_count=1 downstream_latency_ms=N

# Negative case: stop the Collector, then re-run the export.
# Expected: SDK receives gRPC status 14 (UNAVAILABLE).
# Expected stderr: event=sink_failed sink=forwarding (with downstream error message).
```

## Acceptance summary

- With `sink=forwarding` and a healthy downstream: real OTel SDK export -> gRPC OK / HTTP 200, downstream receives the typed record verbatim, stderr `sink_accepted` line includes `downstream` and `downstream_latency_ms`.
- With `sink=forwarding` and a downstream returning 5xx: SDK receives gRPC `UNAVAILABLE` / HTTP 503, stderr `sink_failed` event names the downstream error.
- With `sink=forwarding` and a refused TCP connection: same shape as 5xx; stderr error message names "connection refused".
- With `sink=forwarding` and a timeout (default 5 s, configurable): same shape; stderr error names "downstream timeout".
- ForwardingSink is the only outbound network Aperture originates. CI invariant `no_telemetry_on_telemetry` defends this.

## Complexity drivers

- First outbound network from Aperture. The downstream client (HTTP and/or gRPC) needs its own connection pool, timeout, and retry policy. DISCUSS specifies: no retries at v0 (the SDK retries; Aperture should not double-retry); DESIGN locks the timeout default.
- Three error shapes from the downstream collapse into one shape upstream: `UNAVAILABLE` / 503. The downstream's specific error becomes the stderr message but does not leak into the SDK-facing response.
- For each accepted record, the typed `Export*ServiceRequest` value flows directly from harness output to ForwardingSink input — no re-encoding, no field-by-field translation. The harness's type-path identity guarantee (US-04 AC 2 in `otlp-conformance-harness-v0`) is what makes this safe.

## Known unknowns

- Whether to support both gRPC and HTTP outbound, or only one. DISCUSS picks "one outbound transport, configurable, default HTTP/protobuf" because HTTP is simpler to debug and most downstreams accept it.
- Default downstream timeout. DISCUSS picks 5 s; DESIGN may revisit.

## Out of scope

- Multiple downstream sinks fanned out from one receive. Out of scope for v0 (Sieve and Sluice will own routing complexity in later phases).
- Authentication to the downstream. v0 ships plaintext to a co-located backend; auth hooks land with Aegis (Phase 2).
- Retries / circuit breakers. The SDK retries; Aperture refuses fast and lets the SDK decide.
