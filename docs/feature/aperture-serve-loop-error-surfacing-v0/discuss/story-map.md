# Story Map: aperture-serve-loop-error-surfacing-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-05.
British English. No em dashes in body.

Companion to `user-stories.md` (US-01, US-02, US-03) and
`wave-decisions.md` (D1, D2, D3; carpaccio cut-line). This map fixes the
backbone, marks the walking-skeleton posture, and slices the three
stories into releases with falsifiable learning hypotheses. It does not
restate the AC (they live per story in `user-stories.md`); it sequences
them.

## Persona and emotional arc (single persona, lightweight UX)

Sam, the platform operator, runs a live Kaleidoscope ingest fleet and
scrapes aperture's structured stderr into a log/alert pipeline; he relies
on `/readyz` (the lever his orchestrator acts on) and the process exit
code. The arc is Problem Relief: today a falsely-green `/healthz` (and a
stale `/readyz ready`) sits in front of a dead listener and Sam cannot
tell; after this feature a serving-loop death is loud (one
`serve_loop_failed` event naming the transport) and honest (the process
stops reporting ready and/or exits non-zero), while a normal SIGTERM
restart stays a clean no-op with no false alarm.

## Backbone (the operator's activity spine, left to right)

The horizontal sequence is the lifecycle of a serving loop, not a feature
grouping. Each activity is a step Sam's instance moves through.

```text
  BIND                SERVE                A LOOP DIES            OPERATOR LEARNS
  the socket    ->    accept &       ->   post-bind, the    ->  + the process
  (already            ingest              accept loop dies      stops lying
   honest:            (today: the         (the defect:          (this feature:
   listener_          serve Result        the zombie is         surface + react,
   bind_failed        is swallowed,       born here)            no false alarm)
   surfaces)          let _ = ...await)
        |                  |                    |                      |
   out of scope       out of scope         the trigger          IN SCOPE
   (synchronous       (happy serving       (post-bind           US-01 + US-02 + US-03
    bind already       path unchanged)      Err arm, or
    surfaces)                               unexpected early Ok)
```

The first two backbone cells are already honest or unchanged: binding
errors surface synchronously (`transport.rs:57,124`; `compose.rs:140-173`
-> `event=listener_bind_failed`), and the happy serving path is
untouched. The feature owns only the last two cells: a post-bind death,
and the operator learning plus the process ceasing to lie.

### Backbone-to-story mapping

| Backbone activity | Owning story | Operator outcome |
|---|---|---|
| A loop dies post-bind (gRPC or HTTP) | US-01 | one structured `serve_loop_failed` event names the transport + error on stderr |
| The process stops lying | US-02 | `/readyz` no longer reports ready and/or the process exits non-zero (D2); `/healthz` stays 200 |
| Operator learns honestly on BOTH arms, never on a normal shutdown | US-03 | the previously SILENT HTTP arm is proven; a graceful SIGTERM is a clean no-op (D3) |

## Walking skeleton: No (brownfield)

`F-Skeleton = No` (recorded in `wave-decisions.md`). aperture exists and
is tagged `aperture/v0.1.0`: the transport spawn helpers, the readiness
state machine (`readiness.rs`), the shutdown orchestrator
(`shutdown.rs`), the exit-code seam (`lib.rs:205-227`), and the closed
event vocabulary (`observability.rs:30-51`) are all present. There is no
greenfield end-to-end thread to stand up. Instead the thinnest
end-to-end slice is defined below.

### Thinnest end-to-end slice (the brownfield equivalent of a walking skeleton)

The thinnest slice that delivers a verifiable operator-visible behaviour
on its own is:

> A single transport's post-bind serve `Err` is captured (D1 routing),
> surfaced as one `serve_loop_failed` event (US-01), and drives the D2
> process reaction (US-02), proven by ONE injected-serve-failure
> acceptance test that FAILS against today's `let _ = ...await` swallow
> and a negative-control test that a graceful drain stays clean.

That slice exercises the full vertical thread (capture -> emit ->
process reaction -> falsifiable test) for one arm. The second arm is then
a thin follow-on over the same mechanism. The slice is NOT "add the event
constant" or "change the signature" alone; those are technical layers,
not operator outcomes, and neither is demonstrable to Sam by itself.

## Carpaccio slicing into releases

The default is one coherent slice carrying all three stories, because the
mechanism (capture the serve `Result`, distinguish graceful from fatal,
emit the event, drive the process reaction) is shared by both transports.
The split below is the pre-agreed fallback if DESIGN finds the D1
signature ripple larger than expected (`wave-decisions.md` carpaccio
cut-line). Either way, releases are sliced by operator outcome, not by
technical layer.

### Release R0 (default): one coherent slice, both arms

| Field | Value |
|---|---|
| Scope | US-01 + US-02 + US-03, both transports, full mechanism |
| Operator outcome | a post-bind serve death on either arm is loud and honest; a normal shutdown never false-alarms |
| Learning hypothesis | "Capturing one serve `Result` per transport and routing it to one event + the D2 reaction is enough to kill the zombie on both arms without disturbing the graceful drain." Falsified if the D1 ripple forces the orchestrator or `ShutdownBundle` to grow beyond the enumerated consumer list, or if the graceful-vs-fatal guard cannot be made deterministic in-suite. On falsification, split into R1 + R2 below. |
| Demonstrable in one session | yes (injected-serve-failure test per arm + SIGTERM negative control) |

### Fallback split (if R0 falsified): R1 then R2, sliced along the transport seam

The full mechanism (event constant C5 + readiness/exit reaction D2 +
graceful-vs-fatal guard D3) ships in whichever arm goes first, so the
second arm is a thin follow-on.

#### R1 (carries the mechanism): the SILENT HTTP arm first

| Field | Value |
|---|---|
| Scope | US-01 + US-02 + US-03 for the HTTP arm; introduces the event constant, the D1 routing, the D2 reaction, the D3 guard, the negative control |
| Why HTTP first | the HTTP swallow (`transport.rs:153`) is the UNDISCLOSED half: silent, no comment, flagged by the four-quadrants report as the higher-value finding. It must NOT be deferred behind the disclosed gRPC arm. |
| Operator outcome | the previously silent HTTP serving-loop death surfaces and the process stops lying on the HTTP arm; a normal shutdown stays clean |
| Learning hypothesis | "The shared mechanism, proven on the higher-value silent arm first, surfaces a real post-bind HTTP death (test fails on today's swallow) and leaves the graceful drain a clean no-op (slice-08 suite green, no `serve_loop_failed`)." |

#### R2 (thin follow-on): the DISCLOSED gRPC arm

| Field | Value |
|---|---|
| Scope | apply the R1 mechanism to the gRPC arm (`transport.rs:93`); delete the disclosing comment whose promise is now fulfilled |
| Operator outcome | the gRPC serving-loop death surfaces identically; both arms now covered (US-03 fully satisfied) |
| Learning hypothesis | "With the mechanism already proven on HTTP, the gRPC arm is a same-shape wiring change with no new policy decisions; its own injected-failure test fails on the swallow and passes once wired." |

### Priority Rationale

Priority is by operator-outcome impact and dependency, not by feature
grouping.

1. **US-01 (surface the event) is the spine.** Without a captured,
   routed serve `Result` there is nothing to react to and nothing to
   alert on. It is the prerequisite for US-02 and US-03 (Technical Notes
   in both depend on US-01). Highest priority.
2. **US-02 (process stops lying) is the operator's lever.** An event Sam
   can read is necessary but not sufficient; the zombie still wedges in
   rotation until `/readyz` flips and/or the process exits non-zero (D2).
   This is the half that makes the orchestrator act. Second, because it
   depends on US-01's captured `Result`.
3. **US-03 (both arms proven + no false alarm) is the trust guarantee.**
   It proves the SILENT HTTP arm explicitly (not by gRPC symmetry) and
   pins the graceful-vs-fatal negative control (D3) so a normal SIGTERM
   never pages Sam. Third by sequence, but its negative control is a hard
   regression guard on the live gateway (C2): it must ship in the same
   release as US-01/US-02, never after, so the fix cannot introduce a
   false alarm in production.
4. **If the split is taken, the SILENT HTTP arm leads (R1).** The
   undisclosed half is the higher-value half; deferring it would re-ship
   the exact dishonesty the feature exists to kill.

## Carpaccio taste tests

| Taste test | Verdict | Evidence |
|---|---|---|
| Is each release an end-to-end, demonstrable operator outcome (not a technical layer)? | PASS | R0, R1, R2 each surface a real serving-loop death and drive the process reaction, demonstrable by an injected-failure test in one session. The thinnest slice explicitly rejects "add the constant" / "change the signature" as non-outcomes. |
| Does each release deliver verifiable value on its own? | PASS | R0 covers both arms; R1 covers the higher-value silent arm with the full mechanism; R2 adds the second arm. Each is independently shippable and independently testable. |
| Is the highest-value work first? | PASS | The silent HTTP arm (the four-quadrants higher-value finding) is never deferred: it is in R0, and leads the split as R1. |
| Right-sized (1-3 days, 3-7 scenarios each)? | PASS | Three stories, each 3 UAT scenarios (9 total), one crate, one operator persona, no UI. Bounded consumer ripple (one producer pair, one composition root, one bundle, one orchestrator drain future, two tests). See Scope Assessment. |
| Does the negative control ride with the change (no false-alarm window)? | PASS | US-03's SIGTERM negative control is required in the same release as the surfacing change (Priority Rationale 3), so no release can introduce a graceful-shutdown false alarm. |
| Could a slice ship a half-truth (one arm surfaced, the other still silent)? | GUARDED | Only acceptable as R1 -> R2 where R1 is the SILENT arm and R2 follows immediately; R0 avoids the half-truth entirely by carrying both arms. |

## Scope Assessment: PASS — 3 stories, 1 bounded context (aperture crate), estimated 1-2 days

Oversized signals checked (none tripped):

- User stories: 3 (threshold > 10). PASS.
- Bounded contexts / modules: 1 crate, `aperture`; the touched modules
  (`transport.rs`, `shutdown.rs`, `readiness.rs`, `compose.rs`,
  `observability.rs`, `lib.rs`) are all within it (threshold > 3
  bounded contexts). PASS.
- Walking-skeleton integration points: WS = No (brownfield); the thinnest
  slice has one vertical thread per arm (threshold > 5). PASS.
- Estimated effort: 1-2 days; two swallow-site replacements + one event
  constant + the D2 reaction wiring + the D3 guard, over a bounded,
  enumerated consumer list (threshold > 2 weeks). PASS.
- Independent shippable outcomes: the arms could ship separately (R1/R2),
  but they share one mechanism and one persona; this is a controlled
  carpaccio split, not an oversized feature masquerading as one story.
  PASS.

No split required for size. The carpaccio split above is a DESIGN-ripple
contingency, not a size remedy. Proceeding without restructuring.

## Verified spawn / JoinHandle consumer note (feeds D1, confirmed on this branch)

`spawn_grpc` (`transport.rs:50`) and `spawn_http` (`transport.rs:117`)
are declared `pub async fn` but live in a PRIVATE module (`mod transport;`
at `lib.rs:47`, no `pub`/`pub(crate)` qualifier), and are NOT re-exported
from `lib.rs` (only `config`, `ports`, `testing` are `pub mod`). They are
therefore crate-private in effect: NOT public API. The single producer ->
consumer chain for the two `JoinHandle<()>` values is:

`spawn_grpc`/`spawn_http` (produce `(SocketAddr, JoinHandle<()>)`,
swallow the serve `Result` at `:93`/`:153`)
-> `compose::spawn` (`compose.rs:132,150,158,180-189`, destructures and
stores `grpc_join`/`http_join` into the `ShutdownBundle`)
-> `ShutdownBundle` (`shutdown.rs:125-134`, owns both joins)
-> `orchestrate_shutdown` drain future (`shutdown.rs:185-190`, the SOLE
awaiter today: `let _ = join_grpc.await; let _ = join_http.await;`)
-> exit-code seam `drain_to_exit_code`/`run` (`lib.rs:205-227,224-227`).
Plus the `Drop` abandon-path (`Handle::drop_signal_listeners`,
`lib.rs:161-171`) and two hand-constructed-bundle tests
(`lib.rs:351-356`, `lib.rs:379-430`, the injection seam). This is the
full D1 ripple; it is INTERNAL only.
