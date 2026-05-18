# CI/CD Pipeline — `cinder-to-otlp-json-bridge-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-18
- **Workflow file**: `.github/workflows/ci.yml` (existing — NOT
  modified by this feature)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default;
  `.github/workflows/ci.yml` lines 44–52)

## Posture

The `cinder-to-otlp-json-bridge-v0` feature inherits the existing
five-gate workspace CI contract from ADR-0005 **UNCHANGED**. No new
gate is introduced. No existing gate is removed. No new workflow
file is created. **ZERO edits to `.github/workflows/ci.yml` in this
feature's DISTILL commit** — the Pulse-sink sibling
`cinder-to-pulse-bridge-v0`'s DISTILL commit added the
`gate-5-mutants-self-observe` parallel job (per that feature's A3),
and that job's `--in-diff` path filter `crates/self-observe/**`
matches the new file `crates/self-observe/src/cinder_otlp_json.rs`
automatically. The OTLP-JSON sibling lands as a pure source addition;
the CI machinery is already in place.

## Per-gate mapping to outcome KPIs

| Gate | Tool | Owns (for this feature) | KPI(s) enforced |
|------|------|--------------------------|-----------------|
| Gate 4 — `cargo deny check` | `cargo-deny` | Dependency policy. The writer adds ZERO new external deps; this gate is a no-op-for-this-feature pass. | none directly (transitive: a regression in deny.toml would block the merge that lands the writer, defending the workspace's policy invariants) |
| Gate 1 — `cargo test --workspace --all-targets --locked` | `cargo test` | Acceptance tests for the writer: `tests/cinder_to_otlp_json.rs` Slices 01/02/03 blocks + the compile-time `assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>()` probe (per ADR-0039 §3). | **OK1**, **OK2**, **OK3**, **OK4**, **OK5** (all five). The pass/fail of this gate IS the measurement of the library-contract KPIs. |
| Gate 2 — `cargo public-api` | `cargo-public-api` | (NOT YET) the public surface of `self-observe`. Gate 2 is currently scoped to {harness, spark, sieve, codex}; `self-observe` is NOT graduated in this feature (wave-decisions.md A1, identical posture to the Pulse-sink sibling's A1). ADR-0039 §1 is the audit-trail in lieu. | none directly for this feature (post-graduation: would defend OK1/OK2/OK3 against silent surface drift) |
| Gate 3 — `cargo semver-checks` | `cargo-semver-checks` | (NOT YET) SemVer compliance for `self-observe`. Same scope as Gate 2; not graduated in this feature. | none directly for this feature |
| Gate 5 — `cargo mutants` (PRE-EXISTING per-package job: `gate-5-mutants-self-observe`) | `cargo-mutants` | Mutation testing of `crates/self-observe/src/cinder_otlp_json.rs` via the inherited `--in-diff` cascade. 100% kill rate per ADR-0005 Gate 5 + CLAUDE.md per-feature MT strategy. | Test-suite quality probe supplementing OK1/OK2/OK3/OK5. A surviving mutant indicates a gap in the per-KPI measurement (the acceptance tests cannot distinguish the unmutated writer from a behaviourally-different one). |

## The (non-)workflow change

**This feature contributes ZERO new lines to `.github/workflows/ci.yml`.**

The Pulse-sink sibling `cinder-to-pulse-bridge-v0` already specifies
the addition of `gate-5-mutants-self-observe` (its DEVOPS A3 + its
DISTILL commit). By the time the OTLP-JSON sibling's DISTILL commit
lands, that job already exists in `ci.yml`. Its spec (replicated here
for cross-reference convenience):

| Field | Value |
|-------|-------|
| Job name | `gate-5-mutants-self-observe` |
| `runs-on` | `ubuntu-latest` |
| `needs` | `[gate-2-public-api, gate-3-semver]` (same as the other Gate 5 jobs) |
| `timeout-minutes` | `30` |
| Cache key | `${{ runner.os }}-cargo-mutants-self-observe-${{ hashFiles('**/Cargo.lock') }}` |
| Toolchain | stable (per `rust-toolchain.toml`) |
| `--in-diff` path filter | `crates/self-observe/**` |
| Baseline cascade | `origin/main → HEAD~1 → full` (matches beacon/aperture/spark/sieve/codex) |
| Mutation invocation | `cargo mutants --package self-observe --in-diff "$DIFF_FILE" --no-shuffle --jobs 2` |
| Artefact upload name | `mutants-out-self-observe` |
| Artefact retention | 30 days |

**Behaviour on the OTLP-JSON sibling's DISTILL commit**:

- The commit touches `crates/self-observe/src/cinder_otlp_json.rs`
  (new), `crates/self-observe/src/lib.rs` (two new lines:
  `mod cinder_otlp_json;` and `pub use cinder_otlp_json::CinderToOtlpJsonWriter;`),
  `crates/self-observe/Cargo.toml` (one new `[[test]]` block), and
  `crates/self-observe/tests/cinder_to_otlp_json.rs` (new).
- The path filter `crates/self-observe/**` matches; `--in-diff`
  passes the diff against `origin/main` (or `HEAD~1` on post-merge
  builds) to `cargo mutants`, which scopes mutation to the changed
  hunks within self-observe-owned files.
- The new file `cinder_otlp_json.rs` is mutated in full (every
  hunk is new); the existing files `lumen_bridge.rs`,
  `lumen_otlp_json.rs`, and `cinder_bridge.rs` are NOT mutated
  (zero hunks in the diff against `origin/main` after the
  Pulse-sink sibling's commit lands).
- Per-feature 100% kill rate per CLAUDE.md applies: every mutant on
  `cinder_otlp_json.rs` MUST be killed by the acceptance tests
  before DELIVER review approval.

**Why no per-file second job** (e.g.
`gate-5-mutants-self-observe-cinder-otlp-json`):

Per wave-decisions.md A3: the `--in-diff` cascade already provides
per-commit precision. Adding a second job would double Gate 5 fan-
out on self-observe-touching commits without scoping benefit. The
shared single-job posture is correct at N=4 writer files; per-
writer fan-out becomes warranted at ~8 writer files (when the
Sluice/Augur/Ray/Strata bridges and their OTLP-JSON variants land).
ADR-0038 §4 + ADR-0039 §4 deferred the same per-file refactor for
the source-tree layout (`bridges/` subdirectory); the per-job
refactor follows the same logic.

## Gates NOT modified

| Gate | Why not modified |
|------|------------------|
| Gate 4 (`cargo deny`) | Workspace-wide already; the writer adds zero new external deps so no `deny.toml` change is required. |
| Gate 1 (`cargo test --workspace`) | The new `tests/cinder_to_otlp_json.rs` is auto-discovered via the new `[[test]]` block in `crates/self-observe/Cargo.toml` (wave-decisions.md A2). The workflow invocation `cargo test --workspace --all-targets --locked` (line 182) needs no edit. |
| Gate 2 (`cargo public-api`) | `self-observe` not graduated in this feature (wave-decisions.md A1). ADR-0039 §1 is the audit-trail. |
| Gate 3 (`cargo semver-checks`) | Same as Gate 2. |
| Pre-existing Gate 5 jobs (harness, aperture, spark, sieve, codex, self-observe) | Independent. The `gate-5-mutants-self-observe` job from the Pulse-sink sibling runs in parallel; existing per-package jobs are unaffected. **This feature does not add a sixth `self-observe` job; it inherits the one created by the Pulse-sink sibling.** |
| Prism Gates 6-11 (TS/React side) | Out of scope — the feature touches only Rust crates. The path filter on the existing Prism gates excludes Rust-only commits. |

## Pre-commit and pre-push hooks

| Hook | Action this feature requires |
|------|------------------------------|
| `scripts/hooks/pre-commit` | None. The hook runs `cargo test --workspace` (mirrors Gate 1); the new test file is auto-discovered (per A2). |
| `scripts/hooks/pre-push` | None. The hook's per-pkg loop for Gates 2/3 iterates `[otlp-conformance-harness, spark, sieve, codex]`; `self-observe` is not added per A1. |

If/when wave-decisions.md A1 is reversed in a future feature
("self-observe graduates to Gates 2/3"), the pre-push hook's per-pkg
loop will need the same `self-observe` extension — out of scope for
this feature.

## Trunk-Based Development integration

The workflow already encodes TBD (per `.github/workflows/ci.yml`
lines 44–52). Every push to `main` triggers the full pipeline; every
PR runs the same gates before merge. The concurrency-group cancels
superseded runs (`cancel-in-progress: true`). No branching deviation
for this feature.

Per memory `project_kaleidoscope_pure_trunk_based`: main has no
required-status-checks and no enforce_admins; CI is feedback, not a
gate. This means a determined committer CAN merge a writer change
that fails Gate 5 — but doing so violates the per-feature MT
contract and would be caught at peer review under nWave. The CI is
the alarm; the human contract is the enforcement.

## DORA-metric framing (for completeness)

For a library-only feature with no deployment:

- **Deployment frequency**: N/A (no deploy). Closest analog: merge-
  to-main frequency; this feature targets one merge at DELIVER-wave
  close.
- **Lead time for changes**: from commit to "available to
  downstream consumers" is the time-to-merge-on-main. The five
  gates' aggregate wall-clock (~10-30 min p95) bounds this.
- **Change failure rate**: failed Gate 1 or Gate 5 percentage over
  the next 10 commits. Target: 0%. Baseline: not historically
  tracked; will be observable from `mutants-out-self-observe` and
  test artefacts on this feature's commits onward.
- **Time to restore**: revert-and-fix-forward time for a Gate 5
  regression. Per memory `feedback_fix_forward_post_merge_correction`,
  small CI/test defects on closed waves are addressed by direct
  push + wave-decisions.md append, not by a new feature.

DORA metrics are not dashboard-tracked for this feature; they are
listed here to satisfy the platform-engineering-foundations skill's
framing.

## Summary

| Question | Answer |
|----------|--------|
| Is the existing 5-gate workflow sufficient? | **Yes.** Gate 1 catches OK1/OK2/OK3/OK4/OK5 via the acceptance tests. Gate 4 is a workspace-level no-op for this feature (zero new external deps). Gates 2/3 are NOT graduated for self-observe in this feature (A1) — ADR-0039 §1 is the lock. Gate 5 inherits the `gate-5-mutants-self-observe` job from the Pulse-sink sibling. |
| Which gate enforces each KPI? | Gate 1 enforces all five KPIs (OK1/OK2/OK3/OK4/OK5). Gate 5 (pre-existing `gate-5-mutants-self-observe` job) is the supplemental test-quality probe. |
| Workflow file path | `.github/workflows/ci.yml` (single CI workflow file; existing) |
| New workflow files | NONE (per brief constraint) |
| Modifications to existing workflow | **NONE** (per A3 — the Pulse-sink sibling's DISTILL commit already added the only Gate 5 spec change required; this feature's DISTILL commit makes ZERO workflow edits) |
| Modifications to pre-commit hook | NONE |
| Modifications to pre-push hook | NONE |
| New CI dependencies | NONE (cargo-mutants already installed for existing Gate 5 jobs) |
| Files touched by DISTILL commit | `crates/self-observe/src/cinder_otlp_json.rs` (new) + `crates/self-observe/src/lib.rs` (two lines added) + `crates/self-observe/Cargo.toml` (one `[[test]]` block) + `crates/self-observe/tests/cinder_to_otlp_json.rs` (new) — four files in `crates/self-observe/`, ZERO in `.github/workflows/` |
