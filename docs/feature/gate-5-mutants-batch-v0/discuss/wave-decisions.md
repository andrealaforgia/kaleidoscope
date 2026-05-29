# Wave Decisions — gate-5-mutants-batch-v0 / DISCUSS

British English. No em dashes in body.

- **Wave**: DISCUSS
- **Author**: Luna (`nw-product-owner`)
- **Date**: 2026-05-29
- **Mode**: lightweight. Single-story batch infrastructure slice that
  closes the residual gap surfaced by the
  `gate-5-mutants-lumen-v0` DISCUSS audit (recorded in
  `docs/feature/gate-5-mutants-lumen-v0/discuss/story-map.md`
  appendix, lines 129 to 141). No JTBD, no walking-skeleton coverage
  analysis, no journey YAML, no `.feature` file.

## Requirements Summary

The feature adds eight new GitHub Actions jobs to
`.github/workflows/ci.yml`, one per residual crate sprovvisto di
`gate-5-mutants-*` coverage. Each new job replicates the shape of the
existing `gate-5-mutants-lumen` job at workflow lines 1210 to 1295
(or, equivalently, `gate-5-mutants-log-query-api` at lines 1123 to
1208) with four token substitutions per crate:

| Token | Sibling value (`lumen`) | This feature (per crate) |
|---|---|---|
| Package name (`cargo mutants --package`) | `lumen` | one of the eight target package names |
| Diff filter path glob | `crates/lumen/**` | `crates/<crate-dir>/**` |
| Cache key shard | `cargo-mutants-lumen-` | `cargo-mutants-<crate>-` |
| Artefact name (`upload-artifact`) | `mutants-out-lumen` | `mutants-out-<crate>` |

The eight target crates, with their exact `package.name` values from
`crates/<dir>/Cargo.toml`:

| # | Crate dir | `package.name` | Library/binary | Risk if not closed |
|---|---|---|---|---|
| 1 | `aegis` | `aegis` | library | JWT validation, RBAC, tenant catalogue parsing — mutations on auth predicate land silently |
| 2 | `augur` | `augur` | library | Z-score and rare-event observer arithmetic — mutations on comparator or threshold land silently |
| 3 | `sluice` | `sluice` | library | Queue port FIFO + bounded capacity logic — mutations on at-least-once or bounded-capacity predicate land silently |
| 4 | `beacon-server` | `beacon-server` | binary + lib | Scheduler + HTTP client wiring for PromQL backend — mutations on rule-firing logic land silently |
| 5 | `cinder` | `cinder` | library | Tier metadata + lifecycle policy evaluator — mutations on age-based comparator land silently |
| 6 | `loom` | `loom` | binary + lib | Git-backed validate / plan / apply for TOML catalogues — mutations on diff logic land silently |
| 7 | `integration-suite` | `integration-suite` | test-only | Cross-crate composition assertions — see Flag 3 below |
| 8 | `kaleidoscope-gateway` | `kaleidoscope-gateway` | binary + lib | Host composition wiring (aperture + StorageSink) — small surface but real |

All other elements of each new job are byte-identical to the sibling:
toolchain pin, `cargo-mutants` installer action and pin, runner
(`ubuntu-latest`), timeout (`30 minutes`), `needs` graph
(`[gate-2-public-api, gate-3-semver]`), `fetch-depth: 0` checkout,
`--no-shuffle --jobs 2` invocation, the `origin/main → HEAD~1 → full`
baseline cascade, the empty-diff short-circuit, and the
`if: success() || failure()` artefact upload.

No production source file is touched. No `Cargo.toml`, `Cargo.lock`,
`deny.toml`, or `rust-toolchain.toml` is touched. No ADR is modified.

## DISCUSS Decisions

| D# | Topic | Value |
|----|-------|-------|
| DD1 | Feature type | Infrastructure (CI-only) |
| DD2 | Walking skeleton needed | No (single batch story; the eight jobs are isomorphic clones of the same precedent) |
| DD3 | Research depth | Lightweight (pattern confirmed twice on `main`: `gate-5-mutants-lumen` commit d96a807 and `gate-5-mutants-query-http-common` commit a6175f1) |
| DD4 | JTBD | No (infrastructure feature; no end-user job statement) |
| DD5 | Number of stories | 1 (US-01 batch) |
| DD6 | Story tag | `@infrastructure` (no user-visible value; Decision enabled is maintenance signal on every Predicate or store extension in the eight crates) |
| DD7 | KPI count | 6 (K1 eight jobs exist; K2 each uses `--in-diff origin/main`; K3 other 17 unchanged; K4 total 17 → 25; K5 YAML still parses; K6 zero regression on other CI jobs) |
| DD8 | Peer review | Skipped (Luna self-validates per `dor-validation.md`; pattern is established) |
| DD9 | Deliverables in DISCUSS | 5 files under `docs/feature/gate-5-mutants-batch-v0/discuss/`: `user-stories.md`, `story-map.md`, `outcome-kpis.md`, `dor-validation.md`, `wave-decisions.md` |

## Why batch is the right size

The scope of this feature is "close the residual CI gap from the
`gate-5-mutants-lumen-v0` audit". It is not "fix every CI gap
forever". The scope is closed, finite, and auditable: the eight
target crates are enumerated explicitly with their exact `package.name`
values, and there is no reasonable interpretation under which a ninth
crate could be added under this feature ID. The pattern is already
confirmed shipped twice on `main` (`gate-5-mutants-lumen` at commit
d96a807 and `gate-5-mutants-query-http-common` at commit a6175f1).
Opening eight separate features would be pure ceremony without
incremental information value: there is nothing further to learn
about the shape of the per-crate Gate 5 job that the two precedent
features have not already taught. A future feature that closes a
different category of CI gap (a new fifth gate, a different
graduation tier, a different crate that did not exist at the time of
this audit) would be a separate feature, not a continuation of this
one.

## Flags raised to DESIGN

Three flags. The pattern decides the answer in two; one is a
genuine new decision.

### FLAG 1: Job placement order in `.github/workflows/ci.yml`

**Question for Morgan**: where do the eight new job blocks sit in
the workflow file?

**Options**:

- **Option A (RECOMMENDED)**: alphabetic insertion inside the
  existing gate-5 block. `aegis`, `augur`, `beacon-server`, `cinder`,
  `integration-suite`, `kaleidoscope-gateway`, `loom`, `sluice` slot
  into their alphabetic positions relative to the existing 17 jobs.
  Long-term readability argument: a maintainer scanning the workflow
  file for `gate-5-mutants-<X>` finds it without a full-file `grep`.
- **Option B**: append all eight at the end of the gate-5 block,
  immediately after `gate-5-mutants-query-http-common` (the last
  gate-5 job, at line 1809). Temporal argument: this is where new
  gate-5 jobs accrete; the precedent in `query-http-common-v0`
  appended. Disadvantage: the file becomes order-by-when-added,
  not order-by-name.
- **Option C**: append at the end of the gate-5 block but in
  alphabetic order among themselves. A hybrid that respects accretion
  but limits the in-block disorder to the new eight.

**Luna's recommendation**: Option A. Alphabetic insertion is the
most resilient to future growth; the file becomes self-indexing.
Morgan may prefer Option B for VCS-blame friendliness; either is
acceptable. Option C is dominated by A.

### FLAG 2: `needs:` graph

**Question for Morgan**: should each new job's `needs:` graph be a
verbatim copy of the sibling's, or do any of the eight crates warrant
a different upstream-job set?

**Options**:

- **Option A (RECOMMENDED)**: verbatim copy of
  `gate-5-mutants-lumen`'s `needs:` graph for all eight, which is
  `[gate-2-public-api, gate-3-semver]`. Every existing
  `gate-5-mutants-*` job uses this same graph (17 of 17); the
  precedent is uniform.
- **Option B**: a different graph for some subset (e.g. `aegis`,
  which has the auth predicates, could gain `gate-1-test` as an
  explicit upstream). No precedent supports this; every existing
  sibling uses the same `[gate-2-public-api, gate-3-semver]` pair.

**Luna's recommendation**: Option A for all eight. PIN. The uniform
precedent across 17 jobs is itself a contract; deviating would
introduce a maintenance asymmetry without a justifying argument.

### FLAG 3: Special cases for small crates

**Question for Morgan**: how should the eight jobs handle the
small-crate case where `cargo mutants --in-diff origin/main` produces
zero viable mutants (because the crate's diff is small, or because
the crate is test-only)?

**Context**:

- `integration-suite` has a minimal `src/lib.rs` (the entire crate is
  effectively the `tests/` directory; `package.name` is
  `integration-suite`). Mutation-testing a test-only crate is a
  different conversation: the mutations would target the test
  harness itself, not production code. The `gate-5-mutants-lumen-v0`
  story-map appendix explicitly flagged this: "a future feature MAY
  decide that this crate is excluded by policy rather than included
  with a job".
- `kaleidoscope-gateway` has 486 LOC of `src/` (`lib.rs` plus
  `main.rs`) — a real but small surface. `beacon-server` is similar
  in shape (binary + thin library wrapping `beacon`).

**Options**:

- **Option A (RECOMMENDED, future-proof)**: ship the job anyway for
  all eight, including `integration-suite`. The empty-diff
  short-circuit and the `cargo mutants` no-viable-mutants exit code
  both produce a green job verdict; the job is a no-op on PRs that
  do not touch the crate or where the diff contains no viable
  mutation targets. The job becomes active automatically once a
  future PR adds real production code to the crate.
- **Option B**: ship the job for seven crates and explicitly exclude
  `integration-suite` by policy. The exclusion would be recorded as
  an ADR or as a comment in the workflow file. The argument: a
  test-only crate has no production behaviour to mutation-test, so
  the gate is conceptually misapplied.
- **Option C**: ship the job for seven crates and defer
  `integration-suite` to a separate future decision (the same posture
  as the `gate-5-mutants-lumen-v0` story-map appendix).

**Luna's recommendation**: Option A (ship all eight). The job is
cheap when it has nothing to do (the short-circuit is sub-minute);
the future-proofing value is real (a maintainer adding production
code to any of the eight crates gets the signal automatically without
a workflow edit). If Morgan prefers Option B, the exclusion should be
recorded as a one-line comment in the workflow file at the position
where the missing job would sit, so a future maintainer reading the
file sees the intentional gap rather than wondering whether it is an
oversight.

## Constraints Established

- **ADR-0005 immutability**. The five-gate contract is unchanged;
  this feature adds eight per-crate instances of Gate 5, not a sixth
  gate. No ADR is created, modified, or superseded by this slice.
- **No production source change**. The DELIVER commit edits
  `.github/workflows/ci.yml` and nothing else. The crafter's
  authorised file list is `{.github/workflows/ci.yml}`.
- **Pattern fidelity**. Each of the eight new jobs is byte-shaped
  after `gate-5-mutants-lumen` (lines 1210 to 1295). Differences are
  limited to the four token substitutions per crate documented in
  Requirements Summary.
- **Zero rename, zero deletion**. The 17 pre-existing
  `gate-5-mutants-*` jobs are not renamed, not deleted, and not
  re-scoped. K3 enforces this as a guardrail.
- **Zero new dependency**. `cargo-mutants` is installed by
  `taiki-e/install-action` from a precompiled binary; the workspace
  dependency graph (`Cargo.toml`, `Cargo.lock`, `deny.toml`) is
  unchanged.
- **No public-API change**. None of the eight crates is added to or
  removed from Gate 2 (`cargo public-api`) or Gate 3
  (`cargo semver-checks`) locked sets. No new public-API snapshot is
  required.
- **Pure trunk-based, no CI gates**. Per project policy, CI is
  feedback, not a gate. Adding the eight jobs does NOT make them
  required status checks; they are forward signals for maintainers.
- **No 1.0.0 bump**. Per project policy, none of the eight crates is
  bumped to 1.0.0. The CI workflow edit does not touch any crate's
  public-API surface.
- **No commit by Luna**. Per the orchestrator's standing rule, the
  main orchestrator commits. Luna writes the five DISCUSS artefacts
  and stops.

## Upstream Changes

None. This feature does not revise any earlier wave or any ADR.

The feature is the operationalisation of the residual maintenance
note recorded in
`docs/feature/gate-5-mutants-lumen-v0/discuss/story-map.md` lines
129 to 141 (the appendix enumerating eight crates without
`gate-5-mutants-*` coverage post-lumen). It does NOT retract or
modify the appendix; the appendix is accurate at the time of writing,
and this feature simply closes the eight items it enumerated.

The slice composes additively on top of every prior CI contract:
ADR-0005 (the five gates), ADR-0047 (per-crate `--in-diff` mutation
scoping), ADR-0048 (per-crate Gate 5 precedent), ADR-0052 (per-crate
graduation pattern), ADR-0055 (Gate 2 / Gate 3 locked-set posture),
and the per-feature mutation testing strategy recorded in
`CLAUDE.md`. None of these are altered.

## Handoff

**Next agent**: `nw-solution-architect` (DESIGN wave) acting as
`nw-platform-architect`, or directly the DEVOPS wave if the
orchestrator decides the DESIGN step is a passthrough for an
infrastructure-only feature.

**What the next wave receives**:

1. `user-stories.md` — US-01 batch with three Gherkin scenarios and
   acceptance criteria covering all eight crates.
2. `story-map.md` — single-story backbone "Identify 8 → Replicate
   pattern → Verify all 8", walking skeleton = US-01.
3. `outcome-kpis.md` — K1 to K6 with measurement plan.
4. `dor-validation.md` — 9-item DoR PASSED, anti-pattern scan clean.
5. `wave-decisions.md` — this file, including FLAG 1 (placement),
   FLAG 2 (`needs:` graph), and FLAG 3 (small-crate handling) for
   the next wave to resolve.

**What the next wave decides**:

- FLAG 1 resolution (alphabetic vs append placement). Luna recommends
  Option A (alphabetic insertion).
- FLAG 2 resolution (`needs:` graph). Luna recommends Option A
  (verbatim copy of `[gate-2-public-api, gate-3-semver]` for all
  eight). PIN.
- FLAG 3 resolution (small-crate handling). Luna recommends Option A
  (ship all eight; rely on the empty-diff short-circuit and the
  cargo-mutants no-viable-mutants exit code).
- The exact YAML blocks to insert (the four token substitutions per
  crate on the sibling block; the byte-level form is constrained by
  precedent).

**Constraint reminder for the next wave**: the DELIVER commit edits
`.github/workflows/ci.yml` and nothing else. Any departure from this
boundary requires re-opening DISCUSS.
