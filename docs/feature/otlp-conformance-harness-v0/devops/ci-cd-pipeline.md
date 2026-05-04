# CI/CD Pipeline — `otlp-conformance-harness-v0` (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-03.
> **Author**: Apex.
> **Workflow file**: `.github/workflows/ci.yml`.

---

## Framing

Library, not service. There is no "CD" half of CI/CD here in the sense
of progressive delivery, canary deployments, or environment promotion.
"CD" reduces to "merge to `main` is the contract that the gates have
all passed". The five ADR-0005 gates are the entire pipeline.

The platform on which the pipeline runs is **GitHub Actions**, chosen
because the repository lives on GitHub and ADR-0005 explicitly defers
the runner choice to this wave. The roadmap's FOSS-replacement table
excludes GitHub Actions only as a *bundled Kaleidoscope dependency*,
not for the project's own CI; using GitHub Actions to test a CC0 Rust
crate hosted on GitHub is unobjectionable and has no operator-side
SaaS-lock-in implication.

## The five gates

Each gate maps 1:1 to a job in `ci.yml`. Jobs run in **descending
speed order** (fail-fast) per ADR-0005:

| # | Gate | Job in `ci.yml` | Tool | Toolchain | Wall-clock budget |
|---|---|---|---|---|---|
| 4 | Dependency policy | `gate-4-deny` | `cargo-deny check` | n/a (action ships its own) | < 30 s |
| 1 | Test suite | `gate-1-test` | `cargo test --all-targets --locked` | stable 1.85 | 1–3 min |
| 2 | Public-surface lock | `gate-2-public-api` | `cargo public-api` | nightly (`NIGHTLY_PIN`) | 1–2 min |
| 3 | SemVer compliance | `gate-3-semver` | `cargo-semver-checks` | nightly (`NIGHTLY_PIN`) | 1–2 min |
| 5 | Mutation testing | `gate-5-mutants` | `cargo-mutants` | stable 1.85 | 30 s – 5 min |

`gate-1-test` depends on `gate-4-deny`. `gate-2-public-api` and
`gate-3-semver` both depend on `gate-1-test`. `gate-5-mutants` depends
on both Gate 2 and Gate 3 (the slowest gate runs last). Failure in any
upstream job aborts the dependent jobs naturally, preserving the
fail-fast intent.

## Trigger rules

```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
```

This matches Trunk-Based Development (see `branching-strategy.md`).
Every push to `main` runs the full pipeline; every PR runs the same
gates before merge. A `concurrency: cancel-in-progress: true` group
cancels superseded runs on the same branch / PR to save minutes.

There is no separate `tags` trigger in v0 because the crate is
`publish = false` in `Cargo.toml` and there is no release workflow
yet. When v0.1 introduces release publication to crates.io, a tag
trigger and a separate release workflow will be added then.

## Toolchain matrix

The pipeline runs on **`ubuntu-latest`** as the single OS for v0. This
is the most cost-effective and fastest GitHub-hosted runner; the
harness is pure Rust with no platform-specific code (`brief.md`
attribute "Portability"), so per-OS testing buys nothing for v0.
Future iterations may add macOS and Windows jobs to `gate-1-test` if
the harness ever depends on platform-specific code paths, which it
does not currently.

Two channels are used:

1. **Stable Rust 1.85** (project MSRV, pinned via `rust-toolchain.toml`
   at the repo root). Used by Gates 1, 4, and 5. The
   `dtolnay/rust-toolchain@stable` action honours
   `rust-toolchain.toml`.
2. **Pinned nightly** (`NIGHTLY_PIN` env var, currently
   `nightly-2026-04-15`). Used by Gates 2 and 3, both of which require
   nightly compiler internals that `cargo public-api` and
   `cargo-semver-checks` consume.

The nightly date is pinned explicitly rather than tracked
(`@nightly`) so:

- public-API diffs (`cargo public-api`'s output) are reproducible
  across runs;
- a nightly compiler regression cannot break the pipeline overnight
  through no fault of the project;
- the bump is a deliberate maintainer action, surfaced as a one-line
  edit to the env var.

The bump cadence is informal: revisit when Gate 2 or Gate 3 produces
unexpected noise, or quarterly, whichever comes first. There is no
need for a synchronised stable-and-nightly release cadence.

## Cache strategy

```yaml
- uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry
      ~/.cargo/git
      target
    key: ${{ runner.os }}-cargo-<channel>-${{ hashFiles('**/Cargo.lock') }}
    restore-keys: |
      ${{ runner.os }}-cargo-<channel>-
```

Two cache namespaces are used: `cargo-stable-` (Gates 1 and 5) and
`cargo-nightly-` (Gates 2 and 3). They are kept separate because
nightly's `target/` artefacts cannot be reused by stable and vice
versa; mixing them produces spurious recompiles.

Gate 5's `cargo mutants` job uses a third namespace,
`cargo-stable-mutants-`, with a fallback `restore-keys` chain to the
plain `cargo-stable-` namespace. This protects the mutation-test cache
(which gets invalidated by every source change) from polluting the
faster Gate 1 cache (which is stable across mutation runs).

The cache is best-effort: the gates run correctly with a cold cache,
just slower. There is no correctness dependency on the cache.

## Artefact policy

| Artefact | Produced by | Format | Retention | Consumer |
|---|---|---|---|---|
| `verdict-counts.json` | Gate 1 (KPI 4 capture step) | JSON, schema_version=1 | 90 days | KPI 4 reporting (see `kpi-instrumentation.md`) |
| `mutants.out/` | Gate 5 | cargo-mutants native directory | 30 days | Audit trail for the 100 %-kill-rate claim |

The verdict-counts artefact is the central KPI 4 data feed. Its
retention is set to 90 days so a quarterly review can scan three
months of history; longer retention is unnecessary because the metric
of interest is "did KPI 1 hold at 0 % every commit?", which is
computed from the latest run plus the historical CI runs visible in
the GitHub Actions UI.

The artefact is uploaded with `if: success() || failure()` so a
failed CI run still leaves a forensic trail; only a cancelled run
produces no artefact.

`actions/upload-artifact@v4` is used (v3 is deprecated as of January
2025). The upload is unconditional on test outcome (the artefact
exists either way) but conditional on the Gate 1 step having run.

## Mutation-test budget rule (Crafty's Q4)

**Start state**: full `cargo mutants` per push. Local DELIVER run was
~45 s for 39 mutants on a developer workstation; a `ubuntu-latest`
runner is comparable.

**Threshold**: 60 seconds wall-clock for any single feature's full
mutation run. The threshold is wall-clock because it is what a
contributor experiences while waiting for CI.

**Switch-over rule**: when a single feature's full mutation run
exceeds 60 seconds wall-clock for two consecutive merges to `main`,
the maintainer switches Gate 5 from full-run mode to
diff-against-main mode by changing the gate's invocation from:

```yaml
cargo mutants --package otlp-conformance-harness --no-shuffle --jobs 2
```

to:

```yaml
cargo mutants --package otlp-conformance-harness --no-shuffle --jobs 2 --in-diff origin/main
```

The PR that flips the switch must include:
- a one-paragraph note in `docs/feature/<feature>/devops/wave-decisions.md`
  recording the observed run-times that triggered the switch;
- a manual full-run check showing the kill rate is still 100 % at the
  time of the switch (run locally, attached as a CI artefact).

The 60-second threshold is high enough that the harness's v0 surface
(~280 lines, 39 mutants, ~45 s) sits comfortably below it. The
escalation only fires when growth makes the gate genuinely painful —
which is the right time to give up the full-coverage signal.

The `timeout-minutes: 30` ceiling on `gate-5-mutants` is an upper
safety bound; well above the 60 s threshold but well below
runaway-job territory.

## Quality gates classification

Per the `cicd-and-deployment` skill's gate taxonomy:

| Category | Stage | Gate | Type |
|---|---|---|---|
| Local | Pre-commit (recommended) | `cargo fmt --check` | Blocking (developer) — see "Local quality gates" below |
| Local | Pre-push (recommended) | `cargo test -p otlp-conformance-harness --all-targets --locked` | Blocking (developer) |
| PR | Pull request | All five gates as required status checks on `main` | Blocking (merge) — see `branching-strategy.md` |
| CI | Commit stage | Gate 4 (cargo deny) | Blocking (pipeline) |
| CI | Commit stage | Gate 1 (cargo test, all targets) | Blocking (pipeline) |
| CI | Commit stage | Gate 2 (cargo public-api) | Blocking (pipeline) |
| CI | Commit stage | Gate 3 (cargo semver-checks) | Blocking (pipeline) |
| CI | Commit stage | Gate 5 (cargo mutants) | Blocking (pipeline) |
| Deploy | n/a | n/a | n/a |
| Production | n/a | n/a | n/a |

There is no acceptance / capacity / production stage because there is
no runtime to deploy to. Every gate is a CI commit-stage gate; the
notion of "acceptance stage" collapses into the test suite that runs
under Gate 1, which already includes the integration-style corpus
runner from slice 07.

## Local quality gates (recommendation, not enforced by this wave)

This wave does not introduce a `lefthook` / `pre-commit` configuration
because:

- the harness has 73 tests that complete in seconds, so `cargo test`
  before pushing is cheap and the developer's natural reflex anyway;
- a contributor mismatch between local hooks and CI is a real source
  of confusion and the project does not have enough CI-flake history
  to justify the boilerplate.

If a future feature introduces slow local checks that contributors
forget, a `lefthook` configuration can be added then. For v0 the
recommendation in `CONTRIBUTING.md` (when next updated) is:

```sh
# Before committing
cargo fmt
cargo test -p otlp-conformance-harness --all-targets --locked

# Before pushing
cargo deny check
```

Each of these mirrors a CI gate exactly, satisfying the
`cicd-and-deployment` skill's "mirror, not duplicate" principle.

## Pipeline security

| Stage | Check | Tool |
|---|---|---|
| Commit (CI) | Licence policy + advisory database | `cargo deny check` (Gate 4) |
| Commit (CI) | Pin policy (`opentelemetry-proto = "=0.27.0"`) | `cargo deny check` `bans` table (Gate 4) |
| Commit (CI) | Yanked-versions ban | `cargo deny check` `advisories` table (Gate 4) |
| Commit (CI) | Public-API surface drift | `cargo public-api` (Gate 2) |
| Commit (CI) | SemVer-correctness | `cargo-semver-checks` (Gate 3) |
| Commit (CI) | Test-quality (mutation) | `cargo mutants` (Gate 5) |
| Workflow | Action version pinning | Third-party actions pinned to commit SHAs in `ci.yml` |
| Workflow | Token scope | `permissions: read` at workflow level; no job opts in to writes |
| Workflow | Secrets | None declared; none used |

No SAST tool is configured because the harness has no
code-injection surface (no string-templated SQL, no `eval`, no
shell-out, no FFI, no `unsafe`). No DAST tool is configured because
there is no runtime. No SBOM tool is configured beyond `cargo deny`'s
licence enumeration; the SBOM equivalent for a Rust library is
`Cargo.lock`, which is committed and exact-pinned. If a downstream
consumer needs a CycloneDX or SPDX export, `cargo cyclonedx` or
`cargo sbom` can be run on demand against the locked dependency tree.

## DORA-metric posture

The DORA metrics map oddly onto a library with no deployment target.
The mapping the project uses:

| Metric | Mapping for the harness |
|---|---|
| Deployment frequency | Merge-to-`main` frequency. v0 had 8 commits in DELIVER; ongoing target is "every conforming commit lands". |
| Lead time for changes | Time from author's `git push` to merged-on-`main` (gates passing). Target: < 30 minutes including all five gates. |
| Change failure rate | % of `main` commits that subsequently require a revert. Target: 0 % (the gates are designed to make red `main` impossible). |
| Time to restore | Wall-clock from a red `main` to green `main`. Target: < 1 hour, achieved by reverting offending commit immediately. |

The harness is structurally in the **Elite** band on every metric
because the gates make `main` always green: a red commit cannot land,
so change failure rate is 0 % by construction; merge frequency is
limited only by the gates' wall-clock budget. This is the trunk-based
development pay-off the principle of building quality in is meant to
deliver.

## Rejected simple alternatives

Per the skill's "simplest solution check" requirement.

### Alternative 1 — Single `cargo test` step in a one-job workflow

- **What**: a single workflow job invoking `cargo test --all-targets`
  on stable Rust 1.85, no Gates 2–5.
- **Expected impact**: covers KPI 1, 2, 3, 6, but misses Gate 2
  (public-API drift), Gate 3 (SemVer), Gate 4 (licence /
  pin / advisory), Gate 5 (mutation testing).
- **Why insufficient**: ADR-0005 explicitly accepts five gates because
  Gates 2–5 each defend a distinct cross-cutting concern that
  `cargo test` cannot. Rejected.

### Alternative 2 — Run all five gates serially in one job

- **What**: one job, five steps, no `needs:` graph.
- **Expected impact**: simpler YAML; equivalent fail-fast behaviour
  via `set -e`.
- **Why insufficient**: GitHub Actions caches per job; co-mingling
  stable and nightly steps in one job means each step would either
  fight for the same cache key (cache misses) or skip caching
  entirely. The 4-job split is the minimum that lets each toolchain
  reuse its own cache cleanly. Rejected.

### Alternative 3 — Self-hosted runner

- **What**: deploy a self-hosted runner on a developer machine.
- **Expected impact**: zero CI minute consumption.
- **Why insufficient**: introduces an availability dependency on a
  developer's machine being on; introduces a maintenance burden
  (runner updates, host security); and the `ubuntu-latest`
  GitHub-hosted minute count for a single small Rust crate is well
  within the free-tier ceiling. Rejected.

The chosen design (four jobs, fail-fast graph, GitHub-hosted Ubuntu
runner, two cache namespaces, two retention-bounded artefacts) is the
minimum that honours ADR-0005's contract. Each component has a
distinct, named justification.
