<!-- markdownlint-disable MD024 -->

# User Stories: aperture-serve-loop-error-surfacing-v0

## Origin and Job Grounding

No DIVERGE artifacts exist for this feature
(`docs/feature/aperture-serve-loop-error-surfacing-v0/diverge/` is
absent). Origin is the **four-quadrants implementer assessment** for
aperture (`kaleidoscope-4-quadrants-theory/reports/aperture.md`, Q3
"gRPC serve-future errors are swallowed (silent-ish)"). This is the next
item in the swallowed-errors family; the cinder/sluice `append_wal`
swallow was just closed in `cinder-wal-error-surfacing-v0`. The job below
is grounded in that Q3 finding and the project's Earned-Trust posture
(`docs/product/architecture/brief.md` Principle 12). Absence of DIVERGE
is recorded as a risk in `wave-decisions.md`; it does not block, because
the defect and the fix direction are verified directly in code.

## The Operator Job (JTBD, Earned-Trust framing)

> **When** aperture's gRPC or HTTP serving loop dies after the socket is
> bound, **I want** to learn immediately through a structured stderr
> event naming the transport and the error, and the process to stop
> pretending to be healthy (it becomes unready and/or exits non-zero),
> **so that** I can restart or page instead of staring at a green
> `/healthz` in front of a dead listener.

The current behaviour discards the serve-loop error and leaves the
process looking healthy while the listener is dead: the
acked-but-actually-broken lie, the exact shape the Earned-Trust posture
forbids, in the serving layer rather than the storage layer.

## Verified Code Findings (confirming the four-quadrants read)

All confirmed by reading the source on this branch.

| Claim | Verified location | Finding |
|---|---|---|
| gRPC serve `Result` discarded | `crates/aperture/src/transport.rs:89-94` | `let _ = server.await;` inside `tokio::spawn`; task returns `JoinHandle<()>`. |
| gRPC swallow DISCLOSED by a comment | `crates/aperture/src/transport.rs:90-92` | The comment promises slice 08 will surface it; slice 08 surfaced DRAIN outcomes, not serve ERRORS. The promise is unfulfilled for the error case. |
| HTTP serve `Result` discarded, SILENT | `crates/aperture/src/transport.rs:152-158` | `let _ = axum::serve(...).with_graceful_shutdown(...).await;`, no disclosing comment. The higher-value (undisclosed) half. |
| Binding errors surface synchronously | `transport.rs:57,124`; `compose.rs:140-173` | `bind(...)?` maps to `event=listener_bind_failed`. This feature is ONLY the POST-BIND serving loop. |
| `/healthz` is unconditionally 200 | `transport.rs:167-173` | The zombie's green health check. |
| `/readyz` has no failed phase | `transport.rs:179-194`; `readiness.rs:35-75` | `Starting`->503, `Ready`->200, `Draining`->503; no `Failed`. A dead serving loop cannot flip readiness today. |
| Exit-code path owned by the drain orchestrator | `lib.rs:205-227`; `shutdown.rs:99-106,185-190` | `DrainOutcome::exit_code()` is 0 clean / 1 deadline-exceeded; the joins are awaited as `let _ = join.await;` so the serve error has no path into the exit code. |
| Graceful serve-return is `Ok` and NORMAL | `transport.rs:85-87,153-157`; `shutdown.rs:175-218` | Both transports resolve `Ok` on graceful drain; the failure is the `Err` arm. A graceful return must stay a clean no-op. |
| No `serve_loop_failed` constant yet | `observability.rs:30-51` | The closed vocabulary has the `listener_*` / `drain_*` family; one additive constant is needed. |

## Verified Consumer List (who produces / consumes the spawn JoinHandles)

Full table with locations and per-site ripple is in `wave-decisions.md`
(Verified Consumer List). Summary:

- **Producers**: `transport::spawn_grpc` (`transport.rs:50-97`) and
  `spawn_http` (`transport.rs:117-161`) return `(SocketAddr,
  JoinHandle<()>)`; both `pub(crate)`, NOT public API.
- **Composition root**: `compose::spawn` (`compose.rs:132,158-189`)
  stores the joins into the `ShutdownBundle`.
- **Owner**: `ShutdownBundle` (`shutdown.rs:125-134`) holds `grpc_join`
  / `http_join`.
- **Sole awaiter today**: `orchestrate_shutdown` drain future
  (`shutdown.rs:185-190`), `let _ = join.await;` raced against the
  deadline.
- **Drop path**: `Handle::drop_signal_listeners` (`lib.rs:161-171`)
  abandons the joins (Drop is sync). Low ripple.
- **Exit-code seam**: `drain_to_exit_code` / `run` (`lib.rs:205-227`).
- **Tests touching the joins directly**: `lib.rs:351-356` and the
  hand-constructed-bundle test `lib.rs:379-430` (the natural injection
  seam).
- **NOT consumers**: the per-request gRPC service impls and HTTP
  handlers (different layer); `wire_sink` / `probe_or_refuse`; the public
  re-exports.

## System Constraints

(Full text in `wave-decisions.md` > Constraints established. Pinned here
for the crafter and the reviewer.)

- **C1 — aperture is LIVE.** Live OTLP gateway on `:4317`/`:4318`. The
  process-reaction decision (D2) is operator-visible and belongs in this
  feature.
- **C2 — Integrate with the existing shutdown orchestrator; do NOT
  regress graceful drain.** The slice-08 contract and its acceptance
  suite must stay green. A graceful serve-return stays a clean no-op.
- **C3 — Likely NO public-API break (CONFIRM in DESIGN).** The spawn
  helpers, `ShutdownBundle`, and `orchestrate_shutdown` are `pub(crate)`.
  Internal ripple expected. Any leak is semver-MINOR, pre-1.0. **NEVER
  1.0.0.**
- **C4 — Graceful-vs-fatal is load-bearing.** A false alarm on a normal
  shutdown is its own dishonesty (D3).
- **C5 — One additive event constant** (`serve_loop_failed`-style) per
  the closed-vocabulary rules.
- **C6 — `/healthz` stays 200; `/readyz` is the lever** for "this
  instance serves nothing".
- **C7 — In-process simulable.** The hand-constructed-`ShutdownBundle`
  seam (`lib.rs:379-430`) and the stderr-capture seam
  (`testing::stderr_capture`) make the AC falsifiable in-suite.
- **C8 — Mutation testing 100%** on `transport.rs`, `shutdown.rs`,
  `readiness.rs` (Gate 5).
- **C9 — Rust idiomatic** per CLAUDE.md.
- **C10 — Trunk-based, no CI gates** (MEMORY).

---

## US-01: A serving loop that dies after bind emits a structured stderr event naming the transport and error

### Problem

Sam runs a live Kaleidoscope ingest fleet. When aperture's gRPC serving
loop returns an error AFTER the socket is bound (the accept loop dies,
the runtime errors out post-bind), the error is discarded at
`crates/aperture/src/transport.rs:93` (`let _ = server.await;`) with no
event on stderr. The HTTP arm is worse: it swallows silently at
`transport.rs:153` with no disclosing comment at all. Sam scrapes
aperture's structured stderr into his log pipeline and alerts on it, but
there is nothing to alert on: the dead listener emits zero telemetry
about its own death. Sam finds it impossible to know a serving loop has
died, because the process that is supposed to tell him the truth about
its own health says nothing when the most important thing about it
changes.

### Elevator Pitch

- **Before**: when a serving loop dies post-bind, aperture discards the
  error with no event. The dead gRPC arm is disclosed-but-still-silent;
  the dead HTTP arm is silent with no acknowledgement. Sam's log pipeline
  sees nothing.
- **After**: the operator-invocable surface is the running `aperture`
  binary; when a serving loop dies post-bind, aperture emits one
  structured stderr line, `event=serve_loop_failed transport=grpc`
  (or `transport=http`) `error=<reason>`, in the same JSON shape as
  every other aperture event, so Sam's existing log scrape catches it and
  alerts.
- **Decision enabled**: Sam sees exactly which transport died and why,
  the moment it dies, and decides to restart or page, instead of
  discovering a dead listener hours later by noticing telemetry stopped
  arriving.

### Who

- Sam the platform operator | runs a live aperture ingest fleet and
  scrapes its structured stderr into a log/alert pipeline | motivated to
  be told, in machine-parseable form, the moment a serving loop dies,
  naming which transport and why.

### Solution

Replace the two `let _ = <serve>.await` swallows
(`transport.rs:93,153`) with code that, on a NON-graceful serve error,
emits a structured event against a new closed-vocabulary constant
(`event=serve_loop_failed transport=grpc|http error=<...>`). The serve
task must capture and route its `Result` (D1) so the failure reaches a
site that can emit. A graceful-shutdown-driven serve return stays a clean
no-op (no event). DESIGN owns the exact routing shape and the emit site.

### Domain Examples

#### 1: Happy Path / negative control — a normal graceful shutdown emits NO serve_loop_failed

Sam sends SIGTERM to a healthy aperture. Both serving loops resolve `Ok`
on the graceful drain. The existing shutdown event sequence fires
(`shutdown_initiated`, `readiness_changed ready=false
reason=shutdown_drain`, `in_flight_drained`, `shutdown_complete
exit_code=0`) and NO `serve_loop_failed` line appears anywhere in the
captured stderr. The graceful path is untouched.

#### 2: Error/Boundary — the gRPC serving loop dies post-bind

aperture has bound `:4317` and marked ready. The gRPC serving loop then
returns an error (simulated in-suite by an injected `Err`-resolving serve
future). aperture emits exactly one
`event=serve_loop_failed transport=grpc error=<reason>` line on stderr,
at `error` level (loud, never silent), carrying the transport name and
the serve error's reason string. The line is valid JSON in the same
shape every aperture event uses.

#### 3: Edge Case — the HTTP serving loop dies post-bind (the previously SILENT arm)

aperture has bound `:4318` and marked ready. The HTTP (axum) serving loop
returns an error post-bind. aperture emits exactly one
`event=serve_loop_failed transport=http error=<reason>` line. This is the
arm that was previously silent with no disclosing comment; it now surfaces
identically to the gRPC arm. The `transport` field is the only difference
between the two lines.

### UAT Scenarios (BDD)

#### Scenario: A gRPC serving loop that dies after bind is named on stderr

```gherkin
Given aperture has bound its gRPC listener and marked itself ready
When the gRPC serving loop returns an error after the socket is bound
Then aperture emits a structured stderr event naming transport "grpc" and the error
And the event is emitted at error level
And no graceful-shutdown event sequence is emitted
```

#### Scenario: An HTTP serving loop that dies after bind is named on stderr

```gherkin
Given aperture has bound its HTTP listener and marked itself ready
When the HTTP serving loop returns an error after the socket is bound
Then aperture emits a structured stderr event naming transport "http" and the error
And the event is emitted at error level
```

#### Scenario: A normal graceful shutdown emits no serve-loop-failed event (negative control)

```gherkin
Given aperture is healthy with both serving loops running
When aperture receives a graceful shutdown signal and both loops drain cleanly
Then the existing graceful-drain event sequence is emitted
And no serve-loop-failed event appears anywhere on stderr
```

### Acceptance Criteria

- [ ] A gRPC serving-loop error after bind emits one structured event naming transport "grpc" and the error reason, at error level (from scenario 1).
- [ ] An HTTP serving-loop error after bind emits one structured event naming transport "http" and the error reason (from scenario 2) — the previously silent arm now surfaces.
- [ ] A normal graceful shutdown emits the existing drain sequence and NO serve-loop-failed event (negative control, from scenario 3).
- [ ] The new event uses a single closed-vocabulary constant added to `observability.rs`, in the same JSON shape as existing events.

### Outcome KPIs

- **Who**: aperture's gRPC and HTTP serving loops, in the live ingest gateway.
- **Does what**: emit a structured, named stderr event when a serving loop dies post-bind, instead of swallowing the error.
- **By how much**: swallowed serve-loop errors move from 2 sites (today) to 0; the silent HTTP arm moves from undisclosed to surfaced; the failing-serve-loop AC is falsifiable in-suite (passes only when the event is emitted).
- **Measured by**: the injected-serve-failure acceptance test asserting the captured `serve_loop_failed` event per transport; `cargo mutants` kill rate on the two former swallow sites.
- **Baseline**: today both serve `Result`s are discarded (`let _ = ...await`), 2 swallow sites, 0 surfacing events, the HTTP arm undisclosed.

### Technical Notes

- Depends on D1 (the serve `Result` must be captured and routed before it can be emitted).
- One additive event constant (C5); same JSON shape as the rest of the vocabulary.
- Graceful-vs-fatal distinction (D3) gates the emit: only the `Err` arm (and possibly an unexpected early `Ok`) emits.
- Test seam: the hand-constructed-`ShutdownBundle` seam (`lib.rs:379-430`) plus `testing::stderr_capture` (C7).

---

## US-02: A dead serving loop stops the process from reporting healthy/ready

### Problem

When aperture's serving loop dies post-bind, the process keeps running
and keeps lying about its health. `/healthz` returns 200 (the process is
alive). `/readyz` keeps returning whatever the readiness phase last was,
`ready` (200), because a dead serving loop has no path to flip readiness
(`readiness.rs` has `Starting`/`Ready`/`Draining` and no failed phase).
The exit code is unaffected. So Sam's orchestrator (k8s) keeps the pod in
rotation and keeps routing traffic to a listener that accepts nothing,
and his runbook's "is it ready?" check stays green in front of a dead
gateway. Sam finds it impossible to get his orchestrator to act, because
the one signal the orchestrator trusts (`/readyz`) reports ready for an
instance that serves nothing: a zombie.

### Elevator Pitch

- **Before**: a dead serving loop leaves `/readyz` returning `200
  "ready"` and the exit code unchanged; the orchestrator keeps routing
  traffic to a zombie that serves nothing.
- **After**: the operator-invocable surface is the running `aperture`
  binary and its `/readyz` probe; when a serving loop dies post-bind,
  aperture stops reporting healthy at the process level per the D2
  decision — `/readyz` returns 503 (the instance is no longer ready)
  and/or the process exits non-zero — so a `curl /readyz` shows `503` and
  the orchestrator pulls the instance from rotation or restarts it.
- **Decision enabled**: Sam's orchestrator acts on the honest probe (it
  stops routing to, or restarts, the dead instance) and Sam decides
  whether to investigate, instead of a green probe wedging a dead
  gateway in rotation indefinitely.

### Who

- Sam the platform operator | relies on `/readyz` (and the process exit
  code) as the lever his orchestrator acts on | motivated to have a dead
  serving loop pull itself out of rotation honestly rather than report
  ready while serving nothing.

### Solution

On a non-graceful serve death, make the failure observable at the process
level. DESIGN decides D2: flip readiness to unready/failed (so `/readyz`
returns 503) and/or drive a non-zero process exit through the existing
shutdown/exit-code path. `/healthz` deliberately stays 200 (liveness is
still true). Whichever combination DESIGN picks, a zombie that serves
nothing MUST NOT report ready. The decision is recorded in
`wave-decisions.md` D2 and crystallised by DESIGN.

### Domain Examples

#### 1: Happy Path / negative control — a healthy ready instance reports ready

A healthy aperture with both serving loops running answers `GET /readyz`
with `200 "ready"` and `GET /healthz` with `200 "ok"`. No serving loop
has died; the probes correctly report a ready, serving instance. This is
the state the feature must NOT disturb.

#### 2: Error/Boundary — gRPC serving loop dies; readiness goes unready

aperture is ready, then its gRPC serving loop dies post-bind. Per the D2
decision, aperture flips readiness so a subsequent `GET /readyz` returns
`503` (no longer `200 "ready"`). `/healthz` still returns `200` (the
process is up). Sam's orchestrator's readiness probe sees the 503 and
pulls the instance from rotation. The zombie no longer claims to be ready.

#### 3: Edge Case — serving loop death drives a non-zero process exit

aperture is ready, then a serving loop dies post-bind. Per the D2
decision, the process exits with a non-zero code (distinct from the clean
graceful-drain `0`), so a supervisor (k8s restartPolicy, systemd) sees
the failure and restarts the instance rather than leaving it wedged. The
exit code is NOT the clean-drain `0` that a normal SIGTERM produces.

### UAT Scenarios (BDD)

#### Scenario: A healthy instance reports ready (negative control)

```gherkin
Given aperture is healthy with both serving loops running
When an orchestrator probes /readyz and /healthz
Then /readyz returns ready
And /healthz returns ok
```

#### Scenario: A dead serving loop stops reporting ready

```gherkin
Given aperture has marked itself ready
When one of its serving loops dies after the socket is bound
Then a subsequent /readyz probe no longer reports ready
And /healthz still reports the process is alive
```

#### Scenario: A dead serving loop makes the process exit reaction observable

```gherkin
Given aperture has marked itself ready
When a serving loop dies after the socket is bound
Then the process makes the failure observable per the D2 decision
And the exit reaction is distinct from a clean graceful shutdown
And no un-serving instance is presented to an orchestrator as ready
```

### Acceptance Criteria

- [ ] A healthy instance reports `/readyz` ready and `/healthz` ok (negative control, from scenario 1).
- [ ] After a serving-loop death, `/readyz` no longer reports ready while `/healthz` still reports alive (from scenario 2).
- [ ] The process reaction to a serving-loop death is observable and distinct from a clean graceful shutdown, per the D2 decision recorded in `wave-decisions.md` (from scenario 3).
- [ ] A zombie that serves nothing is never presented to an orchestrator as ready.

### Outcome KPIs

- **Who**: Sam operating the live aperture fleet through `/readyz` and the process exit code.
- **Does what**: gets a dead serving loop to stop reporting ready (and/or exit non-zero) so the orchestrator acts, instead of a green probe wedging a zombie in rotation.
- **By how much**: the dead-listener-still-reports-ready window moves from "indefinite" (today: never flips) to "the next probe after the death" (0 stays-ready); a serving-loop death gains a process reaction where today it has none.
- **Measured by**: the serving-loop-death acceptance test asserting `/readyz` flips and the exit reaction fires per D2; absence of any path that reports ready after a serve death.
- **Baseline**: today a dead serving loop leaves `/readyz` at `200 "ready"` and the exit code unchanged; the orchestrator never learns.

### Technical Notes

- Depends on US-01 (the serve `Result` must be captured before it can drive a reaction).
- D2 (readiness lever and/or exit code) is the operator-visible decision DESIGN owns; this story encodes the requirement that a zombie not report ready, not which exact combination is chosen.
- Consider a `ReadinessPhase::Failed` (or `Draining` with a distinct reason), given `Draining` is sticky and never recovers (`readiness.rs:15-21`), which matches a dead listener that never recovers.
- The serve-death exit code must NOT collide with the clean-drain `0`; DESIGN places it relative to the existing `DrainOutcome` exit map (`shutdown.rs:99-106`).
- `/healthz` stays 200 (C6).

---

## US-03: Both transports are covered, with the previously silent HTTP arm explicitly proven, and a graceful shutdown never false-alarms

### Problem

The defect lives on BOTH transports, but they are not symmetric today:
the gRPC swallow (`transport.rs:93`) is disclosed by a comment, while the
HTTP swallow (`transport.rs:153`) is silent with no acknowledgement at
all. A fix that surfaced only the gRPC arm, or that left the HTTP arm's
coverage implicit, would re-ship the exact undisclosed dishonesty the
four-quadrants report flags as the higher-value half. Equally dangerous
in the other direction: a fix that surfaces too eagerly and fires
`serve_loop_failed` (or kills the process) on a NORMAL graceful shutdown
would page Sam for nothing and would itself be an Earned-Trust lie (a
false alarm). Sam needs both arms proven AND a hard guarantee that a
normal SIGTERM drain stays a clean no-op.

### Elevator Pitch

- **Before**: the gRPC arm is disclosed-but-silent, the HTTP arm is
  undisclosed-and-silent, and there is no guard distinguishing a fatal
  serve death from a normal graceful shutdown.
- **After**: the running `aperture` binary surfaces a serving-loop death
  on BOTH transports identically (the HTTP arm explicitly proven, not
  implied), AND a normal SIGTERM shutdown produces the EXISTING clean
  drain with NO `serve_loop_failed` event and the existing exit code — so
  Sam sees `serve_loop_failed transport=http` when the HTTP loop dies,
  and sees the ordinary `shutdown_complete exit_code=0` (no false alarm)
  when he restarts the instance normally.
- **Decision enabled**: Sam trusts that a `serve_loop_failed` alert means
  a real serving-loop death on a named transport, not noise from a normal
  restart, so he can page on it without alert fatigue.

### Who

- Sam the platform operator | alerts on `serve_loop_failed` and restarts
  instances routinely with SIGTERM | motivated to have BOTH transports
  covered and to NEVER be paged by a normal graceful shutdown.

### Solution

Ensure the mechanism (US-01 event + US-02 process reaction) covers both
the gRPC and the HTTP arms, with the HTTP arm proven by its own
acceptance scenario (not merely shared-mechanism-implied). Implement the
graceful-vs-fatal distinction (D3): the serve future's `Ok` on a graceful
drain stays a clean no-op; only the `Err` arm (and, per DESIGN's D3 call,
an unexpected early `Ok` before shutdown was requested) surfaces. DESIGN
owns how the serve task knows whether a shutdown was requested.

### Domain Examples

#### 1: Happy Path / negative control — SIGTERM drains cleanly with no false alarm

Sam restarts a healthy aperture with SIGTERM. Both serving loops resolve
`Ok` after the graceful-shutdown oneshot fires. The captured stderr shows
the ordinary slice-08 sequence ending in `shutdown_complete exit_code=0`
and contains NO `serve_loop_failed` line and NO readiness-failed reaction.
The existing slice-08 acceptance suite stays green.

#### 2: Error/Boundary — the HTTP arm (previously silent) is proven to surface

The HTTP serving loop dies post-bind. The same mechanism that covers gRPC
fires for HTTP: `event=serve_loop_failed transport=http error=<reason>`
plus the D2 process reaction. This scenario exists specifically to prove
the previously undisclosed HTTP arm is no longer silent; it is not left
to "the gRPC test covers it by symmetry".

#### 3: Edge Case — an unexpected early Ok before shutdown was requested

A serving loop returns `Ok` but NO shutdown was ever requested (the loop
ended on its own). Per DESIGN's D3 call, aperture treats this as a fatal
"the listener stopped serving" condition and surfaces it (or, if DESIGN
tolerates it at v0, documents the choice). Either way the behaviour is
deliberate, not an accident of a discarded `Result`.

### UAT Scenarios (BDD)

#### Scenario: A graceful shutdown never raises a serve-loop-failed alarm (negative control)

```gherkin
Given aperture is healthy with both serving loops running
When aperture receives SIGTERM and both loops drain cleanly
Then the existing graceful-drain sequence is emitted ending in a clean exit
And no serve-loop-failed event appears on stderr
And the process exit code is the existing clean-drain code
```

#### Scenario: The previously silent HTTP arm surfaces a serving-loop death

```gherkin
Given aperture has bound its HTTP listener and marked itself ready
When the HTTP serving loop dies after the socket is bound
Then aperture surfaces the death on the HTTP transport identically to the gRPC arm
And this is proven by its own acceptance scenario, not implied by the gRPC scenario
```

#### Scenario: A serving loop ending without a shutdown request follows the documented decision

```gherkin
Given no shutdown has been requested
When a serving loop returns from its serve future on its own
Then aperture behaves exactly as the D3 decision specifies (surface as fatal, or tolerate-and-document)
And the behaviour is deliberate, not a discarded result
```

### Acceptance Criteria

- [ ] A normal SIGTERM graceful shutdown emits the existing drain sequence, NO `serve_loop_failed` event, and the existing clean exit code (negative control, from scenario 1).
- [ ] The HTTP arm's serving-loop death is proven to surface by its own acceptance scenario, not merely implied by the gRPC scenario (from scenario 2).
- [ ] The graceful-vs-fatal distinction is implemented so only a non-graceful serve return surfaces; the unexpected-early-`Ok` case follows the D3 decision recorded in `wave-decisions.md` (from scenario 3).
- [ ] The existing slice-08 graceful-shutdown acceptance suite stays green (regression guard).

### Outcome KPIs

- **Who**: Sam alerting on `serve_loop_failed` and restarting instances with SIGTERM.
- **Does what**: gets both transports covered (HTTP arm proven) AND zero false alarms on normal shutdowns, so a `serve_loop_failed` alert is trustworthy.
- **By how much**: transport coverage moves from 1-of-2-disclosed (gRPC only, silent) to 2-of-2-surfaced (both, HTTP explicitly proven); false alarms on graceful shutdown stay at 0 (asserted by the negative control).
- **Measured by**: the per-transport surfacing tests (gRPC AND HTTP) plus the SIGTERM negative-control test asserting no `serve_loop_failed`; the slice-08 suite staying green.
- **Baseline**: today gRPC is disclosed-but-silent, HTTP is undisclosed-and-silent, and there is no graceful-vs-fatal guard.

### Technical Notes

- Depends on US-01 + US-02 (the mechanism they introduce is what this story proves on both arms).
- D3 (graceful-vs-fatal, and the unexpected-early-`Ok` treatment) is the load-bearing distinction DESIGN owns; this story encodes the requirement that a graceful shutdown never false-alarms and that both arms are covered.
- C2 regression guard: the slice-08 graceful-shutdown suite (`tests/slice_08_graceful_shutdown.rs`) must stay green.
- The HTTP-arm scenario is mandatory and explicit precisely because the HTTP swallow was the undisclosed half (the four-quadrants higher-value finding).
