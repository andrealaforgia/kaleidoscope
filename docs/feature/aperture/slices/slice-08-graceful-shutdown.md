# Slice 08 — Graceful shutdown (drain in-flight, observable verdict)

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-AP-09.
> **Depends on**: Slice 02 (`/readyz` state machine), Slice 06 (sink in-flight semantics), Slice 05 (per-transport semaphore for in-flight count).

## Outcome added

On SIGTERM (or SIGINT, or k8s preStop hook):

1. `/readyz` immediately flips to 503 `"draining"`, so any orchestrator with a readiness probe stops sending new traffic to this instance.
2. Listeners stop accepting new connections.
3. In-flight requests drain — they complete the validate -> sink -> respond cycle — to a configurable deadline (default 30 s).
4. On clean drain, the process exits 0 with a stderr line naming the drained count.
5. On deadline expiry, the process exits 1 with a stderr line naming the dropped count. **The drop is loud, never silent.**

This slice is the production-readiness gate. A service that drops in-flight requests on every restart is unfit for any operator-managed deployment.

## What it lights up

| Activity | Slice 08 coverage |
|---|---|
| Bind listeners | New behaviour: listener_closing event on shutdown initiated; new state where listener exists but does not accept new connections. |
| Receive payload | (Reuse — refused at the listener level after readiness flips.) |
| Validate via harness | (Reuse.) |
| Hand off to sink | New behaviour: sink.accept inherits the drain deadline; if it does not return by the deadline, its request becomes a drop. |
| Observe self | `/readyz` gains the `draining` state. New stderr events: `shutdown_initiated`, `listener_closing`, `in_flight_drained`, `drain_deadline_exceeded`, `shutdown_complete`. |
| Shut down gracefully | This is the slice that completes Activity 6. |

## Demo command

```bash
# Terminal 1: Aperture with full config, configured with a small drain deadline so the demo is fast.
APERTURE_DRAIN_MS=5000 cargo run -p aperture -- --config examples/config-forwarding.toml

# Terminal 2: drive a slow client that holds the request open for 2 seconds.
cargo run --example send_one_log_record_slow_grpc &
SLOW_PID=$!

# Quickly:
sleep 1
kill -TERM $(pgrep aperture)

# Expected: the slow request completes (it is in-flight when SIGTERM arrives, the deadline is 5 s, the client is 2 s).
# Expected stderr (terminal 1):
#   event=shutdown_initiated signal=SIGTERM drain_deadline_ms=5000
#   event=readiness_changed ready=false reason=shutdown_drain
#   event=listener_closing transport=grpc
#   event=listener_closing transport=http_protobuf
#   event=in_flight_drained drained_count=1
#   event=shutdown_complete exit_code=0

# Negative case: hold the request open for 10 seconds with deadline 5 s.
# Expected stderr ends with:
#   event=drain_deadline_exceeded dropped_count=1
#   event=shutdown_complete exit_code=1
```

## Acceptance summary

- SIGTERM flips `/readyz` to 503 `"draining"` within 100 ms.
- New connections are refused at the TCP listener level after the readiness flip (TCP reset, not a structured response — listeners stop accepting).
- In-flight requests complete normally if they finish before the deadline.
- On clean drain: stderr `in_flight_drained` event names the drained count, stderr `shutdown_complete` event names exit code 0, process exits 0.
- On deadline expiry: stderr `drain_deadline_exceeded` event names the dropped count (warn level), stderr `shutdown_complete` names exit code 1, process exits 1.
- SIGINT and SIGTERM behave identically.
- SIGKILL is acknowledged in the operator runbook as un-graceful by definition; Aperture does not attempt to handle it.

## Complexity drivers

- The most cross-cutting slice. The drain-deadline propagates into every place that holds a request: the listener, the harness call (always synchronous, fast — no propagation needed), the sink (asynchronous, deadline-respecting).
- The `/readyz` state machine gains its third state (`draining`); the state-machine test must cover all three transitions.
- Counting in-flight is owned by the per-transport semaphore introduced in Slice 05. The drain logic reads the current permit deficit to compute "in-flight count".

## Known unknowns

- Default drain deadline. DISCUSS picks 30 s as a standard k8s-friendly default. DESIGN may refine.
- Whether to gate listener closure on the readiness flip propagating to the orchestrator (i.e. wait one or two readiness-probe periods before closing listeners). DISCUSS leaves this to DESIGN; the v0 contract is "flip, close, drain", but a "flip, wait, close, drain" variant may be safer in practice.

## Out of scope

- Restart-on-config-change. v0 ships restart-as-process-exit; live config reload is out of scope.
- Connection draining at the protocol level (gRPC `GOAWAY` frames, HTTP `Connection: close`). DISCUSS defers the protocol-level signalling decision to DESIGN; v0's contract is at the listener level (do not accept new), not at the protocol level.
