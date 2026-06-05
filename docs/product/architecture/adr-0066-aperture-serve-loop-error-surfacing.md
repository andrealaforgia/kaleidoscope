# ADR-0066 — Aperture surfaces a post-bind serving-loop failure instead of swallowing it

- **Status**: Accepted
- **Date**: 2026-06-05
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture-serve-loop-error-surfacing-v0`
- **Extends (does NOT supersede)**: the slice-08 graceful-shutdown contract (`shutdown.rs:138-229`, `docs/feature/aperture/slices/slice-08-graceful-shutdown.md`, ADR-0010). Serve-loop failure is the **sibling** of drain-deadline-exceeded: both are non-clean process verdicts named on the way out, both ride the same `DrainOutcome` → exit-code seam. This ADR adds the serve-failure arm; it changes nothing about the graceful-drain arm.
- **Sibling precedent**: ADR-0061 (refuse-to-start fail-closed: a requested-but-unimplemented property produces a loud refusal, not a silent downgrade). This ADR is the same reflex one phase later in the lifecycle: a *post-bind* death produces a loud, honest, operator-actionable signal instead of a silent zombie.
- **Supersedes**: none.
- **Superseded by**: none.

## Context

Aperture v0 is the **live** OTLP ingest gateway (`tonic` gRPC on `:4317`, `axum` HTTP/protobuf on `:4318`, tagged `aperture/v0.1.0`). Each transport is started by a `pub(crate)` spawn helper that binds the socket synchronously (binding errors already surface as `event=listener_bind_failed`) and then spawns the serving loop as a Tokio task. The task discards the serve future's `Result`:

| Site | Location | Today |
|---|---|---|
| gRPC serve | `crates/aperture/src/transport.rs:89-94` | `tokio::spawn(async move { let _ = server.await; })` — task type `JoinHandle<()>`. The swallow is **disclosed** by a comment promising slice 08 would surface it. |
| HTTP serve | `crates/aperture/src/transport.rs:152-158` | `tokio::spawn(async move { let _ = axum::serve(...).with_graceful_shutdown(...).await; })` — task type `JoinHandle<()>`. The swallow is **silent**: no disclosing comment. |

Slice 08 surfaced **drain** outcomes (`in_flight_drained` / `drain_deadline_exceeded`) but never the serve **error**: `orchestrate_shutdown` is the sole awaiter of the joins and does `let _ = join_grpc.await; let _ = join_http.await;` (`shutdown.rs:185-190`). The serve future's `Result` is thrown away there too. The disclosing comment's promise is therefore unfulfilled for the error case, and the HTTP arm was never even disclosed — the four-quadrants Q3 report flags it as the higher-value (undisclosed) half.

The operator consequence is the **zombie listener**: when a serving loop dies after the socket is bound (the accept loop errors out post-bind), the process keeps running and keeps lying about its health.

- `/healthz` returns `200` unconditionally (`transport.rs:167-173`) — liveness, which stays true.
- `/readyz` reflects only `Starting`/`Ready`/`Draining` (`readiness.rs:37-41`, `transport.rs:179-194`). There is no `Failed` phase a dead serving loop can flip to, so `/readyz` stays `200 "ready"`.
- The exit code is owned by `DrainOutcome::exit_code()` = `0` clean / `1` deadline (`shutdown.rs:99-106`). A serve death has no path into it.

So a k8s orchestrator keeps a dead instance in rotation, routing telemetry to a listener that accepts nothing, and the operator's stderr scrape and `/readyz` runbook both stay green in front of a dead gateway. This is the *acked-but-actually-broken* lie the project's Earned-Trust posture (Principle 12) forbids, in the serving layer rather than the storage layer. It is the next item in the swallowed-errors family closed for storage in `cinder-wal-error-surfacing-v0` (ADR-0065).

This ADR resolves three DESIGN-flagged decisions (DISCUSS `wave-decisions.md` D1/D2/D3).

### Public-API confirmation (C3, D1)

`mod transport;` is declared **without** a `pub` qualifier at `lib.rs:47`, so the module is crate-private; `spawn_grpc`/`spawn_http` are `pub fn` only *within* the crate and are not re-exported (the only `pub mod` re-exports are `config`, `ports`, `testing`). `ShutdownBundle`, `ReadinessPhase`, `orchestrate_shutdown`, `DrainOutcome`, `ShutdownTrigger` are all `pub(crate)`. **Confirmed: the entire D1 ripple is INTERNAL.** No public type is introduced — the new `ServeError` (below) is `pub(crate)`, carried inside the crate-private bundle and join types, never nameable from outside. No semver break. (Were a public type ever to leak, it would be semver-MINOR at most, pre-1.0. **NEVER 1.0.0.**)

## Decision

Capture the serve future's `Result`, distinguish a graceful return from a fatal one **at the closest point to the failure**, and on a fatal return drive three operator-visible reactions: (1) emit one structured `event=serve_loop_failed` line naming the transport and error; (2) flip readiness to a new sticky `Failed` phase so `/readyz` returns `503`; (3) surface a distinct non-zero exit code `3` through the existing `DrainOutcome` → exit seam. `/healthz` stays `200`. A graceful-shutdown-driven return stays a byte-for-byte clean no-op.

### D1 — Mechanism: typed join result + serve-task self-reaction (chosen)

**The serve closure both (c) self-reacts at the failure site and (a) returns a typed result for the exit code.** Concretely:

- The spawn helpers change their return type from `JoinHandle<()>` to **`JoinHandle<ServeOutcome>`**, where
  ```text
  pub(crate) enum ServeOutcome { Graceful, Failed }   // Copy, Eq; crate-private
  pub(crate) struct ServeError(String);               // crate-private; the serve error rendered to a reason string at the failure site
  ```
  (The error reason is rendered to a `String` *inside* the task, at the failure site, because `tonic`/`axum` serve errors are not `Send + 'static`-uniform and there is no value in keeping the heterogeneous error type alive across the join. The task owns the only place that needs the rich error; the join carries only the verdict the orchestrator needs for the exit code. This keeps the bundle field type concrete and `dyn`-free, honouring C9.)
- Inside each task, the serve future is `.await`ed into a local `Result`. The task then consults a **shutdown-requested flag** (D3) and:
  - **shutdown was requested** → `ServeOutcome::Graceful` (no event, no readiness change; the orchestrator owns the drain narrative). This is the existing behaviour, preserved.
  - **shutdown was NOT requested** (the loop returned `Err`, OR returned `Ok` early — the listener stopped serving on its own) → the task, *at the failure site*:
    1. emits `tracing::error!(event = event::SERVE_LOOP_FAILED, transport = "grpc"|"http", error = %reason)`;
    2. calls `readiness.flip_to_failed()` (D2);
    3. resolves the task to `ServeOutcome::Failed`.

**Why self-react in the task rather than route the raw `Result` to the orchestrator (options a-pure and b):** the failure site is the only place that holds the transport identity, the live readiness handle (already cloned into the task for `mark_*_bound`), and the rich error. Emitting + flipping there is the *least plumbing* (option c) and the most local — the event names the transport without a lookup, and readiness flips the instant the loop dies, not at the next shutdown. The typed join result (option a) is retained **only** so the orchestrator can fold a serve death into the exit code without a second side-channel. Option b (a dedicated `mpsc` of serve outcomes to the composition root) was rejected: it adds a channel, a receiver task, and a join-vs-channel race for no benefit the typed `JoinHandle<ServeOutcome>` does not already give — the join *is* the channel.

**Internal ripple (the complete, bounded list — all `pub(crate)`):**

| # | Site | Location | Change |
|---|---|---|---|
| 1 | `spawn_grpc` | `transport.rs:50-97` | Return `JoinHandle<ServeOutcome>`. Task: await serve → consult shutdown flag → self-react (emit + `flip_to_failed`) or `Graceful`. Render the gRPC serve error to a reason string. |
| 2 | `spawn_http` | `transport.rs:117-161` | Same shape; `transport = "http"`. Render the axum serve error to a reason string. The previously-silent arm now surfaces identically. |
| 3 | shutdown-requested flag wiring | `transport.rs` (both helpers) | A shared flag (D3) set inside the graceful-shutdown closure when the oneshot resolves, read by the task after the serve future returns. |
| 4 | `compose::spawn` | `compose.rs:132,150,158,180-189` | Destructure the new `(addr, JoinHandle<ServeOutcome>)`; store into the bundle. Pass/clone the readiness handle into the helper (already passed today). No new field beyond the join type change. |
| 5 | `ShutdownBundle` | `shutdown.rs:125-134` | `grpc_join`/`http_join` change from `JoinHandle<()>` to `JoinHandle<ServeOutcome>`. No new field. |
| 6 | `orchestrate_shutdown` | `shutdown.rs:185-190` | Replace `let _ = join_grpc.await;` with `let grpc = join_grpc.await; let http = join_http.await;` and fold any `ServeOutcome::Failed` into the drain outcome (a serve death observed during the drain window maps the outcome to the serve-failure verdict). The graceful path is unchanged: under a normal drain both joins resolve `Graceful`. |
| 7 | `DrainOutcome` + `exit_code()` | `shutdown.rs:92-106` | Add a `ServeFailed` variant → `exit_code()` returns **3** (see D2). `Clean` → 0 and `DeadlineExceeded` → 1 unchanged. |
| 8 | `drain_to_exit_code` / `run` | `lib.rs:205-227` | Unchanged in shape; transparently returns 3 when the orchestrator yields `ServeFailed`. The serve-death-while-running (no shutdown ever requested) path reaches the exit code via the run loop observing readiness `Failed` — see "Run-loop reaction" below. |
| 9 | `Handle::drop_signal_listeners` | `lib.rs:161-171` | Touches only the senders; the abandoned-join lines at `:168-170` compile unchanged against the new join type. Low ripple. |
| 10 | Test: `drop_signal_listeners_returns_zero…` | `lib.rs:351-356` | Mechanical: `bundle.grpc_join.await` now yields `ServeOutcome`; the `let _ =` already discards it. |
| 11 | Test: `drain_to_exit_code_returns_one…` | `lib.rs:379-430` | Mechanical: the synthetic `tokio::spawn` tasks now resolve to `ServeOutcome` (return `ServeOutcome::Graceful` after the pending future, or — for the **new** serve-failure test — resolve to `ServeOutcome::Failed` with no shutdown sent). This is the injection seam (see Test seam). |

**Run-loop reaction (the serve-death-while-running path).** A serve loop can die with **no shutdown ever requested** — the operator never sent SIGTERM. In that case `orchestrate_shutdown` is never reached on its own. The composition root's run loop must therefore also observe the death. The chosen seam: `run` (`lib.rs:205-219`) already `select!`-style waits for a shutdown signal; it additionally waits on the two serving joins. Whichever resolves first wins:

- a shutdown signal arrives first → the existing drain path (unchanged);
- a serving join resolves `ServeOutcome::Failed` first (a true post-bind death, no SIGTERM) → `run` initiates the established wind-down for the *surviving* transport (send its shutdown sender so it stops cleanly) and returns exit code **3**. Readiness is already `Failed` (flipped by the dying task), so any `/readyz` probe in the interval already returns 503.

This keeps a single exit-code seam (`drain_to_exit_code`'s `DrainOutcome`) and a single readiness lever, with the dying task as the prompt and the run loop / orchestrator as the reconciler.

### D2 — Process reaction: `ReadinessPhase::Failed` (sticky) + `/readyz` 503 + exit code 3; `/healthz` stays 200

A zombie that serves nothing must not report ready, and a supervisor must be able to restart it. The honest reaction is **both** levers, reconciled with slice-08 so a normal SIGTERM still exits 0:

1. **Readiness** gains a fourth phase **`Failed`** (`readiness.rs`), sticky like `Draining` (a dead listener never recovers; the process exits). `flip_to_failed()` CAS-flips from `Ready` or `Starting` → `Failed` and emits `event=readiness_changed ready=false reason=serve_loop_failed`. `/readyz` maps `Failed → (503, "failed\n")` (`transport.rs:179-194`). The body string `"failed\n"` is the new arm beside `"starting\n"`/`"ready\n"`/`"draining\n"`.
   - **Precedence with `Draining`:** both are sticky 503 terminal states. If a serve loop dies *during* a graceful drain, the phase is already `Draining` (503) and `flip_to_failed`'s CAS finds neither `Ready` nor `Starting`, so it is a no-op — the operator already sees 503, and the drain narrative (not a false serve-failure narrative) owns that window. If `Failed` is reached first and a SIGTERM then arrives, `flip_to_draining`'s CAS likewise finds no `Ready`/`Starting` and is a no-op; `/readyz` stays 503 throughout. Either way `/readyz` is 503 and never flaps back to 200 — the invariant US-02 requires.
2. **Exit code** gains **`3`** — distinct from `0` clean drain, `1` deadline exceeded, `2` config error (ADR-0061). Chosen as the next free integer; it reserves the established `0/1/2` semantics verbatim and gives a supervisor (k8s `restartPolicy`, systemd) a code that unambiguously means "a serving loop died post-bind". The binary's exit-code doc comment (`main.rs:13-21`) gains the `3` line.
3. **`/healthz` stays 200** (C6): liveness ("the process is up") remains true; readiness is the correct lever for "this instance serves nothing".

**Mapping to US-02 AC:** healthy instance reports `/readyz` ready + `/healthz` ok (negative control, untouched); after a serving-loop death `/readyz` → 503 while `/healthz` → 200; the exit reaction (`3`) is observable and distinct from clean-drain `0`; a zombie is never presented as ready (sticky `Failed`).

### D3 — Graceful-vs-fatal distinction, incl. unexpected-early-Ok (the false-alarm guard)

The serve future returns `Ok` on a graceful drain (the oneshot fired) and `Err` on a serving-loop failure — but `Ok` alone is **not** sufficient evidence of "graceful", because a listener that stops serving on its own also returns `Ok` early. The orchestrator/composition root *knows* whether it signalled shutdown; the serve task must learn the same fact. The rule:

> **The discriminator is "was shutdown requested?", not the serve future's `Ok`/`Err`.**
> - **Shutdown WAS requested** (the graceful-shutdown oneshot resolved) → **any** serve return (Ok or Err) is **clean**: `ServeOutcome::Graceful`, no event, no readiness flip. (An `Err` *after* a requested shutdown is a drain-time teardown error, not a post-bind death; it is folded into the existing drain narrative, never `serve_loop_failed`.)
> - **Shutdown was NOT requested** → **any** serve return (Err, **or an unexpected early Ok**) is **fatal**: `serve_loop_failed` + `flip_to_failed` + exit 3. A listener that stops serving without anyone asking is a post-bind death regardless of the `Ok`/`Err` tag.

**Mechanism — how the task knows.** A shared `Arc<AtomicBool>` (`shutdown_requested`, initial `false`) is created per transport in the spawn helper. The existing graceful-shutdown closure — which already `await`s the oneshot (`transport.rs:86`, `:155`) — sets the flag to `true` immediately upon the oneshot resolving, *before* the serve future observes the shutdown. After the serve future returns, the task reads the flag: `true` → `Graceful`; `false` → fatal. This is race-free for the cases that matter: the orchestrator sends the oneshot, the closure flips the flag and then the serve future drains and returns; the task reads `true`. For a genuine post-bind death, the oneshot is never sent, the flag stays `false`, the task reads `false`. (No `dyn`, no channel: one `AtomicBool` per transport, C9-clean.)

**This guarantees:** a normal SIGTERM NEVER emits `serve_loop_failed` (the flag is `true` on the graceful path — US-03 negative control), and a true post-bind death ALWAYS does (the flag is `false` — US-01/US-03 both arms). The unexpected-early-`Ok` is treated as **fatal at v0** (surfaced, not tolerated): a serving loop that ends without a shutdown request is the listener silently stopping, exactly the dishonesty the feature exists to kill; tolerating it would re-ship a quieter zombie.

**Mapping to US-03 AC + the slice-08 negative control:** SIGTERM emits the existing drain sequence, NO `serve_loop_failed`, exit 0 (flag `true`); the HTTP arm surfaces by its own scenario (flag `false` on a non-graceful HTTP return); the graceful-vs-fatal distinction is the flag; the slice-08 acceptance suite stays green because the graceful path is byte-for-byte unchanged.

### One additive event constant (C5)

`observability.rs:30-51` gains `pub const SERVE_LOOP_FAILED: &str = "serve_loop_failed";` in the existing `listener_*`/`drain_*` closed-vocabulary family. Additions are non-breaking (file header + ADR-0009). Same JSON shape as every other event; fields `transport` (`"grpc"`|`"http"`) and `error` (the reason string), level `error`.

## Test seam (for DISTILL)

A real accept loop rarely dies on command, so the failure must be injected in-process and must make the **un-surfaced** path observably wrong (no `serve_loop_failed`, `/readyz` still 200, exit still 0), mirroring the cinder `FailingFsyncBackend` precedent. Two layered seams:

1. **Unit / exit-code seam — hand-constructed `ShutdownBundle` (`lib.rs:379-430`).** The existing synthetic-join test already builds a bundle with `tokio::spawn` tasks. Under D1 those tasks return `ServeOutcome`. The new serve-failure test constructs a `grpc_join` that resolves to `ServeOutcome::Failed` **without any shutdown being sent**, drives it through `drain_to_exit_code` / the run loop, and asserts exit code `3`. Against today's `let _ = ...await` + `JoinHandle<()>` this test cannot even be written (the type can't carry `Failed`); once surfaced it passes only when the verdict propagates. This kills the swallow mutation: a mutant restoring `let _ = join.await` yields exit 0 and the test fails.

2. **Acceptance seam — injected serve future + `testing::stderr_capture`.** DISTILL drives a real spawned transport whose serve future is made to resolve to `Err` (or early `Ok`) post-bind behind the spawn helper (an injectable serve future, the aperture analogue of `FailingFsyncBackend`). Assert: exactly one captured `event=serve_loop_failed transport=grpc|http error=…` at `error` level; a subsequent `/readyz` probe returns 503 `"failed"`; `/healthz` returns 200. The negative control drives a real SIGTERM/`Handle::shutdown` and asserts NO `serve_loop_failed` line and exit 0 (the slice-08 suite, kept green).

Both seams must FAIL against today's swallow and PASS only when the outcome is surfaced — the false-confidence trap (a test that passes on the bug) is the explicit thing to avoid (DISCUSS risk table).

## Alternatives Considered

### Option A — Typed join result + serve-task self-reaction (RECOMMENDED, accepted)

`JoinHandle<ServeOutcome>`; the task emits + flips readiness at the failure site; the join folds into the exit code. Covered above.

**Pros:** least plumbing; the event names the transport without a lookup; readiness flips the instant the loop dies; no new channel, no `dyn`, no public-API leak; the typed join is the single exit-code seam; reuses `ShutdownBundle`, the readiness machine, the `DrainOutcome` exit map, and the closed vocabulary.
**Cons:** the serve error is rendered to a `String` inside the task (the rich error type is not carried across the join). Acceptable: nothing downstream needs the typed error; the reason string is what the operator reads.

### Option B — Log-only (emit `serve_loop_failed`, no readiness flip, no exit change) — REJECTED

Surface the event but leave `/readyz` and the exit code untouched.
**Pros:** smallest diff; no readiness/exit ripple; no new exit code or phase.
**Cons:** **does not close the defect.** The zombie still answers `/readyz 200` and the supervisor still keeps it in rotation; the operator gets a log line they may sample away and no orchestrator lever. This is the cinder-`append_wal`-style "honest about being dishonest" half-fix ADR-0061 Option C rejected for the same reason. US-02 (a zombie must not report ready) is unmet. **Rejected: leaves the zombie in rotation.**

### Option C — Abort-on-any-return (treat every serve return, including graceful, as fatal) — REJECTED

Skip the graceful-vs-fatal discriminator; any serve return emits + exits non-zero.
**Pros:** trivial; no shutdown-requested flag.
**Cons:** **false-alarms on every normal SIGTERM** — the graceful drain resolves the serve future `Ok`, which this option would treat as a serve death, emitting `serve_loop_failed` and exiting non-zero on a routine restart. That is itself an Earned-Trust lie (the false alarm DISCUSS risk-tables as High) and breaks the slice-08 contract + its acceptance suite (C2). **Rejected: re-introduces dishonesty in the other direction and regresses graceful drain.**

### Option D — Dedicated supervisor task / `mpsc` of serve outcomes to the composition root — REJECTED

A separate supervisor task receives serve outcomes over an `mpsc` and centralises the reaction.
**Pros:** centralised reaction site; the spawn helpers stay `JoinHandle<()>`.
**Cons:** adds a channel, a receiver task, and a join-vs-channel race, plus a new always-running task on the live gateway — operational surface for no benefit the typed `JoinHandle<ServeOutcome>` does not already provide (the join *is* the outcome channel). It also moves the reaction *away* from the failure site, losing the transport identity and the live readiness handle the task already holds. Heavier than the problem (the simplest-solution-first rule). **Rejected: more moving parts than the typed join, no added capability.**

## Consequences

### Positive
- The serving-layer *acked-but-actually-broken* lie is closed on **both** transports; the previously-silent HTTP arm surfaces identically to gRPC. Swallowed serve sites move from 2 → 0.
- A dead serving loop now (a) names itself on stderr, (b) flips `/readyz` to 503 so the orchestrator pulls it from rotation, and (c) exits `3` so a supervisor restarts it — the zombie window moves from indefinite to the next probe/exit.
- Reuses every existing seam: spawn helpers, `ShutdownBundle`, the readiness machine, the `DrainOutcome` exit map, the closed vocabulary, the hand-constructed-bundle test seam. No new public type, no new crate, no new always-running task.
- The graceful-vs-fatal flag guarantees zero false alarms on a normal SIGTERM — the slice-08 contract and its suite stay green.

### Negative
- A new exit code (`3`), a new readiness phase (`Failed`), and a new `DrainOutcome` variant (`ServeFailed`) widen three small enums. All are additive and internal; the negative controls (graceful drain, healthy ready) guard non-regression. The binary exit-code doc gains one line.
- The serve error is type-erased to a reason `String` at the failure site (Option A con). No downstream consumer needs the rich type; acceptable.
- The two hand-constructed-bundle tests need a mechanical update to the new join type. Bounded and already enumerated (ripple rows 10-11).

### Trade-off ATAM
- **Sensitivity point — Reliability (fault tolerance / recoverability):** the change converts a silent post-bind death into a fail-loud, supervisor-restartable verdict. The sticky `Failed` phase matches a dead listener that never recovers.
- **Trade-off point — Reliability vs Availability:** exiting `3` on a serve death (rather than limping as a zombie) trades a degraded-but-up instance for a clean restart. Deliberate and Earned-Trust-consistent: a gateway that refuses to pretend it is serving is preferable to one that silently serves nothing. The negative controls bound the availability cost to genuine post-bind deaths.
- **Sensitivity point — Maintainability (testability):** the in-process injection seam (hand-constructed bundle + injectable serve future) makes the failure deterministically reproducible without a flaky real accept-loop kill, the prerequisite for the 100% mutation-kill gate.
- **Security — no new surface:** no new port binds, no new network input, no new credential or filesystem path. The `error` field on `serve_loop_failed` carries a serve-error reason string (a transport/runtime error), not operator-supplied or request-derived data, so there is no injection vector on the stderr stream. The reaction *reduces* exposure: a dead listener is pulled from rotation rather than left accepting connections it cannot serve.
- **Performance Efficiency — negligible:** one `AtomicBool` load per transport per process lifetime (after the serve future returns), one extra `JoinHandle` await already present in the drain path. No steady-state cost; nothing on the request hot path changes.

## Enforcement

- Covered by integration/acceptance tests across both transports (gRPC + HTTP serve-failure surfacing), the SIGTERM negative control (no `serve_loop_failed`, exit 0), and the unit exit-code test (serve death → exit 3), supplying the per-feature 100% mutation-kill coverage required by CLAUDE.md / ADR-0005 Gate 5 on the two former swallow sites, the graceful-vs-fatal branch, the `flip_to_failed` CAS, and the `ServeFailed → 3` exit map.
- No new architectural-style rule is introduced. The existing closed-vocabulary discipline (ADR-0009) governs the one additive event constant; the existing `pub(crate)` module boundary keeps the `ServeOutcome`/`ServeError` types internal.
- **Earned Trust (Principle 12):** the serve task is a driven boundary on the Tokio runtime / OS accept loop. Its honesty probe is the injected-serve-failure acceptance test (the aperture analogue of cinder's `FailingFsyncBackend`): it exercises the specific lie ("the listener stopped serving"), and the test must FAIL against the `let _ = ...await` swallow and PASS only when the death is surfaced. The negative control probes the opposite lie (a graceful shutdown wrongly reported as a death). Asking "what happens if the serving loop lies about still serving?" is answered by both probes.
