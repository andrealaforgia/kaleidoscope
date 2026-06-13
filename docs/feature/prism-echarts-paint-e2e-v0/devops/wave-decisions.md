<!-- markdownlint-disable MD013 MD024 -->

# Wave Decisions — prism-echarts-paint-e2e-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave) — the load-bearing D4 wave (the CI-browser job)
- **Engineer**: Apex (nw-platform-architect)
- **Date**: 2026-06-13
- **Mode**: autonomous overnight; no questions returned to the operator.
- **Inputs grounded**: DESIGN `design/wave-decisions.md` (DD1-DD6, D4
  flagged for DEVOPS, C1-C10), `docs/product/architecture/adr-0075-...md`
  (the paint-signal contract, the narrowed swallow, the D4 CI-browser
  dependency + honest limit), DISCUSS `discuss/outcome-kpis.md` (KPI-1..6;
  KPI-2/KPI-6 are DEVOPS's remit; north-star = the chart genuinely paints
  in a real browser), `apps/prism/playwright.config.ts` (the digest SSOT,
  the chromium/firefox/webkit projects, the `__no-spec-matches-yet__`
  matcher), `apps/prism/e2e/global-setup.ts` (the docker Prometheus
  fixture), `apps/prism/package.json` (pnpm, `@playwright/test` 1.49.1, the
  scripts), `.github/workflows/ci.yml` (the standing 9 Rust gates + prism
  gates 6-11), `docs/feature/prism-v0/devops/environments.yaml` (the prior
  prism DEVOPS environment model), and the `claims-honesty-pass-2-v0/devops`
  sibling for voice.

## nWave-order note (read before judging "missing" code)

nWave order is DISCUSS -> DESIGN -> **DEVOPS** -> DISTILL -> DELIVER.
DEVOPS runs BEFORE DISTILL and DELIVER. The slice-01 + slice-03 spec
**bodies** are not written yet, and `testMatch` still matches no spec —
that is the EXPECTED, CORRECT state at this wave. This wave wires the
CI-browser job that will run them; authoring the specs (DISTILL) and the
paint signal + un-MARK (DELIVER) comes later. Absent specs at DEVOPS-close
is not a defect.

## Headline — the CI-browser job ALREADY EXISTS; DEVOPS adapts it, it does NOT add a duplicate

**Did prism have any CI presence before this feature? YES — a full one.**
The prism-v0 DEVOPS wave already stood up prism gates 6-11 in
`.github/workflows/ci.yml`, INCLUDING **`gate-7-prism-playwright`** — a
Playwright E2E job on ubuntu-latest that sets up pnpm 9 + node 22, installs
the Playwright browsers, runs the suite, and uploads the HTML report. The
D4 "CI-browser job" the DESIGN flag asked DEVOPS to "stand up" is therefore
NOT a greenfield job: it exists and is green today (it ran the full
Chromium/Firefox/WebKit matrix against an empty `testMatch`, so it passed
trivially).

Per existing-infrastructure-first (Core Principle 2, Critical Rule 2 —
"justify every new component with no-existing-alternative"), the right move
is to **ADAPT gate-7-prism-playwright in place** to ADR-0075's D4/C7
contract, NOT to add a duplicate `prism-e2e` job. A second job running the
same Playwright suite would double the compute, double the docker-fixture
provisioning, and split the e2e signal across two job names for no benefit.

## Decision A1 — Deploy strategy: N/A (no deploy surface)

prism v0 ships a static SPA bundle (`apps/prism/dist/`) that an operator
drops into their own reverse proxy; Kaleidoscope deploys nothing
(prism-v0 environments.yaml `operator_deployment_shape`). No
rolling/blue-green/canary applies. This feature is a **test-architecture**
change (a real-browser paint proof), not a runtime change.
**Rollback = `git revert`** of the workflow + doc commit (degenerate; no
runtime surface). Recorded per the rollback-first principle even though the
surface is empty.

## Decision A2 — The CI-browser job: ADAPT gate-7 (chromium-only + continue-on-error), NO new job

Three in-place edits to `gate-7-prism-playwright` (`.github/workflows/ci.yml`):

| # | Edit | Before | After | Why |
|---|------|--------|-------|-----|
| 1 | browser install | `playwright install --with-deps` (all engines) | `playwright install --with-deps chromium` | headless Chromium only (ADR-0075 C7); faster, no firefox/webkit binaries |
| 2 | run step | `pnpm --filter prism playwright` (= `playwright test --pass-with-no-tests`, all projects) | `pnpm --filter prism exec playwright test --project=chromium --pass-with-no-tests` | scope to the chromium project (C7); keep `--pass-with-no-tests` so the job stays green now (0 matched specs) and runs the real specs once DELIVER un-MARKs `testMatch` |
| 3 | job posture | (none) | `continue-on-error: true` | feedback, not a gate (D4 + C8); a red is a red X on the job, the workflow conclusion stays success |

Plus a display-name change ("Chromium/Firefox/WebKit" -> "headless
Chromium") and a header comment documenting the rationale, the
tighten-to-gating path, and the honest CI limit. The job key
`gate-7-prism-playwright` is unchanged (nothing `needs:` it), `needs:
gate-6-prism-vitest` is unchanged, `timeout-minutes: 30` is unchanged, the
checkout / pnpm / node action pins are unchanged, the artefact upload is
unchanged.

**continue-on-error rationale (D4 lean + C8).** Kaleidoscope is pure
trunk-based with no required status checks and no enforce_admins (project
memory `kaleidoscope_pure_trunk_based`); CI is feedback, not a merge gate.
The repo already encodes "visible-but-non-blocking" via the `perf-kpis`
job's `continue-on-error: true`. The paint job follows the same lever:
report without blocking trunk, until it is observed green and stable.

**No job-level env literal pitfall here.** gate-7 needs no job-level `env`
block, so the GitHub Actions job-level `${{ env.X }}` non-evaluation
pitfall (the reason perf-kpis / gates 2-3 inline literals) does not arise.
The Prometheus digest reaches the fixture via the `playwright.config.ts`
SSOT import in `global-setup.ts`, not via a CI env var.

**Firefox/WebKit.** The prism-v0 environments.yaml documents a 3-engine
runtime-matrix as the longer-term intent. The chromium/firefox/webkit
projects stay DEFINED in `playwright.config.ts`; this feature only RUNS
chromium (C7). Re-widening to the full matrix is a one-line `--project`
change for a future slice that wants it — not weakened, just scoped.

## Decision A3 — The docker Prometheus fixture: REUSE, no CI change

`apps/prism/e2e/global-setup.ts` already `docker run`s the digest-pinned
Prometheus fixture; `ubuntu-latest` provides docker, so no extra runner
setup is needed. The `PROMETHEUS_IMAGE_DIGEST` in `playwright.config.ts` is
the SSOT, byte-for-byte equal to gate-11's `services.prometheus.image`
(`prom/prometheus@sha256:378f4e0...1000fe`); bumps are a single atomic
commit per the prism-v0 environments.yaml `digest_bump_process`. **This
feature changes neither the digest nor the fixture.** (gate-7 uses the
globalSetup docker-run path; gate-11 uses a GitHub Actions `services:`
container — two consumers of the one digest, both preserved.)

## Decision A4 — continue-on-error tighten-to-gating path

Once (a) DELIVER un-MARKs `testMatch` to `slice-01-walking-skeleton.spec.ts`
+ `slice-03-error-and-empty-states.spec.ts`, AND (b) `gate-7-prism-playwright`
is OBSERVED green and stable across several `main` runs (watched via
`scripts/ci-watch.sh`), remove `continue-on-error: true` so a paint
regression reds the workflow. Order matters: tighten ONLY after the green
observation. Advertising the gate before it runs green is exactly the
dishonest-gate anti-pattern `claims-honesty-pass-2-v0` exists to retire.

## Decision A5 — Determinism / flake watch

The in-scope paint assertions are designed non-timing-dependent (ADR-0075
D2: canvas pixel non-uniformity against the digest-pinned `up` fixture;
D1: a `finished`-gated attribute, not a wall-clock). The OUT-OF-SCOPE perf
KPI blocks (the slice-01 p95 / operator-time / embedded `< 1000 ms` lines,
known overnight wall-clock flakes — MEMORY `p95_wallclock_flakes_overnight`)
are `test.fixme`d by DESIGN (D5) and do NOT enter this job. So the job
should be deterministic once the specs land; the watch-item is the
canvas/paint-signal assertion staying non-timing-dependent, already
addressed by D2.

## Decision A6 — Mutation: cargo-mutants N/A (TypeScript); StrykerJS already wired

The CLAUDE.md per-feature mutation strategy (100% kill rate, ADR-0005 Gate
5) is implemented for **Rust crates** via `cargo-mutants`. **prism is
TypeScript/React; `cargo-mutants` does not apply to it** — there is no
`crates/<name>` for prism, and no `gate-5-mutants-prism`. So the Rust
mutation-strategy question (the usual DEVOPS Decision 9) is **N/A for this
feature**, and there is **no CLAUDE.md `## Mutation Testing Strategy`
change**.

prism's mutation tooling is **StrykerJS**, already wired as
**`gate-10-mutants-prism`** (in-diff baseline cascade, in
`.github/workflows/ci.yml`), and ADR-0075 C10 already says the paint-signal
branch + the narrowed-swallow branch are the StrykerJS surface to pin IF
the component logic is in the changed set — a DELIVER concern (the crafter
runs Stryker), not a DEVOPS job edit. **No new mutation job is added.**

**How prism test quality is otherwise assured for this feature** (the bar
that replaces a Rust kill-rate gate):

- The **falsifiable paint assertion itself** (D1 ∧ D2): it RED-s against
  HEAD (no `data-prism-chart-painted`, swallowed errors, empty matcher) and
  passes only on a genuine non-blank, non-empty paint. A test that cannot
  fail proves nothing; this one provably can (KPI-1, C4).
- The **un-swallowed errors** (D3 catch-and-surface): a real-browser paint
  failure reds the e2e two independent ways (the signal never flips -> wait
  timeout; `console.error` -> the slice-03 zero-uncaught-error invariant).
- The **zero-uncaught-error invariant** (slice-03) and the visible-message
  assertions for empty / parse-error / transport-error states (KPI-4).
- The **Vitest jsdom suite staying green** (KPI-5 guardrail, gate-6).
- **gate-10 StrykerJS in-diff** remains the standing mutation net for any
  changed prism component logic (C10).

## Decision A7 — Public-API / SemVer: not triggered, no bump

Gates 2/3 (`cargo public-api` / `cargo semver-checks`) are scoped to
`otlp-conformance-harness`, `spark`, `sieve`, `codex` — Rust crates only.
prism is not in the lock and has no Rust public surface. No semver bump;
prism stays `0.1.0`. **Never 1.0.0** (project memory: 1.0.0 is Andrea's
call). Gate 4 (`cargo deny`) is unaffected — no dependency added or
changed (the job reuses the already-present `@playwright/test`).

## Infrastructure Summary

- **New CI jobs: 0.** The D4 CI-browser job is the EXISTING
  `gate-7-prism-playwright`, adapted in place (chromium-only +
  continue-on-error). New environments: 0 (deltas to the prism-v0 e2e
  environment). New dependencies: 0. New deploy surface: 0.
- **Fixture**: reused (global-setup.ts docker Prometheus, digest SSOT
  unchanged).
- **Artefact**: `prism-playwright-report` (HTML report + traces) on
  success-or-failure, retention 30 days — unchanged.
- **Local hook (ADR-0072)**: unaffected. The local pre-commit hook runs the
  fast Rust `--lib` subset; the prism e2e is a CI-side concern, watched via
  `scripts/ci-watch.sh`.

## Honest CI-verification limit (ADR-0075 C6 / KPI-6, load-bearing)

The paint assertion runs LOCALLY today under headless Chromium ("verified
locally under headless Chromium; CI verification pending the browser job").
At THIS DEVOPS-wave close `testMatch` matches no spec, so
`gate-7-prism-playwright` runs **0 specs and passes trivially** — that is
expected and is NOT a paint proof. **A GitHub Actions browser job cannot be
fully verified locally; its real verification is its first green CI run
WITH the un-MARKed specs.** This wave designed and wired the job and
reasoned about its correctness, but **does not claim it works in CI**. No
wave / README / narrative / slide may claim "CI-verified" until the job is
observed green (watch via `scripts/ci-watch.sh`).

## What this DEVOPS wave does NOT do

- Does NOT add a new CI job / gate / environment / dependency / deploy
  surface (it adapts one existing job).
- Does NOT author the slice bodies, the paint signal, or the `testMatch`
  un-MARK (DISTILL/DELIVER; their absence now is the correct nWave state).
- Does NOT change the Prometheus digest or the fixture.
- Does NOT bump any version (never 1.0.0).
- Does NOT change CLAUDE.md (mutation strategy unchanged; cargo-mutants N/A
  for a TS app).
- Does NOT claim the job is "CI-verified" (honest limit above).

## Peer review

`nw-platform-architect-reviewer` is not separately nested-invocable from
this sub-agent context. A structured SELF-REVIEW against the
platform-architect critique dimensions is recorded below; verdict
**APPROVED — 0 critical / 0 high blocking**, flagged for a top-level
reviewer run WITH the nWave-order reminder (so a reviewer does not reject on
the correct, expected absence of the not-yet-written specs).

### Self-review (platform-architect critique dimensions)

| Dimension | Assessment | Verdict |
|---|---|---|
| Measure-before-action | CI read at HEAD; gate-7 (and gates 6-11) confirmed present; the full-matrix install + no continue-on-error + empty matcher confirmed; package.json (pnpm, @playwright/test 1.49.1) and global-setup.ts docker fixture confirmed. No data assumed. | PASS |
| Existing-infrastructure-first | gate-7-prism-playwright reused and adapted, not duplicated; "no-existing-alternative" would have been FALSE for a new `prism-e2e` job, so a new job was correctly rejected. New job count 0. | PASS |
| Simplest-infrastructure-first | three minimal edits to one job + a doc; no new orchestration, no services block added to gate-7 (global-setup docker-runs the fixture), no matrix expansion. | PASS |
| CI job correctness | runs-on ubuntu-latest; chromium-only install + `--project=chromium`; `--pass-with-no-tests` keeps it green now and runs real specs post-un-MARK; action pins + node 22 / pnpm 9 unchanged; YAML re-parsed clean. | PASS |
| chromium-only (C7) | install `chromium`, run `--project=chromium`; firefox/webkit projects defined-but-not-run, re-widen is one line. | PASS |
| continue-on-error (D4 + C8) | added with rationale; matches perf-kpis precedent and pure-trunk-based posture; tighten-to-gating path documented (after observed green). | PASS |
| artefact upload | prism-playwright-report (HTML + traces) on success-or-failure, retention 30d — preserved. | PASS |
| version pinning | checkout / pnpm/action-setup / setup-node / upload-artifact all SHA- or version-pinned (inherited, unchanged). | PASS |
| job-level env literal rule | no job-level env added, so the `${{ env.X }}` pitfall does not arise; digest reaches the fixture via the playwright.config SSOT import, not a CI env var. Explicitly noted. | PASS |
| environment inventory | environments.yaml records the e2e-ci delta, the local-dev environment, platform_coverage (chromium pin + Prometheus digest), and the honest limit; inherits the prism-v0 model by reference. | PASS |
| honest CI-verification limit | stated in the job comment, environments.yaml, and here: 0-spec trivial pass now; real verification = first green CI run with un-MARKed specs; "verified locally" interim claim; no "CI-verified" until observed green. | PASS |
| no overstated readiness | the claim equals the placement (job wired + reasoned, NOT proven in CI); the trivial 0-spec pass is explicitly flagged as not-a-paint-proof. | PASS |
| mutation strategy fit | cargo-mutants N/A (TS app); StrykerJS gate-10 already wired; test quality bar = the falsifiable paint assertion + un-swallowed errors + zero-error invariant; no CLAUDE.md change. | PASS |
| rollback-first | `git revert` of the workflow + doc commit; no runtime surface. Stated (A1). | PASS |

**Self-review verdict: APPROVED — 0 critical / 0 high / 0 medium blocking.**
The CI-browser job is correctly scoped (chromium-only), correctly
non-blocking (continue-on-error feedback), reuses the existing job and
fixture, preserves the digest SSOT and the artefact, and is honest about
its CI-verification limit. Ready for the (separate) top-level reviewer and
for DISTILL/DELIVER to land the specs.
</content>
