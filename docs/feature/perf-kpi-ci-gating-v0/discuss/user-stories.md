<!-- markdownlint-disable MD024 -->

# User Stories: perf-kpi-ci-gating-v0

Feature type: Infrastructure (cross-cutting test-infra). This feature changes
test files, the CI workflow, and documentation only. It does NOT touch any
production source under `crates/*/src/`. Every story is labelled
`@infrastructure` because the user is the Kaleidoscope maintainer working in
their local development loop, and the observable surface is the pre-commit
hook and the CI gate-1-test job rather than an end-user command.

## System Constraints

- British English throughout. No em dashes in body text.
- No production source change. Only `crates/*/tests/*.rs`, `.github/workflows/ci.yml`,
  optional docs, and a new ADR (ADR-0058) are in scope.
- No KPI threshold is altered by this feature. Thresholds remain exactly as
  tuned for the GitHub Actions `ubuntu-latest` runner.
- The environment-variable guard must SKIP (early return, test passes) when the
  variable is absent, never panic. A panic would be indistinguishable from a
  real failure and would not solve the bypass problem.
- The guard reads the variable by presence. Any value (including `1`) counts as
  "set"; absence counts as "skip". The exact contract is a flag to DESIGN.
- No crate is bumped to 1.0.0 by this feature.

## Background and Problem Context

The Kaleidoscope KPI tests measure wall-clock latency with `std::time::Instant`
and assert a p95 threshold tuned for the controlled CI runner. Examples:
`lumen::v1_slice_01_wal_durability::ingest_p95_latency_under_three_milliseconds`
(threshold 3 ms) and `pulse::slice_02_structured_query::query_p95_latency_under_ten_milliseconds`
(threshold 10 ms). These tests flake in the LOCAL pre-commit hook when the
developer machine is under load (for example, during an autonomous development
loop running many parallel cargo builds). The flakes are not regressions: the
fastest ordered samples are tens of microseconds, and the p95 inflation comes
from fsync and scheduler contention under local load. The same tests pass
reliably on `ubuntu-latest`, the environment the thresholds were tuned for. The
local flakes have forced repeated `git commit --no-verify` bypasses, eroding the
pre-commit discipline that keeps main socially green.

---

## US-01: Local pre-commit hook skips wall-clock KPI tests

### Story

As the Kaleidoscope maintainer running the local pre-commit hook, I want the
wall-clock KPI tests to skip automatically when I have not opted in, so that my
pre-commit run is fast and deterministic and I never have to bypass the hook for
a load-induced flake.

### Elevator Pitch

Before: running `git commit` triggers the pre-commit hook, which runs every
wall-clock p95 test, and under machine load `ingest_p95_latency_under_three_milliseconds`
flakes (observed 4 to 6 ms against a 3 ms threshold), forcing `git commit --no-verify`.
After: run `cargo test --workspace` locally without `KALEIDOSCOPE_PERF_TESTS` set
and see each perf test print to stderr `perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run`
and report as passed; the hook completes deterministically.
Decision enabled: the maintainer decides to commit normally, with no bypass.

### Who

- Kaleidoscope maintainer | running the local pre-commit hook during a development
  loop, often with the machine under heavy parallel-build load | wants to commit
  without spurious failures and without abandoning the hook.

### Solution

When the `KALEIDOSCOPE_PERF_TESTS` environment variable is absent, each wall-clock
KPI test returns early (passing) after printing a one-line skip note to stderr.
When the variable is present, the test runs its measurement and threshold assertion
as before. The pre-commit hook does not set the variable, so it skips these tests.

### Domain Examples

#### 1: Happy path, hook under load — Andrea, lumen ingest p95

Andrea runs an overnight autonomous loop with eight parallel cargo builds. He
commits a change to `crates/lumen/src/store.rs`. The pre-commit hook runs
`cargo test --workspace --all-targets --locked`. `KALEIDOSCOPE_PERF_TESTS` is
unset, so `ingest_p95_latency_under_three_milliseconds` prints
`perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run` and passes. The hook
goes green in seconds. No bypass.

#### 2: Opt-in local run — Andrea verifies a perf change deliberately

Andrea wants to sanity-check latency locally on an idle machine. He runs
`KALEIDOSCOPE_PERF_TESTS=1 cargo test -p lumen ingest_p95`. The variable is
present, so the test runs the full 1000-sample measurement and asserts p95 at
or under 3000 µs. It passes (idle machine), giving Andrea local confidence.

#### 3: Edge, opt-in with arbitrary value — variable set to `true`

Andrea exports `KALEIDOSCOPE_PERF_TESTS=true` in his shell for a session. The
guard treats any present value as opt-in, so the perf tests run. Setting the
variable to the empty string is treated as a flag to DESIGN (see wave-decisions
flag 2); the recommended contract is presence-based.

### UAT Scenarios (BDD)

#### Scenario: Wall-clock KPI test skips when the maintainer has not opted in

```gherkin
Given the environment variable KALEIDOSCOPE_PERF_TESTS is not set
When the maintainer runs cargo test for a wall-clock KPI test such as ingest_p95_latency_under_three_milliseconds
Then the test prints "perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run" to stderr
And the test is reported as passed
And no Instant-based measurement is performed
```

#### Scenario: Wall-clock KPI test runs when the maintainer opts in

```gherkin
Given the environment variable KALEIDOSCOPE_PERF_TESTS is set
When the maintainer runs cargo test for the same wall-clock KPI test
Then the test performs its full latency measurement
And the test asserts its p95 threshold exactly as before this feature
```

#### Scenario: Pre-commit hook completes without bypass under machine load

```gherkin
Given the local machine is under heavy parallel-build load
And the pre-commit hook runs cargo test --workspace without setting KALEIDOSCOPE_PERF_TESTS
When the maintainer commits a change
Then every wall-clock KPI test skips and passes
And the pre-commit hook exits green
And no git commit --no-verify is required
```

### Acceptance Criteria

- [ ] When `KALEIDOSCOPE_PERF_TESTS` is absent, each gated test prints the skip
      note to stderr and passes without taking any timing measurement.
- [ ] When `KALEIDOSCOPE_PERF_TESTS` is present, each gated test runs its full
      measurement and threshold assertion unchanged.
- [ ] Running the pre-commit hook locally (which does not set the variable)
      produces no wall-clock perf failures and requires no bypass.

### Outcome KPIs

- **Who**: Kaleidoscope maintainer running the local pre-commit hook.
- **Does what**: completes pre-commit runs without bypassing the hook for perf flakes.
- **By how much**: zero `--no-verify` bypasses attributable to perf flakes after delivery (baseline: repeated bypasses).
- **Measured by**: maintainer report plus absence of perf-flake bypass notes in wave-decisions logs.
- **Baseline**: repeated `--no-verify` bypasses forced by load-induced perf flakes.

### Technical Notes

- The guard mechanism (shared helper versus inline check) is a flag to DESIGN
  (wave-decisions flag 1). Recommended: inline early return for the first slice.
- The skip mechanism (early return versus `#[ignore]` plus `--include-ignored`)
  is a flag to DESIGN (wave-decisions flag 3). Recommended: early return.

---

## US-02: CI enforces wall-clock KPI thresholds

### Story

As the Kaleidoscope maintainer relying on CI as the real KPI gate, I want the
CI gate-1-test job to set the opt-in variable so the wall-clock KPI tests still
run and enforce their thresholds on the controlled runner, so that making the
tests skippable locally does not silently disable them everywhere.

### Elevator Pitch

Before: the wall-clock KPI tests run in both the local hook and CI; once they
become skippable by default there is a risk they never run anywhere.
After: the gate-1-test job in `.github/workflows/ci.yml` sets
`KALEIDOSCOPE_PERF_TESTS` at job level, so on the next push to main the CI log
shows the perf tests executing their measurements and enforcing their thresholds
on `ubuntu-latest`.
Decision enabled: the maintainer trusts CI as the authoritative KPI gate and
decides whether a real latency regression blocks merge.

### Who

- Kaleidoscope maintainer | reading CI results on pushes and pull requests |
  wants the KPI gate to remain real, enforced in the one environment its
  thresholds were tuned for.

### Solution

Add a job-level `env` block to the `gate-1-test` job in `.github/workflows/ci.yml`
that sets `KALEIDOSCOPE_PERF_TESTS`. The value is hard-coded literally in the
job-level block (not referenced from a workflow-level `env`), per the project
memory on GitHub Actions context-availability ordering.

### Domain Examples

#### 1: Happy path — push to main runs perf tests in CI

Andrea pushes a commit to main. The gate-1-test job sets `KALEIDOSCOPE_PERF_TESTS`,
runs `cargo test --workspace --all-targets --locked`, and the CI log shows
`ingest_p95_latency_under_three_milliseconds` performing its measurement and
passing on `ubuntu-latest`. No skip note appears for that test.

#### 2: Edge — a real latency regression is caught in CI

A change accidentally adds an fsync per record to lumen ingest. Locally the perf
tests were skipped, so the hook passed. In CI, `KALEIDOSCOPE_PERF_TESTS` is set,
the test runs, p95 exceeds 3000 µs, the assertion fails, and gate-1-test goes
red, blocking the merge.

#### 3: Edge — pull request enforces the same gate

A contributor opens a PR. The same gate-1-test job runs with the variable set,
so the PR is gated on the KPI thresholds before merge, identically to a direct
push.

### UAT Scenarios (BDD)

#### Scenario: CI runs wall-clock KPI tests on push to main

```gherkin
Given the gate-1-test job sets KALEIDOSCOPE_PERF_TESTS at job level
When a commit is pushed to main and CI runs gate-1-test
Then each wall-clock KPI test performs its measurement
And the CI log shows no skip note for those tests
```

#### Scenario: A genuine latency regression fails the CI gate

```gherkin
Given the gate-1-test job sets KALEIDOSCOPE_PERF_TESTS
And a change pushes a wall-clock KPI p95 above its threshold on ubuntu-latest
When CI runs gate-1-test
Then the threshold assertion fails
And gate-1-test reports a failure that blocks the merge
```

#### Scenario: Pull requests are gated identically to pushes

```gherkin
Given a pull request targets main
When CI runs the gate-1-test job with KALEIDOSCOPE_PERF_TESTS set
Then the wall-clock KPI tests run and enforce their thresholds before merge
```

### Acceptance Criteria

- [ ] The `gate-1-test` job in `.github/workflows/ci.yml` sets
      `KALEIDOSCOPE_PERF_TESTS` via a job-level `env` block with a literal value.
- [ ] On a CI run, the wall-clock KPI tests execute their measurements (no skip
      note in the log for those tests).
- [ ] A p95 above threshold on the runner fails gate-1-test and blocks merge.

### Outcome KPIs

- **Who**: Kaleidoscope CI gate-1-test job.
- **Does what**: executes the wall-clock KPI tests and enforces thresholds.
- **By how much**: 100% of gated tests run (not skipped) in CI; zero gated tests skipped on a CI run.
- **Measured by**: inspection of the gate-1-test CI log for the absence of the skip note across all gated tests.
- **Baseline**: tests currently run in CI (this story preserves that after the local skip is added).

### Technical Notes

- Set the variable in the `gate-1-test` job only. Other Gate jobs (2, 3, 5) do
  not run `cargo test` and need no change.
- Hard-code the literal value in the job-level `env` block; do not reference a
  workflow-level `${{ env.X }}` (project memory: job-level env evaluation quirk).

---

## US-03: KPI thresholds remain unchanged

### Story

As the Kaleidoscope maintainer protecting the integrity of the KPI gate, I want
this feature to leave every threshold value exactly as it is, so that gating the
location of the tests does not weaken what they assert.

### Elevator Pitch

Before: the KPI thresholds (3 ms lumen ingest, 10 ms pulse query, and the rest)
are asserted in both local and CI runs.
After: a diff of the gated test files shows only the addition of the guard at the
top of each test body, with every threshold literal (`3_000`, `10_000`, and so on)
byte-for-byte unchanged.
Decision enabled: the maintainer is confident the CI gate is exactly as strict as
before, only more deterministic about where it runs.

### Who

- Kaleidoscope maintainer | reviewing the feature diff | wants assurance the gate
  is not silently weakened.

### Solution

The guard is added only as an early-return preamble. No threshold literal, sample
count, warm-up loop, or percentile index is modified.

### Domain Examples

#### 1: Happy path — lumen 3 ms threshold preserved

`ingest_p95_latency_under_three_milliseconds` still asserts `p95 <= 3_000` (µs).
The only change is the guard preamble above the measurement.

#### 2: Edge — microsecond-scale threshold preserved

`augur::slice_01_zscore::observe_p95_latency_under_ten_microseconds` still asserts
its 10 µs bound. The guard does not alter the tightest thresholds.

#### 3: Edge — seconds-scale recovery threshold preserved

`lumen::v1_slice_02_snapshot::recovery_p95_latency_under_five_seconds` still
asserts its five-second recovery bound.

### UAT Scenarios (BDD)

#### Scenario: No threshold literal changes in the gated files

```gherkin
Given the feature diff over the gated test files
When the diff is reviewed
Then no threshold literal, sample count, or percentile index is changed
And the only addition per test is the environment-variable guard preamble
```

#### Scenario: Opt-in run reproduces pre-feature assertions

```gherkin
Given KALEIDOSCOPE_PERF_TESTS is set
When a gated test runs
Then it asserts the identical threshold it asserted before this feature
```

### Acceptance Criteria

- [ ] No threshold literal, sample count, or percentile index changes in any
      gated test.
- [ ] The only per-test change is the addition of the guard preamble.

### Outcome KPIs

- **Who**: Kaleidoscope KPI gate.
- **Does what**: retains identical threshold assertions.
- **By how much**: zero threshold values changed (baseline: current threshold set).
- **Measured by**: diff review of the gated test files.
- **Baseline**: the current set of threshold literals across all gated tests.

### Technical Notes

- This story is a guardrail constraint expressed as a story so it is independently
  verifiable in review.

---

## US-04: Complete and uniform coverage of wall-clock KPI tests

### Story

As the Kaleidoscope maintainer wanting one consistent rule, I want every
wall-clock p95 or latency test in the workspace gated by the same mechanism, so
that no straggler test is left to flake the local hook.

### Elevator Pitch

Before: 28 wall-clock KPI tests across 11 crates run unconditionally in the local
hook, any of which can flake under load.
After: a grep for the perf-test entry points shows all 28 tests carry the same
guard, and running `cargo test --workspace` locally without the variable produces
zero wall-clock perf failures.
Decision enabled: the maintainer trusts that no straggler perf test will reappear
as a flake and force a bypass.

### Who

- Kaleidoscope maintainer | running the full workspace test suite locally | wants
  uniform, complete coverage with no exceptions.

### Solution

Apply the guard to every test in the identified inventory (see wave-decisions.md
for the complete list of 28 tests across lumen, pulse, ray, strata, cinder,
sluice, beacon, augur, aegis). Functional tests that do not measure wall-clock
time are explicitly out of scope.

### Domain Examples

#### 1: Happy path — all 28 tests gated

After delivery, the maintainer greps the 11 crates for the perf-test function
signatures and confirms each of the 28 functions begins with the guard.

#### 2: Edge — functional recovery test correctly NOT gated

A test asserting "recovery produces identical state" (no Instant, no temporal
threshold) is correctly left ungated; it is functional, not wall-clock.

#### 3: Edge — the two confirmed flaky tests are covered

Both `lumen ingest_p95_latency_under_three_milliseconds` and
`pulse query_p95_latency_under_ten_milliseconds`, the two confirmed local flakers,
are in the gated set.

### UAT Scenarios (BDD)

#### Scenario: Every wall-clock KPI test in the workspace is gated

```gherkin
Given the inventory of 28 wall-clock KPI tests across 11 crates
When the gated test files are inspected
Then each of the 28 tests carries the environment-variable guard
And no wall-clock KPI test is left ungated
```

#### Scenario: Functional tests are not gated

```gherkin
Given a test that does not measure wall-clock time and asserts no temporal threshold
When the feature is applied
Then that test is left unchanged and ungated
```

#### Scenario: Full local workspace run produces no perf failures

```gherkin
Given KALEIDOSCOPE_PERF_TESTS is not set
When the maintainer runs cargo test --workspace locally under load
Then no wall-clock KPI test fails
And every wall-clock KPI test reports the skip note
```

### Acceptance Criteria

- [ ] All 28 tests in the inventory carry the guard.
- [ ] No functional (non-temporal) test is altered.
- [ ] `cargo test --workspace` locally without the variable produces zero
      wall-clock perf failures.

### Outcome KPIs

- **Who**: Kaleidoscope local workspace test run.
- **Does what**: skips every wall-clock KPI test uniformly when not opted in.
- **By how much**: 28 of 28 wall-clock KPI tests gated; zero ungated stragglers.
- **Measured by**: grep of the 11 crates against the inventory in wave-decisions.md.
- **Baseline**: 0 of 28 gated today.

### Technical Notes

- The authoritative inventory is in wave-decisions.md. DESIGN must reconcile any
  new perf test added between DISCUSS and DELIVER against that inventory.

---

## US-05: Guard mechanism documented for future perf tests

### Story

As a future contributor adding a new wall-clock KPI test, I want the gating
pattern documented, so that I apply the same guard and do not reintroduce a local
flake.

### Elevator Pitch

Before: nothing records why or how wall-clock tests are gated, so a new perf test
risks being added unguarded.
After: ADR-0058 records the policy (KPIs enforced in CI, skipped locally by
default) and the mechanism, and a future contributor reads it and applies the
guard to a new test.
Decision enabled: a contributor decides to add the guard to their new perf test
by following the documented pattern.

### Who

- Future Kaleidoscope contributor | adding a new wall-clock KPI test | wants a
  clear, citable pattern to follow.

### Solution

Document the mechanism and the policy. The recommendation is a dedicated ADR
(ADR-0058) capturing the decision of WHERE KPIs are enforced, citing ADR-0005
(the five gates) without modifying it. The exact mechanism documented depends on
the DESIGN decision on flags 1 and 3.

### Domain Examples

#### 1: Happy path — contributor follows the documented pattern

A contributor adds `query_p95_latency_under_twenty_milliseconds` to a new slice.
They read ADR-0058, copy the guard preamble, and the new test skips locally and
runs in CI without any further coordination.

#### 2: Edge — reviewer cites the ADR

During review, a reviewer notices a new perf test lacks the guard and cites
ADR-0058 as the standard the change must meet.

#### 3: Edge — ADR-0005 left intact

The contributor confirms ADR-0058 references ADR-0005 Gate 1 but does not edit
ADR-0005 itself.

### UAT Scenarios (BDD)

#### Scenario: The gating policy and mechanism are documented

```gherkin
Given ADR-0058 exists
When a contributor reads it
Then it states that wall-clock KPIs are enforced in CI and skipped locally by default
And it describes the guard mechanism to apply to new perf tests
And it cites ADR-0005 without modifying it
```

### Acceptance Criteria

- [ ] ADR-0058 records the WHERE-enforced policy and the guard mechanism.
- [ ] ADR-0058 cites ADR-0005 and leaves it unmodified.

### Outcome KPIs

- **Who**: future contributor adding a wall-clock KPI test.
- **Does what**: applies the documented guard to new perf tests.
- **By how much**: new perf tests added post-feature carry the guard (target 100%).
- **Measured by**: review of subsequent perf-test additions against ADR-0058.
- **Baseline**: no documented pattern today.

### Technical Notes

- Whether to write ADR-0058 is a flag to DESIGN (wave-decisions flag 6).
  Recommendation: yes, because the decision is a policy on where KPIs are enforced.
- This story is optional; it does not block US-01 through US-04.
