# Outcome KPIs — gate-5-mutants-batch-v0

British English. No em dashes in body.

## Feature

Add eight per-crate Gate 5 jobs (`gate-5-mutants-aegis`,
`gate-5-mutants-augur`, `gate-5-mutants-sluice`,
`gate-5-mutants-beacon-server`, `gate-5-mutants-cinder`,
`gate-5-mutants-loom`, `gate-5-mutants-integration-suite`,
`gate-5-mutants-kaleidoscope-gateway`) to
`.github/workflows/ci.yml`. Closes the residual gap recorded by
Luna in `gate-5-mutants-lumen-v0/discuss/story-map.md` lines 129 to
141. No production code change. Pure CI workflow extension.

## Objective

By the close of this feature, every one of the 25 workspace crates
enjoys uniform ADR-0005 Gate 5 enforcement via a per-crate
`gate-5-mutants-<crate>` job. Future extensions to any of the eight
residual crates land with automatic mutation-test signal, matching
the discipline already enjoyed by the other 17 crates.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| K1 | maintainers of the eight residual crates | observe their per-crate `gate-5-mutants-<crate>` job in the post-feature workflow | exactly 8 new jobs exist, one named `gate-5-mutants-aegis`, one `gate-5-mutants-augur`, one `gate-5-mutants-sluice`, one `gate-5-mutants-beacon-server`, one `gate-5-mutants-cinder`, one `gate-5-mutants-loom`, one `gate-5-mutants-integration-suite`, one `gate-5-mutants-kaleidoscope-gateway` | 0 jobs (Luna's residual enumeration, `gate-5-mutants-lumen-v0/discuss/story-map.md` lines 129 to 141) | `grep "gate-5-mutants-aegis:"`, `grep "gate-5-mutants-augur:"`, ... `grep "gate-5-mutants-kaleidoscope-gateway:"` each return exactly one line in `.github/workflows/ci.yml` | Leading (binary outcome, eight binary sub-checks) |
| K2 | maintainers of the eight residual crates | observe each job's script line containing `cargo mutants -p <crate-name>` and `--in-diff` against `origin/main` baseline (path-filtered by `crates/<crate-dir>/**`) | 100% of crate-touching PRs trigger a real `cargo mutants -p <crate-name>` invocation; 100% of non-crate-touching PRs short-circuit; the cascade is `origin/main` first, `HEAD~1` second, full third | not measurable (the jobs did not exist) | for each of the eight new jobs: the job block contains a step whose run script includes the literal `cargo mutants` and the literal `-p <crate-name>` (or `--package <crate-name>`) and the literal `--in-diff "$DIFF_FILE"` and the literal `git diff "$BASELINE" HEAD -- 'crates/<crate-dir>/**'`; a synthetic PR with a known surviving mutation in the crate's `src/` produces a red verdict on the corresponding job | Leading (correctness of the gate plumbing) |
| K3 | every other Kaleidoscope crate maintainer | observe zero regression on the 17 pre-existing `gate-5-mutants-*` jobs | 17 of 17 pre-existing jobs still present, still named identically, still executing the same script body, still wired into the same `needs` graph | 17 jobs (workflow at the close of `gate-5-mutants-lumen-v0` DEVOPS, commit d96a807) | post-feature `diff` on each of the 17 pre-existing job blocks vs the pre-feature commit returns zero lines changed | Guardrail (no regression) |
| K4 | every Kaleidoscope crate maintainer | observes the total `gate-5-mutants-*` count rise from 17 to 25 | `grep -c "^  gate-5-mutants-" .github/workflows/ci.yml` returns 25 post-feature (was 17 pre-feature); equals the workspace crate count | 17 (pre-feature) | post-feature `grep -c "^  gate-5-mutants-" .github/workflows/ci.yml`; cross-checked against `ls crates/ \| wc -l` to confirm 1:1 coverage of the 25 workspace crates | Leading (uniform coverage outcome) |
| K5 | the workflow file as a whole | parses successfully as YAML after the eight-job insertion | `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` exits with status 0 | pre-feature: exits 0 (the file parses) | run the one-liner at the feature-close commit; verify exit 0 and no error output | Guardrail (file validity) |
| K6 | every Kaleidoscope crate maintainer | observes zero regression on every non-`gate-5-mutants` CI job (gate-1 through gate-4 and all other workflow jobs) | every non-`gate-5-mutants` job in the workflow file is byte-identical to its pre-feature form; zero job has been renamed, deleted, re-scoped, or had its `needs:` graph altered | the pre-feature workflow file | `git diff` between pre-feature and post-feature `.github/workflows/ci.yml`, restricted to lines outside the eight new job blocks, returns zero relevant changes | Guardrail (no regression on the rest of CI) |

## Metric Hierarchy

- **North Star**: K1 plus K4. The eight jobs exist; the workspace's
  Gate 5 coverage is uniform at 25 / 25.
- **Leading indicators**: K1 (eight binary existence checks), K2
  (correctness of each job's diff filter and package scope), K4
  (uniform-coverage roll-up). All three are observable on the
  post-feature workflow file and on the first PR per crate.
- **Guardrail metrics**: K3 (zero regression on the 17 pre-existing
  jobs), K5 (YAML parses), K6 (zero regression on the rest of CI).
  Any of these breaching is a hard fail of the feature; the DELIVER
  commit must be revised before the feature is closed.

## Measurement Plan

| KPI | Data source | Collection method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| K1 | `.github/workflows/ci.yml` | eight one-shot `grep` invocations at the feature-close commit, one per crate name | once at close | DELIVER agent (Crafter) |
| K2 | a synthetic crate-touching PR per crate (or the next real PR that touches `crates/<crate-dir>/src/**`) | observation of each job's verdict and log output in the GitHub Actions run | once at close (synthetic per crate where viable); then continuously on every crate-touching PR | DELIVER agent (Crafter), then ambient across all future PRs |
| K3 | `.github/workflows/ci.yml` at the pre-feature commit vs the post-feature commit | `git diff` over the workflow file restricted to lines outside the eight new `gate-5-mutants-<crate>:` blocks | once at close | DELIVER agent (Crafter) |
| K4 | `.github/workflows/ci.yml` plus the workspace `crates/` listing | post-feature `grep -c` over the workflow file; `ls crates/` cross-check | once at close | DELIVER agent (Crafter) |
| K5 | `.github/workflows/ci.yml` | one-shot `python3 -c "import yaml; yaml.safe_load(open(...))"` | once at close | DELIVER agent (Crafter) |
| K6 | `.github/workflows/ci.yml` at the pre-feature commit vs the post-feature commit | `git diff` over the full workflow file; manual review of the diff to confirm only the eight new job blocks are added | once at close | DELIVER agent (Crafter) |

## Hypothesis

We believe that adding eight `gate-5-mutants-<crate>` jobs to
`.github/workflows/ci.yml`, each shaped exactly after the existing
`gate-5-mutants-lumen` job at lines 1210 to 1295 with four token
substitutions per crate (package name, diff filter path, cache key
shard, artefact name), for the maintainers of the eight residual
crates (`aegis`, `augur`, `sluice`, `beacon-server`, `cinder`,
`loom`, `integration-suite`, `kaleidoscope-gateway`) will achieve
uniform ADR-0005 Gate 5 enforcement across all 25 workspace crates.

We will know this is true when:

- K1: the maintainer of each of the eight crates runs
  `grep "gate-5-mutants-<crate>:" .github/workflows/ci.yml` and sees
  exactly one line.
- K2: a synthetic PR with a known surviving mutation in each crate's
  `src/` produces a red verdict on the corresponding new job; a
  synthetic PR that touches no file under any of the eight
  `crates/<crate-dir>/**` paths produces eight green verdicts via
  short-circuit.
- K3: pre vs post-feature comparison of the 17 pre-existing
  `gate-5-mutants-*` jobs shows zero rename, zero deletion, zero
  script change.
- K4: post-feature `grep -c "^  gate-5-mutants-"
  .github/workflows/ci.yml` returns 25, matching the workspace
  crate count.
- K5: the post-feature workflow file parses cleanly as YAML.
- K6: pre vs post-feature comparison of all non-`gate-5-mutants` job
  blocks shows zero changes.

## Notes on KPI shape

All six KPIs are build-time / workflow-file measurements. None
require runtime telemetry. None require a dashboard. This is
consistent with the Kaleidoscope-wide observability posture (the
platform has no live observability stack at v0; a contract-shaped
outcome IS the signal) and with the precedent set by
`gate-5-mutants-lumen-v0` and `gate-5-mutants-query-http-common-v0`
DEVOPS waves (their KPIs were also all build-time measurements).

K1, K4 are existence / count outcomes. K2 is per-crate correctness
of the gate plumbing. K3, K5, K6 are guardrails: zero-delta `diff`
plus a YAML parser smoke. The KPIs are deliberately small in
cardinality because the feature is small in scope: one workflow file
edit, eight near-identical blocks.
