# Wave Decisions — store-fsync-durability-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-04
- **Mode**: Autonomous overnight run. SLIM wave (durability hardening of
  existing crates; no new crate, no deploy surface).
- **Inputs read**: `design/wave-decisions.md`, `design/upstream-changes.md`,
  `docs/product/architecture/adr-0060-earned-trust-store-fsync-durability.md`,
  `.github/workflows/ci.yml`, `scripts/hooks/pre-commit`,
  `scripts/hooks/pre-push`, `CLAUDE.md`.

## Headline

**Existing ADR-0005 gates already cover this feature; no new CI job is
required.** This feature creates NO new crate — it broadens the existing
`crates/wal-recovery` leaf crate (adds the `FsyncBackend` family, moved
from pulse, plus `atomic_write_snapshot`) and edits seven existing stores
plus the gateway composition root. Every crate it touches already owns a
path-filtered `gate-5-mutants-<crate>` job that mutates exactly its changed
lines automatically. **The only DEVOPS-relevant point is ensuring the
out-of-process SIGKILL snapshot-atomicity proving test is deterministic and
runs in BOTH the local pre-commit hook and CI.** Everything else is a
no-op confirmation.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins`. CI is feedback, not a merge gate (project memory). This
wave wires nothing into a branch-protection contract; it confirms the
existing feedback signal covers the change and pins the one environment-
realism concern.

## Key Decisions

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| A1 | New gate-5 job? | **None required** | No new crate. The seam is extracted into the EXISTING `wal-recovery` crate, which already has `gate-5-mutants-wal-recovery` (ci.yml:1900) running `--in-diff` path-filtered on `crates/wal-recovery/**`. It auto-mutates the new `FsyncBackend`/`atomic_write_snapshot` code. Each of the seven stores already owns its own `--in-diff` job that mutates its changed call-site. See CI Delta below. |
| A2 | Snapshot-atomicity proving test (mechanism (a)) | **Deterministic invariant, runs local + CI** | Real out-of-process child `SIGKILL`ed mid-snapshot, parent reopens; assert acked record present AND store opens cleanly. Boolean presence/absence + open-succeeds, NEVER a wall-clock p95 (C6; avoids the overnight p95-flake class). Runs in `cargo test --workspace` — the local pre-commit hook (pre-commit:92) AND CI Gate 1 (ci.yml:184). See Proving-Test Verdict. |
| A3 | WAL-fsync proving test (mechanism (b)) | **In-process, no special environment** | The `LyingFsyncBackend` probe through `open_with_fsync_backend` discards the unsynced bytes a power cut would; deterministic, in-process, plain `cargo test`. No host privilege, no `drop_caches`, no VM. |
| A4 | Environments | **clean + ci only** (no deploy) | Library/storage change. No service deployment surface introduced. `environments.yaml` records local-I/O + child-process realism and the hook-coexistence requirement. |
| A5 | KPI dashboard | **None** | Per ADR-0060 External-integration handoff: no new metric, no new dashboard; refusal rides the existing `event=health.startup.refused` tracing stream. The KPIs (K1..K5) are contract-shaped outcomes (passing proving tests, 100% kill rate, byte-identical public API), consistent with the Kaleidoscope no-live-observability-stack posture at v0. |

## CI Delta — which existing gates auto-cover the change

This feature touches eight crates (the seven stores plus the broadened
`wal-recovery` leaf) and the gateway composition root. Each already owns a
`gate-5-mutants-<crate>` job in `.github/workflows/ci.yml` that runs
`cargo mutants --package <crate> --in-diff "$DIFF_FILE"` against a
`git diff "$BASELINE" HEAD -- 'crates/<crate>/**'` (baseline cascade
`origin/main` → `HEAD~1` → full). The `--in-diff` filter means each job
mutates ONLY the lines this feature changes — no per-feature wiring needed.

| Touched crate / path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|----------------------|------------------------|---------------------|-------------|----------|
| `crates/wal-recovery` | gains `FsyncBackend` family (moved from pulse) + `atomic_write_snapshot` | `gate-5-mutants-wal-recovery` | 1900 | ✓ `--in-diff` on `crates/wal-recovery/**` |
| `crates/lumen` | `sync_all` on append + `atomic_write_snapshot`; `open_with_fsync_backend` | `gate-5-mutants-lumen` | 1212 | ✓ `--in-diff` on `crates/lumen/**` |
| `crates/ray` | same | `gate-5-mutants-ray` | 1469 | ✓ `--in-diff` on `crates/ray/**` |
| `crates/strata` | same | `gate-5-mutants-strata` | 1552 | ✓ `--in-diff` on `crates/strata/**` |
| `crates/cinder` | same | `gate-5-mutants-cinder` | 2249 | ✓ `--in-diff` on `crates/cinder/**` |
| `crates/sluice` | same (fallible `apply_record`) | `gate-5-mutants-sluice` | 2584 | ✓ `--in-diff` on `crates/sluice/**` |
| `crates/beacon` (`src/state_store.rs`) | same | `gate-5-mutants-beacon` | 1637 | ✓ `--in-diff` on `crates/beacon/**` (covers `state_store.rs`) |
| `crates/pulse` | re-export shim + snapshot-only `atomic_write_snapshot` | `gate-5-mutants-pulse` | 1386 | ✓ `--in-diff` on `crates/pulse/**` |
| `crates/kaleidoscope-gateway` (`src/composition.rs`) | `probe_or_refuse` extended to per-store roots | `gate-5-mutants-kaleidoscope-gateway` | 2418 | ✓ `--in-diff` on `crates/kaleidoscope-gateway/**` |

**No touched crate lacks a gate-5 job.** The `cinder`, `sluice`,
`beacon-server`, and `kaleidoscope-gateway` jobs were the residual gap that
`gate-5-mutants-batch-v0` closed; the workspace now has uniform 25/25 Gate 5
coverage, so this feature inherits gating for free on every crate it edits.

A note on the `pulse` re-export shim and Gate 2 (`cargo public-api`): the
`FsyncBackend` family moving from pulse into wal-recovery is invisible to
pulse's public surface because pulse re-exports it (`pub use wal_recovery::…`).
Gate 2 is CI-only (pre-push locally) and pulse is among the graduated
packages; the re-export keeps the surface byte-identical (C1, K5). The
`open_with_fsync_backend` constructors are inherent methods, not trait
members, so the store trait surfaces stay byte-identical. **No Gate 2
delta is expected**; if Gate 2 reports a change, that is a real DELIVER
defect (a leaked or dropped symbol), not noise.

## Proving-Test Verdict (the one real DEVOPS concern)

### Mechanism (a) — snapshot atomicity, out-of-process SIGKILL

| Property | Verdict |
|----------|---------|
| Crash mechanism | A **separate child PROCESS** (`std::process::Command` spawning a real cargo-built helper, or `std::process` of the test binary), `SIGKILL`ed mid-snapshot; parent reopens. NOT `fork()`-in-tokio (UB; ADR-0049 §3; D7/C5). |
| Determinism | **Deterministic invariant**: assert the acked record is PRESENT after reopen AND the store `open()`s cleanly (no torn file blocks parse). Boolean presence/absence + open-succeeds. **NEVER a wall-clock p95.** This dodges the overnight p95-flake class (`p95_wallclock_flakes_overnight` in project memory) by construction (C6). |
| Local hook | **Yes.** `scripts/hooks/pre-commit:92` runs `cargo test --workspace --all-targets --locked`; the proving test is a workspace integration test, so it runs in the local pre-commit hook. |
| CI | **Yes.** `gate-1-test` (ci.yml:184) runs the identical `cargo test --workspace --all-targets --locked` on `ubuntu-latest`. Same invocation, same scope. |
| Platform nuance — DELIVER concern | `SIGKILL` semantics: on Linux (CI `ubuntu-latest`) and macOS (Andrea's local) signal 9 is uncatchable/unblockable and terminates immediately, so the kill is faithful on both. The test MUST spawn a real OS process (a `[[bin]]`/`[[test]]` helper or `std::process::Command`), NOT a tokio task or `fork`. If a small helper binary is needed, it is a **DELIVER concern** (crafter adds the `[[bin]]`/test target); flag it so the helper builds under `--all-targets` in both the hook and Gate 1. The child must write to a **tmp dir** (`tempfile`/`TempDir`), not a fixed path, so concurrent test runs and the two environments do not collide. |
| Determinism risk to watch in DELIVER | The kill must land WHILE the snapshot temp is being written, before the atomic `rename`. A naive "spawn then immediately kill" can race to either side non-deterministically. The deterministic shape is: have the child signal readiness (e.g. write a sentinel / block on a known point) so the parent kills at a controlled moment, OR assert the disjunction "canonical path holds the OLD whole snapshot OR the NEW whole snapshot, never a torn one" — which holds regardless of WHEN the kill lands and is therefore timing-independent. The latter (a crash-at-any-point invariant) is the robust, flake-free framing and matches ADR-0060 §2's "recoverable state at ANY point". Flag to crafter: prefer the any-point invariant over a timed kill. |

### Mechanism (b) — WAL fsync, in-suite lying substrate

| Property | Verdict |
|----------|---------|
| Mechanism | In-process `LyingFsyncBackend` (`no_op`/`truncating`) injected via `open_with_fsync_backend`; discards exactly the unsynced bytes a power cut would. |
| Determinism | Fully deterministic, in-process; presence/absence assertion, no timing. |
| Environment | Plain `cargo test`; no special host, no privilege, no `drop_caches`. Runs in the local hook and CI Gate 1 identically. |

**Overall verdict: PASS.** Both mechanisms are deterministic invariants
(never wall-clock thresholds) and both run under `cargo test --workspace`
in the local pre-commit hook AND CI Gate 1. The single DELIVER-side caveat
is the helper-binary / any-point-invariant shape for mechanism (a),
flagged above for the crafter.

## Production Readiness (scoped to a library/storage change)

No service deploy, no rollout, no rollback-of-traffic. The applicable
production-readiness items:

- [x] Acceptance tests defined and run in both environments (the two
      proving mechanisms; DISTILL authors them, DELIVER turns them green).
- [x] Mutation gate (Gate 5, 100% kill) auto-covers every touched crate
      via existing `--in-diff` jobs (CI Delta above).
- [x] Public-surface lock (Gate 2) preserved by the pulse re-export and
      inherent-constructor seam (C1, K5).
- [x] No new event / metric / dashboard (ADR-0060 handoff); refusal rides
      the existing `event=health.startup.refused` tracing stream.
- [x] Rollback posture: this is a behaviour-preserving durability hardening
      under trunk-based development. "Rollback" = `git revert` of the
      feature commits; there is no deployed artefact or migration to
      reverse. The atomic-snapshot change is forward-and-backward
      compatible on-disk (no WAL format change, C8; a `.tmp` left by an old
      crash is simply ignored on reopen), so a revert reads existing
      snapshots unchanged.
- [n/a] Canary/blue-green/rolling — no deployment surface.
- [n/a] On-call / runbook — operators run the binary; no Kaleidoscope-
      operated service. The refusal-on-lying-substrate behaviour is the
      operator-facing signal and reuses the documented ADR-0049 vocabulary.

## Self-Review

The `nw-platform-architect-reviewer` Agent is not invocable from this
subagent context. A structured self-review against the DEVOPS dimensions is
recorded in `self-review.md`. **An independent top-level
`nw-platform-architect-reviewer` run is recommended before DISTILL.**

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (A1; the existing 25
  `gate-5-mutants-*` jobs are untouched; trunk-based, no required checks).
- Does not write production code or the proving tests (crafter owns DELIVER;
  acceptance-designer owns the test specs in DISTILL).
- Does not change `CLAUDE.md` (per-feature mutation 100% already pinned).
- Does not proceed into DISTILL.
