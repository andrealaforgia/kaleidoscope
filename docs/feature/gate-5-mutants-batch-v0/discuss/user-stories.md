<!-- markdownlint-disable MD024 -->

# User Stories — gate-5-mutants-batch-v0

British English. No em dashes in body.

Single-story batch slice. Pure CI workflow extension. No production
code change. Eight new `gate-5-mutants-<crate>` jobs added to
`.github/workflows/ci.yml`, one per residual crate from the
`gate-5-mutants-lumen-v0` audit.

## System Constraints

- **No production source change**. The single DELIVER commit edits
  `.github/workflows/ci.yml` only. No file under `crates/*/src/` is
  touched.
- **No new tooling**. `cargo-mutants` is already installed by 17
  sibling jobs.
- **No new dependency**. The new jobs consume only `cargo`, `git`,
  and the workspace as it stands.
- **ADR-0005 immutability**. The five-gate contract is unchanged;
  this feature adds eight per-crate instances of Gate 5, not a sixth
  gate.
- **Pattern fidelity**. Each new job is byte-shaped after the
  existing `gate-5-mutants-lumen` job at `.github/workflows/ci.yml`
  lines 1210 to 1295 (equivalently after
  `gate-5-mutants-log-query-api` at lines 1123 to 1208). Differences
  are limited per crate to: package name, diff filter path, cache key
  shard, artefact name.
- **Uniform `needs:` graph**. All eight new jobs use
  `[gate-2-public-api, gate-3-semver]`, matching every existing
  sibling. PIN.

---

## US-01: Eight `gate-5-mutants-<crate>` jobs shipped

Tag: `@infrastructure`

### Elevator Pitch

- **Before**: 8 of the 25 workspace crates (`aegis`, `augur`,
  `sluice`, `beacon-server`, `cinder`, `loom`, `integration-suite`,
  `kaleidoscope-gateway`) have no `gate-5-mutants-<crate>` job in
  `.github/workflows/ci.yml`. A mutation on the JWT signature
  predicate in `aegis`, on the Z-score comparator in `augur`, on the
  bounded-capacity check in `sluice`, on the age-based lifecycle
  comparator in `cinder`, on the TOML catalogue diff in `loom`, or on
  the host wiring in `beacon-server` / `kaleidoscope-gateway` lands
  silently on `main`. The other four ADR-0005 gates (`cargo test`,
  `cargo public-api`, `cargo semver-checks`, `cargo deny`) all stay
  green. The "100% kill rate per crate" invariant from ADR-0005
  Gate 5 is quietly unenforced for eight crates while the other 17
  enjoy uniform coverage.
- **After**: a maintainer runs `grep -c "^  gate-5-mutants-"
  .github/workflows/ci.yml` and finds 25 jobs (was 17). The eight new
  jobs are named `gate-5-mutants-aegis`, `gate-5-mutants-augur`,
  `gate-5-mutants-sluice`, `gate-5-mutants-beacon-server`,
  `gate-5-mutants-cinder`, `gate-5-mutants-loom`,
  `gate-5-mutants-integration-suite`, and
  `gate-5-mutants-kaleidoscope-gateway`. Each runs
  `cargo mutants -p <crate-name> --in-diff origin/main` on every PR
  that touches `crates/<crate-dir>/**`. A mutation that survives in
  any of the eight crates surfaces as a red check on the PR status
  panel.
- **Decision enabled**: every one of the 25 workspace crates has
  uniform ADR-0005 Gate 5 enforcement. Future extensions to the auth
  predicate in `aegis`, the observer arithmetic in `augur`, the queue
  port logic in `sluice`, the tier metadata in `cinder`, the
  validate-plan-apply flow in `loom`, or the host composition in
  `beacon-server` / `kaleidoscope-gateway` land with the same
  mutation-test discipline that the read APIs, `lumen`, `pulse`,
  `ray`, `strata`, `beacon`, and `kaleidoscope-cli` already enjoy.

### Problem

In the DISCUSS wave of `gate-5-mutants-lumen-v0` (commit a11910f),
Luna audited the workspace and recorded an enumeration of nine crates
without a `gate-5-mutants-*` job in the workflow file (see
`docs/feature/gate-5-mutants-lumen-v0/discuss/story-map.md` lines 129
to 141). The `lumen` feature subsequently closed one of those nine
(commit d96a807, also the sibling
`gate-5-mutants-query-http-common-v0` confirmed the pattern at commit
a6175f1). Eight crates remain residual: `aegis`, `augur`, `sluice`,
`beacon-server`, `cinder`, `loom`, `integration-suite`,
`kaleidoscope-gateway`.

A maintainer extending the JWT validation in `aegis::validate`, the
Welford-algorithm Z-score in `augur::ZScoreObserver`, the bounded
capacity in `sluice::Queue`, the age comparator in
`cinder::LifecyclePolicy`, the TOML diff in `loom::plan`, or the host
wiring in `beacon-server`'s scheduler today has no automatic signal
that a mutation in their code is caught by the test suite. The
discipline depends on a manual `cargo mutants --package <crate>` run
that nobody owns. The five gates appear green on the PR while the
"100% kill rate per crate" invariant from ADR-0005 Gate 5 is
silently unenforced for those eight crates.

### Who

- **User type**: Kaleidoscope crate maintainer extending any one of
  the eight residual crates (`aegis`, `augur`, `sluice`,
  `beacon-server`, `cinder`, `loom`, `integration-suite`,
  `kaleidoscope-gateway`).
- **Context**: opens a PR that touches `crates/<crate-dir>/src/**`
  for one of the eight. Reads the GitHub PR status panel before
  merging. Trusts a green panel as evidence that ADR-0005 Gate 5 has
  fired against the diff.
- **Motivation**: wants every extension to land with the same
  mutation-resistance enforcement that the 17 already-covered crates
  enjoy. Wants uniform coverage across all 25 workspace crates.

### Solution

Add eight new GitHub Actions jobs to `.github/workflows/ci.yml`, one
per residual crate. Each new job is named
`gate-5-mutants-<package-name>` and replicates the shape of the
existing `gate-5-mutants-lumen` job at lines 1210 to 1295, with four
token substitutions per crate: package name, diff filter path
(`crates/<crate-dir>/**`), cache key shard, and artefact name. Each
job is wired into the same `needs` graph as the sibling jobs
(`[gate-2-public-api, gate-3-semver]`). No other workflow file is
touched. No production source file is touched. No `Cargo.toml`,
`Cargo.lock`, or `deny.toml` is touched.

### Target crates enumerated

The exact `package.name` values are taken from each crate's
`Cargo.toml`:

| # | Crate dir | `package.name` | Why close | Risk if not closed |
|---|---|---|---|---|
| 1 | `crates/aegis/` | `aegis` | JWT validation + TOML tenant catalogue + RBAC predicates | mutation on auth comparator lands silently |
| 2 | `crates/augur/` | `augur` | Z-score and rare-event observer arithmetic | mutation on threshold or comparator lands silently |
| 3 | `crates/sluice/` | `sluice` | queue port FIFO + at-least-once + bounded capacity | mutation on capacity predicate or order check lands silently |
| 4 | `crates/beacon-server/` | `beacon-server` | scheduler + HTTP client for PromQL backend | mutation on rule-firing trigger lands silently |
| 5 | `crates/cinder/` | `cinder` | tier metadata + age-based lifecycle evaluator | mutation on age comparator (`>=` to `>`) lands silently |
| 6 | `crates/loom/` | `loom` | Git-backed validate / plan / apply for TOML catalogues | mutation on diff logic or plan ordering lands silently |
| 7 | `crates/integration-suite/` | `integration-suite` | cross-crate composition assertions (test-only) | future production code would land without coverage; see Flag 3 in `wave-decisions.md` |
| 8 | `crates/kaleidoscope-gateway/` | `kaleidoscope-gateway` | host composition binary (aperture + StorageSink wiring) | mutation on startup wiring or sink injection lands silently |

### Domain Examples

#### 1. Happy path: a future `aegis::Catalogue` extension lands with mutation coverage

A maintainer adds a `disabled_tenants: Vec<TenantId>` field to the
TOML catalogue parser in `crates/aegis/src/catalogue.rs`, a builder
method, and a new `is_authorised` arm that short-circuits when the
tenant is in the disabled list. They open a PR. The CI workflow runs
17 pre-existing `gate-5-mutants-*` jobs PLUS the eight new ones. The
`gate-5-mutants-aegis` job's `--in-diff` filter targets
`crates/aegis/**`, picks up the edited `catalogue.rs`, mutates the
`disabled_tenants.contains(&tenant_id)` call to
`!disabled_tenants.contains(&tenant_id)`, and the unit test in the
catalogue test module catches the mutation. The job reports zero
surviving mutations. The PR shows a green check for
`gate-5-mutants-aegis`. The maintainer merges with confidence.

#### 2. Edge case: a PR that touches none of the eight target crates short-circuits in seconds for each new job

A maintainer opens a PR that adds a route to
`crates/log-query-api/src/lib.rs` and touches none of the eight
target crates. Each of the eight new jobs runs
`git diff origin/main HEAD -- 'crates/<crate-dir>/**' > "$DIFF_FILE"`.
The diff file is empty for all eight. Each script emits its
`No <crate>-touching changes vs origin/main; skipping mutation
testing.` message and exits zero. All eight new jobs report green in
seconds. No `cargo mutants` invocation runs in any of them. CI spend
is bounded by the per-crate-touching subset of PRs.

#### 3. Failure mode: a surviving mutation in a new `cinder::LifecyclePolicy` age boundary raises a red check on `gate-5-mutants-cinder`

A maintainer adds a `min_age_days: u64` field to
`crates/cinder/src/lifecycle.rs`, a builder method, and a new
`should_demote` arm with `record.age_days >= bound`. They write a
unit test that exercises `bound = 30` against `age_days = 60`
(demotes) and `age_days = 10` (does not demote). They forget the
boundary test at `age_days = 30`. The `gate-5-mutants-cinder` job
mutates `>=` to `>`. No test catches the mutation. The job reports
one surviving mutation. The PR shows a red check for
`gate-5-mutants-cinder`. The maintainer adds a boundary test at
`age_days = 30` and pushes again; the mutation now dies; the check
turns green.

### UAT Scenarios (BDD)

Three scenarios. Each describes a verifiable outcome on the
post-feature workflow. The scenario titles describe WHAT the
maintainer observes, not HOW the job blocks are implemented.

```gherkin
Scenario: All eight new gate-5-mutants jobs exist in the CI workflow
  Given the .github/workflows/ci.yml file at the feature-close commit
  When a maintainer runs grep -c "^  gate-5-mutants-" .github/workflows/ci.yml
  Then the count is exactly 25
  And there is exactly one job named gate-5-mutants-aegis
  And there is exactly one job named gate-5-mutants-augur
  And there is exactly one job named gate-5-mutants-sluice
  And there is exactly one job named gate-5-mutants-beacon-server
  And there is exactly one job named gate-5-mutants-cinder
  And there is exactly one job named gate-5-mutants-loom
  And there is exactly one job named gate-5-mutants-integration-suite
  And there is exactly one job named gate-5-mutants-kaleidoscope-gateway
  And each of the eight new job blocks contains a step that runs cargo mutants -p <crate-name> --in-diff origin/main with the package name matching the job name
  And each of the eight new job blocks uses the diff filter path glob crates/<crate-dir>/**
```

```gherkin
Scenario: A PR that does not touch any of the eight target crates short-circuits all eight new jobs in seconds
  Given a PR whose git diff against origin/main is empty under crates/aegis/**, crates/augur/**, crates/sluice/**, crates/beacon-server/**, crates/cinder/**, crates/loom/**, crates/integration-suite/**, and crates/kaleidoscope-gateway/**
  When the eight new gate-5-mutants jobs run
  Then each job emits the message "No <crate>-touching changes vs origin/main; skipping mutation testing." with its own crate name
  And each job exits with status 0
  And no cargo mutants invocation is recorded in any of the eight job logs
  And each job completes in under one minute
```

```gherkin
Scenario: Zero regression on the 17 pre-existing gate-5-mutants jobs and zero regression on every other CI job
  Given the post-feature workflow at .github/workflows/ci.yml
  When a maintainer enumerates jobs matching gate-5-mutants-
  Then exactly 25 jobs are returned
  And the 17 pre-feature gate-5-mutants jobs are still present with unchanged names
  And none of the 17 pre-feature gate-5-mutants jobs has had its needs graph altered
  And none of the 17 pre-feature gate-5-mutants jobs has had its script altered
  And every non-gate-5-mutants job in the workflow file is byte-identical to its pre-feature form
  And python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" exits with status 0
```

### Acceptance Criteria

- [ ] AC-1: `grep -c "^  gate-5-mutants-" .github/workflows/ci.yml`
      returns exactly 25 at the feature-close commit (was 17
      pre-feature).
- [ ] AC-2: there is exactly one job block named
      `gate-5-mutants-aegis` whose script line contains
      `cargo mutants -p aegis` (or `--package aegis`) and the diff
      filter path glob `crates/aegis/**`.
- [ ] AC-3: there is exactly one job block named
      `gate-5-mutants-augur` whose script line contains
      `cargo mutants -p augur` (or `--package augur`) and the diff
      filter path glob `crates/augur/**`.
- [ ] AC-4: there is exactly one job block named
      `gate-5-mutants-sluice` whose script line contains
      `cargo mutants -p sluice` (or `--package sluice`) and the diff
      filter path glob `crates/sluice/**`.
- [ ] AC-5: there is exactly one job block named
      `gate-5-mutants-beacon-server` whose script line contains
      `cargo mutants -p beacon-server` (or `--package beacon-server`)
      and the diff filter path glob `crates/beacon-server/**`.
- [ ] AC-6: there is exactly one job block named
      `gate-5-mutants-cinder` whose script line contains
      `cargo mutants -p cinder` (or `--package cinder`) and the diff
      filter path glob `crates/cinder/**`.
- [ ] AC-7: there is exactly one job block named
      `gate-5-mutants-loom` whose script line contains
      `cargo mutants -p loom` (or `--package loom`) and the diff
      filter path glob `crates/loom/**`.
- [ ] AC-8: there is exactly one job block named
      `gate-5-mutants-integration-suite` whose script line contains
      `cargo mutants -p integration-suite` (or
      `--package integration-suite`) and the diff filter path glob
      `crates/integration-suite/**`.
- [ ] AC-9: there is exactly one job block named
      `gate-5-mutants-kaleidoscope-gateway` whose script line
      contains `cargo mutants -p kaleidoscope-gateway` (or
      `--package kaleidoscope-gateway`) and the diff filter path glob
      `crates/kaleidoscope-gateway/**`.
- [ ] AC-10: each of the eight new jobs has
      `needs: [gate-2-public-api, gate-3-semver]`, matching every
      existing sibling.
- [ ] AC-11: zero rename or removal across the 17 pre-existing
      `gate-5-mutants-*` jobs, verified by a `diff` of pre and
      post-feature job-name enumerations.
- [ ] AC-12: zero net new external dependency, verified by `diff` on
      `Cargo.toml`, `Cargo.lock`, `deny.toml`, and the
      `taiki-e/install-action` tool field (still `cargo-mutants`).
- [ ] AC-13: `python3 -c "import yaml;
      yaml.safe_load(open('.github/workflows/ci.yml'))"` exits with
      status 0 at the feature-close commit.

### Outcome KPIs

See `outcome-kpis.md` for the full K1 to K6 table.

- **Who**: maintainers of the eight residual crates opening PRs that
  touch their respective `crates/<crate-dir>/src/**`.
- **Does what**: receive an automated mutation-test signal as a CI
  check on the PR status panel for their crate, with no manual
  `cargo mutants --package <crate>` invocation required.
- **By how much**: target = 8 new jobs in the workflow file (one per
  residual crate); baseline = 0 (no jobs in the pre-feature workflow
  for any of the eight).
- **Measured by**: post-feature `grep` over
  `.github/workflows/ci.yml`; observation of each job's verdict on
  the first post-feature PR that touches the respective crate (or a
  synthetic mutation PR opened by the maintainer to confirm the gate
  fires).
- **Baseline**: zero; the pre-feature workflow has no
  `gate-5-mutants-<crate>` job for any of the eight (verified by
  `gate-5-mutants-lumen-v0/discuss/story-map.md` lines 129 to 141).

### Technical Notes

- The new jobs' bodies are parameterised on the existing
  `gate-5-mutants-lumen` job (workflow lines 1210 to 1295). This is
  the immediate same-shape precedent shipped most recently
  (commit d96a807).
- The DESIGN wave (Morgan / Apex) decides:
  - The exact placement of the eight new job blocks in the YAML file
    (alphabetic insertion is the recommended slot; appending at the
    end of the gate-5 block is a secondary option). See FLAG 1 in
    `wave-decisions.md`.
  - Whether to copy the `needs` graph of `gate-5-mutants-lumen`
    verbatim (`[gate-2-public-api, gate-3-semver]`) for all eight,
    which is the recommended posture and the standard for every
    sibling job. PIN. See FLAG 2 in `wave-decisions.md`.
  - How to handle the `integration-suite` test-only-crate case
    (recommended: ship the job anyway and rely on the empty-diff
    short-circuit; alternative: explicit policy exclusion). See
    FLAG 3 in `wave-decisions.md`.
- The DELIVER wave is the single workflow edit. The DELIVER commit
  message must reference this feature ID
  (`gate-5-mutants-batch-v0`).
- No public-API change. None of the eight crates is in Gate 2 or
  Gate 3's locked set (per ADR-0005 and the `gate-2-public-api` /
  `gate-3-semver` job scopes), so no semver-checks regression is
  possible.
- No `deny.toml` change. `cargo-mutants` is installed by
  `taiki-e/install-action` from a precompiled binary, not added to
  the workspace dependency graph.

### Dependencies

- **Resolved**: ADR-0005 (CI contract, Gate 5 named); the 17
  pre-existing `gate-5-mutants-*` jobs (pattern reference); the
  `gate-5-mutants-lumen-v0` DISCUSS / DESIGN / DEVOPS waves (pattern
  precedent and gap enumeration); the `query-http-common-v0` DEVOPS
  wave (precedent for adding a new gate-5 job via a single workflow
  file edit).
- **Tracked, not blocking**: none. This feature closes the residual
  gap identified by the `gate-5-mutants-lumen-v0` audit. Post-feature,
  all 25 workspace crates have a `gate-5-mutants-*` job. A future
  feature that creates a new workspace crate will need to add a
  matching job; that is a separate concern (a workspace-template
  convention or a CI-codegen check) and is OUT of this feature's
  scope.
