# Outcome KPIs — gate-5-mutants-lumen-v0

British English. No em dashes in body.

## Feature

Add `gate-5-mutants-lumen` as a per-crate Gate 5 job in
`.github/workflows/ci.yml`, closing the honest gap recorded by Apex
in `docs/feature/log-body-text-search-v0/devops/wave-decisions.md`
lines 56 to 89. No production code change. Pure CI workflow
extension.

## Objective

By the close of this feature, the `lumen` crate enjoys the same
ADR-0005 Gate 5 enforcement posture as the read APIs and the other
fifteen crates already covered by per-crate `gate-5-mutants-*` jobs.
Predicate extensions land with automatic mutation-test signal.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| K1 | the `lumen` crate maintainer | observes a `gate-5-mutants-lumen` job in the post-feature workflow | exactly 1 job exists, named `gate-5-mutants-lumen` | 0 jobs (Apex gap note, 2026-05-27) | `grep "gate-5-mutants-lumen:" .github/workflows/ci.yml` returns exactly one line | Leading (binary outcome) |
| K2 | the `lumen` crate maintainer | observes the job's `--in-diff` filter targeting `crates/lumen/**` | 100% of lumen-touching PRs trigger a real `cargo mutants --package lumen` invocation; 100% of non-lumen-touching PRs short-circuit | not measurable (the job did not exist) | a synthetic PR with a known surviving mutation in `crates/lumen/src/predicate.rs` produces a red verdict on the new job (verifies the gate fires); a separate synthetic PR that touches only `crates/log-query-api/src/lib.rs` produces the short-circuit log message and a green verdict (verifies the diff filter excludes non-lumen changes) | Leading (correctness of the gate plumbing) |
| K3 | every other Kaleidoscope crate maintainer | observes zero regression on the sixteen pre-existing `gate-5-mutants-*` jobs | 16 of 16 pre-existing jobs still present, still named identically, still executing the same script body, still wired into the same `needs` graph | 16 jobs (workflow at the close of `query-http-common-v0` DEVOPS, 2026-05-27) | post-feature `grep -c "^  gate-5-mutants-" .github/workflows/ci.yml` returns 17; pre vs post `diff` on the sixteen pre-existing job bodies returns zero lines changed | Guardrail (no regression) |
| K4 | the workspace dependency budget | observes zero net new external dependency, zero net new workspace crate, zero net new installed tool | 0 net new entries on each of `Cargo.toml` (workspace), `Cargo.lock`, `deny.toml`, and the `taiki-e/install-action` tool field | the dependency graph at the close of `query-http-common-v0` DEVOPS | `diff` on the four artefacts pre vs post-feature returns zero relevant lines; the `cargo-mutants` installer line in the new job is byte-identical to the sibling job's installer line | Guardrail (zero net cost) |

## Metric Hierarchy

- **North Star**: K1. The job exists. Everything else cascades from
  this binary fact.
- **Leading indicators**: K1 (binary existence) and K2 (correctness
  of the diff filter and the package scope). Both are observable on
  the post-feature workflow file and on the first lumen-touching PR.
- **Guardrail metrics**: K3 (zero regression on the sixteen siblings)
  and K4 (zero net new dependency or tool). Either of these breaching
  is a hard fail of the feature; the DELIVER commit must be revised
  before the feature is closed.

## Measurement Plan

| KPI | Data source | Collection method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| K1 | `.github/workflows/ci.yml` | one-shot `grep` at the feature-close commit | once at close | DELIVER agent (Crafter) |
| K2 | A synthetic lumen-touching PR (or the next real PR that touches `crates/lumen/src/**`) | observation of the job's verdict and log output in the GitHub Actions run | once at close (synthetic); then continuously on every lumen-touching PR | DELIVER agent (Crafter), then ambient across all future PRs |
| K3 | `.github/workflows/ci.yml` at the pre-feature commit vs the post-feature commit | `git diff` over the workflow file restricted to lines outside the new `gate-5-mutants-lumen:` block | once at close | DELIVER agent (Crafter) |
| K4 | `Cargo.toml`, `Cargo.lock`, `deny.toml`, and the new job's `taiki-e/install-action` tool field | `git diff` on each artefact at the feature-close commit; byte-level comparison of the installer line against the sibling job | once at close | DELIVER agent (Crafter) |

## Hypothesis

We believe that adding a `gate-5-mutants-lumen` job to
`.github/workflows/ci.yml`, shaped exactly after the existing
`gate-5-mutants-log-query-api` job at lines 1123 to 1208 with four
token substitutions (package name, diff filter path, cache key shard,
artefact name), for the `lumen` crate maintainer extending
`Predicate` will achieve the ADR-0005 Gate 5 enforcement posture on
the `lumen` crate.

We will know this is true when:

- K1: the maintainer runs `grep "gate-5-mutants-lumen:"
  .github/workflows/ci.yml` and sees exactly one line.
- K2: a synthetic PR with a known surviving mutation in
  `crates/lumen/src/predicate.rs` produces a red verdict on the
  new job; a synthetic PR that touches no file under
  `crates/lumen/**` produces a green verdict via short-circuit.
- K3: pre vs post-feature comparison of the sixteen pre-existing
  `gate-5-mutants-*` jobs shows zero rename, zero deletion, zero
  script change.
- K4: pre vs post-feature comparison of the workspace's external
  dependency surface (`Cargo.toml`, `Cargo.lock`, `deny.toml`) shows
  zero net new entries.

## Notes on KPI shape

All four KPIs are build-time / workflow-file measurements. None
require runtime telemetry. None require a dashboard. This is
consistent with the Kaleidoscope-wide observability posture (the
platform has no live observability stack at v0; a contract-shaped
outcome IS the signal) and with the precedent set by
`query-http-common-v0` DEVOPS (the K1 to K4 in that feature were
also all build-time measurements).

K1 is binary (one job exists or it does not). K2 is two binary
sub-checks (positive and negative diff filter behaviour). K3 is a
zero-delta `diff` over sixteen blocks. K4 is a zero-delta `diff` over
four artefacts plus one byte-identical installer line. The KPIs are
deliberately small in cardinality because the feature is small in
scope.
