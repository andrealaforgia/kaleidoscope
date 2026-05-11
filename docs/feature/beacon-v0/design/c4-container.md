# Beacon v0 — C4 Container

```mermaid
flowchart TB
    subgraph beacon_workspace["beacon (Rust workspace)"]
        direction TB
        subgraph beacon_crate["crates/beacon (library)"]
            Loader["cue loader<br/>+ schema validation"]
            Eval["evaluator<br/>pure: (rules, fetch, now) -> result"]
            StateMachine["per-rule state machine<br/>Inactive → Pending → Firing → Resolved"]
            Inhibit["inhibition + grouping<br/>pure function"]
            Sink["Sink trait<br/>async emit(incident)"]
            SLOSynth["SLO MWMBR synthesiser<br/>CUE SLO -> 5 PromQL rules"]
            Loader --> Eval
            Eval --> StateMachine
            StateMachine --> Inhibit
            Inhibit -.->|incident batch| Sink
            SLOSynth --> Loader
        end

        subgraph sinks_crate["crates/beacon/src/sinks (adapters)"]
            Webhook[WebhookSink]
            Smtp[SmtpSink]
            Mattermost2[MattermostSink]
            Zulip2[ZulipSink]
            OnCall2[OnCallSink]
            Webhook -.->|implements| Sink
            Smtp -.->|implements| Sink
            Mattermost2 -.->|implements| Sink
            Zulip2 -.->|implements| Sink
            OnCall2 -.->|implements| Sink
        end

        subgraph server_crate["crates/beacon-server (binary)"]
            Cli["CLI: --rules <dir> --backend <url>"]
            Scheduler["Scheduler<br/>tokio::time::interval"]
            HttpClient["Prom HTTP client<br/>(shared shape w/ Prism)"]
            Telemetry["OTLP telemetry<br/>(optional, env-gated)"]
            SignalHandler["SIGHUP handler<br/>triggers Loader reload"]

            Cli --> Loader
            Cli --> Scheduler
            Scheduler -->|fires tick| Eval
            Eval -->|fetch effect| HttpClient
            HttpClient --> Prom_External["Prometheus / Mimir<br/>(external)"]
            HttpClient --> Eval
            Eval -->|incident batch| sinks_crate
            sinks_crate -.->|telemetry spans| Telemetry
            SignalHandler --> Loader
        end
    end

    style beacon_crate fill:#dfd
    style sinks_crate fill:#dfe
    style server_crate fill:#fef
```

## Module shape

| Module | Role | Pure / IO |
|---|---|---|
| `beacon::loader` | Parse `.cue` → `Rule` / `Slo` structs | Pure (file IO at boundary) |
| `beacon::evaluator` | `(rules, fetch_fn, now)` → `EvaluationResult` | Pure |
| `beacon::state_machine` | Per-rule state transitions | Pure |
| `beacon::inhibition` | Apply inhibition + grouping | Pure |
| `beacon::sinks::*` | Adapter implementations | IO (async) |
| `beacon::slo` | MWMBR synthesis from CUE SLO | Pure |
| `beacon_server::scheduler` | `tokio::time::interval` tick loop | IO |
| `beacon_server::http` | PromQL HTTP client (`reqwest`) | IO |
| `beacon_server::signal` | `SIGHUP` reload trigger | IO |
| `beacon_server::telemetry` | OTLP exporter wiring | IO |

## Library vs binary split

The `beacon` crate is a library. Its public API is the
evaluator + sink trait + CUE loader. Consumers (the
`beacon-server` binary, future embedders) wire the IO concerns
(scheduler, HTTP client, signal handler) at the binary layer.

This mirrors the Aperture split (library + service) and is the same
shape as Prism's reducer + Scheduler seam. The benefit is that the
load-bearing logic — rule evaluation, inhibition, SLO synthesis —
is testable as pure functions, and the IO concerns are testable
independently with mocks.

## Concurrency model

The evaluator is single-threaded per rule set. The scheduler ticks
on a `tokio::time::interval`; each tick spawns a per-rule task that
issues the PromQL fetch via `reqwest` and posts the result back to
the main evaluator via a Tokio channel. The evaluator processes
results serially and emits sink calls; each sink emission is its
own task. Sinks are independent — a slow Mattermost API cannot
block webhook delivery to other rules.

Bounded channels with backpressure. If sink emission queues grow
beyond a configured limit, the evaluator emits a telemetry warning
and drops oldest. The contract is: "every incident is best-effort
delivered; persistent failures are recorded but do not block the
evaluator".
