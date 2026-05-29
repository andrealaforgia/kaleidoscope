<!-- markdownlint-disable MD024 -->

# User Stories — gate-5-mutants-lumen-v0

British English. No em dashes in body.

Single-story slice. Pure CI workflow extension. No production code change.

## System Constraints

- **No production source change**. The single DELIVER commit edits
  `.github/workflows/ci.yml` only. No file under `crates/lumen/src/` is
  touched.
- **No new tooling**. `cargo-mutants` is already installed by sixteen
  sibling jobs.
- **No new dependency**. The job consumes only `cargo`, `git`, and the
  workspace as it stands.
- **ADR-0005 immutability**. The five-gate contract is unchanged; this
  feature adds one per-crate instance of Gate 5, not a sixth gate.
- **Pattern fidelity**. The new job is byte-shaped after the existing
  `gate-5-mutants-log-query-api` job at `.github/workflows/ci.yml`
  lines 1123 to 1208. Differences are limited to: package name, diff
  filter path, cache key shard, artefact name.

---

## US-01: `gate-5-mutants-lumen` job shipped

Tag: `@infrastructure`

### Elevator Pitch

- **Before**: a mutation that flips `==` to `!=` inside
  `Predicate::matches`, or that swaps a `Some` arm and a `None` arm,
  lands silently on `main`. The other four ADR-0005 gates (`cargo
  test`, `cargo public-api`, `cargo semver-checks`, `cargo deny`) all
  stay green. The "100% kill rate per crate" invariant from ADR-0005
  Gate 5 is violated quietly, because the workflow has no job that
  runs `cargo mutants` on the `lumen` package.
- **After**: a maintainer runs `gh workflow view ci.yml` (or `grep
  "gate-5-mutants-lumen:" .github/workflows/ci.yml`) and finds a job
  named `gate-5-mutants-lumen` whose script line executes
  `cargo mutants --package lumen --in-diff "$DIFF_FILE"
  --no-shuffle --jobs 2`. On every PR that touches
  `crates/lumen/**`, the job runs; a mutation that survives surfaces
  as a red check on the PR status panel.
- **Decision enabled**: `lumen` maintainers receive the mutation
  signal automatically. The next `Predicate` extension (a new
  `body_contains`-shaped arm, a new `service` arm, a new severity
  comparator) lands with the same coverage discipline that the
  read APIs already enjoy.

### Problem

Apex (Kaleidoscope's platform architect) closed
`log-body-text-search-v0` on 2026-05-27 with an honest finding in
`docs/feature/log-body-text-search-v0/devops/wave-decisions.md` lines
56 to 89: the recent `Predicate` extensions in
`crates/lumen/src/predicate.rs` are covered by `cargo test
--workspace` (Gate 1) but are NOT covered by any per-crate `cargo
mutants` job. The workflow has sixteen `gate-5-mutants-*` jobs as of
the previous feature close, none of them with `--package lumen`.

A maintainer extending `Predicate` today has no automatic signal that
a mutation in the `matches` arm (e.g. flipping `body.contains(needle)`
to `!body.contains(needle)`) is caught by the test suite. The
discipline depends on a manual `cargo mutants --package lumen` run
that nobody owns. The five gates appear green on the PR while the
"100% kill rate per crate" invariant from ADR-0005 Gate 5 is
silently unenforced for `lumen`.

### Who

- **User type**: Kaleidoscope crate maintainer extending the `lumen`
  storage engine (predicates, store adapters, query shapes).
- **Context**: opens a PR that touches `crates/lumen/src/**`. Reads
  the GitHub PR status panel before merging. Trusts a green panel as
  evidence that ADR-0005 Gate 5 has fired against the diff.
- **Motivation**: wants every Predicate extension to land with the
  same mutation-resistance enforcement that `log-query-api`, `pulse`,
  `ray`, `strata`, `beacon`, and `kaleidoscope-cli` already enjoy.

### Solution

Add one new GitHub Actions job to `.github/workflows/ci.yml`. The new
job is named `gate-5-mutants-lumen` and replicates the shape of the
existing `gate-5-mutants-log-query-api` job at lines 1123 to 1208,
with four token substitutions: package name
(`log-query-api` → `lumen`), diff filter path
(`crates/log-query-api/**` → `crates/lumen/**`), cache key shard, and
artefact name. The job is wired into the same `needs` graph as the
sibling jobs (`gate-2-public-api`, `gate-3-semver`). No other workflow
file is touched. No production source file is touched.

### Domain Examples

#### 1. Happy path: a future `Predicate::host_contains` extension lands with mutation coverage

A maintainer adds a `host_contains: Option<String>` field to
`crates/lumen/src/predicate.rs`, a `host_contains(host: &str)`
builder, a new `matches` arm calling `host.contains(needle)`, and a
new `is_empty` clause. They open a PR. The CI workflow runs sixteen
`gate-5-mutants-*` jobs PLUS the new `gate-5-mutants-lumen` job. The
new job's `--in-diff` filter targets `crates/lumen/**`, picks up the
edited `predicate.rs`, mutates the `host.contains(needle)` call to
`!host.contains(needle)`, and the unit test in
`crates/lumen/src/predicate.rs` `mod tests` catches the mutation. The
job reports zero surviving mutations. The PR shows a green check for
`gate-5-mutants-lumen`. The maintainer merges with confidence.

#### 2. Edge case: a PR that touches no file under `crates/lumen/**` short-circuits in seconds

A maintainer opens a PR that adds a route to
`crates/log-query-api/src/lib.rs` but does not touch any file under
`crates/lumen/`. The new `gate-5-mutants-lumen` job runs `git diff
origin/main HEAD -- 'crates/lumen/**' > "$DIFF_FILE"`. The diff file
is empty. The script emits `No lumen-touching changes vs
origin/main; skipping mutation testing.` and exits zero. The job
reports green in seconds. No `cargo mutants` invocation runs. CI
spend is bounded by the lumen-touching subset of PRs.

#### 3. Failure mode: a surviving mutation in a new `min_duration_ms` arm raises a red check

A maintainer adds a `min_duration_ms: Option<u64>` field to
`Predicate`, a `min_duration_ms(value: u64)` builder, and a new
`matches` arm with `record.duration_ms.unwrap_or(0) >= bound`. They
write a unit test that exercises `bound = 1000` against
`duration_ms = 2000` (matches) and `duration_ms = 500` (does not
match). They forget the boundary test at `duration_ms = bound`. The
`gate-5-mutants-lumen` job mutates `>=` to `>`. No test catches the
mutation. The job reports one surviving mutation. The PR shows a red
check for `gate-5-mutants-lumen`. The maintainer adds a boundary test
at `duration_ms = 1000` and pushes again; the mutation now dies; the
check turns green.

### UAT Scenarios (BDD)

Three scenarios. Each scenario describes a verifiable outcome on the
post-feature workflow. The scenario titles describe WHAT the
maintainer observes, not HOW the job is implemented.

```gherkin
Scenario: The new gate-5-mutants-lumen job exists in the CI workflow
  Given the .github/workflows/ci.yml file at the feature-close commit
  When a maintainer runs grep "gate-5-mutants-lumen:" .github/workflows/ci.yml
  Then exactly one line is returned
  And the line is of the form "  gate-5-mutants-lumen:"
  And the surrounding job block contains a step that runs cargo mutants with --package lumen
  And the script line contains --in-diff "$DIFF_FILE"
  And the diff filter uses the path glob 'crates/lumen/**'
```

```gherkin
Scenario: A PR that does not touch crates/lumen/** short-circuits in seconds
  Given a PR whose git diff against origin/main is empty under crates/lumen/**
  When the gate-5-mutants-lumen job runs
  Then the job emits the message "No lumen-touching changes vs origin/main; skipping mutation testing."
  And the job exits with status 0
  And no cargo mutants invocation is recorded in the job log
  And the job completes in under one minute
```

```gherkin
Scenario: Zero regression on the other sixteen gate-5-mutants jobs
  Given the post-feature workflow at .github/workflows/ci.yml
  When a maintainer enumerates jobs matching gate-5-mutants-
  Then exactly seventeen jobs are returned
  And the sixteen pre-feature jobs are still present
  And the sixteen pre-feature job names are unchanged
  And the seventeenth job is gate-5-mutants-lumen
  And none of the sixteen pre-feature jobs has had its needs graph altered
  And none of the sixteen pre-feature jobs has had its script altered
```

### Acceptance Criteria

- [ ] AC-1: `grep "gate-5-mutants-lumen:" .github/workflows/ci.yml`
      returns exactly one line at the feature-close commit.
- [ ] AC-2: the new job's script line contains the literal
      `cargo mutants` and the literal `--package lumen`.
- [ ] AC-3: the new job's script line uses the diff filter path glob
      `crates/lumen/**`.
- [ ] AC-4: a synthetic PR that does not touch any file under
      `crates/lumen/**` causes the new job to emit `No
      lumen-touching changes vs origin/main; skipping mutation
      testing.` and exit with status 0.
- [ ] AC-5: post-feature, `grep -c "^  gate-5-mutants-"
      .github/workflows/ci.yml` returns 17 (was 16 pre-feature).
- [ ] AC-6: zero rename or removal across the sixteen pre-existing
      `gate-5-mutants-*` jobs, verified by a `diff` of pre and
      post-feature job-name enumerations.
- [ ] AC-7: zero net new external dependency, verified by `diff` on
      `Cargo.toml`, `Cargo.lock`, `deny.toml`, and the `taiki-e/install-action`
      tool field (still `cargo-mutants`).

### Outcome KPIs

See `outcome-kpis.md` for the full K1 to K4 table.

- **Who**: the `lumen` crate maintainer opening a PR that touches
  `crates/lumen/src/**`.
- **Does what**: receives an automated mutation-test signal as a
  required-shape CI check on the PR status panel, with no manual
  `cargo mutants --package lumen` invocation required.
- **By how much**: target = 1 (the new job exists, exercises the
  lumen diff, and reports a verdict on every lumen-touching PR);
  baseline = 0 (no `gate-5-mutants-lumen` job in the pre-feature
  workflow).
- **Measured by**: post-feature `grep` over
  `.github/workflows/ci.yml`; observation of the job's verdict on
  the first post-feature lumen-touching PR (or a synthetic mutation
  PR opened by the maintainer to confirm the gate fires).
- **Baseline**: zero; the pre-feature workflow has no
  `gate-5-mutants-lumen` job (verified by Apex's honest gap note at
  `docs/feature/log-body-text-search-v0/devops/wave-decisions.md`
  lines 56 to 89).

### Technical Notes

- The new job's body is parameterised on the existing
  `gate-5-mutants-log-query-api` job (the immediate sibling at
  workflow lines 1123 to 1208). Lumen is the storage backing for
  `log-query-api`, so the sibling choice is semantic, not arbitrary.
- The DESIGN wave (Apex / Morgan) decides:
  - The exact placement of the new job block in the YAML file
    (immediately after `gate-5-mutants-log-query-api` is the
    recommended slot; the alphabetic slot inside the gate-5 block is
    a secondary option). See FLAG 1 in `wave-decisions.md`.
  - Whether to copy the `needs` graph of
    `gate-5-mutants-log-query-api` verbatim
    (`[gate-2-public-api, gate-3-semver]`), which is the recommended
    posture and the standard for every sibling job. See FLAG 2 in
    `wave-decisions.md`.
- The DELIVER wave is the single workflow edit. The DELIVER commit
  message must reference this feature ID
  (`gate-5-mutants-lumen-v0`).
- No public-API change. `lumen`'s `cargo public-api` baseline is
  untouched. `lumen` is not in Gate 2 or Gate 3's locked set (per
  ADR-0005 and the `gate-2-public-api` / `gate-3-semver` job
  scopes), so no semver-checks regression is possible.
- No `deny.toml` change. `cargo-mutants` is installed by
  `taiki-e/install-action` from a precompiled binary, not added to
  the workspace dependency graph.

### Dependencies

- **Resolved**: ADR-0005 (CI contract, Gate 5 named); the existing
  sixteen `gate-5-mutants-*` jobs (pattern reference); the
  `log-body-text-search-v0` DEVOPS wave decisions (honest gap note
  that this feature closes); the `query-http-common-v0` DEVOPS wave
  decisions (precedent for adding a new gate-5 job via a single
  workflow file edit).
- **Tracked, not blocking**: eight other crates lack a
  `gate-5-mutants-*` job (see `story-map.md` appendix). They are
  documented for future maintenance work, not promoted to this
  feature's scope.
