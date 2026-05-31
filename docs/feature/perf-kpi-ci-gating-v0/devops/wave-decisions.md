# Wave Decisions — perf-kpi-ci-gating-v0 / DEVOPS

- **Wave**: DEVOPS (slim, documentation-only)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-31
- **Mode**: slim doc-only. This feature is test-infrastructure only. It
  gates the 28 wall-clock p95 KPI tests behind the presence-based
  environment variable `KALEIDOSCOPE_PERF_TESTS` (skip locally, run in
  CI) per ADR-0058. No threshold literal changes. No new crate, no new
  workspace member, no new CI job, no new deployment artefact, no new
  dependency. This wave records the environment contract and the
  inherited CI posture. Per the DESIGN handoff, Apex does NOT edit
  `.github/workflows/ci.yml`; the `gate-1-test` env block and the 28
  guard lines land TOGETHER in the DELIVER commit (Crafty), because the
  env block without the guards is a no-op and the guards without the env
  block would silently disable the KPIs everywhere. Shape and brevity
  mirror the slim sibling precedent at
  `docs/feature/log-body-regex-search-v0/devops/wave-decisions.md`.

## DEVOPS Decisions

| D# | Topic | Value |
|----|-------|-------|
| DD1 | deployment_target | N/A (test-infrastructure only; no new binary, no new container) |
| DD2 | container_orchestration | N/A (no container image produced or altered) |
| DD3 | cicd_platform | inherit GitHub Actions; ADR-0005 five-gate contract unchanged and cited, never modified |
| DD4 | existing_infrastructure | extend; one job-level `env` block added to the existing `gate-1-test` job (by Crafty in DELIVER, not here); no new infra, no new CI job |
| DD5 | observability | inherit existing gates; no new metric, dashboard, or alert. This feature RENDERS the Gate 1 local run deterministic by skipping the load-sensitive wall-clock measurements in the pre-commit hook while keeping them enforced in CI |
| DD6 | deployment_strategy | N/A (pure trunk-based; recovery is fix-forward / git revert; the change is additive guard preamble plus one CI env line) |
| DD7 | continuous_learning | N/A (no live observability stack at v0/v1) |
| DD8 | git_branching | inherit pure trunk-based (project default; main has no required-status-checks and no enforce_admins) |
| DD9 | mutation_testing | inherit per-feature, 100% kill rate (CLAUDE.md, ADR-0005 Gate 5); the 28 guard lines are covered by the existing per-crate `gate-5-mutants-*` jobs via `--in-diff`. See Mutation coverage note below |

## CI vs local enforcement

The behaviour split is the whole point of the feature.

- **Local pre-commit hook** (`scripts/hooks/pre-commit`, `cargo test
  --workspace` at line 92): the variable is ABSENT, so `is_err()` is
  true, so each of the 28 perf tests prints its skip note to stderr and
  returns early as `ok`. The hook is fast and deterministic under
  machine load. No `--no-verify` bypass is forced.
- **CI `gate-1-test` job** (`.github/workflows/ci.yml:136`, `cargo test
  --workspace` at line 182): in DELIVER, Crafty adds a job-level `env`
  block setting `KALEIDOSCOPE_PERF_TESTS: "1"` (hardcoded literal,
  consistent with the `NIGHTLY_PIN` job-level pattern at lines 74, 271,
  376). The variable is PRESENT, so the guard falls through and the 28
  tests run their full measurement and enforce their UNCHANGED
  thresholds on `ubuntu-latest`, the runner the thresholds were tuned
  for. The KPIs remain REAL gates: a genuine latency regression turns
  `gate-1-test` red and blocks the merge.

The `gate-1-test` job has `needs: gate-4-deny` and currently NO `env:`
block (CONFIRMED by read). No other gate job runs `cargo test`, so no
other job is affected.

## No new tooling

Zero new workspace crate. Zero new workspace member. Zero new binary.
Zero new dependency (no `Cargo.toml` edit, no `Cargo.lock` diff). Zero
new public event name. Zero new graduation tag. Zero new `deny.toml`
policy change. Zero crate bumped to 1.0.0. The guard uses only
`std::env::var` and `eprintln!` from the Rust standard library.

## Mutation coverage note

The 28 guard lines are covered by the existing per-crate Gate 5 jobs via
`--in-diff`, which scope to the modified files. All nine perf crates have
a job (CONFIRMED by grep): `gate-5-mutants-lumen` (line 1210),
`gate-5-mutants-pulse` (1384), `gate-5-mutants-ray` (1467),
`gate-5-mutants-strata` (1550), `gate-5-mutants-beacon` (1635),
`gate-5-mutants-aegis` (1898), `gate-5-mutants-augur` (1981),
`gate-5-mutants-cinder` (2147), `gate-5-mutants-sluice` (2482).

Consideration recorded, not resolved here (resolution is Crafty's at
DELIVER): a guard of the shape `if std::env::var(...).is_err() { return }`
carries trivial mutants (for example negating the condition, or deleting
the early `return`). `cargo mutants` can only KILL those mutants if the
gated test actually executes its post-guard body during the mutation run,
which requires `KALEIDOSCOPE_PERF_TESTS` to be SET in the mutation job
environment. If the mutation job runs with the variable absent, every
gated test short-circuits and a mutant that deletes the guard would
survive unkilled. Crafty must ensure the variable is set in whichever
environment the perf-crate Gate 5 jobs use to exercise the guard, OR
confirm that the existing `--in-diff` mutation surface for these crates
does not depend on the guarded bodies. This note flags the consideration;
the 100% kill-rate gate (ADR-0005 Gate 5) is the binary signal that it
was handled correctly.

## DELIVER does the ci.yml edit

Per the DESIGN handoff (`../design/wave-decisions.md`, DD5 and the DEVOPS
Handoff section), the `gate-1-test` job-level `env` block and the 28
guard-line preambles are ONE coherent change and land together in a
single `feat` commit at DELIVER. Apex (this wave) does NOT touch
`.github/workflows/ci.yml`. Crafty (DELIVER) makes BOTH edits atomically:
the four-line guard preamble at all 28 sites AND the two-line
`gate-1-test` env block. Splitting the one-line CI edit from the guards
it gates would leave main in a window where CI sets a variable no test
reads, severing a coherent change across two waves for no benefit. The
pre-commit hook is left untouched; its absence of the variable IS the
local-skip mechanism.

## Inherited from slim precedent

This wave inherits the structure and per-decision shape of
`docs/feature/log-body-regex-search-v0/devops/wave-decisions.md` (slim
DEVOPS, 2026-05-29). Both are slim waves that verify the existing
ADR-0005 five-gate contract covers a small, coherent change and record
the inherited posture without amending any workflow file in the DEVOPS
wave itself. The DEVOPS posture is identical at the workflow and
deployment layers. The one structural difference is that the regex
sibling required no CI edit at all, whereas this feature does require a
two-line `gate-1-test` env addition, which by deliberate design is
deferred to DELIVER so it lands atomically with the guard lines it
governs.

## Upstream Changes

None. Zero DISCUSS assumptions changed by this DEVOPS wave. Zero DESIGN
assumptions changed: the DESIGN handoff (`../design/wave-decisions.md`)
is ratified verbatim. Apex documents the environment contract and the
inherited CI posture; the atomic implementation (guard lines plus the
`gate-1-test` env block) is Crafty's at DELIVER.
