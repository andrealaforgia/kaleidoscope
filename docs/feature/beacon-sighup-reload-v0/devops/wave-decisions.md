# Wave Decisions — beacon-sighup-reload-v0 (DEVOPS, SLIM)

British English throughout, no em dashes.

> **Author**: Apex (`nw-platform-architect`), DEVOPS wave, 2026-06-05.
> **Mode**: SLIM. The feature adds a SIGHUP reload to the beacon-server
> binary (signal handler + atomic catalogue swap, `main.rs`) plus ONE new
> inherent `InhibitionResolver` constructor (`inhibition.rs`). No new
> crate. No new CLI or HTTP surface.
> **Governing design**: ADR-0063 (single-orchestrator atomic swap; the
> `beacon.reload.succeeded` / `beacon.reload.refused` events;
> malformed-reload-keeps-previous safety AC) and the DESIGN feature digest
> (`../design/wave-decisions.md`). Stories US-01 (apply-via-SIGHUP) and
> US-02 (refuse-malformed-keep-previous) (`../discuss/user-stories.md`).

## nWave-order reminder (read before reviewing)

The nWave order is **DISCUSS -> DESIGN -> DEVOPS -> DISTILL -> DELIVER**.
This is the **DEVOPS** wave; it runs **before** DISTILL and DELIVER. The
proving tests and the production code **do not exist yet**, and that is the
**expected, correct** state at this wave. Absent code/tests are NOT a
defect here. The DEVOPS job is to confirm the CI/infra is ready and to
specify the proving-test environment for DISTILL/DELIVER to build. A
reviewer must not reject this wave for "missing tests" or "missing
implementation".

## Headline

**Existing gates cover the change; no new CI job is required.** The only
real DEVOPS point is the SIGHUP-reload proving-test **determinism**: it is
**event-synchronised, not a wall-clock p95**.

## Decision 1 — CI delta: no new crate, no new job

No new crate is introduced, so no new CI job is needed. The two changed
production files are already covered by existing path-filtered, `--in-diff`
Gate 5 mutation jobs, and the new in-suite SIGHUP proving test runs under
the existing Gate 1 `cargo test --workspace`.

| Changed file | Existing gate that covers it | ci.yml citation |
|---|---|---|
| `crates/beacon-server/src/main.rs` (SIGHUP arm + reload orchestrator) | `gate-5-mutants-beacon-server` | job at **ci.yml:2166**; `--in-diff` on `'crates/beacon-server/**'` at **ci.yml:2220**; `cargo mutants --package beacon-server --in-diff` at **ci.yml:2228-2232** (the `--in-diff` flag at **2231**) |
| `crates/beacon/src/inhibition.rs` (new `rebuild_from` constructor) | `gate-5-mutants-beacon` | job at **ci.yml:1637**; `--in-diff` on `'crates/beacon/**'` at **ci.yml:1696**; `cargo mutants --package beacon --in-diff` at **ci.yml:1704-1708** (the `--in-diff` flag at **1706**) |
| the new SIGHUP reload proving test (a `crates/beacon-server/tests/*.rs` target) | `gate-1-test` | `cargo test --workspace --all-targets --locked` at **ci.yml:184** (job at **ci.yml:136**) |

Both Gate 5 beacon jobs use the standard baseline cascade
(`origin/main` -> `HEAD~1` -> full) and short-circuit to a zero-second exit
on an empty diff, so a commit touching only these two crates runs exactly
these two mutation jobs and skips every sibling crate's job. The
per-feature **100% kill rate** on the two modified production files is the
enforcement (CLAUDE.md; ADR-0005 Gate 5; ADR-0063 "Enforcement"). No new
job, no edit to any sibling job, no edit to the `gate-1-test` invocation.

Project posture: pure trunk-based, **no required status checks** (CI is
feedback, not a gate). CLAUDE.md is not rewritten by this wave.

## Decision 2 — THE determinism concern (the one real DEVOPS point)

The SIGHUP reload proving test must spawn `beacon-server`, edit the rules
dir, send SIGHUP, and assert two things:

- **(US-01)** a newly-added rule begins firing after the reload;
- **(US-02)** a malformed reload keeps the previous catalogue, emits a
  refusal event, and does not crash.

This couples a **signal** + an **evaluation interval** + a **sink
observation**. The project carries an overnight **p95-flake class** on
lumen/pulse wall-clock KPI tests (MEMORY: `project_p95_wallclock_flakes_
overnight`). This proving test must **not** join that class. The
determinism discipline for DISTILL/DELIVER:

1. **Synchronise on the structured reload EVENT as the happen-before
   anchor.** The design names two stable events on beacon-server's existing
   `tracing` stream: `beacon.reload.succeeded` (INFO) and
   `beacon.reload.refused` (WARN) (ADR-0063 "Observables"; DESIGN digest
   "Observables"). The test waits for the relevant event on the spawned
   child's stderr **before** asserting downstream observables. The event is
   the happen-before point, not a fixed sleep after `kill -HUP`.

2. **Observe by polling under a generous bound, not by a timed wait.** Poll
   the webhook/sink/stderr for the awaited observable (the new rule's
   Firing incident at the sink for US-01; the refusal event + still-alive
   process + unchanged `since` for US-02) and return as soon as it appears.
   This is the slice-07 / crash-target `spawn_until_settled` shape and the
   verifier's B02 mock-sink pattern. Fail only if the generous upper bound
   elapses with the observable absent.

3. **Drive a short evaluation interval for SPEED, not as the assertion
   threshold.** The per-rule `interval` is a TOML field
   (`crates/beacon/src/loader.rs:274-275`; `main.rs:223` does
   `tokio::time::interval(rule.interval)`), so the test seeds its rule files
   with a short `interval` to make the test fast. The short interval makes
   the awaited observable arrive quickly; it is never the thing asserted.

4. **The assertion form is presence-under-a-bound, never p95.** The
   assertion must be "the awaited event/firing **was observed**" (a
   boolean presence under a generous bound), and **never** "it happened
   within X ms p95". No percentile, no latency budget, no wall-clock
   threshold appears in this test. This is the explicit guard against the
   overnight flake class.

## Decision 3 — Portability of the signal + subprocess test

The test runs on **Linux** (CI ubuntu-latest) and **macOS** (local
pre-commit/pre-push hook). POSIX **SIGHUP is portable across both**, so the
test is **not** OS-gated. It sends SIGHUP to the spawned child **by pid**
(`std::process::Child::id()` plus a `nix`/`libc` `kill`, or a signal
crate); the exact Rust surface is the DELIVER crafter's to choose. The
real-subprocess + signal + sink shape runs identically in both substrates.

**Writable-rules-dir wrinkle (verifier-flagged).** beacon-server persists
durable state at `<rules>/.beacon-state/store` (`main.rs:107`). The test
must therefore own a **fresh, writable tmp rules directory** (for example a
`tempfile::tempdir`) seeded with the initial rule files; it edits files in
that same directory before sending SIGHUP, and the durable store writes
underneath it. A read-only or shared rules dir would break both the
edit-then-SIGHUP step and the store. The tempdir is dropped at test end,
which is also the clean target.

## Decision 4 — Coexistence

This feature lands alongside the in-flight `gate-5-mutants-batch-v0` work
and the recently-closed `log-body-regex-search-v0`. Because the two beacon
Gate 5 jobs are path-filtered and `--in-diff`, a commit touching only
`crates/beacon-server/**` and `crates/beacon/**` triggers exactly those two
mutation runs and skips every sibling crate's job at zero cost. The SIGHUP
proving test is a new `tests/*.rs` target under Gate 1's existing
`--workspace` run and coexists with the `smoke.rs` wiremock tests already
present in `crates/beacon-server/tests/`.

## Deliverables

- `docs/feature/beacon-sighup-reload-v0/devops/environments.yaml` — slim:
  clean + ci; a real-subprocess + signal + sink proving test; tmp writable
  rules dir; coexistence note.
- `docs/feature/beacon-sighup-reload-v0/devops/wave-decisions.md` — this
  file.

## Quality gates (DEVOPS)

- [x] CI delta resolved: no new crate -> no new job; existing gates cited
      by ci.yml line.
- [x] The proving-test environment is specified (real subprocess + POSIX
      signal + mock backend + sink catcher; tmp writable rules dir).
- [x] Determinism discipline pinned: event-synchronised happen-before,
      poll-under-bound, short-interval-for-speed, presence-not-p95.
- [x] Portability confirmed: POSIX SIGHUP on Linux and macOS; signal to
      child by pid.
- [x] Rollback posture: pure trunk-based, fix-forward or git-revert; no
      required checks; no daemon image to roll back.
- [x] Mutation enforcement: per-feature 100% on the two modified
      production files via the two existing beacon Gate 5 jobs.
- [x] nWave order honoured: DEVOPS before DISTILL/DELIVER; absent
      code/tests are the expected state.

## Peer review

`@nw-platform-architect-reviewer` dispatch was attempted (see "Self-review"
below; the Task-tool reviewer is not invocable from this subagent context).
A rigorous structured self-review was performed in lieu, and a top-level
`@nw-platform-architect-reviewer` run is FLAGGED for the parent to dispatch
before DISTILL, INCLUDING the nWave-order reminder above so the reviewer
does not reject on a wave-ordering misunderstanding.

### Self-review (structured)

- **Pipeline soundness**: PASS. No new job is the correct call; both
  changed crates are covered by existing path-filtered `--in-diff` Gate 5
  jobs, and the new subprocess test runs under Gate 1 `--workspace`. Lines
  cited and verified against ci.yml.
- **Infrastructure**: PASS. No network, no image, no cloud target. The
  only side-effecting dependency is the local filesystem (rules dir +
  durable store) plus POSIX signals; both are reflected in
  environments.yaml `driven-dependencies`.
- **Deployment readiness / rollback**: PASS. Pure trunk-based,
  fix-forward or git-revert; no daemon image is shipped, so there is no
  rollout to roll back. Rollback is repository-level.
- **Observability**: PASS for this feature's scope. The two named
  structured events are the operator-visible observables (ADR-0063) and
  are also the test's happen-before anchor; no new dashboard/SLO is in
  scope for a signal-handler feature.
- **Determinism (primary risk)**: PASS. The discipline forbids p95/
  wall-clock assertions and mandates event-synchronised presence-under-
  bound, directly addressing the overnight flake class.
- **Handoff completeness**: PASS. environments.yaml + wave-decisions.md
  give DISTILL/DELIVER the proving-test shape, the determinism rules, the
  portability constraint, and the writable-tmp-dir wrinkle.
- **nWave order**: PASS. Stated explicitly so the absent code/tests are
  not mistaken for a defect.

Residual: none blocking. The exact Rust signal surface (nix vs libc vs a
signal crate) is deliberately left to the DELIVER crafter, consistent with
the project rule that only the crafter writes production source.

## Changelog

- 2026-06-05: DEVOPS wave (SLIM) authored. Confirmed no new CI job
  (existing `gate-5-mutants-beacon-server`, `gate-5-mutants-beacon`, and
  `gate-1-test` cover the change; ci.yml lines cited). Pinned the SIGHUP
  proving-test determinism discipline (event-synchronised, not p95).
  Confirmed POSIX-SIGHUP portability across Linux and macOS and the
  writable-tmp-rules-dir wrinkle. Authored environments.yaml. Flagged a
  top-level reviewer run with the nWave-order reminder.

## Peer review outcome

Self-review passed; an independent top-level nw-platform-architect-reviewer
was then run (with the nWave-order reminder). Verdict: CONDITIONALLY
APPROVED, 0 blockers. It verified every cited gate at its line
(gate-5-mutants-beacon-server ci.yml:2166 --in-diff :2231;
gate-5-mutants-beacon ci.yml:1637 --in-diff :1706; gate-1-test :184),
confirmed no new CI job is needed, and judged the determinism discipline
sound (event-synchronised on beacon.reload.succeeded/refused,
poll-under-bound, short interval for speed not assertion, presence not
p95). No methodology error this run.

Three conditions carried into the DISTILL/DELIVER handoff (none a DEVOPS
defect; all proving-test implementation discipline):

1. The SIGHUP proving test synchronises on the structured reload EVENT
   (presence under a generous bound), NEVER a fixed sleep / wall-clock
   p95 after kill -HUP. (Already this wave's discipline; reaffirmed.)
2. ADD an explicit state-carryover assertion: a rule that was Firing
   before the reload, with a specific `since`, is STILL Firing with the
   SAME `since` after a successful reload (no spurious re-page, no reset),
   and likewise after a REFUSED reload. This exercises
   InhibitionResolver::rebuild_from + the name-matching carryover
   (ADR-0063 sub-decisions 2/3) rather than only "a new rule fires".
3. The SIGHUP subprocess test is POSIX-only; gate it `#[cfg(unix)]` (or a
   documented skip) so a future Windows CI does not fail on the absent
   signal. Document the Unix-only boundary in the test.

DISTILL writes the acceptance test to these; DELIVER honours the
sub-decision-4 ordering invariant (build+validate new before touching
old) in a code comment + test.
