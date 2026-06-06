# ADR-0070 — Wall-clock perf KPIs are a non-gating CI signal, not a build gate

- **Status**: Accepted
- **Date**: 2026-06-06
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `perf-kpi-ci-non-gating-v0`
- **Supersedes**: ADR-0058 §3 (the CI-gating clause) and the gating
  consequence it records. The guard mechanism (§1, §2, §4, §5, §6) and the
  no-threshold-chasing stance of ADR-0058 are PRESERVED, not superseded.
- **Superseded by**: none
- **Related**: ADR-0058 (`perf-kpi-ci-gating-v0` — established the
  presence-based `KALEIDOSCOPE_PERF_TESTS` env guard at all 28 wall-clock KPI
  test sites and set the variable in `gate-1-test`; this ADR REVERSES the
  *gating* decision and PRESERVES the guard; cited and partially superseded as
  recorded above). ADR-0005 (the five-gate CI contract; Gate 1 is `cargo test
  --workspace`; cited, NOT modified — this ADR adds a non-gating job alongside
  the five gates, it does not add or amend a gate). ADR-0049
  (`earned-trust-fsync-probe-v0` — added per-record `sync_all` to the pulse
  WAL append; §4 pins per-record `sync_all` over `sync_data` and batched
  fsync; the durable-cost root cause for the pulse-family budgets; cited, NOT
  modified). ADR-0060 (`store-fsync-durability-v0` — generalised per-record
  `sync_all` + atomic snapshot across all seven file-backed stores including
  `cinder.place` and `sluice.enqueue`; §3 is the WAL fsync procedure; the
  durable-cost root cause ADR-0058 did not foresee; cited, NOT modified).

## Context

ADR-0058 (Accepted, 2026-05-31) recorded WHERE the 28 wall-clock KPI tests
run. Its Decision §3 reads, verbatim:

> **CI opt-in.** The `gate-1-test` job in `.github/workflows/ci.yml` sets
> `KALEIDOSCOPE_PERF_TESTS: "1"` in a job-level `env` block with a hardcoded
> literal, consistent with the existing `NIGHTLY_PIN` workaround for the GitHub
> Actions job-level env evaluation quirk in gates 2 and 3. The local pre-commit
> hook does NOT set the variable, so it skips these tests.

This clause makes a wall-clock perf breach **fail the build-gating Gate 1**: a
perf assertion failure turns `gate-1-test` red and is indistinguishable, on the
GitHub Actions run page, from a real correctness regression. Verified in code:
`.github/workflows/ci.yml:140-141` sets the variable in the `gate-1-test`
job-level `env` block; the gating invocation is `cargo test --workspace
--all-targets --locked` at `.github/workflows/ci.yml:184`.

ADR-0058 solved the LOCAL flake (the pre-commit hook does not set the variable,
so perf tests self-skip locally — re-verified this wave at
`scripts/hooks/pre-commit:92-93`). It did NOT foresee a CI-side problem that
two later Earned-Trust durability features introduced:

**The durable-cost root cause ADR-0058 omitted.** After ADR-0058 was accepted,
ADR-0049 (`earned-trust-honour-fsync`, 2026-05-27) and ADR-0060
(`store-fsync-durability`, 2026-06-04) added a **per-record `sync_all`** to the
WAL append path of the file-backed stores. `cinder.place` now fsyncs every
record (verified at `crates/cinder/src/file_backed.rs:433`,
`fsync_backend.fsync_file(wal.get_ref())`, with the inline comment citing
ADR-0049 §4 / ADR-0060 §3). `sluice.enqueue` and the per-crate WAL `ingest`
paths gained the same per-record fsync under ADR-0060 §3. On GitHub Actions
shared, virtualised storage, fsync p95 is routinely **milliseconds**, far above
budgets such as `place` 200 us or `enqueue` 300 us — budgets written for a
*non-durable* op before these durability features landed. The
`place_p95_latency_under_two_hundred_microseconds` test
(`crates/cinder/tests/v1_slice_01_wal_durability.rs:255`) measures 1000 durable
`place()` calls and asserts `samples[950]` (p95) <= 200 us. On CI this KPI is
**unreachable for a now-durable op**: a stale-budget-against-a-changed-op
problem, NOT a code regression. The budget did not become wrong; the operation
became durable underneath it, and the gate did not move with it.

This is a wrong-gate problem, and it has a known harm. A build that flakes red
on hardware variance trains the maintainer and contributors to **mentally
discount red builds** — the very habit that destroys the value of a CI signal
under pure trunk-based development. Per project memory
`project_kaleidoscope_pure_trunk_based`, `main` has no required-status-checks
and no `enforce_admins`: **CI is feedback, not a hard merge gate.** A
wall-clock perf KPI on shared CI hardware is exactly the kind of signal that
should be *visible feedback*, not a *blocking gate*.

ADR-0058 explicitly rejected raising the thresholds, and this ADR honours that
rejection unchanged (project memory `project_p95_wallclock_flakes_overnight`
forbids threshold-raises as the fix). The fix here is not the budget VALUE and
not the guard MECHANISM — both are correct. The fix is the **gating semantics
and the location**: the wall-clock KPIs move from *gating* Gate 1 to a
*tracked, non-gating* job.

ADRs in this repository are immutable (superseded, never edited). ADR-0005,
ADR-0049, ADR-0060 are Accepted and referenced as precedents; they are NOT
modified. ADR-0058's guard mechanism is preserved; only its §3 gating clause is
superseded by this ADR. ADR-0070 is the next free number (the highest existing
was 0069, verified by `ls docs/product/architecture/adr-*.md`).

## Decision

### 1. `gate-1-test` stops setting `KALEIDOSCOPE_PERF_TESTS` (F1, US-01)

The build-gating `gate-1-test` job in `.github/workflows/ci.yml` MUST NOT set
`KALEIDOSCOPE_PERF_TESTS`. The job-level `env` block at
`.github/workflows/ci.yml:140-141` is removed (the `env:` key and its single
`KALEIDOSCOPE_PERF_TESTS: "1"` entry). With the variable absent, every one of
the 28 wall-clock KPI tests hits the ADR-0058 early-return preamble
(`if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() { eprintln!(...); return; }`,
verified at `crates/cinder/tests/v1_slice_01_wal_durability.rs:256`) and
self-skips with no measurement taken. `gate-1-test` therefore goes green iff the
non-perf correctness suite passes. The gating invocation `cargo test --workspace
--all-targets --locked` (`.github/workflows/ci.yml:184`) is unchanged: every
non-perf test still executes and asserts (C2; US-03 negative control). This is
the single lever that de-gates all 28 tests at once (F5; C5): one env key
removed, the whole family self-skips in the gating job.

### 2. A separate non-gating `perf-kpis` job runs the family (F1, US-02)

A NEW CI job (working name `perf-kpis`) sets `KALEIDOSCOPE_PERF_TESTS: "1"` in
its own job-level `env` block (hardcoded literal, per the ADR-0058 §3 note about
the GitHub Actions job-level env evaluation quirk) and runs the wall-clock KPI
family via `cargo test --workspace --all-targets --locked`. The env-var presence
runs the whole guarded family (all 28 tests across 11 crates), so any test
carrying the ADR-0058 preamble is automatically included; no per-test
enumeration is needed and a future guarded test is picked up for free (F5;
inventory-drift mitigation).

### 3. The non-gating mechanism is `continue-on-error: true` on the job (F1)

The `perf-kpis` job is marked **`continue-on-error: true`**. This is the
GitHub-native, visible-but-non-blocking mechanism: the job RUNS, a perf
assertion failure marks the JOB with a red X (visible on the run page), but the
**overall workflow conclusion stays success** and nothing downstream is blocked.
Alternatives rejected below: a `|| true` on the step (hides the breach entirely —
the step shows green even on a real breach, defeating C4 visibility), and a
separate workflow file (more moving parts than warranted for a single job; loses
the at-a-glance run-page co-location with the gates). `continue-on-error: true`
gives the exact semantics the JTBD asks for: the breach is a non-blocking red
signal, the merge is never blocked, the signal is one click away on the same run
page (C7; trunk-based "CI is feedback, not a gate").

### 4. Trigger: every push, same triggers as the gates (F1)

The `perf-kpis` job runs on **every push and every PR**, the same triggers as
the existing gates, so the perf signal is timely. A nightly-only schedule was
considered and rejected (below): a once-a-day signal is too coarse to spot a
*sustained* regression promptly, and being non-gating already removes the cost
argument for batching it nightly. The DEVOPS wave owns the exact placement
(a job in `ci.yml` versus a separate workflow file) and the `needs:` wiring; the
DESIGN contract is: non-gating, every-push, runs the whole family with the
variable set.

### 5. Keep the asserts, in the non-gating job; the p95 prints on breach (F3, US-02)

The non-gating job keeps the tests **asserting** (assert-but-non-gating), it
does NOT convert them to a report-only logging mode. Rationale:

- The asserts ARE the KPI definition. A breach surfaces as a non-blocking red
  signal on the `perf-kpis` job (via `continue-on-error`), which is exactly the
  "visible but not blocking" posture the JTBD wants. Converting to report-only
  would discard the budget as a machine-checkable definition and turn every KPI
  into a number a human must eyeball.
- **The p95 number already prints on breach.** The existing assert message
  includes the measured value:
  `assert!(p95_us <= 200, "KPI 1: place p95 must be ≤ 200 µs; got {p95_us} µs ...")`
  (verified at `crates/cinder/tests/v1_slice_01_wal_durability.rs:282-285`). On a
  breach, the panic message prints the got-value to the job log, so C4
  (visibility-on-breach) holds with NO test change. This is the load-bearing
  observation that lets F3 resolve to "keep asserts" without any source edit.

**No test change is required (preferred).** The only change is WHERE the tests
run (the new job's env), driven entirely by the CI workflow. The 28 test files
are untouched (C8; D1).

**Print-on-PASS is OUT OF SCOPE for this feature (deferred).** Today the p95
number prints only on breach (the assert message); on a PASS the assert succeeds
silently and no number is logged. Emitting the p95 on every run (pass or breach)
would require adding an `eprintln!` of the measured value to each of the 28
tests — a 28-file source edit that contradicts D1 ("the 28 tests are NOT
modified") and C8 (CI + docs only, no `crates/*/src` or test-body change beyond
the env lever). Visibility-on-breach (C4 as written: "the measured p95 value is
printed ... regardless of pass or breach" — satisfied for the breach case by the
assert message) is met without it. Print-on-PASS is recorded here as a clean,
optional successor (a uniform `eprintln!("{kpi} p95 = {p95_us} µs")` before each
assert) if the maintainer later wants the trend visible on green runs; it is NOT
in this feature's scope.

### 6. The durable-op honesty note (F4, US-04)

This ADR is the citable home for the honesty note. Recorded here, once:

- **The durable-op budgets reflect durable fsync cost since ADR-0049 /
  ADR-0060.** `place` (200 us), `enqueue` (300 us), and the per-crate WAL
  `ingest` budgets each now include a **per-record `sync_all`** on the WAL append
  path (ADR-0049 §4 for pulse; ADR-0060 §3 for the other six stores). These
  budgets were written for *non-durable* operations before that durability
  landed.
- **They are dev-indicative, not CI-contractual.** On dev hardware the budgets
  are meaningful targets; on GitHub Actions shared/virtualised storage fsync p95
  is routinely milliseconds, so the durable-op budgets are NOT a contract the CI
  runner can be held to. That is why they run in a non-gating job (Decision 2-3),
  not in Gate 1.
- **This is a deliberate Earned-Trust consequence, NOT a regression.** A
  durable-op p95 of several milliseconds on CI is the *expected* cost of an
  honest `sync_all` per record on shared storage. A contributor seeing it should
  read it as durable-fsync cost, not a code regression.
- **Threshold-raising is explicitly NOT the fix.** Per project memory
  `project_p95_wallclock_flakes_overnight` and ADR-0058's still-valid rejection
  of threshold-raises, the correct posture is the non-gating signal (Decision
  2-3), never absorbing CI variance into a larger budget literal.
- **The caveat applies to fsync-bearing durable ops only.** `place`, `enqueue`,
  and the WAL `ingest` family pay the per-record fsync. In-memory ops keep
  meaningful budgets even on CI: `cinder get_tier` (50 us, in-mem read),
  `augur observe` (10 us / 20 us, in-mem), `sluice enqueue_and_dequeue` (50 us,
  in-mem path). Their budgets are NOT dev-indicative caveats; they remain
  legitimate even on shared CI storage (subject only to ordinary scheduler
  noise, which is why they too live in the non-gating job rather than the gate).

**Placement: ADR-only (no per-site comment).** A one-line reference comment at
each durable-op test site was considered and rejected for this feature: it would
touch up to ~7 of the 28 test files for documentation only, against D1/C8's
"the 28 tests are NOT modified". The ADR is the single citable home; a contributor
who hits a breach follows the `perf-kpis` job to this ADR. A per-site comment
remains available as optional future polish if the indirection proves a friction
point in practice.

### 7. No local hook change (F6, C6)

`scripts/hooks/pre-commit` already does NOT set `KALEIDOSCOPE_PERF_TESTS`
(verified at `scripts/hooks/pre-commit:92-93`: `cargo test --workspace
--all-targets --locked` with no env). The perf tests already self-skip locally
per ADR-0058. **No change is made to the local hook**, and the variable MUST NOT
be added to it. Adding it would re-introduce the local flake ADR-0058 fixed.

### 8. No crate change, no version bump, no public-API impact (C8)

This feature touches `.github/workflows/ci.yml` and documentation only. No
`crates/*/src` production source changes, no test-body changes, no `Cargo.toml`
version bump. The public-API / SemVer note is therefore **none**: Gate 2
(`cargo public-api`) and Gate 3 (`cargo semver-checks`) see no surface change.
No crate is bumped; never 1.0.0 (CLAUDE.md; project memory
`semver_one_zero_is_andreas_call`).

## Reuse Analysis (MANDATORY)

| Capability needed | Existing asset | Verdict | Justification |
|---|---|---|---|
| Make perf tests skip when not wanted | The ADR-0058 presence-based env guard (`if KALEIDOSCOPE_PERF_TESTS unset -> eprintln + return`), byte-identical at all 28 sites | **REUSE (unchanged)** | The guard is the lever. Removing the var from `gate-1-test` makes all 28 self-skip there; setting it in the new job runs them all. No new mechanism; no guard edit (`crates/cinder/tests/...:256` representative of all 28). |
| Print the p95 on breach (C4 visibility) | The existing assert message embedding the got-value (`"...got {p95_us} µs..."`, `crates/cinder/tests/...:282-285`) | **REUSE (unchanged)** | The number already prints on a panic. No report-only mode, no `eprintln!` edit needed for visibility-on-breach. This is what lets F3 resolve to "keep asserts" with zero test change. |
| Run a CI job whose failure does not block the workflow | GitHub Actions `continue-on-error: true` job primitive | **REUSE (GitHub primitive)** | Native, well-understood, visible-but-non-blocking. No bespoke `\|\| true` wrapper, no separate workflow file, no third-party action. |
| Hardcode the env literal at job level | The existing `NIGHTLY_PIN` job-level literal pattern (ADR-0058 §3; gates 2/3) | **REUSE (pattern)** | The new job sets `KALEIDOSCOPE_PERF_TESTS: "1"` as a hardcoded literal in its own job-level `env`, same shape ADR-0058 already used and the workspace already relies on for the job-level env quirk. |
| The workflow file to edit | `.github/workflows/ci.yml` (existing) | **EXTEND** | Remove the env block from `gate-1-test` (Decision 1); add one new job (Decision 2). Extend the existing file, do not create a parallel workflow. |
| The non-gating perf job itself | none (no perf-specific job exists; perf rode inside `gate-1-test`) | **CREATE (the only new asset)** | Justified: there is no existing non-gating job that sets the variable. The de-gating (Decision 1) removes perf from Gate 1; something must still run the family (C4). The new `perf-kpis` job is the minimal new asset, composed entirely of reused primitives (the guard, the `continue-on-error` primitive, the literal-env pattern). |
| Document the durable-op honesty note | none (ADR-0058 rejected threshold-raises but did not record the durable-cost reason) | **CREATE (this ADR)** | The reason did not exist when ADR-0058 was written (ADR-0049/0060 landed later). This ADR is the citable home (Decision 6). |

**Reuse verdict**: EXTEND `ci.yml`; REUSE the ADR-0058 self-skip guard, the
existing assert-message visibility, the `continue-on-error` GitHub primitive,
and the job-level-literal env pattern; CREATE only the single new `perf-kpis`
job and this ADR. No code, no test-body, no crate version touched.

## Consequences

### Positive

- **A red `gate-1-test` means exactly one thing: a correctness regression.**
  Hardware-variance perf breaches produce zero false reds in the gating job
  (US-01; KPI-1 target 0). The maintainer can trust red again.
- **Correctness gating is fully preserved.** `gate-1-test` still runs `cargo
  test --workspace --all-targets --locked`; every non-perf test still executes
  and asserts; a real correctness break still reds Gate 1 (US-03 negative
  control; C2).
- **The perf signal stays visible.** The non-gating `perf-kpis` job runs the
  whole family on every push and surfaces a breach as a non-blocking red X with
  the p95 number in the log (US-02; C4; KPI-2 target 100% of runs produce
  readable numbers on breach).
- **The honesty about durable cost is recorded once, citably.** A future
  contributor reads a durable-op breach as expected fsync cost, not a regression,
  and does not threshold-chase (US-04; KPI-4 guardrail 0 threshold-raise commits).
- **No durability is weakened, no threshold is raised, no test is deleted**
  (C1, C3). The fix is gating semantics and location, nothing else.
- **Aligned with the trunk-based posture.** A non-gating, feedback-only perf
  signal matches `project_kaleidoscope_pure_trunk_based` (CI is feedback, not a
  gate; C7).

### Negative

- **A perf breach can land on `main` un-noticed if no one reads the job.** Being
  non-gating, a real sustained regression is observed only by a maintainer
  reading the `perf-kpis` trend, not stopped automatically. Accepted: this is the
  deliberate trade (visible feedback over a gate that flakes on shared hardware),
  and the JTBD explicitly wants a *human* trend judgement, not an
  auto-block, given the CI substrate cannot honour the durable-op budgets.
- **On a PASS the p95 number is not logged** (only on breach, via the assert
  message). Accepted for this feature; print-on-PASS is a recorded successor
  (Decision 5) deferred to keep the 28 test files untouched.
- **Silent re-gating drift risk.** A future feature could re-add
  `KALEIDOSCOPE_PERF_TESTS=1` to a gating job. Mitigated: this ADR makes
  non-gating the citable standard, and the DEVOPS structural acceptance (the
  "For Acceptance Designer" note in the brief) checks that `gate-1-test` carries
  no perf env and that the `continue-on-error` perf job exists.

## Alternatives considered

### Raise the thresholds to absorb CI fsync variance (rejected)

For: the budgets would pass on CI. Against: project memory
`project_p95_wallclock_flakes_overnight` forbids threshold-raises as the fix,
and ADR-0058 already rejected it (still-valid stance, preserved). Raising a
budget to several milliseconds to absorb shared-disk fsync would erase the
budget's meaning as a dev-hardware target and hide a genuine regression behind a
loosened number. The budget is correct for the op's *intent*; the op became
durable underneath it. Rejected, hard. (C3.)

### Delete the wall-clock KPI tests (rejected)

For: no flake, no maintenance. Against: discards the perf coverage entirely; a
real sustained regression would go completely unobserved (violates C4). The
tests are valuable as a tracked signal; only their *gating* on shared CI hardware
is the problem. Rejected — de-gating is not deleting.

### Keep perf gating, but on dedicated/self-hosted hardware (considered, rejected for v0)

For: a controlled runner could honour the durable-op budgets, restoring a
trustworthy gate. Against: it introduces a self-hosted runner — operational
cost, security surface, and maintenance that the project (pure trunk-based, no
CI as a gate per `project_kaleidoscope_pure_trunk_based`) deliberately avoids.
It also re-creates the train-the-team-to-ignore-red harm the first time the
dedicated runner has a slow minute. The non-gating signal achieves the actual
goal (visibility without false blocking) at zero infrastructure cost. Considered
and rejected for v0; recorded as available future work if a dedicated perf runner
is ever wanted for richer latency tracking.

### `|| true` on the perf test step instead of `continue-on-error` (rejected)

For: also non-blocking. Against: `|| true` makes the STEP exit zero, so the step
shows GREEN even on a real breach — the breach becomes invisible, defeating C4.
`continue-on-error: true` keeps the breach VISIBLE (red X on the job) while
non-blocking. Rejected for hiding the signal.

### A separate workflow file for the perf job (considered, deferred to DEVOPS)

For: full isolation. Against: more moving parts than a single job warrants, and
it loses at-a-glance co-location with the gates on the same run page. DESIGN
recommends a single `continue-on-error` job in `ci.yml`; the DEVOPS wave owns
the final placement (job vs separate file) and may choose otherwise with
rationale, provided the contract holds (non-gating, every-push, whole family,
var set).

### Convert the tests to report-only (log p95, never panic) (rejected)

For: the number always prints, even on PASS. Against: it discards the assert as
the machine-checkable KPI definition and requires editing all 28 test bodies
(violates D1/C8). The existing assert message already prints the number on
breach (C4 satisfied), so report-only buys only print-on-PASS, which is recorded
as a deferred successor without the 28-file edit. Rejected for this feature.

## Verification

- **Structural (the acceptance is structural, see the brief's "For Acceptance
  Designer" note)**: `gate-1-test` carries NO `KALEIDOSCOPE_PERF_TESTS` in its
  job-level `env`; a new `continue-on-error: true` perf job exists and DOES set
  `KALEIDOSCOPE_PERF_TESTS: "1"` and runs `cargo test --workspace`.
- **Negative control (US-03; C2)**: a deliberately failing non-perf correctness
  test still reds `gate-1-test` — DISTILL demonstrates this on a throwaway
  branch. De-gating perf must provably NOT de-gate correctness.
- **Self-skip (US-01)**: with the variable absent in `gate-1-test`, all 28 perf
  tests hit the ADR-0058 early-return preamble and pass without measuring; the
  gating invocation `cargo test --workspace --all-targets --locked` is unchanged.
- **Visibility-on-breach (US-02; C4)**: the existing assert message prints the
  measured p95 (`crates/cinder/tests/...:282-285`); on a breach the `perf-kpis`
  job log shows the number and a non-blocking red X; the overall workflow
  conclusion stays success.
- **No threshold / test / durability change (C1, C3, C8)**: no budget literal,
  sample count, warm-up loop, or percentile index changes; no test is deleted;
  no `sync_all` is removed; the durability work of ADR-0049/0060 is untouched.
- **No public-API impact (C8)**: Gate 2 / Gate 3 see no surface change; no crate
  version bump.

## External-integration handoff

None. This is a CI-workflow and documentation change. No network service, no
third-party API, no consumer-driven contract test recommendation. The perf tests
read and write the in-process filesystem under a tempdir; their durability cost
is the in-process fsync the ADR-0049/0060 lineage made honest.
