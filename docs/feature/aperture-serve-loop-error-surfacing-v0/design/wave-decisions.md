# Wave Decisions: aperture-serve-loop-error-surfacing-v0 (DESIGN)

Author: Morgan (nw-solution-architect). Wave: DESIGN. Date: 2026-06-05.
Mode: PROPOSE (autonomous). British English. No em dashes in body.

Resolves the three DISCUSS-flagged decisions D1/D2/D3. The full
rationale, alternatives, and consequences live in
`docs/product/architecture/adr-0066-aperture-serve-loop-error-surfacing.md`.
This file is the feature-scoped resolution + the Reuse Analysis table +
the internal ripple map + constraints + upstream changes.

## Paradigm

Rust idiomatic (CLAUDE.md): data + free functions + traits only where
polymorphism is genuinely needed. The serve outcome is routed by a
concrete typed `JoinHandle<ServeOutcome>` and one `AtomicBool` per
transport; no new `dyn Trait`, no channel. Set; not re-asked.

## The three decisions, resolved (one line each)

- **D1** — Mechanism: the spawn closures return `JoinHandle<ServeOutcome>`
  AND self-react at the failure site (emit `serve_loop_failed` + flip
  readiness); the typed join folds a serve death into the exit code.
  Hybrid of option (c) self-react and option (a) typed result. Ripple is
  INTERNAL only; no public-API break; no new public type.
- **D2** — Process reaction: a new sticky `ReadinessPhase::Failed`
  (`/readyz` -> 503 `"failed"`) PLUS a distinct non-zero exit code
  **`3`**; `/healthz` stays 200.
- **D3** — Graceful-vs-fatal: the discriminator is a per-transport
  `Arc<AtomicBool> shutdown_requested`, set inside the existing
  graceful-shutdown closure when the oneshot resolves; shutdown-requested
  -> any return is clean (`Graceful`); not-requested -> any return (Err
  OR unexpected early Ok) is fatal (`Failed`). Early-Ok treated as fatal
  at v0.

## D1 — mechanism detail

```text
pub(crate) enum ServeOutcome { Graceful, Failed }   // Copy, Eq
pub(crate) struct ServeError(String)                 // reason rendered at the failure site
```

Each spawned task:

```text
let result = serve_future.await;                 // Ok on graceful, Err on fatal, early-Ok on self-stop
if shutdown_requested.load(Acquire) {
    ServeOutcome::Graceful                        // normal drain: no event, no flip
} else {
    tracing::error!(event = SERVE_LOOP_FAILED, transport = "...", error = %reason);
    readiness.flip_to_failed();
    ServeOutcome::Failed
}
```

The orchestrator (and the run loop, for the no-SIGTERM death) folds a
`ServeOutcome::Failed` into `DrainOutcome::ServeFailed` -> exit code 3.

## D2 — process reaction detail

- `ReadinessPhase::Failed = 3` added to `readiness.rs`; sticky like
  `Draining`. `flip_to_failed()` CAS `Ready|Starting -> Failed`, emits
  `readiness_changed ready=false reason=serve_loop_failed`.
- `/readyz`: `Failed -> (503, "failed\n")`, beside the existing
  `starting`/`ready`/`draining` arms.
- Precedence: `Draining` and `Failed` are both sticky terminal 503
  states; whichever lands first wins, the other's CAS no-ops. `/readyz`
  is 503 either way and never flaps back to 200.
- Exit map: `Clean -> 0`, `DeadlineExceeded -> 1`, config error `-> 2`
  (ADR-0061), **`ServeFailed -> 3`** (new, next free integer).
- `/healthz` stays 200 (C6).

## D3 — graceful-vs-fatal detail

A per-transport `Arc<AtomicBool>` `shutdown_requested` (init `false`).
The graceful-shutdown closure already awaits the oneshot
(`transport.rs:86`, `:155`); it sets the flag `true` the instant the
oneshot resolves, before the serve future drains. The task reads the
flag after the serve future returns. SIGTERM path -> flag `true` ->
`Graceful` -> NO `serve_loop_failed`. Post-bind death -> flag `false` ->
`Failed`. Unexpected early `Ok` with flag `false` -> `Failed` (fatal at
v0; a listener stopping unbidden is the zombie the feature kills).

## Reuse Analysis (MANDATORY) — this feature EXTENDS, creates NO new public type

| Concern | Existing component reused | Extension (additive, internal) | New public type? |
|---|---|---|---|
| Spawn the serving loop | `spawn_grpc` / `spawn_http` (`transport.rs:50,117`, `pub(crate)`) | Return type `JoinHandle<()>` -> `JoinHandle<ServeOutcome>`; task self-reacts | No (crate-private) |
| Carry the serve verdict to the orchestrator | `ShutdownBundle` (`shutdown.rs:125`, `pub(crate)`) | `grpc_join`/`http_join` field type changes; no new field | No |
| Map a verdict to an exit code | `DrainOutcome` + `exit_code()` (`shutdown.rs:92-106`, `pub(crate)`) | Add `ServeFailed` variant -> exit `3` | No |
| Drive `/readyz` | `ReadinessPhase` + `ReadinessState` (`readiness.rs:37`, `pub(crate)`) | Add sticky `Failed` phase + `flip_to_failed()`; `/readyz` `Failed -> 503 "failed"` | No |
| Name the failure on stderr | closed event vocabulary (`observability.rs:30-51`, ADR-0009) | One additive constant `SERVE_LOOP_FAILED` | No |
| Know shutdown was requested | the existing graceful-shutdown oneshot closure (`transport.rs:86,155`) | A per-transport `Arc<AtomicBool>` set inside the same closure | No |
| Inject a serve failure in-suite | hand-constructed `ShutdownBundle` test seam (`lib.rs:379-430`) + `testing::stderr_capture` | Synthetic join resolves to `ServeOutcome::Failed`; injectable serve future | No |

**Verdict: EXTEND-ONLY. No new public type, no public-API break, no new
crate, no new always-running task.** New internal types (`ServeOutcome`,
`ServeError`) are `pub(crate)`, carried inside crate-private bundle/join
types, never nameable from outside. Confirms C3. Any future leak would be
semver-MINOR, pre-1.0, NEVER 1.0.0.

## Internal ripple map (complete, bounded — all pub(crate))

```text
spawn_grpc / spawn_http (transport.rs:50,117)
  -> return JoinHandle<ServeOutcome>; task: await -> read shutdown flag
     -> Graceful | (emit serve_loop_failed + flip_to_failed -> Failed)
  -> compose::spawn (compose.rs:132,150,158,180-189)
       stores joins into ShutdownBundle
       -> ShutdownBundle.grpc_join/http_join : JoinHandle<ServeOutcome> (shutdown.rs:125-134)
            -> orchestrate_shutdown drain future (shutdown.rs:185-190)
                 awaits joins, folds Failed -> DrainOutcome::ServeFailed
                 -> DrainOutcome::exit_code() ServeFailed -> 3 (shutdown.rs:92-106)
                      -> drain_to_exit_code / run (lib.rs:205-227) -> exit 3
       run loop also selects on the joins for the no-SIGTERM death path (lib.rs:205-219)
  -> Handle::drop_signal_listeners (lib.rs:161-171)  [senders only; joins abandoned; low ripple]
  -> Tests: lib.rs:351-356 (mechanical), lib.rs:379-430 (injection seam + new serve-failure exit-3 test)

readiness.rs: + ReadinessPhase::Failed (sticky) + flip_to_failed() + /readyz Failed -> 503 "failed"
observability.rs: + SERVE_LOOP_FAILED constant
main.rs:13-21: + exit-code 3 doc line
```

NOT consumers (ripple does not reach): per-request gRPC service impls /
HTTP handlers (`transport.rs:214-463,501-701`); `wire_sink` /
`probe_or_refuse` (`compose.rs:28-96`); the public re-exports (`run`,
`spawn`, `Handle`, `config`, `ports`, `testing`).

## C4 — serve-loop outcome routing (Component / sequence)

```mermaid
sequenceDiagram
    participant OS as OS accept loop
    participant Task as serve task (grpc|http)
    participant Flag as shutdown_requested (AtomicBool)
    participant Read as ReadinessState
    participant Err as stderr (closed vocab)
    participant Orch as orchestrate_shutdown / run loop
    participant Exit as DrainOutcome -> exit code

    Note over Task: socket already bound (listener_bound emitted)
    OS-->>Task: serve future resolves (Ok | Err | early Ok)
    Task->>Flag: load()
    alt shutdown WAS requested (flag = true)
        Note over Task: graceful drain - clean no-op
        Task-->>Orch: JoinHandle resolves ServeOutcome::Graceful
        Orch->>Exit: Clean -> 0  (or DeadlineExceeded -> 1)
    else shutdown NOT requested (flag = false)
        Task->>Err: error! event=serve_loop_failed transport=.. error=..
        Task->>Read: flip_to_failed()  (sticky)
        Read->>Err: readiness_changed ready=false reason=serve_loop_failed
        Note over Read: /readyz -> 503 "failed"; /healthz stays 200
        Task-->>Orch: JoinHandle resolves ServeOutcome::Failed
        Orch->>Exit: ServeFailed -> 3
    end
```

## Constraints carried (C1-C10, from DISCUSS)

- **C1** aperture is LIVE: D2 process reaction is in-scope. Met (readiness + exit 3).
- **C2** integrate with the shutdown orchestrator, do NOT regress graceful drain: the graceful path is byte-for-byte unchanged; slice-08 suite stays green (D3 flag). Met.
- **C3** no public-API break: confirmed INTERNAL (Reuse Analysis verdict). Met.
- **C4** graceful-vs-fatal load-bearing: D3 `shutdown_requested` flag. Met.
- **C5** one additive event constant: `SERVE_LOOP_FAILED`. Met.
- **C6** `/healthz` stays 200, `/readyz` is the lever: `Failed -> 503`. Met.
- **C7** in-process simulable: hand-constructed bundle (`lib.rs:379-430`) + `stderr_capture` + injectable serve future. Met.
- **C8** mutation 100% on `transport.rs`/`shutdown.rs`/`readiness.rs` (+ `lib.rs`/`observability.rs`): enforced by the per-transport surfacing tests, the negative control, and the exit-3 unit test. Carried to DELIVER.
- **C9** Rust idiomatic: typed `JoinHandle<ServeOutcome>` + `AtomicBool`, no `dyn`, no channel. Met.
- **C10** trunk-based, no CI gates: CI is feedback. Acknowledged.

## Upstream changes

None. No new dependency, no new crate, no new infra, no schema change, no
public-API change. Pure intra-crate change to `aperture`. The
`opentelemetry-proto` / `tonic` / `axum` pins are untouched. Downstream
embedders (notably `gateway`) consume only the public re-exports, which
are unchanged.

## Notes for downstream waves

- **DISTILL** (`nw-acceptance-designer`): driving port = the running
  `aperture` binary's stderr + `/readyz` + `/healthz` + process exit
  code. Lock the D2 reaction (`/readyz` 503 `"failed"`, exit 3) and the
  D3 branch (SIGTERM -> NO `serve_loop_failed`, exit 0) as ACs. Use both
  injection seams; do NOT inherit a serve-failure test that passes on the
  swallow, nor a negative control that cannot tell a graceful shutdown
  from a fatal death.
- **DELIVER** (`nw-software-crafter`): only the crafter writes
  `crates/aperture/src/`. Implement the ripple map above; keep the
  graceful path byte-for-byte; 100% mutation kill on the modified files
  (Gate 5); keep the slice-08 suite green; add the `3` line to
  `main.rs`'s exit-code doc.
- **DEVOPS** (`nw-platform-architect`): no new infra. Inherits ADR-0005's
  five gates. Mutation scope = `transport.rs`, `shutdown.rs`,
  `readiness.rs`, `lib.rs`, `observability.rs`. No external integration,
  no contract test needed (the serve loop is an in-process boundary on
  the Tokio runtime / OS accept loop, probed by the injected-failure
  acceptance test, not a third-party API).
