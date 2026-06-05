# Wave Decisions: aperture-serve-loop-error-surfacing-v0 (DISCUSS)

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-05.
British English. No em dashes in body.

## Origin

The four-quadrants implementer assessment for aperture
(`kaleidoscope-4-quadrants-theory/reports/aperture.md`, Q3 finding
"gRPC serve-future errors are swallowed (silent-ish)"). This is the next
item in the swallowed-errors family; the cinder/sluice `append_wal`
swallow was just closed in `cinder-wal-error-surfacing-v0`. Same
Earned-Trust dishonesty (acked-but-actually-broken), in the serving
layer rather than the storage layer.

No DIVERGE artifacts exist for this feature
(`docs/feature/aperture-serve-loop-error-surfacing-v0/diverge/` is
absent). The job below is grounded in the four-quadrants Q3 finding and
the project's Earned-Trust posture (`brief.md` Principle 12). Absence of
DIVERGE is recorded as a risk; it does not block, because the defect and
the fix direction are verified directly in code on this branch.

## The Operator Job (JTBD, Earned-Trust framing)

> **When** aperture's gRPC or HTTP serving loop dies after the socket is
> bound, **I want** to learn immediately through a structured stderr
> event naming the transport and the error, and the process to stop
> pretending to be healthy (it becomes unready and/or exits non-zero),
> **so that** I can restart or page instead of staring at a green
> `/healthz` in front of a dead listener.

The current behaviour discards the serve-loop error with no event and no
effect on the process: `/healthz` keeps returning 200, `/readyz` may
still say `ready`, the exit code is unaffected, but the listener is dead
and no telemetry can land. A running process that looks healthy and
serves nothing: the zombie. The HTTP arm has no disclosing comment at
all, so it is silent. This is the acked-but-actually-broken lie the
Earned-Trust posture forbids.

## Feature framing decisions (DISCUSS, decided)

| ID | Decision | Rationale |
|---|---|---|
| F-Type | **Backend** (gateway serving-layer correctness + observability) | No UI surface; the operator touchpoint is the structured stderr event stream, the `/readyz` probe, and the process exit code. |
| F-Skeleton | **Walking Skeleton = No** (brownfield) | aperture exists and is tagged `aperture/v0.1.0`. The transport spawn helpers, the readiness state machine, the shutdown orchestrator, and the closed event vocabulary all exist. This feature replaces two swallow sites and wires the process reaction; it is not a greenfield bootstrap. |
| F-UX | **UX research = Lightweight** | Single persona (an operator watching logs and orchestrator probes). Emotional arc is Problem Relief (a falsely-green probe in front of a dead listener -> a loud, honest failure the operator can act on). No journey-visual / journey-yaml artifacts produced: backend feature with no screen flow. The stderr event shape and the probe/exit reaction are the only operator surfaces and are captured in the AC. |
| F-JTBD | **The learn-when-the-listener-dies operator job** (above) | Grounded in the four-quadrants Q3 finding + the Earned-Trust posture. |
| F-Slicing | **Single coherent slice, US-01..US-03, with the SILENT HTTP arm explicit (US-03)** | The mechanism (capture the serve `Result`, distinguish graceful from fatal, emit the event, drive the process reaction) is shared by both transports; one slice carries both arms. The carpaccio cut-line, if DESIGN finds the signature ripple large, is gRPC-arm-first (disclosed) then HTTP-arm (silent). See "Carpaccio cut-line" below. |

## Verified Code Findings (confirming the four-quadrants read)

All confirmed by reading the source on this branch.

| Claim | Verified location | Finding |
|---|---|---|
| gRPC serve future is spawned and its result discarded | `crates/aperture/src/transport.rs:89-94` | `let _ = server.await;` inside `tokio::spawn`, the task returns `JoinHandle<()>`. |
| gRPC swallow is DISCLOSED by a comment | `crates/aperture/src/transport.rs:90-92` | "The serve future's error is swallowed at v0... Slice 08 will surface drain outcomes through the shutdown orchestrator." |
| Slice 08 surfaced DRAIN outcomes, NOT serve-loop ERRORS | `crates/aperture/src/shutdown.rs:138-229` | `orchestrate_shutdown` emits `in_flight_drained` / `drain_deadline_exceeded` and maps an exit code, but it only ever does `let _ = join_grpc.await; let _ = join_http.await;` (lines 188-189): the join result (and thus the serve error) is still discarded. The disclosing comment's promise is unfulfilled for the error case. |
| HTTP serve future is spawned and its result discarded | `crates/aperture/src/transport.rs:152-158` | `let _ = axum::serve(listener, router).with_graceful_shutdown(...).await;` inside `tokio::spawn`, returns `JoinHandle<()>`. |
| HTTP swallow is SILENT (no disclosing comment) | `crates/aperture/src/transport.rs:152-158` | No comment acknowledges the swallow on the HTTP arm. The four-quadrants report calls this out as the higher-value (undisclosed) half. |
| Binding errors DO surface synchronously | `crates/aperture/src/transport.rs:57,59` (gRPC), `:124` (HTTP); `crates/aperture/src/compose.rs:140-148,158-173` | `TcpListener::bind(...).await?` and the tonic `from_listener` map to `event=listener_bind_failed` + `ApertureError`. This feature is ONLY about the POST-BIND serving-loop error. |
| `/healthz` is unconditionally 200 while the process is up | `crates/aperture/src/transport.rs:167-173` | "always returns 200" by design (liveness). A dead serving loop does not change it: the zombie's green health check. |
| `/readyz` reflects the readiness phase only | `crates/aperture/src/transport.rs:179-194`; `crates/aperture/src/readiness.rs:69-75` | `Starting`->503, `Ready`->200, `Draining`->503. There is no `Failed`/`Unready` phase a dead serving loop could flip to today. |
| The exit-code path exists and is owned by the drain orchestrator | `crates/aperture/src/lib.rs:205-227`; `crates/aperture/src/shutdown.rs:99-106` | `run` -> `drain_to_exit_code` -> `shutdown_with_trigger` -> `orchestrate_shutdown` -> `DrainOutcome::exit_code()` (0 clean / 1 deadline-exceeded). A serve-loop failure has no path into this exit code today. |
| `Handle::wait_until_ready` is a no-op | `crates/aperture/src/lib.rs:119-121` | Returns `Ok(())` unconditionally. Relevant only as context; not load-bearing for this feature. |
| The closed event vocabulary has room; no `serve_loop_failed` constant yet | `crates/aperture/src/observability.rs:30-51` | Existing constants include `LISTENER_BOUND`, `LISTENER_CLOSING`, `LISTENER_BIND_FAILED`, `IN_FLIGHT_DRAINED`, `DRAIN_DEADLINE_EXCEEDED`, `INTERNAL_INVARIANT_VIOLATION`. There is no `serve_loop_failed`-shaped name. The vocabulary is closed but additions are non-breaking (file header + ADR-0009). This feature adds one constant. |
| A graceful serve-return is NORMAL (Ok) and must NOT alarm | `crates/aperture/src/transport.rs:85-87,153-157`; `crates/aperture/src/shutdown.rs:175-218` | Both `serve_with_incoming_shutdown` (gRPC) and `with_graceful_shutdown` (HTTP) resolve `Ok` on a graceful drain after the oneshot fires. The failure to surface is the non-graceful arm; a graceful return stays a clean no-op. |

## Verified Consumer List (who produces / consumes the spawn JoinHandles today)

The error-surfacing change ripples through whoever produces or consumes
`grpc_join` / `http_join`. All verified on this branch.

| Site | Location | Role | Ripple under the fix |
|---|---|---|---|
| `transport::spawn_grpc` | `crates/aperture/src/transport.rs:50-97` | **Produces** `(SocketAddr, JoinHandle<()>)`. The serve `Result` is discarded at line 93. | The closure must capture and route the serve `Result` (return `JoinHandle<Result<(), E>>` or send on a channel). **pub(crate)** internal helper, NOT public API (see C3). |
| `transport::spawn_http` | `crates/aperture/src/transport.rs:117-161` | **Produces** `(SocketAddr, JoinHandle<()>)`. The serve `Result` is discarded at line 153. | Same closure-signature change. **pub(crate)**, NOT public API. |
| `compose::spawn` | `crates/aperture/src/compose.rs:132,158-159,180-189` | Destructures `(addr, join)` from each spawn helper; stores `grpc_join` / `http_join` into the `ShutdownBundle`. | Consumes the new handle type; wires whatever channel/handle the fix introduces into the bundle and/or the process-reaction path. This is the composition root that owns the wiring decision. |
| `ShutdownBundle` (struct) | `crates/aperture/src/shutdown.rs:125-134` | **Owns** `grpc_join: JoinHandle<()>` / `http_join: JoinHandle<()>` plus the shutdown senders, limiters, readiness, deadline. | The field types change if the handle type changes; or a new field carries the serve-error channel. |
| `orchestrate_shutdown` | `crates/aperture/src/shutdown.rs:138-229`, specifically the drain future at `:185-190` | The **only** site that awaits the joins today: `let _ = join_grpc.await; let _ = join_http.await;`, raced against the deadline. | This is where a serve `Result` would naturally be observed. DESIGN decides whether the orchestrator distinguishes a fatal serve error from a clean drain here, or whether the serve-error path is separate from the shutdown path entirely (a non-graceful death is not a shutdown the operator requested). See D2/D3. |
| `Handle::drop_signal_listeners` | `crates/aperture/src/lib.rs:161-171` | On `Drop`, takes the bundle, sends both shutdown senders, then **abandons** the joins (Drop is sync, cannot await). | If the handle type changes, the field access at `:165-167` still compiles (it touches the senders, not the joins); the abandoned joins at `:168-170` are unaffected. Low ripple. |
| `Handle::shutdown_with_trigger` | `crates/aperture/src/lib.rs:141-151` | Takes the bundle, passes it to `orchestrate_shutdown`. | Unaffected by the field-type change unless the orchestrator's signature changes. |
| `drain_to_exit_code` / `run` | `crates/aperture/src/lib.rs:205-227` | Maps the `DrainOutcome` to a `u8` exit code; `run` is the binary path. | If a serve-loop failure must drive a non-zero exit (D2), this is the exit-code seam it must reach. DESIGN decides whether `run` learns about a serve death through the orchestrator's outcome or a separate channel. |
| Test: `drop_signal_listeners_returns_zero_after_bundle_already_consumed` | `crates/aperture/src/lib.rs:351-356` | Awaits the joins directly: `bundle.grpc_join.await; bundle.http_join.await;`. | Mechanical test update if the handle type changes. |
| Test: `drain_to_exit_code_returns_one_when_deadline_exceeded` | `crates/aperture/src/lib.rs:379-430` | Constructs a `ShutdownBundle` by hand with synthetic `tokio::spawn` joins. | Mechanical: the synthetic joins must match the new field type. This is the existing seam for the negative-control and process-reaction tests (D3 test-seam). |

### NOT consumers (ripple does NOT reach them)

- The gRPC service impls (`LogsServiceImpl` / `TraceServiceImpl` /
  `MetricsServiceImpl`, `transport.rs:501-701`) and the HTTP handlers
  (`handle_logs` / `handle_traces` / `handle_metrics`,
  `transport.rs:214-463`) handle PER-REQUEST outcomes, not the serving
  loop itself. The serve-loop error is a different layer; these are
  untouched.
- `wire_sink` / `probe_or_refuse` (`compose.rs:28-65`) are the
  startup-probe path, orthogonal to the serving loop.
- Public re-exports in `lib.rs` (`run`, `spawn`, `Handle`,
  `config`, `ports`, `testing`) are the public surface; the spawn
  helpers are NOT among them (C3).

## Decisions FLAGGED for DESIGN (the heart of the feature)

DISCUSS encodes the REQUIREMENT (surface the error, stop pretending to be
healthy, do not false-alarm on a graceful shutdown). DESIGN
(`nw-solution-architect`) owns the exact mechanism.

### D1 — The spawn-closure signature change + the consumer ripple (load-bearing, likely INTERNAL only)

- **What**: the spawned tasks currently return `JoinHandle<()>`
  (`transport.rs:89,152`). Surfacing the error means the closure must
  capture and route the serve `Result`: return
  `JoinHandle<Result<(), E>>`, or send the `Result` on a channel to the
  composition root / orchestrator. DESIGN picks the shape.
- **Public-API impact (verify, likely NONE)**: `spawn_grpc` and
  `spawn_http` are `pub(crate)` internal helpers
  (`transport.rs:50,117`); they are NOT re-exported from `lib.rs` (which
  exposes only `run`, `spawn`, `Handle`, `config`, `ports`, `testing`).
  `ShutdownBundle` and `orchestrate_shutdown` are `pub(crate)` too. So
  unlike cinder's `TieringStore` trait change (a genuine public-API
  break), this ripple is expected to be INTERNAL and NOT flag Gate 2 /
  Gate 3. **DESIGN MUST CONFIRM** there is no public-surface leak (e.g.
  via a returned error type that becomes nameable). If a public type is
  introduced, it is a semver-MINOR at most, pre-1.0. **NEVER 1.0.0.**
- **Consumer ripple DESIGN must wire** (verified list above): the
  producer pair (`spawn_grpc` / `spawn_http`), the composition root
  (`compose::spawn`), the `ShutdownBundle` fields, the orchestrator's
  drain future, and the two hand-constructed-bundle tests in `lib.rs`.

### D2 — The process reaction to a dead serving loop (operator-visible, load-bearing)

- **What**: on a non-graceful serve death, does aperture (a) log only,
  (b) flip `/readyz` to unready/failed, (c) exit non-zero, or (d) some
  combination?
- **Why it is flagged, not decided here**: it is an operator-visible
  behavioural policy with a real interaction with the existing shutdown
  orchestrator and exit-code model. DISCUSS requires only that a zombie
  that serves nothing MUST NOT report ready, and SHOULD make the failure
  observable at the process level (US-02 AC). DESIGN picks the exact
  combination and documents it; the chosen behaviour becomes a locked AC
  for DISTILL.
- **Luna's lean** (non-binding input for DESIGN): readiness-unready PLUS
  a non-zero exit is the most Earned-Trust-consistent default. A dead
  listener that still answers `/readyz` 200 is the precise lie the
  feature exists to kill, so flipping readiness is close to mandatory; a
  non-zero exit lets a supervisor (k8s, systemd) restart the zombie
  rather than leaving it wedged. `/healthz` deliberately stays 200
  (liveness = "the process is up", which remains true); the readiness
  probe is the correct lever. DESIGN confirms against the orchestrator's
  model: a serve death is NOT a graceful drain, so it likely should NOT
  reuse the clean `DrainOutcome::Clean { exit_code: 0 }` path; it needs
  its own non-zero exit (distinct from, or aliased to, the
  deadline-exceeded `1`, or a new code). Consider whether a new
  `ReadinessPhase::Failed` (or reuse of `Draining` with a distinct
  `reason`) is the cleanest readiness lever, given `Draining` is sticky
  and never recovers (`readiness.rs:15-21`) which matches a dead
  listener that never recovers.

### D3 — Distinguishing a graceful serve-return (normal) from a fatal serve error (must surface)

- **What**: the serve future returns `Ok(())` on a graceful drain (the
  oneshot fired) and `Err(e)` on a serving-loop failure. The failure to
  surface is the `Err` arm. There is also a subtler case: an unexpected
  early `Ok` BEFORE shutdown was requested (the loop ended cleanly but
  nobody asked it to) which is arguably also a fatal "the listener
  stopped serving" condition.
- **Why it is flagged**: a false alarm on a normal SIGTERM shutdown would
  be its own Earned-Trust dishonesty, so the graceful-vs-fatal
  distinction is load-bearing. DISCUSS requires only that a graceful
  shutdown stays a clean no-op (no `serve_loop_failed` event, existing
  exit code) AND a non-graceful death surfaces (US-03 negative control +
  the both-transports AC). DESIGN decides exactly how the serve task
  knows whether a shutdown was requested (e.g. observe whether the
  oneshot was consumed, or pass a flag), and whether an unexpected-early-
  `Ok` is treated as fatal or tolerated at v0.
- **Test seam (flagged)**: a real accept-loop death is hard to force
  deterministically. The feature needs an injectable serve failure
  in-process. DESIGN/DISTILL pick the injection (e.g. a serving future
  that resolves to `Err`, or closing the underlying listener out from
  under the serve loop). The existing hand-constructed-`ShutdownBundle`
  test in `lib.rs:379-430` (synthetic `tokio::spawn` joins) is the
  established seam for driving a controlled join outcome; it is the
  natural place to inject an `Err`-resolving serve future.

## Carpaccio cut-line

If DESIGN finds the D1 signature ripple larger than expected, the slice
splits along the transport seam:

- **Half A (gRPC arm, disclosed)**: replace the `transport.rs:93`
  swallow; wire the gRPC serve `Result` through to the event + process
  reaction.
- **Half B (HTTP arm, silent)**: replace the `transport.rs:153` swallow;
  same mechanism.

The mechanism is shared, so one slice is likely right. But if a split is
needed, the SILENT HTTP arm (Half B) is the higher-value half (the
four-quadrants report flags it as the undisclosed dishonesty), and it
should not be deferred behind the disclosed gRPC arm. Whichever ships
first must carry the full mechanism (event constant + readiness/exit
reaction + graceful-vs-fatal distinction) so the second arm is a thin
follow-on.

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| **No DIVERGE artifacts** — JTBD not validated through a DIVERGE wave | Medium | Low | The job is grounded in the four-quadrants Q3 finding + the Earned-Trust posture (`brief.md` Principle 12) and verified directly in code. The defect and fix direction are unambiguous. Recorded here; does not block. |
| **False alarm on a normal graceful shutdown** (D3) | Medium | High | A `serve_loop_failed` event (or a process-kill) fired on a normal SIGTERM drain would be its own dishonesty and would page operators for nothing. The graceful-vs-fatal distinction (D3) is load-bearing; the negative-control AC (US-03) asserts a SIGTERM produces the EXISTING clean drain with NO `serve_loop_failed` event and the existing exit code. DISTILL must not inherit a test that cannot tell the two apart. |
| **A serve-failure test that passes on the bug** (the false-confidence trap) | Medium | High | The injected serve failure must make the un-surfaced path OBSERVABLY wrong: no `serve_loop_failed` event captured, `/readyz` still 200, exit still 0. The test passes only when the event IS emitted AND the process reaction (D2) fires. Reuse the stderr-capture seam (`observability.rs:121-139`, `testing::stderr_capture`) and the hand-constructed-bundle seam (`lib.rs:379-430`). DESIGN/DISTILL must not inherit a test that cannot fail on the swallow. |
| **aperture is the LIVE ingest gateway** (real blast radius) | High (it is live) | High | aperture is tagged `v0.1.0` and is the live OTLP gateway on `:4317`/`:4318`. The fix must integrate with the EXISTING shutdown orchestrator and exit-code mapping and must NOT regress graceful drain (the slice-08 contract). The negative-control AC (US-03) is the regression guard; the existing slice-08 acceptance suite (`tests/slice_08_graceful_shutdown.rs`) must stay green. |
| **Signature ripple touches more than expected** (D1) | Low | Medium | The consumer list is enumerated and bounded (one producer pair, one composition root, one bundle struct, one orchestrator drain future, two hand-constructed-bundle tests). It is expected to be INTERNAL (pub(crate)), not a public-API break. If it balloons, the carpaccio cut-line (gRPC then HTTP) is the pre-defined split. DESIGN confirms no public-surface leak. |
| **Gate 2 / Gate 3 unexpectedly flag a public-API change** (D1) | Low | Low | Expected to be internal-only. If a public error type leaks, it is a deliberate semver-MINOR, pre-1.0. **NEVER 1.0.0** (CLAUDE.md / MEMORY). Annotate any public-api diff in DESIGN/DELIVER. |

## Constraints established

- **C1 — aperture is LIVE.** The live OTLP gateway on `:4317`/`:4318`
  (`brief.md`; `lib.rs:1-31`). The process-reaction decision (D2) is
  operator-visible and load-bearing; it belongs in this feature, not
  deferred.
- **C2 — Integrate with the existing shutdown orchestrator, do NOT
  regress graceful drain.** The slice-08 contract
  (`shutdown.rs:138-229`; `tests/slice_08_graceful_shutdown.rs`) must
  stay green. A graceful-shutdown-driven serve return stays a clean
  no-op with the existing exit code and event sequence. The serve-error
  path is additive, not a rewrite of the drain path.
- **C3 — Likely NO public-API break (CONFIRM in DESIGN).** `spawn_grpc`,
  `spawn_http`, `ShutdownBundle`, and `orchestrate_shutdown` are all
  `pub(crate)`; they are not re-exported from `lib.rs`. Unlike cinder's
  trait change, this ripple is expected to be internal and NOT flag Gate
  2 / Gate 3. DESIGN confirms no public-surface leak. Any leak is a
  semver-MINOR at most, pre-1.0. **NEVER 1.0.0.**
- **C4 — Graceful-vs-fatal distinction is load-bearing.** A false alarm
  on a normal shutdown is its own dishonesty (D3). The serve future's
  `Ok` on graceful drain must be told apart from its `Err` on a serving
  failure.
- **C5 — One new event constant, additive.** The closed vocabulary
  (`observability.rs:30-51`) gains a `serve_loop_failed`-style constant
  naming the `transport` and `error`. Additions are non-breaking per the
  file header + ADR-0009. The constant joins the existing
  `listener_*` / `drain_*` family.
- **C6 — `/healthz` stays 200; `/readyz` is the lever.** `/healthz` is
  liveness ("the process is up", which stays true). The readiness probe
  is the correct lever for "this instance serves nothing" (D2).
- **C7 — The failure is in-process simulable.** The serve loop can be
  made to fail post-bind in-process (an injected `Err`-resolving serve
  future, or closing the underlying listener). The
  hand-constructed-`ShutdownBundle` seam (`lib.rs:379-430`) and the
  stderr-capture seam (`testing::stderr_capture`) make the AC
  falsifiable in-suite; no real accept-loop death needed (C7 flagged in
  D3 as a DESIGN/DISTILL injection choice).
- **C8 — Mutation testing 100%** on the modified files
  (`transport.rs`, `shutdown.rs`, `readiness.rs`) per ADR-0005 Gate 5 /
  CLAUDE.md. The swallow-to-surface change (the discarded `Result` must
  not be re-deletable without a surviving test), the graceful-vs-fatal
  branch, and the process-reaction must each be pinned.
- **C9 — Rust idiomatic** per CLAUDE.md: data + free functions + traits
  only where polymorphism is genuinely needed. No new `dyn Trait`
  indirection where the serve `Result` can be routed by a concrete
  channel or a typed `JoinHandle`.
- **C10 — Pure trunk-based, no CI gates** (MEMORY). CI is feedback, not
  a merge gate.

## Notes for downstream waves

- **DESIGN** (`nw-solution-architect`): own D1-D3. Pick the
  serve-`Result` routing shape (D1) and CONFIRM no public-API leak; pick
  the process reaction (D2: readiness lever + exit code, in view of the
  sticky `Draining` phase and the existing `DrainOutcome` exit map);
  decide the graceful-vs-fatal mechanism and the injection seam (D3).
  Produce the ADR if the process-reaction model warrants one (a new
  exit code or a new `ReadinessPhase::Failed` is ADR-worthy). Confirm the
  serve-failure injection is falsifiable.
- **DISTILL** (`nw-acceptance-designer`): the BDD scenarios in
  `user-stories.md` are the source; the D2 process-reaction and the D3
  graceful-vs-fatal branch DESIGN picks become locked ACs. Do NOT
  inherit a serve-failure test that passes on the swallow, and do NOT
  inherit a negative-control that cannot tell a graceful shutdown from a
  fatal serve death.
- **DELIVER** (`nw-software-crafter`): only the crafter writes
  `crates/*/src/`. Replace the two swallows + wire the process reaction +
  add the event constant. 100% mutation kill on the modified files (Gate
  5). Keep the slice-08 graceful-drain suite green.
