# Wave Decisions: aperture-serve-loop-error-surfacing-v0 (DISTILL)

Author: Quinn (nw-acceptance-designer). Wave: DISTILL. Date: 2026-06-05.
Mode: PROPOSE (autonomous overnight run). British English. No em dashes
in body.

This file records the DISTILL decisions: the wave-decision reconciliation
against DISCUSS / DESIGN / DEVOPS, the walking-skeleton strategy, the
falsifiability guarantee, the `#[ignore]`-until-DELIVER classification
with the proven-RED evidence, and the injection-seam decision. The
scenario-to-AC map, the adapter coverage table, the error-path ratio, and
the mandate self-review live in the sibling
`acceptance-test-scenarios.md`.

The acceptance/integration suite is
`crates/aperture/tests/serve_loop_error_surfacing.rs` (the driving-port
black-box tests) plus the minimal injection seam
`crates/aperture/src/testing.rs > spawn_with_injected_serve_failure`
(a RED `unimplemented!` scaffold; DELIVER implements the body). The new
`[[test]]` block is in `crates/aperture/Cargo.toml`.

## Wave-Decision Reconciliation (0 contradictions)

DISTILL is the conjunction point: the DISCUSS requirement, the DESIGN
resolution (ADR-0066 + `design/wave-decisions.md`), and the DEVOPS CI /
environment confirmation are all read and reconciled before any scenario
is locked. The acceptance tests encode the locked ACs verbatim; they do
not re-open any decision.

| Decision | DISCUSS (Luna) | DESIGN (Morgan, ADR-0066) | DEVOPS (Apex) | DISTILL encoding | Contradiction? |
|---|---|---|---|---|---|
| **D1** serve-`Result` routing | flagged: capture + route the serve `Result`; expected INTERNAL (`pub(crate)`), CONFIRM no public leak | resolved: `JoinHandle<ServeOutcome>` + serve-task self-react; `ServeOutcome`/`ServeError` `pub(crate)`; no public type | confirmed: no public-API break, aperture stays `0.1.0`, not enrolled in Gate 2/3 | tests drive ONLY the public surface (`spawn`, `Handle`, `testing::spawn_with_injected_serve_failure`); never name `ServeOutcome`/`ReadinessPhase`/`ShutdownBundle` (all `pub(crate)`, unreachable from `tests/`) | **None** |
| **D2** process reaction | required: zombie MUST NOT report ready; SHOULD make the failure observable at process level; `/healthz` stays 200 | resolved: sticky `ReadinessPhase::Failed` -> `/readyz` 503 `"failed"`; distinct exit code **3**; `/healthz` stays 200 | confirmed as the operator signal set (stderr event + `/readyz` flip + exit 3); no new metric/dashboard | scenarios assert `/readyz` 503 `"failed"`, `/healthz` 200, and exit code 3 via the real binary | **None** |
| **D3** graceful-vs-fatal | required: a graceful shutdown stays a clean no-op (no false alarm); a non-graceful death surfaces | resolved: discriminator is `shutdown_requested` `AtomicBool`; not-requested -> any return (Err OR early Ok) is fatal; early-Ok fatal at v0 | confirmed: the negative control must distinguish `shutdown_requested=true` from `=false`; falsifiability mandatory (C-DEVOPS-4) | two negative controls (graceful drain emits NO `serve_loop_failed`; healthy instance reports ready) PASS today; an `InjectServeFailure::GrpcEarlyOk` scenario pins the not-requested-early-Ok -> fatal leg | **None** |
| Public-API / semver | C3: likely no break, CONFIRM; NEVER 1.0.0 | confirmed INTERNAL; no new public type | C-DEVOPS-2: NO break, NO bump, stays `0.1.0`, NEVER 1.0.0 | the suite imports only `aperture::config`, `aperture::ports`, `aperture::testing`, `aperture::spawn`, `aperture::Handle` (all already public); adds no public type | **None** |
| Test seam | C7: in-process simulable; the hand-constructed-`ShutdownBundle` seam + `stderr_capture` make the AC falsifiable | ADR-0066 "Test seam": two layered seams (unit exit-code bundle seam + acceptance injected-serve-future seam, the `FailingFsyncBackend` analogue) | C-DEVOPS-3: deterministic, runs in pre-commit AND CI Gate 1; no wall-clock threshold | the acceptance seam is `spawn_with_injected_serve_failure(config, sink, InjectServeFailure::{Grpc,Http,GrpcEarlyOk})`; the exit-code/process seam is the real binary via subprocess (`CARGO_BIN_EXE_aperture`) | **None** |
| Regression guard | C2: do NOT regress graceful drain; slice-08 suite stays green | graceful path byte-for-byte unchanged; slice-08 stays green | C-DEVOPS-5: guardrails stay green; healthy/graceful negative controls must not regress | the two PASS-today negative controls ARE the in-process guardrails; the SIGTERM exit-0 subprocess control mirrors the slice-08 `#[ignore]` precedent | **None** |
| Mutation gate | C8: 100% on `transport.rs`/`shutdown.rs`/`readiness.rs` | carried to DELIVER (C8); the surfacing + exit-3 + negative-control tests pin it | C-DEVOPS-1/6/D9: existing `gate-5-mutants-aperture --in-diff` covers all five files; no new job; no CLAUDE.md change | DISTILL authors the falsifiable tests that supply the mutation-kill coverage; DELIVER turns them green and runs Gate 5 | **None (carried)** |

**Reconciliation verdict: 0 contradictions.** Every locked AC traces to a
DISCUSS requirement, a DESIGN resolution, and a DEVOPS confirmation that
agree. The only "open" items are the ones explicitly handed to DELIVER
(implement the seam body, turn the RED tests green, run Gate 5), which is
the correct nWave ordering, not a contradiction.

### nWave-order note (for the reviewer)

In nWave, **DISTILL runs BEFORE DELIVER**. At DISTILL time the production
fix does NOT exist and the injection-seam body does NOT exist: that is the
EXPECTED and CORRECT state. `#[ignore]`-d behaviourally-RED tests plus an
`unimplemented!()` seam are the planned DISTILL deliverable, NOT a defect
and NOT a rejection reason. The reviewer evaluates scenario quality,
business-language purity, the >=40% error-path ratio, the driving-adapter
subprocess scenario, and falsifiability against the present swallow, NOT
the absence of the fix.

## Walking-skeleton strategy: Strategy C (real-local-IO)

aperture is brownfield (F-Skeleton = No in DISCUSS): the harness, the
readiness machine, the shutdown orchestrator, and the closed event
vocabulary already exist. The "walking skeleton" here is therefore the
thinnest real-IO slice that proves the operator-visible contract end to
end, not a greenfield bootstrap.

**Strategy C, real-local-IO, chosen** (not in-memory doubles for the
boundary under test):

- **Real in-process listeners.** Every serve-failure scenario binds REAL
  loopback listeners (`127.0.0.1:0`, ephemeral ports) through the
  production spawn path and probes `/readyz` / `/healthz` over a REAL
  `reqwest` HTTP client across the wire. The death is injected behind the
  already-bound listener (the failure is post-bind, by definition), so the
  probes exercise the genuine readiness arm, not a stubbed status.
- **Real subprocess for the binary exit codes.** The exit-code ACs (the
  one signal an in-process harness cannot produce honestly) drive the REAL
  `aperture` binary as a child process (`CARGO_BIN_EXE_aperture`) and read
  its real OS exit code. This is the genuine process boundary a supervisor
  (k8s `restartPolicy`, systemd) reads.
- **Injected serve future, not a faked readiness state.** The only thing
  faked is the *trigger* (a serve future forced to resolve `Err` / early
  `Ok` post-bind, the aperture analogue of cinder's
  `FailingFsyncBackend`). Everything downstream of the trigger (the event
  emission, the `flip_to_failed`, the exit-code fold) is real production
  code. This is the falsifiability requirement: faking the *outcome* would
  be Fixture Theater.

**Why not in-memory doubles for the boundary:** an InMemory readiness or a
stubbed `/readyz` could not catch the wiring bug this feature exists to
fix (the serve `Result` never reaching readiness/exit), nor the
output-format details (`503` with body `"failed\n"`, exit code exactly
`3`). The boundary under test IS the wiring; it must be exercised with
real local IO. `RecordingSink` (an in-memory `OtlpSink`) is used only for
the orthogonal data-plane seam, never to stand in for the serve-failure
boundary.

## Falsifiability note (each failure test asserts an observable the swallow cannot satisfy)

Every RED failure scenario asserts a SPECIFIC operator-visible observable
that today's `let _ = server.await;` (gRPC, `transport.rs:93`) and
`let _ = axum::serve(...).await;` (HTTP, `transport.rs:153-157`) swallow
CANNOT produce. The test therefore cannot pass on the bug; it passes only
once the surfacing-and-reaction fix lands. This is the explicit
false-confidence guard DISCUSS (risk table), DESIGN (ADR-0066 "Test
seam"), and DEVOPS (C-DEVOPS-4) all require.

| RED scenario | Asserted observable | Why the swallow CANNOT satisfy it |
|---|---|---|
| gRPC serve death named on stderr | exactly one `event=serve_loop_failed transport=grpc error=<reason>` at `error` level | the discarded `Result` emits nothing; `expect_stderr_event` panics on the missing event |
| HTTP serve death named on stderr | exactly one `serve_loop_failed transport=http` (the previously SILENT arm) | the silent `let _ = axum::serve(...)` emits nothing; proven by its OWN scenario, never implied by gRPC |
| dead loop stops reporting ready | `/readyz` flips to 503 `"failed"`; `/healthz` stays 200 | there is no `Failed` readiness phase today (`readiness.rs:37-41`), so `/readyz` stays 200 `"ready"` after a death |
| readiness Failed is sticky | `/readyz` stays 503 `"failed"` across repeated probes, never flaps to 200 | the flip never happens today, so the sticky invariant has nothing to hold |
| early-Ok without shutdown is fatal | one `serve_loop_failed transport=grpc` for an unexpected early `Ok` | a discarded early `Ok` is indistinguishable from a graceful return today; the `shutdown_requested` discriminator (D3) does not exist |
| binary exits 3 on injected serve death | real process exit code `3`, distinct from clean-drain 0 / deadline 1 / config 2 | the swallowed serve error has no path into the exit code; the binary would exit 0 (or run until killed) |
| binary exits 0 + silent on real SIGTERM | real process exit code `0`, NO `serve_loop_failed` on stderr | RED-by-pending-fixture: the process-spawning SIGTERM fixture is not yet landed (the in-process graceful negative control already proves the behaviour green) |

The two PASS-today negative controls assert the OPPOSITE observables (a
graceful drain emits NO `serve_loop_failed`; a healthy instance reports
`/readyz` ready + `/healthz` ok). They are the guardrails a correct fix
must not regress; a fix that fired on a graceful return (D3
mis-implemented) would make them go RED, catching the false-alarm
dishonesty.

## `#[ignore]`-until-DELIVER decision (proven RED, not BROKEN)

aperture already exists, so the harness, the stderr-capture seam, the HTTP
probes, and the public `spawn`/`Handle` surface all resolve and compile.
The serve-failure scenarios are RED because the seam they drive is
`unimplemented!` and, once DELIVER implements it, they would still FAIL
against the present swallow. They are therefore `#[ignore]`-d until DELIVER
with an explicit reason on each, so they cannot masquerade as passes and
so trunk stays green.

**Verified evidence (already run during the prior verified DISTILL pass;
not re-run here):**

- **Compiles cleanly.** `cargo test -p aperture --no-run` builds all test
  executables including `serve_loop_error_surfacing`.
- **DEFAULT run (trunk-green gate).**
  `cargo test -p aperture --test serve_loop_error_surfacing` =>
  `test result: ok. 3 passed; 0 failed; 7 ignored`. The 3 un-ignored
  PASSING negative controls are
  `graceful_shutdown_emits_no_serve_loop_failed_event`,
  `healthy_instance_reports_ready_and_alive`, and
  `binary_preserves_config_error_exit_code_two`. Trunk stays green; the
  pre-commit `cargo test --workspace` passes.
- **IGNORED run (proven RED-not-BROKEN).**
  `cargo test -p aperture --test serve_loop_error_surfacing -- --ignored`
  => `test result: FAILED. 0 passed; 7 failed; 3 filtered out`. All 7 fail
  via clean **panics** (RED), not compile errors:
  - **5 panic at the `unimplemented!()` seam** (`testing.rs:204`):
    `grpc_serving_loop_death_after_bind_is_named_on_stderr`,
    `http_serving_loop_death_after_bind_is_named_on_stderr`,
    `dead_serving_loop_stops_reporting_ready_but_stays_alive`,
    `readyz_failed_phase_is_sticky_and_never_flaps_back_to_ready`,
    `early_ok_without_shutdown_request_is_treated_as_fatal`.
  - **2 panic at behavioural subprocess assertions** in the test file:
    `binary_exits_three_on_injected_serve_death` (line 472, exit-3
    assertion) and `binary_exits_zero_and_silent_on_real_sigterm`
    (line 510, the explicit RED placeholder for the pending SIGTERM
    fixture).

Each of the 7 carries an `#[ignore = "RED until DELIVER: ..."]` reason
naming exactly what is missing and why it fails on the swallow. This is
the RED-not-BROKEN classification (Mandate 7): the suite compiles, the
negative controls are GREEN, and the failure scenarios fail for a business
reason (the contract is unmet), not a setup/compile error.

## Seam decision (the minimal seam DELIVER must implement)

The acceptance layer needs a deterministic post-bind serve death, because
a real accept loop rarely dies on command. DISTILL ships the MINIMAL seam
and leaves the body to DELIVER, mirroring the cinder
`FailingFsyncBackend` / `open_with_fsync_backend` precedent (a test-only
failure-injection helper kept beside the production type, the failure
forced behind the real boundary).

```text
// crates/aperture/src/testing.rs  (DISTILL scaffold; DELIVER implements)

pub enum InjectServeFailure { Grpc, Http, GrpcEarlyOk }   // Copy, Eq

pub async fn spawn_with_injected_serve_failure(
    config: Config,
    sink: Arc<dyn OtlpSink>,
    which: InjectServeFailure,
) -> Result<Handle, ApertureError> {
    unimplemented!(/* DELIVER: bind listeners, then resolve the named
                      transport's serve future to Err (or early Ok) with
                      shutdown_requested=false so the serve task
                      self-reacts: serve_loop_failed + flip_to_failed +
                      exit 3. */)
}
```

- **Mirrors cinder.** `FailingFsyncBackend` makes `fsync` fail behind the
  real WAL boundary; `spawn_with_injected_serve_failure` makes the serve
  future fail behind the real (already-bound) listener boundary. Both are
  public dev-only test helpers (`aperture::testing`, ADR-0007), both keep
  the production reaction code real, both fail observably on the swallow.
- **Why a new public seam and not the hand-constructed-bundle seam alone.**
  ADR-0066 names two layered seams: the unit/exit-code seam (the
  hand-constructed `ShutdownBundle` at `lib.rs:379-430`, DELIVER's unit
  test) and this acceptance seam. The acceptance tests need a REAL spawned
  transport with REAL probeable listeners, which the in-process bundle
  alone cannot give; hence the spawn helper. `ServeOutcome` /
  `ReadinessPhase::Failed` / `ShutdownBundle` are `pub(crate)` and
  intentionally unreachable from `tests/`, so the seam is the only honest
  way to drive the boundary from the acceptance layer.
- **The binary-level trigger** (`APERTURE_TEST_INJECT_SERVE_FAILURE`) that
  `binary_exits_three_on_injected_serve_death` expects is the subprocess
  analogue: a test-only env var DELIVER wires inside `run` to drive the
  same injected death so the real binary's exit code can be read. DELIVER
  owns gating it to the test matrix only.

DELIVER replaces the `unimplemented!` body and the env-var trigger; it
does NOT change the test file's assertions. The seam's signature, the
`InjectServeFailure` variants, and the observable contract are locked by
DISTILL.

## Notes for downstream waves

- **DELIVER** (`nw-software-crafter`): only the crafter writes
  `crates/aperture/src/`. Implement the ADR-0066 ripple map; implement the
  `spawn_with_injected_serve_failure` body and the
  `APERTURE_TEST_INJECT_SERVE_FAILURE` binary trigger; un-ignore the 7 RED
  scenarios as each goes green (one at a time, the outer-loop sequence
  below); keep the graceful path byte-for-byte; keep the slice-08 suite
  green; 100% mutation kill (Gate 5) on the five modified files; add the
  `3` line to `main.rs`'s exit-code doc. Do NOT bump
  `crates/aperture/Cargo.toml` (stays `0.1.0`).
- **Outer-loop implementation sequence** (one scenario at a time): (1) the
  seam body + `grpc_serving_loop_death_after_bind_is_named_on_stderr`;
  (2) `http_serving_loop_death_after_bind_is_named_on_stderr` (the silent
  arm); (3) `dead_serving_loop_stops_reporting_ready_but_stays_alive`;
  (4) `readyz_failed_phase_is_sticky_and_never_flaps_back_to_ready`;
  (5) `early_ok_without_shutdown_request_is_treated_as_fatal`;
  (6) the binary trigger + `binary_exits_three_on_injected_serve_death`;
  (7) the SIGTERM fixture + `binary_exits_zero_and_silent_on_real_sigterm`.
  The two negative controls and the config-error exit-2 control are
  already green and act as guardrails throughout.
</content>
</invoke>
