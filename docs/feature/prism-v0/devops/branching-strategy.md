# Prism v0 — Branching strategy

- **Wave**: DEVOPS
- **Author**: `@nw-platform-architect` (Apex, dispatched by Bea)
- **Date**: 2026-05-08
- **Inputs**: project memory ("Kaleidoscope is pure trunk-based, no CI
  gates"; "Fix-forward + Post-merge correction"); existing CI workflow
  posture (no required-status-checks, no enforce_admins).
- **Companion**: `ci-cd-pipeline.md`, `wave-decisions.md`.

---

## 1. Strategy — pure trunk-based

Prism v0 inherits the project's existing branching posture without
modification:

- **One long-lived branch**: `main`.
- **Direct commits to main** allowed.
- **Short-lived feature branches** acceptable for in-progress work
  but NOT required.
- **No required-status-checks** on `main`.
- **No `enforce_admins`** on `main`.
- **No PR-mandatory workflow**. Pull requests are optional and used
  primarily for review when the change benefits from a second pair
  of eyes.

This matches the project memory exactly:

> Kaleidoscope is pure trunk-based, no CI gates — main has no
> required-status-checks and no enforce_admins; CI is feedback, not
> a gate.

---

## 2. CI posture

### 2.1 Triggers

The Prism gates fire on the same triggers as the existing Rust gates:

```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
```

**Push to main**: every push triggers the full Prism + Rust pipeline.
The cancel-in-progress concurrency block prevents stacked runs on
the same branch.

**Pull request to main**: every PR triggers the same gates. Results
appear as commit-status checks on the PR but they do NOT block
merge — Andrea can merge on red and fix-forward.

### 2.2 What CI fails do NOT do

- **Do NOT block merge**. Merge is always allowed.
- **Do NOT require approval from a second reviewer**. Andrea is the
  sole reviewer at v0.
- **Do NOT page anyone**. Failed gates are visible in the GitHub
  Actions UI and in commit-status indicators.

### 2.3 What CI fails DO trigger

- **Fix-forward**: a follow-up commit that addresses the regression,
  pushed directly to main.
- **Post-merge correction note**: per project memory, small CI / test /
  infra defects on closed nWave waves are addressed by appending to
  the relevant `wave-decisions.md`, not by opening a new feature.

---

## 3. Discipline that keeps `main` green

Without required-status-checks, "main is green" is a social
discipline rather than a structural one. The discipline is mature in
the project's Rust-only era and Prism v0 inherits it unchanged.

### 3.1 Local pre-commit gates

The pre-commit hook (extended per `ci-cd-pipeline.md > Pre-commit
hook contract`) runs:

- Rust: fmt, clippy, deny check, test workspace (existing).
- Prism: lint, format:check, typecheck, vitest (added; conditional
  on `apps/prism/package.json` presence).

Total wall-clock for a Rust-only commit on a contributor's laptop:
~30 seconds. For a full-stack commit: ~90 seconds. The hook is
contributor-friendly: a missing tool yields a yellow `[skip]`
warning, not a hard failure.

> **HIGH-1 note (Forge iter-1)**: the wall-clock targets above are
> aspirational, not benchmarked. At slice 01 implementation, the
> crafter benchmarks the hook on a clean machine and reports
> actual timings in the slice-completion document. If full-stack
> hook time exceeds 2 minutes, revisit the gate set: move
> Playwright out of pre-commit (CI-only); parallelise Vitest with
> a worker pool; or move slow-path lint rules to pre-push.
> Contributors bypassing the hook because it is too slow is the
> failure mode this monitoring guards against.

### 3.2 Local pre-push gates

The pre-push hook runs the nightly-toolchain-bound Rust gates
(`cargo public-api`, `cargo semver-checks`). Prism v0 adds nothing
to pre-push (the TS ecosystem has no analogue to those gates; an
SPA has no published library API surface).

### 3.3 Fix-forward ritual

When CI fails on main:

1. Andrea identifies the failure from the GitHub Actions UI.
2. Andrea commits the fix directly to main.
3. If the failure is a small CI / test / infra defect on a closed
   wave (per project memory), Andrea appends a fix-forward note to
   the relevant `wave-decisions.md` describing what changed and
   why. This keeps the wave-decisions document the single source
   of truth for the wave's lifecycle without re-opening the wave.
4. CI re-runs on the new commit; the previous run's red status
   stays red but is superseded by the new commit's green.

### 3.4 Verification ritual

Per project memory ("Verify every commit captures actual changes"):
after every commit, Andrea runs `git show --stat HEAD` to confirm
the commit captured what was intended. This is especially load-
bearing for fix-forward commits where a silent miss is most
expensive.

---

## 4. Release shape — same-as-trunk

Prism v0 has no release branch, no release tag at v0 (the v0 closure
is a wave-decisions.md update, not a published artefact). When v0.x
or v1 introduces published releases, the shape graduates as follows:

### 4.1 Tag-based releases

For Phase 2+ when Prism produces a versioned bundle for the operator
to pin:

- A semver tag `prism-vX.Y.Z` on a commit on main triggers a release
  workflow.
- The release workflow runs the same gates plus a `pnpm --filter
  prism build` and uploads the `dist/` folder as a GitHub release
  asset.
- Operators pull `dist/` from the release asset and deploy.

This release workflow does NOT exist at v0; it is documented as the
v0.x graduation shape.

### 4.2 Why no release branches at v0

- **Single product version supported at a time**. Operators pin the
  specific commit / asset they deploy; if v0.2 introduces a breaking
  config schema change, operators upgrade at their cadence and
  Kaleidoscope does not maintain a v0.1 fix branch.
- **Conway's law shape**: one operator (Andrea), one designer, one
  developer. Release branches add overhead with no team benefit.
- **Cargo workspace's posture is the same**. No release branches for
  the Rust crates either.

---

## 5. Pull request posture

### 5.1 When to open a PR

PRs are optional. Open a PR when:

- The change benefits from a second pair of eyes (Andrea sometimes
  asks Bea or a wave-architect agent for a focused review on a
  specific concern).
- The change is large enough that the commit-by-commit story
  benefits from being reviewed in one place.
- An external contributor (post-v0, hypothetically) submits a
  change.

PRs are NOT required for:

- The bulk of Andrea's solo work (direct commits to main are the
  default).
- DELIVER slice-by-slice work (each slice's commit is the wave's
  natural review unit; the post-slice peer review by the matching
  reviewer agent is the formal review surface).

### 5.2 PR review surface (when used)

The wave-architect-reviewer agents (Atlas for DESIGN, Scholar for
DISTILL, Crafty for DELIVER, Apex's own reviewer for DEVOPS) are
the formal review surface within the nWave methodology. Their
critiques live in YAML feedback files. PRs in the GitHub sense are
a supplementary surface, not a replacement for the wave-internal
reviewer cycle.

---

## 6. Comparison table — strategies considered

The orchestrator's brief locks the strategy as pure trunk-based per
project memory. For completeness, this table records why other
strategies are not adopted:

| Strategy | Why not |
|---|---|
| GitHub Flow (PR-required, status-checks-required) | Requires a second reviewer for every change; Andrea is solo. Adds PR overhead with no team-coordination benefit. |
| GitFlow (`develop`, `release/*`, `hotfix/*`) | Optimised for scheduled releases with multiple in-flight versions. Prism v0 has no scheduled-release cadence; operators deploy at their own pace from main commits. |
| Release branching (`release/1.x`, `release/2.x`) | Optimised for supporting multiple product versions in production simultaneously. Kaleidoscope supports one v0 at a time. |
| Trunk-based with required-status-checks | The middle ground: trunk-based shape but CI gates merge. Project memory rejects this explicitly: CI is feedback, not a gate. The fix-forward + post-merge correction posture absorbs flakes; required-status-checks would slow Andrea's solo cadence with no defect-rate benefit. |

---

## 7. Branch protection rules — explicitly NONE

For the avoidance of doubt, the project's `main` branch has:

- ❌ No "Require a pull request before merging".
- ❌ No "Require status checks to pass before merging".
- ❌ No "Require branches to be up to date before merging".
- ❌ No "Require signed commits" (commits are signed informally per
  Andrea's preference but not gated).
- ❌ No "Require linear history".
- ❌ No "Restrict pushes" / "Restrict who can push".
- ❌ No "Do not allow bypassing the above settings" (a.k.a. enforce_admins).

The above remains the case after Prism v0 lands. The Prism gates
extend the CI feedback surface without changing the branch
protection posture.

---

## 8. CI run cancellation policy

Inherited from the existing workflow:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

A new push to a branch (main or a feature branch) cancels any
in-progress run for that same `(workflow, ref)` pair. This is
critical for the trunk-based posture: Andrea pushing five commits in
quick succession should not queue five 30-minute mutation-testing
runs; only the latest commit's run survives.

The Prism gates inherit the same concurrency block. No
Prism-specific concurrency configuration.

---

## 9. Cross-references

- **Project memory**: `Kaleidoscope is pure trunk-based, no CI gates`;
  `Fix-forward + Post-merge correction notes`.
- **Pre-commit hook contract**: `ci-cd-pipeline.md` § 4.
- **CI workflow contract**: `ci-cd-pipeline.md` § 5.
- **Verification ritual**: project memory `Verify every commit captures actual changes`.
