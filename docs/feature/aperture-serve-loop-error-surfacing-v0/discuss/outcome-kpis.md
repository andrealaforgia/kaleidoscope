# Outcome KPIs: aperture-serve-loop-error-surfacing-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-05.
British English. No em dashes in body.

Companion to `user-stories.md` (US-01/US-02/US-03 each carry a per-story
Outcome KPIs block) and `wave-decisions.md` (D1/D2/D3). This file
consolidates the feature-level KPIs with numeric targets, baselines, and
measurement methods, so DEVOPS (`platform-architect`) can design tracking
and DISTILL/DELIVER can assert them. Every KPI is falsifiable: each names
a test that MUST FAIL against today's `let _ = ...await` swallow and PASS
only once the behaviour is correct.

All baselines are verified on this branch (see `wave-decisions.md` >
Verified Code Findings).

## Feature-level KPIs

### KPI-1 — Swallowed serve-loop sites: 2 -> 0 (US-01)

- **Who**: aperture's gRPC and HTTP serving loops in the live ingest
  gateway.
- **Does what**: route the post-bind serve `Result` instead of
  discarding it.
- **By how much**: swallow sites move from **2 to 0**.
- **Baseline**: 2 sites discard the serve `Result` today,
  `transport.rs:93` (`let _ = server.await;`, gRPC, disclosed by a
  comment) and `transport.rs:158` / the `let _ =` at `:153-157` (HTTP
  `axum::serve(...).with_graceful_shutdown(...).await`, silent, no
  comment).
- **Measured by**: a static check that no `let _ = <serve>.await` remains
  on either arm, plus `cargo mutants` on `transport.rs` showing the
  former swallow lines are now covered (a surviving mutant on a deleted
  swallow would mean the new path is untested). Target: 0 surviving
  mutants on the two former swallow sites (see KPI-6).

### KPI-2 — A fatal serve error emits exactly one `serve_loop_failed` event per arm (US-01)

- **Who**: Sam's log/alert pipeline scraping aperture stderr.
- **Does what**: on a non-graceful serve death, emit one structured
  `event=serve_loop_failed transport=grpc|http error=<reason>` line at
  error level, in the same JSON shape as every other aperture event.
- **By how much**: exactly **1** event per injected post-bind serve
  failure per transport (not 0, not 2); event count for the
  graceful-drain path stays **0** (see KPI-4).
- **Baseline**: **0** events today; the serve `Result` is discarded on
  both arms; the closed vocabulary has no `serve_loop_failed` constant
  (`observability.rs:30-51`).
- **Measured by**: the injected-serve-failure acceptance test (one per
  transport) capturing stderr via `testing::stderr_capture` and asserting
  exactly one `serve_loop_failed` line with the correct `transport` field
  and a non-empty `error`. **Falsifiability**: this test MUST FAIL
  against today's `let _ = ...await` swallow (which emits nothing). A test
  that passes on the swallow is rejected (`wave-decisions.md` risk
  "A serve-failure test that passes on the bug").

### KPI-3 — The process reaction to a dead serving loop is observable (US-02, D2)

- **Who**: Sam's orchestrator acting on `/readyz` and the process exit
  code.
- **Does what**: a dead serving loop stops the process reporting ready
  and/or makes it exit non-zero (the exact combination is D2, DESIGN's
  call), while `/healthz` stays 200.
- **By how much**: the dead-listener-still-reports-ready window moves
  from **indefinite** (today readiness never flips on a serve death) to
  **the next probe after the death** (0 probes stay-ready after the
  reaction fires); and a serve death yields a **non-zero** exit reaction
  distinct from the clean-drain `0`.
- **Baseline**: today `/readyz` stays `200 "ready"` (no `Failed` phase;
  `readiness.rs:37-41` has only `Starting`/`Ready`/`Draining`) and the
  exit code is unaffected (`DrainOutcome::exit_code()` is 0 clean / 1
  deadline-exceeded; a serve error has no path in,
  `shutdown.rs:99-106,185-190`).
- **Measured by**: the serving-loop-death acceptance test asserting
  `/readyz` no longer reports ready AND/OR the exit reaction is non-zero
  per the locked D2 choice, while `/healthz` still returns 200.
  **Falsifiability**: MUST FAIL today (where `/readyz` stays ready and
  exit stays 0). The precise observable is locked by DESIGN's D2
  decision; this KPI tracks "a zombie is never presented as ready",
  which is mandatory regardless of which D2 combination is chosen.

### KPI-4 — Graceful-shutdown false-alarm rate: 0 (US-03, D3)

- **Who**: Sam restarting instances routinely with SIGTERM.
- **Does what**: a normal graceful shutdown stays a clean no-op, no
  `serve_loop_failed` event and no readiness-failed/exit reaction.
- **By how much**: **0** false alarms. A SIGTERM drain emits the existing
  slice-08 sequence (`shutdown_initiated`, `readiness_changed
  ready=false reason=shutdown_drain`, `in_flight_drained`,
  `shutdown_complete exit_code=0`) and **0** `serve_loop_failed` lines,
  with the existing clean-drain exit code **0**.
- **Baseline**: not applicable as a regression today (no surfacing
  exists), but the existing graceful drain emits the slice-08 sequence
  and exit `0`; this KPI guarantees the new code does not perturb it.
- **Measured by**: the SIGTERM negative-control acceptance test (US-03
  scenario 1) asserting the existing sequence, exit `0`, and **0**
  `serve_loop_failed` lines; AND the existing slice-08 suite
  (`tests/slice_08_graceful_shutdown.rs`) staying green. **Falsifiability
  guard**: the negative control must be able to tell a graceful return
  (`Ok`, `transport.rs:85-87,153-157`) from a fatal `Err`; a test that
  cannot distinguish them is rejected (`wave-decisions.md` risk "False
  alarm on a normal graceful shutdown").

### KPI-5 — Transport coverage: 1-of-2-disclosed -> 2-of-2-surfaced, HTTP arm explicitly proven (US-03)

- **Who**: Sam alerting on `serve_loop_failed` across both transports.
- **Does what**: both the gRPC and the HTTP serving-loop deaths surface
  identically, with the previously SILENT HTTP arm proven by its own
  acceptance scenario (not implied by the gRPC scenario).
- **By how much**: coverage moves from **1-of-2 disclosed** (gRPC only,
  and even that silent at runtime) to **2-of-2 surfaced**; the HTTP arm
  moves from **undisclosed** to **proven** by a dedicated test.
- **Baseline**: gRPC swallow disclosed-but-silent (`transport.rs:90-94`);
  HTTP swallow undisclosed-and-silent (`transport.rs:152-158`).
- **Measured by**: two distinct surfacing tests (one gRPC, one HTTP), the
  HTTP one present in its own right (US-03 scenario 2), each asserting the
  per-transport `serve_loop_failed` event and the D2 reaction.

### KPI-6 — Mutation kill: 100% on the modified transport/shutdown/readiness lines (C8, Gate 5 guardrail)

- **Who**: the project's correctness guarantee (ADR-0005 Gate 5).
- **Does what**: every mutation on the changed lines is killed by a test,
  so the swallow cannot silently return and the graceful-vs-fatal branch
  and the process reaction cannot be weakened undetected.
- **By how much**: **100%** kill rate on the modified files
  (`transport.rs`, `shutdown.rs`, `readiness.rs`), 0 surviving mutants on
  the changed lines.
- **Baseline**: the swallow lines are currently uncovered for the error
  path (a mutant deleting `let _ = server.await;` survives, since nothing
  asserts the serve error is observed).
- **Measured by**: `cargo mutants` scoped to the modified files per the
  per-feature mutation strategy (CLAUDE.md); the run is the Gate 5 check
  in DELIVER. Target: 100% kill, including the new graceful-vs-fatal
  branch (D3) and the D2 reaction (a readiness-flip or exit-code mutation
  must be killed).

## KPI-to-story trace

| KPI | Primary story | Decision dependency | Baseline locus |
|---|---|---|---|
| KPI-1 swallow sites 2 -> 0 | US-01 | D1 | `transport.rs:93,153-158` |
| KPI-2 one event per arm | US-01 | D1, D3 | `observability.rs:30-51`; serve discards |
| KPI-3 process reaction observable | US-02 | D2 | `readiness.rs:37-41`; `shutdown.rs:99-106,185-190` |
| KPI-4 false-alarm rate 0 | US-03 | D3 | `transport.rs:85-87,153-157`; slice-08 suite |
| KPI-5 both arms, HTTP proven | US-03 | D1, D2, D3 | `transport.rs:90-94` (gRPC), `:152-158` (HTTP) |
| KPI-6 100% mutation kill | all (guardrail) | C8 | the swallow lines are uncovered for the error path |

## Measurement timing

- **DISTILL** (`acceptance-designer`): KPI-2, KPI-3, KPI-4, KPI-5 become
  executable acceptance assertions; each must fail on today's swallow
  before DELIVER makes it pass (the EDD failing-test-first discipline).
- **DELIVER** (`software-crafter`): KPI-1 (static, no `let _ = ...await`
  remains) and KPI-6 (`cargo mutants` 100% on the three modified files)
  are the Gate 5 closing checks.
- **DEVOPS** (`platform-architect`): KPI-2 and KPI-3 are the operator-
  facing signals (the `serve_loop_failed` event stream and the `/readyz`
  flip / non-zero exit) worth wiring into fleet observability; this file
  is the tracking-design input.
