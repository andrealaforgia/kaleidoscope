# Branching Strategy — Kaleidoscope (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-03.
> **Author**: Apex.
> **Scope**: Repository-wide. Although authored as part of the
> `otlp-conformance-harness-v0` DEVOPS wave, this strategy applies to
> every Kaleidoscope crate that lands subsequently.

---

## Strategy

**Trunk-Based Development.** A single long-lived branch named `main`.
Short-lived feature branches optional, never required. Direct commits
to `main` are permitted by humans and AI agents, gated by the five
ADR-0005 CI checks. Every commit on `main` is releasable; every
release is a tag on `main`.

This was already the project's de-facto pattern in DISCUSS, DESIGN,
DISTILL, and DELIVER for `otlp-conformance-harness-v0` (eight commits
direct to `main`, no feature branches, Conventional Commits naming).
This document codifies it as the project rule.

## Why Trunk-Based Development

Per the `cicd-and-deployment` skill, Trunk-Based Development suits
"high-performing teams, continuous deployment, mature test suites".
The Kaleidoscope project has all three for AI-agent-driven workflows:

- The "team" is a small set of AI agents with deterministic,
  spec-driven behaviour and a single human reviewer.
- "Continuous deployment" maps imperfectly onto a library, but the
  spirit (every commit on `main` is shippable) holds: the five
  ADR-0005 gates make a non-shippable commit impossible to land.
- The test suite is mature for v0 (73/73 passing, 100 % mutation kill
  rate).

Long-lived feature branches would actively harm this workflow because:

1. They split CI signal across branches, reducing the protective
   coverage of `main`.
2. They invite merge conflicts that are expensive to reconcile in an
   AI-agent loop (the agent-orchestrator does not have a human's tacit
   sense of "your colleague was working on this; defer to them").
3. Conventional Commits lose much of their power when buried inside a
   squash-merged feature branch.

## Branch protection rules (`main`)

Configured in the GitHub repository's "Settings → Branches → Branch
protection rules" page. The required posture is:

| Rule | Setting | Rationale |
|---|---|---|
| Require pull request reviews before merging | Off (allow direct push) | Trunk-Based Development; AI-agent commits go direct. The peer-review gate happens upstream of the commit, in the wave's `peer-review-iteration-N.md` artefacts. |
| Require status checks to pass before merging | On | All five gates (`gate-4-deny`, `gate-1-test`, `gate-2-public-api`, `gate-3-semver`, `gate-5-mutants`) must be green. |
| Require branches to be up to date before merging | On | Forces re-running the gates against the latest `main` before merge. |
| Require linear history | On | No merge commits on `main`. Every commit is a fast-forward of an earlier commit; Conventional Commits messages stay grep-able. |
| Require signed commits | Off (for now) | Not enforced because the contribution surface is currently a single human + AI agents on his machine; revisit when external contributors arrive. |
| Require conversation resolution before merging | n/a | No PR conversations to resolve when direct push is allowed. |
| Restrict pushes that create matching branches | Off | Direct push permitted. |
| Allow force pushes | **Off** | Force-push to `main` is forbidden. |
| Allow deletions | **Off** | `main` cannot be deleted. |

The combination "no force-push, no delete, linear history, all five
status checks required" is the minimum that prevents a contributor (or
a confused agent) from accidentally rewriting `main`'s history or
landing untested code. Reviews are not required because the
upstream-of-commit peer-review gate (carried out by the
`*-reviewer` agents and recorded in the wave's
`peer-review-iteration-N.md` files) plays that role.

## Required status checks (the five gates)

The branch-protection setting "Require status checks to pass before
merging" must list, by job name, all five jobs from
`.github/workflows/ci.yml`:

```
gate-4-deny
gate-1-test
gate-2-public-api
gate-3-semver
gate-5-mutants
```

These names are stable across the workflow's lifetime (renaming a job
in the YAML breaks the branch-protection wiring), so future workflow
edits should preserve them or update the protection rules in lockstep.

## Commit message convention

Conventional Commits, in line with what DELIVER already produced:

```
<type>(<scope>): <subject>

[optional body, wrapped at 72 chars]

[optional footer(s)]
```

Allowed `<type>` values: `feat`, `fix`, `docs`, `chore`, `refactor`,
`test`, `build`, `ci`, `perf`. Allowed `<scope>` values are unbounded
in v0; the convention so far is `otlp-harness` for crate-level
changes, `workspace` for repository-root changes, and feature names
for documentation under `docs/feature/`.

This is convention, not enforcement. A commit-lint hook or a CI step
checking message format can be added if drift becomes a problem; for
v0 it has not.

## Release workflow

For v0 the harness is `publish = false` and emits no releases. Tags
are reserved for future v0.x or v1 releases when the crate ships to
crates.io. The release workflow shape (deferred):

1. Bump `version` in `crates/otlp-conformance-harness/Cargo.toml`.
2. Update `OTLP_SPEC_VERSION` if the spec pin changed.
3. `cargo public-api` and `cargo semver-checks` against the previous
   release tag (the existing nightly Gates 2 and 3 already do this on
   every commit; the release workflow re-runs them deliberately).
4. Tag `v<MAJOR>.<MINOR>.<PATCH>` on the merge commit.
5. A new GitHub Actions workflow (not authored in this wave) reacts to
   the tag and publishes to crates.io.

Step 5 is intentionally out of v0's scope: there is nothing to publish
yet. The release workflow YAML will be added when the first release
is cut.

## Branch hygiene rules

| Rule | Enforcement |
|---|---|
| `main` is the only branch with branch-protection rules | Repository setting |
| Feature branches, when used, are short-lived (< 1 day target) | Convention |
| Feature branches that diverge from `main` for > 7 days are deleted on sight | Convention |
| Stale tracking branches (`origin/<gone>`) are not pruned automatically; contributors run `git fetch --prune` locally | Convention |

## Hand-off to the next feature's DEVOPS wave

When the second Kaleidoscope crate (Codex, per the roadmap) reaches
its DEVOPS wave, its platform-architect should:

1. Read this document; do not re-derive the strategy.
2. Add the Codex crate's path to the workflow's path filters (if path
   filters get added) without changing any of the five gate jobs.
3. Add the Codex crate's `gate-1-test` invocation to the workflow's
   `cargo test` step (matrix or additional `-p` flag).
4. Confirm the existing branch-protection rules still apply.

The strategy itself is unlikely to change between Phase 0 features.
If it ever does, the change goes through this same DEVOPS-wave
discipline (peer-reviewed, recorded in a `wave-decisions.md`,
back-propagated to this document), not through ad-hoc setting flips
in the GitHub UI.
