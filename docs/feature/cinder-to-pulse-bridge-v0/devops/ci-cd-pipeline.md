# CI/CD Pipeline — `cinder-to-pulse-bridge-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-18
- **Workflow file**: `.github/workflows/ci.yml` (existing — extended,
  not replaced)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default;
  `.github/workflows/ci.yml` lines 44–52)

## Posture

The `cinder-to-pulse-bridge-v0` feature inherits the existing
five-gate workspace CI contract from ADR-0005 **UNCHANGED**. No new
gate is introduced. No existing gate is removed. No new workflow
file is created. The single CI workflow change in this feature is
the addition of one parallel Gate 5 job
(`gate-5-mutants-self-observe`), applied in the same DISTILL commit
that lands the source file and the acceptance test file (per
wave-decisions.md A3).

## Per-gate mapping to outcome KPIs

| Gate | Tool | Owns (for this feature) | KPI(s) enforced |
|------|------|--------------------------|-----------------|
| Gate 4 — `cargo deny check` | `cargo-deny` | Dependency policy. The bridge adds ZERO new external deps; this gate is a no-op-for-this-feature pass. | none directly (transitive: a regression in deny.toml would block the merge that lands the bridge, defending the workspace's policy invariants) |
| Gate 1 — `cargo test --workspace --all-targets --locked` | `cargo test` | Acceptance tests for the bridge: `tests/cinder_to_pulse.rs` Slices 01/02/03 blocks + the compile-time `assert_send_sync::<CinderToPulseRecorder>()` probe. | **OK1**, **OK2**, **OK3**, **OK4** (all four). The pass/fail of this gate IS the measurement of the four library-contract KPIs. |
| Gate 2 — `cargo public-api` | `cargo-public-api` | (NOT YET) the public surface of `self-observe`. Gate 2 is currently scoped to {harness, spark, sieve, codex}; `self-observe` is NOT graduated in this feature (wave-decisions.md A1). ADR-0038 §1 is the audit-trail in lieu. | none directly for this feature (post-graduation: would defend OK1/OK2/OK3 against silent surface drift) |
| Gate 3 — `cargo semver-checks` | `cargo-semver-checks` | (NOT YET) SemVer compliance for `self-observe`. Same scope as Gate 2; not graduated in this feature. | none directly for this feature |
| Gate 5 — `cargo mutants` (NEW per-package job: `gate-5-mutants-self-observe`) | `cargo-mutants` | Mutation testing of `crates/self-observe/src/cinder_bridge.rs` via `--in-diff` cascade. 100% kill rate per ADR-0005 Gate 5 + CLAUDE.md per-feature MT strategy. | Test-suite quality probe supplementing OK1/OK2/OK3. A surviving mutant indicates a gap in the per-KPI measurement (the acceptance tests cannot distinguish the unmutated bridge from a behaviourally-different one). |

## The one CI workflow change

DISTILL adds the following job block to `.github/workflows/ci.yml`,
mirroring the existing `gate-5-mutants-{aperture,spark,sieve,codex}`
shape. The job is independent of the others in the Gate 5 fan-out;
adding it does not affect the existing five jobs' wall-clock.

**Spec**:

| Field | Value |
|-------|-------|
| Job name | `gate-5-mutants-self-observe` |
| `runs-on` | `ubuntu-latest` |
| `needs` | `[gate-2-public-api, gate-3-semver]` (same as the other Gate 5 jobs) |
| `timeout-minutes` | `30` |
| Cache key | `${{ runner.os }}-cargo-mutants-self-observe-${{ hashFiles('**/Cargo.lock') }}` |
| Cache restore key | `${{ runner.os }}-cargo-mutants-self-observe-` / `${{ runner.os }}-cargo-stable-` |
| Toolchain | stable (per `rust-toolchain.toml`) |
| `--in-diff` path filter | `crates/self-observe/**` |
| Baseline cascade | `origin/main → HEAD~1 → full` (matches beacon/aperture/spark/sieve/codex) |
| Mutation invocation | `cargo mutants --package self-observe --in-diff "$DIFF_FILE" --no-shuffle --jobs 2` |
| Artefact upload name | `mutants-out-self-observe` |
| Artefact retention | 30 days |

**Skeleton YAML (DISTILL pastes and fills into `.github/workflows/ci.yml`
after the existing `gate-5-mutants-codex` block, lines ~860)**:

```yaml
  gate-5-mutants-self-observe:
    name: Gate 5 — cargo mutants (self-observe)
    runs-on: ubuntu-latest
    needs:
      - gate-2-public-api
      - gate-3-semver
    timeout-minutes: 30
    steps:
      - name: Check out repository
        uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
        with:
          fetch-depth: 0

      - name: Install stable Rust toolchain
        uses: dtolnay/rust-toolchain@e97e2d8cc328f1b50210efc529dca0028893a2d9 # v1
        with:
          toolchain: stable

      - name: Cache Cargo registry, git index and target/ (self-observe)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-self-observe-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-self-observe-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (self-observe, in-diff)
        # Same --in-diff cascade as the aperture/spark/sieve/codex
        # jobs. Baseline preference order:
        #   1. origin/main if it differs from HEAD (PR case)
        #   2. HEAD~1 if origin/main matches HEAD (push to main)
        #   3. Otherwise full mutation suite
        # Empty diff (commit does not touch crates/self-observe/)
        # short-circuits to zero-second exit.
        #
        # Per-feature 100% kill rate per CLAUDE.md and ADR-0005
        # Gate 5. The bridge file crates/self-observe/src/cinder_bridge.rs
        # is the DESIGN-scoped mutation target for this feature
        # (the existing lumen_bridge.rs and lumen_otlp_json.rs are
        # already shipped and were validated by prior reviews; the
        # --in-diff filter limits today's mutation run to whichever
        # files the commit actually touched).
        run: |
          DIFF_FILE=$(mktemp)
          BASELINE=""
          if git rev-parse --verify origin/main >/dev/null 2>&1 && \
             [ "$(git rev-parse origin/main)" != "$(git rev-parse HEAD)" ]; then
            BASELINE="origin/main"
          elif git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
            BASELINE="HEAD~1"
          fi

          if [ -n "$BASELINE" ]; then
            git diff "$BASELINE" HEAD -- 'crates/self-observe/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No self-observe-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- self-observe diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package self-observe \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package self-observe \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (self-observe)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-self-observe
          path: mutants.out/
          retention-days: 30
```

DISTILL is responsible for pasting this block; DEVOPS does not edit
the workflow file itself (per the brief constraint "Do NOT add new
CI workflow files" and per the established Beacon/Sieve/Codex
precedent where the DISTILL commit lands all of source skeleton + CI
extension atomically).

## Gates NOT modified

| Gate | Why not modified |
|------|------------------|
| Gate 4 (`cargo deny`) | Workspace-wide already; the bridge adds zero new external deps so no `deny.toml` change is required. |
| Gate 1 (`cargo test --workspace`) | The new `tests/cinder_to_pulse.rs` is auto-discovered via the new `[[test]]` block in `crates/self-observe/Cargo.toml` (wave-decisions.md A2). The workflow invocation `cargo test --workspace --all-targets --locked` (line 182) needs no edit. |
| Gate 2 (`cargo public-api`) | `self-observe` not graduated in this feature (wave-decisions.md A1). ADR-0038 §1 is the audit-trail. |
| Gate 3 (`cargo semver-checks`) | Same as Gate 2. |
| Existing Gate 5 jobs (harness, aperture, spark, sieve, codex) | Independent. The new `gate-5-mutants-self-observe` runs in parallel; existing per-package jobs are unaffected. |
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
gate. This means a determined committer CAN merge a bridge change
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
| Is the existing 5-gate workflow sufficient? | **Yes.** Gate 1 catches OK1/OK2/OK3/OK4 via the acceptance tests. Gate 4 is a workspace-level no-op for this feature (zero new external deps). Gates 2/3 are NOT graduated for self-observe in this feature (A1) — ADR-0038 §1 is the lock. |
| Which gate enforces each KPI? | Gate 1 enforces all four KPIs (OK1/OK2/OK3/OK4). Gate 5 (new `gate-5-mutants-self-observe` job, per A3) is the supplemental test-quality probe. |
| Workflow file path | `.github/workflows/ci.yml` (single CI workflow file; existing) |
| New workflow files | NONE (per brief constraint) |
| Modifications to existing workflow | ADD one parallel Gate 5 job (`gate-5-mutants-self-observe`), pasted by DISTILL into the existing file alongside the source-file commit. Zero other workflow YAML edit. |
| Modifications to pre-commit hook | NONE |
| Modifications to pre-push hook | NONE |
| New CI dependencies | NONE (cargo-mutants already installed for existing Gate 5 jobs) |
