# Wave Decisions — gate-5-mutants-lumen-v0 / DISCUSS

British English. No em dashes in body.

- **Wave**: DISCUSS
- **Author**: Luna (`nw-product-owner`)
- **Date**: 2026-05-29
- **Mode**: lightweight. Single-story infrastructure slice. No JTBD,
  no walking-skeleton coverage analysis, no journey YAML, no
  `.feature` file. Closes the honest gap recorded by Apex on
  2026-05-27.

## Requirements Summary

The feature adds one new GitHub Actions job to
`.github/workflows/ci.yml` named `gate-5-mutants-lumen`, replicating
the shape of the existing `gate-5-mutants-log-query-api` job at
workflow lines 1123 to 1208 with four token substitutions:

| Token | Sibling value (`log-query-api`) | This feature value (`lumen`) |
|---|---|---|
| Package name (`cargo mutants --package`) | `log-query-api` | `lumen` |
| Diff filter path glob | `crates/log-query-api/**` | `crates/lumen/**` |
| Cache key shard | `cargo-mutants-log-query-api-` | `cargo-mutants-lumen-` |
| Artefact name (`upload-artifact`) | `mutants-out-log-query-api` | `mutants-out-lumen` |

All other elements are byte-identical to the sibling: toolchain pin,
`cargo-mutants` installer action and pin, runner (`ubuntu-latest`),
timeout (`30 minutes`), `needs` graph
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
| DD2 | Walking skeleton needed | No (the slice IS the skeleton; single story, single workflow edit) |
| DD3 | Research depth | Lightweight (pattern is established; sixteen sibling jobs to clone) |
| DD4 | JTBD | No (infrastructure feature; no end-user job statement) |
| DD5 | Number of stories | 1 (US-01 only) |
| DD6 | Story tag | `@infrastructure` (no user-visible value; Decision enabled is maintenance signal on lumen Predicate extensions) |
| DD7 | KPI count | 4 (K1 binary existence; K2 correctness of plumbing; K3 zero regression on sixteen siblings; K4 zero new dependency) |
| DD8 | Peer review | Skipped (overnight pattern-stretch authorisation); Luna self-validates per `dor-validation.md` |
| DD9 | Deliverables in DISCUSS | 5 files under `docs/feature/gate-5-mutants-lumen-v0/discuss/`: `user-stories.md`, `story-map.md`, `outcome-kpis.md`, `dor-validation.md`, `wave-decisions.md` |

## Flags raised to DESIGN

Two flags. Both are minor; the pattern decides the answer in both
cases.

### FLAG 1: Job placement in `.github/workflows/ci.yml`

**Question for Apex**: where exactly does the new `gate-5-mutants-lumen`
job block sit in the workflow file?

**Options**:

- **Option A (RECOMMENDED)**: immediately after
  `gate-5-mutants-log-query-api` (which ends at line 1208), and
  immediately before `gate-5-mutants-trace-query-api` (which begins
  at line 1210). The semantic argument: `lumen` is the storage
  backing for `log-query-api`, so the adjacency reads naturally to a
  maintainer scanning the workflow file. Insert at line 1209.
- **Option B**: append at the end of the `gate-5-mutants-*` block,
  immediately after `gate-5-mutants-query-http-common` (which is the
  last gate-5 job in the workflow; the immediate sibling
  `query-http-common-v0` DEVOPS wave placed its new job there as
  well). The temporal argument: this is the location where new
  gate-5 jobs accrete. Insert just before the `# Prism v0 gates
  6-11` comment block.
- **Option C**: alphabetic slot inside the gate-5 block. `lumen`
  sorts between `log-query-api` and `pulse`, which collapses to
  Option A.

**Luna's recommendation**: Option A. The semantic adjacency to
`log-query-api` is the same shape that the body-contains slice
exposed and is the most readable to the next maintainer extending
either crate.

### FLAG 2: `needs:` graph

**Question for Apex**: should the new job's `needs:` graph be a
verbatim copy of the sibling's, or does the `lumen` package warrant
a different upstream-job set?

**Options**:

- **Option A (RECOMMENDED)**: verbatim copy of
  `gate-5-mutants-log-query-api`'s `needs:` graph, which is
  `[gate-2-public-api, gate-3-semver]`. Every existing
  `gate-5-mutants-*` job uses this same graph; the precedent is
  uniform across all sixteen jobs.
- **Option B**: a different graph, e.g. adding `gate-1-test` or
  `gate-4-deny` as explicit upstream gates. No precedent supports
  this; every existing sibling uses the same `[gate-2-public-api,
  gate-3-semver]` pair.

**Luna's recommendation**: Option A. The uniform precedent across
sixteen jobs is itself a contract; deviating would introduce a
maintenance asymmetry without a justifying argument. The DESIGN wave
may revisit if `lumen`'s position in the build graph differs in a
way I have not observed; the recommended default is "copy
verbatim".

## Constraints Established

- **ADR-0005 immutability**. The five-gate contract is unchanged;
  this feature adds one per-crate instance of Gate 5, not a sixth
  gate. No ADR is created, modified, or superseded by this slice.
- **No production source change**. The DELIVER commit edits
  `.github/workflows/ci.yml` and nothing else. The crafter's
  authorised file list is `{.github/workflows/ci.yml}`.
- **Pattern fidelity**. The new job is byte-shaped after
  `gate-5-mutants-log-query-api`. Differences are limited to the
  four token substitutions documented in Requirements Summary.
- **Zero rename, zero deletion**. The sixteen pre-existing
  `gate-5-mutants-*` jobs are not renamed, not deleted, and not
  re-scoped. K3 enforces this as a guardrail.
- **Zero new dependency**. `cargo-mutants` is installed by
  `taiki-e/install-action` from a precompiled binary; the workspace
  dependency graph (`Cargo.toml`, `Cargo.lock`, `deny.toml`) is
  unchanged. K4 enforces this as a guardrail.
- **No public-API change**. `lumen` is not in Gate 2 (`cargo
  public-api`) or Gate 3 (`cargo semver-checks`) locked set, so no
  semver-checks regression is possible. No new public-API snapshot
  is required.
- **Pure trunk-based, no CI gates**. Per project policy, CI is
  feedback, not a gate. Adding `gate-5-mutants-lumen` does NOT make
  it a required status check; it is a forward signal for
  maintainers. The merge policy remains "fix-forward" and "no
  required-status-checks on main".
- **No 1.0.0 bump**. Per project policy, the `lumen` crate stays at
  its current version. The CI workflow edit does not require a
  version bump on any crate (the workflow file is not part of any
  crate's public-API surface).
- **No commit by Luna**. Per the orchestrator's standing rule, the
  main orchestrator commits. Luna writes the five DISCUSS
  artefacts and stops.

## Upstream Changes

None. This feature does not revise any earlier wave or any ADR.

The feature is the consequence of Apex's honest gap note in
`docs/feature/log-body-text-search-v0/devops/wave-decisions.md` lines
56 to 89, which itself was a CORRECTION of an earlier DESIGN handoff
that had speculatively referenced "the workspace-default mutants gate
at the lumen crate". The current slice operationalises Apex's
forward-looking item by adding the missing job. It does NOT retract
or modify either Apex's note or the body-contains DESIGN handoff;
both are accurate at the time of writing, and Apex's gap finding is
the source-of-record for this slice's existence.

The slice composes additively on top of every prior CI contract:
ADR-0005 (the five gates), ADR-0047 (per-crate `--in-diff` mutation
scoping), ADR-0048 (per-crate Gate 5 precedent), ADR-0052
(per-crate graduation pattern), ADR-0055 (Gate 2 / Gate 3 locked-set
posture), and the per-feature mutation testing strategy recorded in
`CLAUDE.md`. None of these are altered.

## Handoff

**Next agent**: `nw-solution-architect` (DESIGN wave) acting as
`nw-platform-architect`, or directly the DEVOPS wave if the
orchestrator decides the DESIGN step is a passthrough for an
infrastructure-only feature.

**What the next wave receives**:

1. `user-stories.md` — US-01 with three Gherkin scenarios and seven
   AC.
2. `story-map.md` — single-story backbone, walking skeleton, and the
   appendix enumerating the eight other crates without
   `gate-5-mutants-*` coverage (future maintenance).
3. `outcome-kpis.md` — K1 to K4 with measurement plan.
4. `dor-validation.md` — 9-item DoR PASSED, anti-pattern scan clean.
5. `wave-decisions.md` — this file, including FLAG 1 (placement) and
   FLAG 2 (`needs:` graph) for the next wave to resolve, both with
   recommended options.

**What the next wave decides**:

- FLAG 1 resolution (placement of the new job block in the YAML
  file). Luna recommends Option A (after
  `gate-5-mutants-log-query-api`, line 1209).
- FLAG 2 resolution (`needs:` graph for the new job). Luna recommends
  Option A (verbatim copy of the sibling's
  `[gate-2-public-api, gate-3-semver]`).
- The exact YAML block to insert (the four token substitutions on
  the sibling block; the byte-level form is constrained by
  precedent).
- Any caching strategy adjustment (Luna recommends none; the cache
  key shard substitution is the only delta).

**Constraint reminder for the next wave**: the DELIVER commit edits
`.github/workflows/ci.yml` and nothing else. Any departure from this
boundary requires re-opening DISCUSS.
