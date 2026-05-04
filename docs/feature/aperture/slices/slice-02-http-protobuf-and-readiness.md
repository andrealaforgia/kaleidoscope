# Slice 02 — HTTP/protobuf transport plus `/healthz` and `/readyz`

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-AP-02, US-AP-03 (HTTP arm), US-AP-04 (HTTP arm).
> **Depends on**: Slice 01.

## Outcome added

OpenTelemetry SDKs that prefer or require OTLP/HTTP/protobuf get first-class treatment: `POST /v1/logs` with `Content-Type: application/x-protobuf` and a real `ExportLogsServiceRequest` body round-trips to HTTP 200. The same HTTP listener also serves `/healthz` (always 200 while the process is up) and `/readyz` (200 once both listeners are bound, 503 during startup).

## What it lights up (across the six backbone activities)

| Activity | Slice 02 coverage |
|---|---|
| Bind listeners | HTTP `:4318` joins gRPC `:4317`. |
| Receive payload | `POST /v1/logs` with `application/x-protobuf` body. |
| Validate via harness | `validate_logs(bytes, Framing::HttpProtobuf)`. The same call site that handles gRPC switches on the inbound transport to pick the right `Framing` variant. |
| Hand off to sink | (StubSink reused unchanged.) |
| Observe self | `/healthz`, `/readyz` (with startup-state-machine `starting` -> `ready`). New stderr events: `readiness_changed`. |
| Shut down gracefully | (Best-effort still; Slice 08 lands the full state machine.) |

## Demo command

```bash
# Terminal 1: Aperture, same as Slice 01 but with HTTP listener too.
cargo run -p aperture -- --config examples/config-stub.toml

# Terminal 2:
curl -fsS http://localhost:4318/healthz                     # -> "ok"
curl -fsS http://localhost:4318/readyz                      # -> "ready"

# Send a real ExportLogsServiceRequest body (captured from the OTel Rust SDK).
curl -fsS \
  -H 'Content-Type: application/x-protobuf' \
  --data-binary @examples/fixtures/logs-minimal.bin \
  http://localhost:4318/v1/logs                             # -> HTTP 200
```

## Acceptance summary

- HTTP listener accepts on `:4318` after startup.
- `GET /healthz` -> 200 `"ok"` always while process is up.
- `GET /readyz` returns 503 `"starting"` before listeners bind, 200 `"ready"` after.
- `POST /v1/logs` with the right Content-Type and a valid body returns HTTP 200 (record reaches the StubSink).
- `POST /v1/logs` with `Content-Type: application/json` returns HTTP 415.
- `POST /v1/profile` returns HTTP 404.
- An empty body to `POST /v1/logs` returns HTTP 400 with the harness's violation Display string verbatim in the body.

## Complexity drivers

- First integration of `hyper` Server alongside the existing `tonic` Server. The two share the same Tokio runtime per Q2.
- The `/healthz` and `/readyz` endpoints multiplex on the HTTP port — three concerns (OTLP `/v1/*`, liveness, readiness) on one port. DESIGN owns the routing scheme (likely a path-prefix dispatch).
- The transport-to-`Framing` mapping is now a real two-arm choice rather than a hard-coded `GrpcProtobuf`.

## Known unknowns

- Whether `/healthz` and `/readyz` should be on the HTTP port or on a dedicated admin port. DISCUSS lands on multiplexed HTTP because operators expect to probe one port. DESIGN may revisit if security review surfaces a reason to split.

## Out of scope

- Traces (Slice 03), metrics (Slice 04).
- Concurrency cap (Slice 05).
- ForwardingSink (Slice 06).
- Drain-aware `/readyz` `"draining"` state (Slice 08 — until then `/readyz` only flips between starting and ready).
