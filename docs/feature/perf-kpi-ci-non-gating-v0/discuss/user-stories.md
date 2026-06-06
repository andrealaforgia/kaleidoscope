<!-- markdownlint-disable MD024 -->

# User Stories: perf-kpi-ci-non-gating-v0

Infrastructure / CI-hygiene feature. The persona is the **maintainer**
(Andrea) reading a GitHub Actions result, plus any contributor whose PR
runs the pipeline. There is no end-user surface; the operator-invocable
surface is the CI workflow and the local pre-commit hook.

## Context: this feature corrects ADR-0058

A prior feature, `perf-kpi-ci-gating-v0`, shipped ADR-0058 (Accepted,
2026-05-31): "Wall-clock KPI tests are enforced in CI, skipped locally
by default." It added the `KALEIDOSCOPE_PERF_TESTS` env guard to all 28
wall-clock KPI tests and set `KALEIDOSCOPE_PERF_TESTS: "1"` at the
`gate-1-test` job level (`.github/workflows/ci.yml:141`), so a perf
breach **fails the build-gating Gate 1**.

That decision solved the *local* flake (the hook does not set the
variable, so perf tests self-skip locally) but it created a new problem
it did not foresee: `place` and the other durable-op KPIs now include a
**per-record fsync** (ADR-0049 honour-fsync, ADR-0060 store fsync). On
CI's shared, virtualised storage, fsync p95 is routinely milliseconds,
far above the 200 us budget written for a *non-durable* `place`. So the
KPI is **unreachable on CI for a now-durable op** — a stale-budget +
wrong-gate problem, not a code regression. ADR-0058 explicitly rejected
raising thresholds; this feature honours that (we do NOT touch
thresholds) and instead changes **where/how** the perf KPIs run: from
*gating* Gate 1 to a *tracked, non-gating* signal.

This feature's DESIGN wave will author a new ADR that **supersedes the
CI-gating decision of ADR-0058** while preserving its guard mechanism
and its no-threshold-chasing stance.

## System Constraints

These cross-cutting constraints apply to every story below and are
flagged to DESIGN / DEVOPS:

- **C1 — Durability is not weakened.** The per-record fsync stays. The
  durable-op budgets (`place` 200 us, `enqueue` 300 us, the WAL `ingest`
  budgets) now reflect *durable* cost. This is a deliberate consequence
  of the Earned-Trust durability work (ADR-0049, ADR-0060), NOT a
  regression.
- **C2 — Correctness gating is not loosened.** Gate 1 still hard-gates
  the entire non-perf `cargo test --workspace --all-targets --locked`
  suite. De-gating perf must NOT de-gate correctness (negative control).
- **C3 — No threshold chasing.** No threshold literal, sample count,
  warm-up loop, or percentile index is changed at DISCUSS level. The
  memory `project_p95_wallclock_flakes_overnight` forbids
  threshold-raises as the fix. The change is *location and gating
  semantics*, not budget values.
- **C4 — Visibility preserved.** De-gating must not mean deleting. The
  p95 numbers must still RUN and be reported as a tracked CI signal.
- **C5 — Whole family, not just `place`.** Scope is all 28 wall-clock
  KPI tests across 11 crates (lumen, pulse, ray, strata, cinder, sluice,
  beacon, augur, aegis — inventory in `story-map.md`), so fixing only
  `place_p95` does not leave the family flaky.
- **C6 — Local hook is already correct.** `scripts/hooks/pre-commit`
  does NOT set `KALEIDOSCOPE_PERF_TESTS` (it runs `cargo test
  --workspace --all-targets --locked` at line 92-93), so perf tests
  already self-skip locally per ADR-0058. No local de-gating is needed.
  This is a verified fact, not an assumption.
- **C7 — Trunk-based posture.** Per memory
  `project_kaleidoscope_pure_trunk_based`, main has no
  required-status-checks; CI is feedback, not a hard merge block. A
  non-gating perf job aligns with this posture: it reports, it does not
  block.
- **C8 — No crate version impact.** This is CI workflow + (possibly)
  test-harness reporting only. No crate is bumped; no `crates/*/src/`
  production source changes are expected. Never 1.0.0.
- **C9 — British English; no em dashes in body prose** (project house
  style; em dashes used only as structural separators in headings/lists,
  consistent with surrounding docs).

---

## US-01: A perf breach on noisy CI hardware no longer fails the build

### Elevator Pitch

- **Before**: A slow CI fsync inflates `place` p95 to milliseconds; the
  `gate-1-test` job goes RED on the GitHub Actions run page,
  indistinguishable from a real correctness regression. The maintainer
  cannot tell hardware noise from a real break, and the team learns to
  ignore red.
- **After**: On the GitHub Actions run page, `gate-1-test` shows GREEN
  whenever the correctness suite passes, even when `place` p95 was
  4.2 ms this run. The perf numbers appear in a separate job, not in the
  gating one. (`gate-1-test` no longer sets `KALEIDOSCOPE_PERF_TESTS=1`.)
- **Decision enabled**: When the maintainer sees Gate 1 RED, they now
  decide "this is a real regression, investigate" with confidence,
  instead of "probably just the runner, ignore it".

### Problem

The maintainer (Andrea) is the person who reads the GitHub Actions
result for every push to `main` and every PR. Today a slow CI disk turns
`gate-1-test` red because `place_p95_latency_under_two_hundred_micro
seconds` measures a now-durable `place()` (per-record fsync, ADR-0049 /
ADR-0060) against a 200 us budget that was written before `place` was
durable. On `ubuntu-latest` shared storage, fsync p95 is routinely
multiple milliseconds. The maintainer finds it impossible to tell a real
correctness regression from runner disk variance, so the workaround is
to mentally discount red builds, which is the dangerous habit this
feature exists to kill.

### Who

- Maintainer (Andrea) | reads every CI run on `main` and PRs | wants a
  red build to mean "something is actually broken", not "the runner's
  disk was slow this minute".
- PR contributor | pushes a branch, watches the checks | wants to know
  whether their change broke correctness, undistracted by perf noise.

### Solution

The build-gating `gate-1-test` job stops setting
`KALEIDOSCOPE_PERF_TESTS=1`. With the variable absent, the 28 wall-clock
KPI tests self-skip (the ADR-0058 early-return guard already in every
test body) and pass without measuring. Gate 1 therefore goes green iff
the correctness suite passes. Thresholds, guard code, and durability are
all untouched.

### Domain Examples

#### 1: Happy path — durable `place` is slow on CI, build stays green

On push `abc1234` to `main`, the `ubuntu-latest` runner's fsync p95 for
`cinder::place` measures 4.2 ms (21x the 200 us budget) because the
shared disk was contended. Under this feature, `gate-1-test` does not set
`KALEIDOSCOPE_PERF_TESTS`, so `place_p95_latency_under_two_hundred_micro
seconds` self-skips, the correctness suite passes, and `gate-1-test`
shows GREEN. The maintainer is not paged.

#### 2: Edge case — `enqueue` 300 us breached, all other crates fine

On PR #57, `sluice::enqueue_p95_latency_under_three_hundred_micro
seconds` would have measured 1.1 ms on the runner (durable WAL fsync),
while every correctness test across all 11 crates passes. Gate 1 is
green; the contributor's PR is mergeable. The perf number is captured by
US-02's non-gating job, not by Gate 1.

#### 3: Boundary — every perf test would breach, build still green

On a runner having an unusually slow I/O minute, all 28 wall-clock KPI
tests would breach their budgets simultaneously. Because `gate-1-test`
does not set the variable, all 28 self-skip; Gate 1 is green because no
correctness test failed. Hardware variance produces zero false reds in
the gating job.

### UAT Scenarios (BDD)

#### Scenario: Durable place latency breach does not fail the build

```gherkin
Given the gate-1-test job runs the workspace test suite
And the runner's fsync p95 for cinder place is 4.2 ms this run
And the place wall-clock KPI budget is 200 microseconds
When the correctness suite passes and no perf test is run in gate-1-test
Then the gate-1-test job reports success
And the GitHub Actions run page shows Gate 1 green
```

#### Scenario: The gating job does not opt in to wall-clock perf tests

```gherkin
Given the gate-1-test job definition in .github/workflows/ci.yml
When a maintainer inspects the job-level env block
Then KALEIDOSCOPE_PERF_TESTS is not set in the gate-1-test job
And the 28 wall-clock KPI tests self-skip during gate-1-test
```

#### Scenario: A simultaneous family-wide breach produces no false red

```gherkin
Given the runner has an unusually slow I/O minute
And all 28 wall-clock KPI budgets would be exceeded if measured
When gate-1-test runs without KALEIDOSCOPE_PERF_TESTS set
Then no wall-clock KPI test executes its measurement
And gate-1-test reports success because every correctness test passed
```

### Acceptance Criteria

- [ ] The `gate-1-test` job in `.github/workflows/ci.yml` does NOT set
  `KALEIDOSCOPE_PERF_TESTS` (the `env` block at line ~141 is removed or
  the variable omitted).
- [ ] With the variable absent, every wall-clock KPI test self-skips and
  passes (the ADR-0058 early-return preamble is unchanged).
- [ ] `gate-1-test` reports success when the non-perf correctness suite
  passes, regardless of what any wall-clock p95 would have measured.
- [ ] No threshold, sample count, warm-up loop, or percentile index is
  modified (C3).

### Outcome KPIs

- **Who**: maintainer + PR contributors reading CI results.
- **Does what**: stop discounting / ignoring red Gate-1 builds caused by
  perf noise.
- **By how much**: zero perf-induced false reds in `gate-1-test` (target
  0; current state: recurring, the "old, annoying problem").
- **Measured by**: count of `gate-1-test` failures attributable to a
  wall-clock KPI assertion over a rolling 30-day window (GitHub Actions
  run history).
- **Baseline**: `gate-1-test` currently red on perf breach (>=1
  occurrence is the trigger for this feature; the `place` flake is the
  named instance).

### Technical Notes (constraints / dependencies)

- Touches `.github/workflows/ci.yml` `gate-1-test` job only. No
  `crates/*/src/` change. (C8)
- Depends on the ADR-0058 guard already being present in all 28 tests
  (verified present; this feature relies on it, does not re-add it).
- Negative control lives in US-03 (correctness still gates).
- DESIGN flag F2: should the new ADR supersede ADR-0058's CI-gating
  clause? Recommended YES.

---

## US-02: The perf KPIs still run and report in a separate, non-gating job

### Elevator Pitch

- **Before**: The only place the wall-clock p95 numbers were produced
  was inside the gating `gate-1-test` job, where a breach was fatal and
  the numbers were drowned by a red X. There was no way to *watch* perf
  without *blocking* on it.
- **After**: On the GitHub Actions run page there is a new
  `perf-kpis` (informational) job. It runs the 28 wall-clock KPI tests
  with `KALEIDOSCOPE_PERF_TESTS=1`, prints each crate's p95 numbers to
  the job log, and is marked non-gating (continue-on-error) so a breach
  shows as a warning, not a workflow failure.
- **Decision enabled**: The maintainer opens the `perf-kpis` job log and
  decides, from the trend of the numbers, whether a *sustained* drift
  (not a one-minute spike) warrants opening a follow-up — without the
  number ever blocking a merge.

### Problem

The maintainer wants the wall-clock performance KPIs to remain a
*visible, tracked* signal. De-gating Gate 1 (US-01) removes the false
red, but if that were all, the perf numbers would vanish entirely
(nothing would set `KALEIDOSCOPE_PERF_TESTS`), and a *real* sustained
performance regression would go unobserved. The maintainer needs the
numbers reported somewhere they can read them, separated from the
build-blocking gate.

### Who

- Maintainer (Andrea) | periodically reviews perf trend | wants the p95
  numbers reported per run, visible but never blocking.

### Solution

Add a new CI job (working name `perf-kpis`) that sets
`KALEIDOSCOPE_PERF_TESTS=1` and runs the wall-clock KPI tests, surfacing
each crate's p95 to the job log. The job is non-gating: a breach does not
fail the workflow. DESIGN/DEVOPS owns the exact mechanism (see flag F3:
`continue-on-error: true` on an asserting job vs a report-only mode that
logs the p95 and never panics). Either way, the numbers are always
visible; a breach never blocks.

### Domain Examples

#### 1: Happy path — perf job green, numbers logged

On push `def5678`, the `perf-kpis` job runs all 28 tests with the
variable set. `cinder place` p95 logs at 0.9 ms, `sluice enqueue` at
1.2 ms, `augur observe` at 8 us. All within or near budget; the job is
green and the numbers are in the log for the maintainer to scan.

#### 2: Edge case — perf job breaches but does not fail the workflow

On push `ghi9012`, `cinder place` p95 logs at 5.0 ms (budget 200 us).
The `perf-kpis` job surfaces the breach as a warning / annotation but the
overall workflow conclusion is still success (the job is non-gating).
The merge is not blocked.

#### 3: Boundary — perf job reports a genuine sustained regression

A code change accidentally adds a second fsync per `place`. Over five
consecutive `main` pushes, `place` p95 in the `perf-kpis` log climbs from
0.9 ms to 9 ms and stays there (not a one-run spike). The maintainer
reads the trend, recognises a sustained regression rather than noise, and
opens a follow-up — a decision the report-only signal enabled.

### UAT Scenarios (BDD)

#### Scenario: Perf KPIs run and report in a dedicated job

```gherkin
Given a push to main triggers the CI workflow
When the perf-kpis job runs with KALEIDOSCOPE_PERF_TESTS set to 1
Then each crate's wall-clock p95 numbers appear in the perf-kpis job log
And the maintainer can read them on the GitHub Actions run page
```

#### Scenario: A perf breach in the perf job does not fail the workflow

```gherkin
Given the perf-kpis job is running the wall-clock KPI tests
And cinder place p95 measures 5.0 ms against a 200 microsecond budget
When the perf-kpis job completes
Then the breach is surfaced as a warning, not a workflow failure
And the overall CI workflow conclusion is success
```

#### Scenario: The perf numbers remain visible even on a breach

```gherkin
Given a wall-clock KPI exceeds its budget on this run
When the maintainer opens the perf-kpis job log
Then the measured p95 value is printed for that KPI
And the value is readable regardless of pass or breach
```

### Acceptance Criteria

- [ ] A new non-gating CI job runs the 28 wall-clock KPI tests with
  `KALEIDOSCOPE_PERF_TESTS=1`.
- [ ] The job is configured so a perf breach does NOT fail the overall
  workflow (continue-on-error or report-only — DESIGN flag F3).
- [ ] Each crate's measured p95 appears in the job log and is readable on
  the run page (C4: visibility preserved).
- [ ] The job runs the whole family (all 28 tests, 11 crates), not just
  `place` (C5).

### Outcome KPIs

- **Who**: maintainer reviewing perf trend.
- **Does what**: reads per-run p95 numbers from a dedicated job.
- **By how much**: 100% of CI runs on `main` produce readable p95 numbers
  in the `perf-kpis` log (target 100%; baseline: numbers only emitted
  inside the gating job, drowned by red on breach).
- **Measured by**: presence of p95 values in the `perf-kpis` job log per
  run (GitHub Actions log inspection).
- **Baseline**: today perf numbers exist only inside `gate-1-test` and
  are not separable from the gating outcome.

### Technical Notes (constraints / dependencies)

- Depends on US-01 (Gate 1 must stop owning the perf run first, or the
  numbers would be produced twice).
- DESIGN flag F3: assert-but-non-gating-job vs report-only mode. Both
  satisfy C4; report-only guarantees the number prints even on breach.
- DEVOPS owns whether this is a new job in `ci.yml` or a separate
  workflow file, and whether it runs on every push or on a schedule.

---

## US-03: A real correctness regression still fails the build (negative control)

### Elevator Pitch

- **Before**: Gate 1 was red for two indistinguishable reasons — a real
  correctness break OR a perf-on-slow-hardware breach. Red meant "maybe
  broken, maybe noise".
- **After**: On the GitHub Actions run page, Gate 1 RED means exactly one
  thing — a correctness test failed. A `cargo test` assertion failure in
  any non-perf test still turns `gate-1-test` red and blocks the merge
  signal, exactly as before.
- **Decision enabled**: The maintainer treats every Gate 1 RED as a real
  defect requiring action, because de-gating perf provably did not
  de-gate correctness.

### Problem

The whole value of US-01 collapses if de-gating perf accidentally
loosens correctness gating. The maintainer needs proof that removing
`KALEIDOSCOPE_PERF_TESTS=1` from `gate-1-test` changes only the perf
behaviour and leaves the correctness suite as a hard gate. Without this
negative control, "make red trustworthy" could degenerate into "make red
rare by gating less", which is the opposite of the intent.

### Who

- Maintainer (Andrea) | needs red to be trustworthy in BOTH directions |
  wants real breaks to still fail loudly.

### Solution

No new mechanism — this story asserts the invariant that the US-01 change
preserves. `gate-1-test` continues to run `cargo test --workspace
--all-targets --locked`; any non-perf assertion failure fails the job.
The DISTILL wave should include a negative-control check (for example, a
deliberately failing correctness test on a throwaway branch, or a review
assertion) demonstrating Gate 1 still goes red on a real break.

### Domain Examples

#### 1: Happy path — a real correctness break fails Gate 1

A change breaks `cinder` WAL recovery so a recovery *correctness* test
(state-equality, not latency) fails. `gate-1-test` runs the workspace
suite, the assertion fails, and Gate 1 goes RED — blocking the merge
signal. The perf de-gating did not hide this.

#### 2: Edge case — perf de-gated, correctness break in a different crate

After US-01 lands, a regression in `lumen` query *correctness* (wrong
rows returned) fails its functional test. Even though no perf test runs
in `gate-1-test`, the functional failure still turns Gate 1 RED.

#### 3: Boundary — only the perf budget would breach, nothing else

A change makes `place` 10x slower but functionally correct. The perf
budget would breach, but no correctness test fails. Gate 1 is GREEN
(US-01); the slowdown surfaces in the `perf-kpis` job (US-02), not as a
false red. This confirms the gate now discriminates perf from
correctness.

### UAT Scenarios (BDD)

#### Scenario: A correctness assertion failure still fails Gate 1

```gherkin
Given the gate-1-test job runs cargo test --workspace --all-targets --locked
And a non-perf correctness test asserts wrong recovered state
When the workspace test suite runs without KALEIDOSCOPE_PERF_TESTS set
Then the correctness test fails
And gate-1-test reports failure
And the GitHub Actions run page shows Gate 1 red
```

#### Scenario: De-gating perf does not change correctness gating

```gherkin
Given KALEIDOSCOPE_PERF_TESTS is no longer set in gate-1-test
When the full non-perf correctness suite runs
Then every correctness test is still executed and asserted
And a failure in any of them still fails gate-1-test
```

#### Scenario: A pure slowdown with correct behaviour does not fail Gate 1

```gherkin
Given a change makes place ten times slower but functionally correct
When gate-1-test runs without KALEIDOSCOPE_PERF_TESTS set
Then no correctness test fails
And gate-1-test reports success
And the slowdown is visible only in the non-gating perf-kpis job
```

### Acceptance Criteria

- [ ] `gate-1-test` continues to run `cargo test --workspace
  --all-targets --locked` (the correctness invocation is unchanged).
- [ ] A failing non-perf correctness test fails `gate-1-test`.
- [ ] Removing `KALEIDOSCOPE_PERF_TESTS=1` changes only perf-test
  execution; every non-perf test still executes and asserts (C2).
- [ ] DISTILL includes a demonstrable negative control proving Gate 1
  goes red on a real correctness break.

### Outcome KPIs

- **Who**: maintainer interpreting Gate 1 status.
- **Does what**: treats every Gate 1 RED as a real defect.
- **By how much**: 100% of Gate 1 reds are correctness-attributable, 0%
  perf-attributable (the complement of US-01's KPI).
- **Measured by**: classification of `gate-1-test` failures over a
  rolling 30-day window.
- **Baseline**: today Gate 1 reds are a mix of correctness and perf
  noise, not separable.

### Technical Notes (constraints / dependencies)

- This is the negative control for US-01; they must be validated
  together.
- No new code; asserts an invariant the US-01 edit must preserve.

---

## US-04: The durable-op budgets are documented as dev-indicative, not CI-contractual

### Elevator Pitch

- **Before**: A reader of `place_p95_latency_under_two_hundred_micro
seconds` (or the ADR-0058 inventory) sees a hard 200 us budget and
  reasonably assumes it is a contractual CI gate — with no note that
  `place` became durable (per-record fsync) after that budget was
  written, making it unreachable on CI storage.
- **After**: A reader of the honesty note (in the new ADR and/or a
  comment block referenced from the durable-op tests) sees that the
  durable-op budgets are **indicative on dev hardware, not contractual on
  CI**, and that this is a deliberate consequence of the Earned-Trust
  durability work (ADR-0049, ADR-0060), not a regression.
- **Decision enabled**: A future contributor who sees `place` p95 at
  4 ms on CI decides "this is expected durable-fsync cost on shared
  storage, not a regression" instead of "the budget is broken, let me
  raise it" (which the memory forbids).

### Problem

The durable-op KPI budgets (`place` 200 us, `enqueue` 300 us, the WAL
`ingest` budgets) were written for *non-durable* operations. The
durability work (ADR-0049 honour-fsync, ADR-0060 store fsync) added a
per-record fsync, so these ops now pay durable cost. On CI's
shared/virtualised storage, fsync p95 is routinely milliseconds. Without
an explicit honesty note, a future maintainer reading a breach will
either think the code regressed or be tempted to chase the threshold —
both wrong. The reasoning must be recorded once, citably.

### Who

- Future maintainer / contributor | encounters a durable-op p95 breach |
  needs to know the budget is dev-indicative and why, so they neither
  panic nor threshold-chase.

### Solution

Document, in the new ADR (and referenced from the durable-op test sites
where appropriate), that: (a) the durable-op budgets reflect *durable*
cost since ADR-0049 / ADR-0060; (b) they are indicative on dev hardware,
not contractual on CI shared storage; (c) raising them is explicitly NOT
the fix (the memory forbids threshold-chasing); (d) the correct posture
is the non-gating perf signal (US-02). DESIGN/DEVOPS owns the exact
placement.

### Domain Examples

#### 1: Happy path — contributor reads the note, interprets a breach correctly

A contributor sees `place` p95 at 4.2 ms in the `perf-kpis` log, follows
the reference to the honesty note, reads that durable `place` includes an
fsync and the 200 us budget is dev-indicative, and correctly concludes
"expected on CI storage, not a regression". No threshold change is made.

#### 2: Edge case — note distinguishes durable ops from in-memory ops

A contributor checks `cinder get_tier` (50 us, an in-memory read with NO
fsync). The note clarifies that the durable-op caveat applies to
fsync-bearing ops (`place`, `enqueue`, WAL `ingest`), not to in-memory
reads like `get_tier`, whose budgets remain meaningful even on CI.

#### 3: Boundary — note forbids the wrong fix explicitly

A contributor, frustrated by a breach, considers raising the `place`
budget to 5 ms. The honesty note (and the memory it cites) explicitly
states threshold-raising is the wrong fix and points to the non-gating
job as the right posture. The contributor does not raise the threshold.

### UAT Scenarios (BDD)

#### Scenario: The durable-op budgets are documented as dev-indicative

```gherkin
Given the new ADR for this feature
When a maintainer reads the honesty note
Then it states the durable-op budgets reflect durable fsync cost since ADR-0049 and ADR-0060
And it states the budgets are indicative on dev hardware, not contractual on CI
```

#### Scenario: The note attributes the cost to durability, not regression

```gherkin
Given a contributor sees a durable-op p95 breach on CI
When they read the honesty note
Then it explains the breach is expected durable-fsync cost on shared CI storage
And it states this is a deliberate consequence of the Earned-Trust durability work, not a regression
```

#### Scenario: The note forbids threshold-chasing as the fix

```gherkin
Given a contributor considers raising a durable-op budget to absorb CI variance
When they read the honesty note
Then it states raising the threshold is explicitly not the fix
And it points to the non-gating perf job as the correct posture
```

### Acceptance Criteria

- [ ] The new ADR documents that durable-op budgets (`place`, `enqueue`,
  WAL `ingest`) reflect durable fsync cost since ADR-0049 / ADR-0060.
- [ ] The note states these budgets are dev-indicative, not
  CI-contractual.
- [ ] The note states threshold-raising is NOT the fix and cites the
  non-gating posture (US-02) and the project memory.
- [ ] The note distinguishes fsync-bearing durable ops from in-memory
  ops (for example `get_tier`, `observe`) whose budgets stay meaningful.

### Outcome KPIs

- **Who**: future maintainers / contributors encountering a durable-op
  breach.
- **Does what**: correctly attribute the breach to durable cost and
  refrain from threshold-chasing.
- **By how much**: 0 threshold-raise commits to durable-op budgets after
  this note lands (guardrail target 0; baseline: the memory records past
  pressure toward threshold-raises).
- **Measured by**: git history of changes to durable-op threshold
  literals after this feature.
- **Baseline**: durable-op budgets currently carry no honesty note;
  ADR-0058 rejected threshold-raises but did not explain the durable-cost
  reason.

### Technical Notes (constraints / dependencies)

- Pure documentation (new ADR + optional referenced comment). No code,
  no threshold change (C3).
- Depends on nothing; can land with US-01 in the same slice.
- DESIGN flag F4: exact placement (ADR-only vs ADR + per-site comment
  reference).
