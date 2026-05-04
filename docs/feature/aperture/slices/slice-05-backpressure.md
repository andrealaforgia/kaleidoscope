# Slice 05 — Backpressure (concurrency cap, deterministic refusal)

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-AP-07.
> **Depends on**: Slice 04.

## Outcome added

Each transport (gRPC, HTTP/protobuf) carries a configurable `max_concurrent_requests` cap. Once the cap is reached, additional requests are **deterministically refused** — never blocked, never silently dropped, never internally queued — and every refusal is observable on stderr.

This is the riskiest unvalidated assumption (load behaviour) addressed before slice 06's first outbound network. Per Andrea's locked Q4 decision: cap, refuse, never block, never drop silently.

## What it lights up

| Activity | Slice 05 coverage |
|---|---|
| Bind listeners | (Reuse.) |
| Receive payload | New gate: per-transport semaphore with capacity `max_concurrent_requests`. Failed acquire -> immediate refusal. |
| Validate via harness | (Reuse — refused requests do not reach the harness.) |
| Hand off to sink | (Reuse.) |
| Observe self | New stderr event: `concurrency_cap_hit` with `transport` and `cap`. |
| Shut down gracefully | (Reuse — drain in Slice 08 will use the same semaphore counts to compute "in-flight".) |

## Demo command

```bash
# Terminal 1: Aperture configured with a tiny cap so overload is easy to trigger.
APERTURE_GRPC_MAX_CONCURRENT=2 APERTURE_HTTP_MAX_CONCURRENT=2 \
  cargo run -p aperture -- --config examples/config-stub.toml

# Terminal 2: drive 4 simultaneous gRPC exports (a small bash loop with parallel curls).
cargo run --example send_four_concurrent_logs_grpc

# Expected: 2 of the 4 receive gRPC OK; 2 of the 4 receive
#   gRPC status 8 (RESOURCE_EXHAUSTED) with grpc-message naming cap=2
# Expected stderr: 2 lines with event=concurrency_cap_hit transport=grpc cap=2

# Terminal 2: same for HTTP.
parallel-curl -n 4 http://localhost:4318/v1/logs ...
# Expected: 2 of 4 -> HTTP 200; 2 of 4 -> HTTP 503 with header "Retry-After: 1"
```

## Acceptance summary

- gRPC: 5th simultaneous request when cap=4 receives `grpc-status: 8` (`RESOURCE_EXHAUSTED`); `grpc-message` names the cap.
- HTTP: 5th simultaneous POST when cap=4 receives HTTP 503 with `Retry-After: 1` header; body names the cap.
- For every refused request, exactly one `event=concurrency_cap_hit` stderr line is written, with `transport` and `cap` fields.
- No request beyond the cap is internally queued, blocked, or silently dropped. The `@property` UAT in `journey-aperture.feature` defends this invariant.
- The cap is independent per transport: a saturated gRPC listener does not block HTTP requests, and vice versa.

## Complexity drivers

- Per-transport semaphore lifecycle: acquired on connection accept, released on response sent (or on connection drop). Holding a permit for the full request lifetime — not just the validate-and-hand-off — is intentional, because the sink is part of "in-flight" and slice 08 will reuse this for drain.
- Mapping refusal to the right error shape per transport. gRPC's `RESOURCE_EXHAUSTED` and HTTP's 503 + `Retry-After` are the OTel-canonical refusal shapes; the wire format must match what an OTel SDK's retry policy expects.

## Known unknowns

- Default value of `max_concurrent_requests` per transport. DISCUSS picks 1024; DESIGN may revisit. Independent of the default, the cap is operator-tunable.
- Whether to expose a per-tenant or per-source cap as well as a per-transport cap. Out of scope for v0; multi-tenancy is Aegis's job in Phase 2.

## Out of scope

- Internal queueing (Sluice's job, Phase 7).
- Adaptive caps based on system load (out of scope for v0; potentially Pulse-driven in Phase 4).
- ForwardingSink and graceful shutdown (Slices 06 and 08).
