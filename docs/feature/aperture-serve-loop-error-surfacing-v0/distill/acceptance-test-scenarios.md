# Acceptance Test Scenarios: aperture-serve-loop-error-surfacing-v0 (DISTILL)

Author: Quinn (nw-acceptance-designer). Wave: DISTILL. Date: 2026-06-05.
Mode: PROPOSE (autonomous overnight run). British English. No em dashes.

The executable scenarios live in
`crates/aperture/tests/serve_loop_error_surfacing.rs` (the driving-port
black-box suite). The injection seam is
`crates/aperture/src/testing.rs > spawn_with_injected_serve_failure`
(a RED `unimplemented!` scaffold; DELIVER implements the body). The
`[[test]]` block is in `crates/aperture/Cargo.toml`. This document is the
human-readable scenario catalogue, the AC trace, the adapter coverage
table, the error-path ratio, and the mandate self-review. The strategy,
falsifiability, `#[ignore]` evidence, and seam decisions are in the
sibling `wave-decisions.md`.

## Driving port (black-box)

The only thing the suite touches is the **running aperture instance**,
observed through:

- its **structured stderr** (`testing::stderr_capture` /
  `capture_stderr_events`), for the `serve_loop_failed` event vocabulary;
- its **`/readyz` + `/healthz`** probes over a real `reqwest` client;
- its **process exit code**, read from a real child process
  (`CARGO_BIN_EXE_aperture`).

No internal type is reached: `ServeOutcome`, `ReadinessPhase::Failed`, and
`ShutdownBundle` are `pub(crate)` and intentionally unreachable from the
`tests/` crate (Mandate 1, CM-A).

## Scenario catalogue (test fn -> AC -> tag)

10 scenarios across US-01 / US-02 / US-03. Tags: `@walking_skeleton`
(real-IO user-value E2E), `@driving_adapter` (real process boundary via
subprocess), `@ignore-RED` (behaviourally RED until DELIVER),
`negative-control` (PASSES today, guardrail).

| # | Test fn | US / AC | Asserts (observable) | Tag(s) | State today |
|---|---|---|---|---|---|
| 1 | `grpc_serving_loop_death_after_bind_is_named_on_stderr` | US-01 AC1 (KPI-2/5) | exactly one `serve_loop_failed transport=grpc error=<reason>` at `error` level | `@walking_skeleton` `@ignore-RED` | RED (panics at seam) |
| 2 | `http_serving_loop_death_after_bind_is_named_on_stderr` | US-01 AC2 / US-03 AC2 (KPI-5) | exactly one `serve_loop_failed transport=http` (the previously SILENT arm, proven by its own scenario) | `@walking_skeleton` `@ignore-RED` | RED (panics at seam) |
| 3 | `graceful_shutdown_emits_no_serve_loop_failed_event` | US-01 AC3 / US-03 AC1 (KPI-4) | the slice-08 drain sequence fires; NO `serve_loop_failed` line | `negative-control` | GREEN (guardrail) |
| 4 | `healthy_instance_reports_ready_and_alive` | US-02 AC1 (KPI-3) | `/readyz` 200 `"ready"`, `/healthz` 200 `"ok"` | `negative-control` | GREEN (guardrail) |
| 5 | `dead_serving_loop_stops_reporting_ready_but_stays_alive` | US-02 AC2 (KPI-3) | `/readyz` flips to 503 `"failed"`; `/healthz` stays 200 | `@walking_skeleton` `@ignore-RED` | RED (panics at seam) |
| 6 | `readyz_failed_phase_is_sticky_and_never_flaps_back_to_ready` | US-02 AC4 (KPI-3, `@property`) | `/readyz` stays 503 `"failed"` across repeated probes; never flaps to 200 | `@ignore-RED` `@property` | RED (panics at seam) |
| 7 | `early_ok_without_shutdown_request_is_treated_as_fatal` | US-03 AC3 (D3) | one `serve_loop_failed transport=grpc` for an unexpected early `Ok` (not-requested -> fatal) | `@ignore-RED` | RED (panics at seam) |
| 8 | `binary_preserves_config_error_exit_code_two` | US-02 AC3 (KPI-3) | the REAL binary exits `2` on a malformed config (the established 0/1/2 map preserved) | `@driving_adapter` | GREEN (guardrail) |
| 9 | `binary_exits_three_on_injected_serve_death` | US-02 AC3 (KPI-3) | the REAL binary exits `3` on an injected post-bind serve death, distinct from 0/1/2 | `@driving_adapter` `@ignore-RED` | RED (panics at exit-3 assertion) |
| 10 | `binary_exits_zero_and_silent_on_real_sigterm` | US-03 AC1 / AC4 (KPI-4) | the REAL binary exits `0` on SIGTERM with NO `serve_loop_failed` on stderr | `@driving_adapter` `negative-control` `@ignore-RED` | RED (pending SIGTERM fixture; behaviour already proven green in-process by #3) |

### US -> AC -> scenario coverage (every AC mapped)

- **US-01** (a serving loop that dies post-bind names the transport on
  stderr): AC1 -> #1; AC2 -> #2; AC3 (graceful negative control) -> #3;
  AC4 (single closed-vocabulary constant, same JSON shape) -> asserted by
  #1/#2 (the `event`/`transport`/`error` field shape at `error` level).
- **US-02** (a dead loop stops reporting healthy/ready): AC1 (healthy
  negative control) -> #4; AC2 (`/readyz` flips, `/healthz` stays) -> #5;
  AC3 (process reaction distinct from clean drain) -> #8 (exit-2 map
  preserved) + #9 (exit-3 on death); AC4 (zombie never presented as ready)
  -> #6 (sticky `Failed`).
- **US-03** (both arms covered, HTTP proven, no false alarm): AC1 (SIGTERM
  clean, no false alarm) -> #3 (in-process) + #10 (subprocess); AC2 (HTTP
  arm proven by its own scenario) -> #2; AC3 (graceful-vs-fatal, early-Ok
  per D3) -> #7; AC4 (slice-08 suite stays green) -> guarded by #3/#4/#10
  and carried to DELIVER as the slice-08 regression suite.

**Coverage verdict: all 3 stories and every AC have at least one
scenario.** No story or AC is uncovered (Dim 4 / Dim 8 Check A).

## Adapter coverage table (driving adapter = the running aperture)

The driving adapter is the running aperture instance, exercised through
two real-IO surfaces. There is no in-memory double standing in for the
boundary under test (Strategy C, real-local-IO; see `wave-decisions.md`).

| Driving-adapter surface | Real-IO mechanism | Scenarios exercising it | What it proves |
|---|---|---|---|
| stderr event vocabulary | `testing::stderr_capture` over the production `tracing` registry | #1, #2, #3, #7, #10 | the `serve_loop_failed` event is emitted (or, for controls, NOT emitted) with the right transport / level |
| `/readyz` + `/healthz` probes | real `reqwest` client over real loopback listeners (`127.0.0.1:0`) bound by the production spawn path | #4, #5, #6 | a dead loop flips `/readyz` to 503 `"failed"` (sticky) while `/healthz` stays 200 |
| process exit code | the REAL `aperture` binary as a child process (`CARGO_BIN_EXE_aperture`), real OS exit code | #8 (exit 2), #9 (exit 3), #10 (exit 0) | a supervisor reads a distinct exit code over the genuine process boundary |
| serve-failure trigger (the only fake) | `spawn_with_injected_serve_failure(.., InjectServeFailure::{Grpc,Http,GrpcEarlyOk})`, the `FailingFsyncBackend` analogue | #1, #2, #5, #6, #7 | the post-bind death is deterministic; everything downstream of the trigger is real production code |

**Real-IO audit (Dim 9c):** the serve-failure boundary is exercised with
real listeners + real subprocess in every applicable scenario; no
`@in-memory` tag appears on any walking-skeleton or driving-adapter
scenario. `RecordingSink` (in-memory `OtlpSink`) appears only as the
orthogonal data-plane sink, never as a stand-in for the serve-failure
boundary. Litmus (Dim 9d): "if the real listener / real binary were
deleted, would the WS still pass?" -> No; the probes and the exit codes
would have nothing to read. The WS tests wiring, not a double.

## Error-path ratio (>=40% required)

Classified by whether the scenario EXERCISES a serve death/failure
(error path) or asserts the healthy / graceful guardrail (negative
control).

- **Error-path scenarios (6):** #1 grpc death, #2 http death, #5 dead loop
  stops ready, #6 sticky-failed, #7 early-Ok-fatal, #9 binary exit-3 on
  death.
- **Negative controls / guardrails (4):** #3 graceful drain (no event),
  #4 healthy reports ready, #8 binary config-error exit-2, #10 binary
  SIGTERM exit-0.

**Error-path ratio = 6 / 10 = 60%** (>= 40%, PASS). The feature is a
failure-surfacing feature; the majority weighting on error paths is
correct and the negative controls are exactly the false-alarm / regression
guards DISCUSS and DEVOPS require.

## Self-review checklist (Mandate 7 / driving-adapter / falsifiable-substrate)

| Item | Status | Evidence |
|---|---|---|
| **Mandate 7 RED-not-BROKEN** | tick | suite compiles (`--no-run` builds it); default run `3 passed; 0 failed; 7 ignored` (negative controls GREEN); `--ignored` run `7 failed` via clean panics (5 at `unimplemented!` seam `testing.rs:204`, 2 at behavioural subprocess assertions), never compile errors |
| **No Fixture Theater** | tick | Given steps set up PRECONDITIONS (ephemeral-port config, injected death trigger) only; the EXPECTED OUTPUT (the event, the 503, the exit 3) is never pre-seeded; each RED test fails on the swallow because the output is absent, so it cannot pass without DELIVER's production code |
| **Falsifiable against the present swallow** | tick | every RED scenario asserts an observable (`serve_loop_failed` stderr line / `/readyz` 503 `"failed"` / `/healthz` 200 / exit 3) the `let _ = ...await` swallow cannot produce; the falsifiability table is in `wave-decisions.md` |
| **Driving-port only (Mandate 1, CM-A)** | tick | imports are only `aperture::config`, `aperture::ports`, `aperture::testing`, `aperture::spawn`, `aperture::Handle`; no `ServeOutcome` / `ReadinessPhase` / `ShutdownBundle` (all `pub(crate)`, unreachable from `tests/`) |
| **Driving-adapter real-process scenario** | tick | #8/#9/#10 run the REAL `aperture` binary as a child via `CARGO_BIN_EXE_aperture` and read the real OS exit code; the in-process harness cannot produce an exit code, so the subprocess boundary is genuinely exercised |
| **Business-language purity (Mandate 2, CM-B)** | tick | scenario titles and asserts speak operator language (a serving loop dies, names the transport on stderr, stops reporting ready, exits with a distinct code); `transport=grpc`/`http`, `/readyz`, exit codes are the operator's literal contract surface (the structured stderr vocabulary + probe + exit code ARE the operator UI), not leaked implementation jargon |
| **Walking-skeleton user-centricity (Dim 5)** | tick | WS titles describe the operator goal ("a serving loop that dies after bind is named on stderr", "a dead serving loop stops reporting ready but stays alive"), not layer plumbing; Then steps assert operator observations (the stderr line, the 503, the exit code) |
| **Observable-behaviour assertions (Dim 7)** | tick | every Then checks a return value from the driving port (HTTP status/body via `reqwest`, captured stderr events, process exit code); no assertion touches private fields, mock call counts, or `pub(crate)` internal state |
| **Coverage + traceability (Dim 4 / Dim 8)** | tick | all 3 stories and every AC mapped (table above); environments `clean` + `with-pre-commit` + `ci` (DEVOPS `environments.yaml`) are boolean/exit-code assertions with no wall-clock threshold, so they run identically in the pre-commit hook and CI Gate 1 (C-DEVOPS-3) |
| **Happy-path bias (Dim 1)** | tick | error-path ratio 60% (6/10); the negative controls are deliberate false-alarm / regression guards, not happy-path padding |
| **Regression guards green** | tick | #3 (graceful, no event), #4 (healthy ready), #8 (config exit-2) PASS today; trunk stays green; the slice-08 suite is carried to DELIVER as the graceful-drain guardrail (C-DEVOPS-5) |

**Self-review verdict: all items ticked.** The suite is RED-not-BROKEN,
falsifiable, driving-port-only, real-IO at the boundary under test, and
business-language-pure, with a 60% error-path ratio and full US/AC
traceability.
</content>
