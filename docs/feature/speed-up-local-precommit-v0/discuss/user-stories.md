<!-- markdownlint-disable MD024 -->

# User Stories — speed-up-local-precommit-v0

## Persona

**Devon, the committing maintainer** (stand-in for both Andrea committing
by hand and the nWave crafter agent committing on his behalf). Devon works
in short, frequent loops: edit a crate, stage, `git commit`. Devon wants
`main` to stay socially green and does NOT want to reach for
`git commit --no-verify`. Today every commit triggers
`cargo test --workspace --all-targets --locked`, which runs the fsync-heavy
durability suites (lumen/ray/strata/cinder/sluice/beacon/pulse
`v1_slice_0x` WAL/snapshot/torn-tail tests, the `*_crash_target` tests) and
the aperture/cli subprocess tests — 10-20 minutes per commit, and once a
wedged hook ran for hours under leaked-process contention. That wait trains
Devon to bypass the hook.

## The Job (JTBD, supplied verbatim by Andrea)

> When I commit locally, the pre-commit gate finishes in about 5 minutes or
> less so I keep my flow, while the deep, slow, I/O-bound tests run in CI
> where the time is not mine to wait on — and I watch CI results at a
> regular cadence so the deep coverage still has eyes and a real failure is
> caught quickly, rather than a 10-20 minute local wait on every commit
> that trains me to reach for `--no-verify`.

Forces:

- **Push** (frustration): every commit costs 10-20 min; a wedged hook once
  ran for hours.
- **Pull** (attraction): a commit that gates in <= 5 min, deep coverage
  still enforced somewhere.
- **Anxiety** (fear of the new): "if the deep tests leave the commit path,
  a deep regression slips into main unnoticed."
- **Habit** (inertia): the long wait already trains Devon toward
  `--no-verify`, which silently drops ALL gates.

## System Constraints (cross-cutting)

- CI gate-1 (`cargo test --workspace --all-targets --locked`,
  `.github/workflows/ci.yml:182`) MUST remain unchanged — it is the
  authoritative deep gate. No test is deleted; CI is not weakened.
- Kaleidoscope is pure trunk-based, no required-status-checks; "CI is
  feedback, not a gate". The local hook is a courtesy, so moving the slow
  tests' GATING to CI is posture-consistent.
- The fast local subset MUST still fail on the cheap/common mistakes:
  compile errors, unit-test breaks, fmt, clippy.
- Exact subset, clippy scope, and watching mechanism are DESIGN/DEVOPS
  decisions (see `wave-decisions.md` D1-D6). Stories below state the
  observable outcome, not the implementation.
- No crate version change; never 1.0.0.

---

## US-01: The local commit gate finishes in five minutes or less

### Problem

Devon commits many times a day. Every `git commit` runs
`cargo test --workspace --all-targets --locked`, whose durability and
subprocess suites are I/O-bound and take 10-20 minutes. The wait breaks
Devon's flow and pressures him toward `--no-verify`.

### Who

- Devon, the committing maintainer (human or crafter agent) | working in
  short edit-commit loops | motivated to keep `main` green without losing
  flow.

### Elevator Pitch

- **Before**: `git commit` runs `cargo test --workspace --all-targets
  --locked` and the developer waits 10-20 min (sometimes hours when wedged)
  before the commit is created.
- **After**: `git commit` runs the toolchain check, `cargo fmt --check`,
  `cargo clippy`, `cargo deny`, and a FAST test subset, prints
  `[pass] all pre-commit gates green`, and the commit is created in <= 5 min.
- **Decision enabled**: Devon decides to keep committing in-flow rather
  than reaching for `--no-verify`, because the gate no longer costs him a
  coffee break.

### Solution

Slim the local `scripts/hooks/pre-commit` so its test step runs a fast
subset (DESIGN picks the exact subset, D1 — recommended `cargo test
--workspace --lib`) instead of the full `--all-targets` run. Keep the
toolchain check, fmt, clippy, and deny. The full deep run stays in CI.

### Domain Examples

#### 1: Happy Path — Devon edits a pure function in `crates/codex`

Devon fixes a serialization helper in `crates/codex/src/lib.rs`, stages it,
and runs `git commit -m "fix(codex): trim trailing newline"`. The hook
runs toolchain check + fmt + clippy + deny + the fast test subset, prints
`[pass] all pre-commit gates green`, and creates the commit in under 5
minutes (vs ~14 min before, dominated by the lumen/pulse durability suites
he never touched).

#### 2: Edge Case — Devon touches a durability crate

Devon edits `crates/lumen/src/wal.rs`. The hook still runs the fast subset
in <= 5 min and creates the commit. The deep durability tests
(`lumen/tests/v1_slice_01_wal_durability.rs`,
`v1_slice_03_torn_tail_recovery.rs`) do NOT run locally; they run in CI
gate-1. Devon relies on the CI-watching cadence (US-04) to confirm the
deep suite stayed green for his lumen change.

#### 3: Error/Boundary — the hook would exceed 5 minutes

On Devon's machine under heavy parallel load, the slimmed hook completes in
roughly 3-4 minutes; even with clippy `--all-targets` it stays under the
5-minute bar. If a future change to the subset pushed it back over 5 min,
that is a regression against this story's AC and would be caught by the
US-01 timing check.

### UAT Scenarios (BDD)

#### Scenario: A commit that touches only fast code gates quickly

```gherkin
Given Devon has edited a pure function in crates/codex
And he has staged the change
When he runs git commit
Then the pre-commit hook runs the toolchain check, fmt, clippy, deny, and a fast test subset
And the hook completes in 5 minutes or less
And the commit is created
```

#### Scenario: A commit touching a durability crate still gates quickly

```gherkin
Given Devon has edited crates/lumen/src/wal.rs
And he has staged the change
When he runs git commit
Then the pre-commit hook does not run the fsync-heavy durability test binaries
And the hook completes in 5 minutes or less
And the commit is created
```

#### Scenario: The fast hook does not run the slow integration and subprocess suites

```gherkin
Given the slimmed pre-commit hook is installed
When the hook's test step runs
Then it does not execute the durability, snapshot, torn-tail, or subprocess test binaries locally
And those suites remain runnable in CI gate-1
```

### Acceptance Criteria

- [ ] `the-local-hook-finishes-under-5-minutes`: a `git commit` (any
  staged change) completes the full pre-commit hook in <= 5 minutes on a
  developer machine under normal load.
- [ ] The hook still runs the toolchain check, `cargo fmt --check`,
  `cargo clippy`, and `cargo deny`.
- [ ] The hook's test step does NOT execute the fsync-heavy durability,
  snapshot, torn-tail, or subprocess test binaries.

### Outcome KPIs

- **Who**: the committing maintainer (Devon / crafter agent)
- **Does what**: completes the pre-commit hook
- **By how much**: p95 wall-clock <= 5 min (from 10-20 min baseline)
- **Measured by**: timing the hook (`time` around the hook, or the hook
  printing its own elapsed) across a sample of real commits
- **Baseline**: 10-20 min (full `--all-targets --workspace` run)

### Technical Notes

- Exact fast subset is DESIGN D1 (recommended `cargo test --workspace
  --lib`). DESIGN MUST measure and confirm <= 5 min.
- Clippy scope is DESIGN D2 (recommended keep `--all-targets --locked`
  unless measured to blow the budget).

---

## US-02: The fast subset still catches the cheap, common mistakes

### Problem

A fast hook is worthless if it stops catching the everyday errors the hook
exists for. Devon needs the slimmed hook to still fail on a broken unit
test, a formatting drift, or a clippy lint — the mistakes that are cheap to
make and cheap to catch — so the courtesy gate still does its job.

### Who

- Devon, the committing maintainer | who relies on the hook to stop the
  obvious breakage before it reaches `main` | motivated to keep `main`
  socially green.

### Elevator Pitch

- **Before**: the hook catches unit/fmt/clippy failures, but only after a
  10-20 min full-suite wait.
- **After**: `git commit` still fails fast with `[fail] cargo test` /
  `[fail] cargo fmt --check` / `[fail] clippy` when a unit test, format,
  or lint is broken — within the same <= 5 min window.
- **Decision enabled**: Devon trusts the fast hook enough to keep using it
  (not `--no-verify`), because it still red-flags his own mistakes before
  the commit lands.

### Solution

The slimmed hook keeps fmt, clippy, and deny unchanged, and its fast test
subset (unit tests) still fails the commit on a broken unit test. Only the
slow integration/durability/subprocess gating moves to CI.

### Domain Examples

#### 1: Happy Path — a broken unit test fails the commit

Devon introduces an off-by-one in a `#[cfg(test)]` unit test's subject in
`crates/sieve/src/lib.rs`. `git commit` runs the fast subset, the unit
test fails, the hook prints `[fail] cargo test`, and the commit is
rejected — in under 5 minutes.

#### 2: Edge Case — a formatting drift fails the commit

Devon leaves an unformatted block in `crates/pulse/src/query.rs`.
`git commit` runs `cargo fmt --all -- --check`, the hook prints
`[fail] cargo fmt --check` with `run: cargo fmt --all`, and rejects the
commit before it ever reaches the test step.

#### 3: Error/Boundary — a clippy lint fails the commit

Devon writes `let _ = x.clone();` where clippy flags a redundant clone in
`crates/ray/src/lib.rs`. `git commit` runs
`cargo clippy --all-targets --locked -- -D warnings`, the hook prints
`[fail] cargo clippy`, and rejects the commit.

### UAT Scenarios (BDD)

#### Scenario: A broken unit test fails the fast local hook

```gherkin
Given Devon has broken a unit test in crates/sieve
And he has staged the change
When he runs git commit
Then the fast test subset runs the broken unit test
And the hook fails with a cargo test failure
And the commit is not created
```

#### Scenario: A formatting drift fails the fast local hook

```gherkin
Given Devon has left unformatted code in crates/pulse
When he runs git commit
Then the hook fails the cargo fmt check
And tells Devon to run cargo fmt --all
And the commit is not created
```

#### Scenario: A clippy lint fails the fast local hook

```gherkin
Given Devon has written code that triggers a clippy warning in crates/ray
When he runs git commit
Then the hook fails the clippy gate
And the commit is not created
```

### Acceptance Criteria

- [ ] `the-fast-subset-still-catches-unit-fmt-clippy-failures`: a broken
  unit test, a formatting drift, and a clippy warning each fail the
  slimmed hook and reject the commit.
- [ ] The fmt, clippy, and deny gates behave identically to today (only
  the test scope changes).

### Outcome KPIs

- **Who**: the committing maintainer
- **Does what**: has cheap/common mistakes (unit break, fmt, clippy)
  caught by the local hook
- **By how much**: 100% of unit/fmt/clippy failures still caught locally
  (no regression in cheap-mistake detection)
- **Measured by**: the negative-control scenarios above (inject each
  failure class, confirm the hook rejects)
- **Baseline**: 100% caught today (but at 10-20 min cost)

### Technical Notes

- The fast subset MUST include unit tests so a unit-test break still fails
  the hook (rules out a subset that runs zero tests).

---

## US-03: The deep suite still runs in CI as the authoritative gate

### Problem

Moving the slow tests off the local blocking path is only safe if those
tests still run SOMEWHERE that gates `main`'s health. Devon needs proof
that the deep suite (durability, snapshot, torn-tail, subprocess,
integration) is still executed on every push — in CI — so removing them
locally does not remove the coverage.

### Who

- Devon, the committing maintainer | who is trading local wait for CI
  coverage | motivated to know the deep coverage did not silently vanish.

### Elevator Pitch

- **Before**: the deep suite runs both locally (blocking, slow) and in CI
  (gate-1) — duplicated.
- **After**: the deep suite runs in CI gate-1 on every push and PR
  (`cargo test --workspace --all-targets --locked`, unchanged), and the
  CI run's result is the authoritative deep verdict the developer reads on
  the Actions page / via `gh run view`.
- **Decision enabled**: Devon decides it is safe to slim the local hook,
  because the same deep invocation he removed locally is demonstrably still
  enforced in CI.

### Solution

Leave `.github/workflows/ci.yml` `gate-1-test` exactly as-is
(`cargo test --workspace --all-targets --locked`). This story is a
verification/honesty story: it asserts the deep gate is untouched and is
the new single home for deep gating.

### Domain Examples

#### 1: Happy Path — a clean push shows deep gate green in CI

Devon pushes his codex fix. CI gate-1 runs the full
`cargo test --workspace --all-targets --locked`, including the lumen/pulse
durability suites, and reports green on the Actions page.

#### 2: Edge Case — a durability change is exercised only in CI

Devon's lumen WAL change passed the fast local hook (durability tests
skipped locally). On push, CI gate-1 runs
`lumen/tests/v1_slice_01_wal_durability.rs` and the torn-tail recovery
test, confirming the change is durability-safe — coverage that used to run
locally now runs in CI.

#### 3: Error/Boundary — a deep-only regression reds CI gate-1

Devon's change subtly breaks `pulse/tests/v1_slice_05_torn_tail_recovery.rs`,
which the fast local hook did not run. The push goes through, but CI
gate-1 fails on that test and shows a red X on the Actions page — exactly
the case US-04's cadence is designed to surface quickly.

### UAT Scenarios (BDD)

#### Scenario: The deep suite runs in CI on every push

```gherkin
Given the CI workflow gate-1-test is configured
When a commit is pushed to main
Then CI runs cargo test --workspace --all-targets --locked
And that run includes the durability, snapshot, torn-tail, subprocess, and integration suites
```

#### Scenario: The deep CI gate is unchanged by this feature

```gherkin
Given this feature slims the local pre-commit hook
When the change is reviewed
Then .github/workflows/ci.yml gate-1-test still runs cargo test --workspace --all-targets --locked
And no test is deleted from any crate
```

#### Scenario: A deep-only regression is caught by CI, not the fast local hook (negative control)

```gherkin
Given a commit subtly breaks pulse/tests/v1_slice_05_torn_tail_recovery.rs
And the fast local hook does not run that durability test
When the commit is pushed to main
Then the fast local hook still created the commit
And CI gate-1 fails on the torn-tail recovery test
And the failure is visible on the Actions page
```

### Acceptance Criteria

- [ ] `the-deep-suite-still-runs-in-CI`: CI gate-1
  (`cargo test --workspace --all-targets --locked`) runs on every push and
  PR and is unchanged by this feature.
- [ ] No test file is deleted from any crate; CI is not weakened.

### Outcome KPIs

- **Who**: the committing maintainer (relying on CI for deep coverage)
- **Does what**: retains deep-suite coverage on every push
- **By how much**: 100% of the deep suite still executed in CI (zero
  coverage moved off-platform; only the LOCAL gating removed)
- **Measured by**: CI gate-1 invocation diff (must remain
  `cargo test --workspace --all-targets --locked`) + crate test-file count
  (no deletions)
- **Baseline**: deep suite runs in CI today (and redundantly locally)

### Technical Notes

- This story constrains the feature to NOT touch ci.yml gate-1. A reviewer
  or DESIGN diff check enforces it.

---

## US-04: A CI-results-watching cadence is established

### Problem

Once the deep tests leave the local blocking path, nobody is forced to wait
for them — so a deep-only regression could sit on `main` unnoticed. Devon
needs a concrete, low-friction way to watch CI results at a regular cadence
so a deep failure is caught quickly, rather than discovered days later. The
cadence is the mitigation that makes moving the deep tests off the local
path honest.

### Who

- Devon, the committing maintainer (human or agent) | who pushes to main
  and must keep it green | motivated to catch a deep-only regression in
  minutes/hours, not days.

### Elevator Pitch

- **Before**: deep tests gate locally, so the developer always "sees" them
  (by waiting); there is no separate CI-watching habit because the local
  wait did the watching.
- **After**: the developer/agent runs one command
  (e.g. `scripts/ci-watch.sh`, wrapping `gh run list --branch main` /
  `gh run view`) that prints the latest main CI run's status, on a
  documented cadence (e.g. after every push, plus a periodic poll while
  the agent works).
- **Decision enabled**: Devon decides whether `main` is healthy and
  whether to drop everything and fix-forward, based on the at-a-glance CI
  status the command prints — replacing the lost local-wait signal.

### Solution

DESIGN/DEVOPS deliver a concrete watching mechanism (D3 — recommended a
small `scripts/ci-watch.sh` over `gh`, plus a documented cadence in
CLAUDE.md / brief) and document the honesty trade (D5) that a deep-only
regression can reach main and is caught by this cadence rather than by a
local block.

### Domain Examples

#### 1: Happy Path — Devon checks CI after a push and sees green

After pushing his codex fix, Devon runs `scripts/ci-watch.sh`, which prints
the latest `main` run as `gate-1-test: success` (with the run URL). Devon
moves on, confident main is green.

#### 2: Edge Case — the agent polls mid-task and sees an in-progress run

The crafter agent, working on the next slice, runs the watch command on
its documented cadence. It prints `gate-1-test: in_progress` for the prior
push. The agent notes it and re-checks on the next cadence tick rather than
assuming green.

#### 3: Error/Boundary — a deep-only failure is surfaced by the cadence

Devon's torn-tail regression (US-03 example 3) reds CI gate-1. On his next
cadence check, `scripts/ci-watch.sh` prints
`gate-1-test: failure` with the failing run URL. Devon opens the run, sees
`pulse v1_slice_05_torn_tail_recovery` failed, and fixes forward — caught
within one cadence interval, not days.

### UAT Scenarios (BDD)

#### Scenario: The watch command reports the latest main CI status

```gherkin
Given the CI-watching mechanism is installed
When Devon runs the watch command after a push
Then it prints the latest main CI run status and its URL
And Devon can tell at a glance whether gate-1 passed, failed, or is running
```

#### Scenario: A deep-only failure is surfaced by the cadence

```gherkin
Given a pushed commit reds CI gate-1 on a deep test the local hook did not run
When Devon runs the watch command on the documented cadence
Then it reports the failed run with a link to the failure
And Devon can begin a fix-forward
```

#### Scenario: The cadence and the honesty trade are documented

```gherkin
Given the local hook no longer blocks on deep tests
When a developer reads the project guidance
Then a documented cadence for watching CI results is present
And the trade (a deep-only regression can reach main and is caught by CI plus the cadence) is stated explicitly
```

### Acceptance Criteria

- [ ] `a-CI-results-watching-cadence-is-established`: a concrete,
  low-friction mechanism (a command/script) reports the latest `main` CI
  run status and URL, and a documented cadence for running it exists.
- [ ] The honesty trade (deep-only regression can reach main, caught by CI
  + the cadence under the trunk-based posture) is documented explicitly.

### Outcome KPIs

- **Who**: the committing maintainer (human or agent)
- **Does what**: checks CI results on a regular cadence after moving deep
  tests off the local path
- **By how much**: deep-only regressions detected within one cadence
  interval (target: same working session / < 1 hour), not days
- **Measured by**: presence of the watch command + documented cadence;
  time-to-detection on the next observed deep-only CI failure
- **Baseline**: today there is no separate cadence (the local wait was the
  de facto watch); deep-only regression detection latency is unbounded
  once the local block is removed without this mitigation

### Technical Notes

- Mechanism + cadence are DESIGN/DEVOPS D3. Recommended a `gh`-based
  `scripts/ci-watch.sh` plus a CLAUDE.md / brief cadence note.
- The honesty trade is DESIGN D5 (an ADR is the natural home, consistent
  with ADR-0070).
