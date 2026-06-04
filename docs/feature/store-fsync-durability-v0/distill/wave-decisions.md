# Wave Decisions — store-fsync-durability-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Agent**: Scholar (`nw-acceptance-designer`)
- **Date**: 2026-06-04
- **Mode**: Autonomous overnight run. Decide everything; never ask.

## Key decisions

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| DWD-1 | I-O strategy | **C (real local I/O + real child process)**, every scenario `@real-io` | The feature is on-disk crash durability; InMemory cannot exhibit a torn snapshot or a page-cache loss. DEVOPS `environments.yaml` pins this realism. See `io-strategy.md`. |
| DWD-2 | The two proving mechanisms | **SEPARATE scenarios per store**: (a) out-of-process SIGKILL for AC-snapshot-atomicity, (b) in-suite lying substrate for AC-wal-fsync | ADR-0060 §1 / brief / `upstream-changes.md` are binding: a SIGKILL cannot prove WAL fsync (page cache survives). A SIGKILL is NEVER the sole proof of a wal-fsync AC. |
| DWD-3 | Mechanism (a) crash shape | **Real child PROCESS** (`*-crash-target` `[[bin]]`) `SIGKILL`ed via `Child::kill`; assert the crash-at-ANY-point invariant | `fork()`-in-tokio is UB (ADR-0049 §3, C5). The any-point invariant (canonical whole-or-absent) is timing-independent → no wall-clock p95 → dodges the overnight p95-flake class (C6; DEVOPS A2). |
| DWD-4 | Mechanism (b) seam | Drive each store's public **`open_with_fsync_backend`** inherent constructor with `LyingFsyncBackend::no_op()` / `::truncating()` | The only mechanism distinguishing `flush` from `sync_all`; inherent method keeps the trait surface byte-identical (C1). |
| DWD-5 | Shared seam home | Scaffold the `FsyncBackend` family + `atomic_write_snapshot` in **`crates/wal-recovery`**; each store re-exports | Matches ADR-0060 §4 exactly (the leaf crate all pillars already depend on inward); avoids the pulse sibling-pillar inversion. Added the `wal-recovery` dep to strata/sluice/beacon (DELIVER keeps it). |
| DWD-6 | Kill-target location | A per-store **`[[bin]]` `<store>-crash-target`** in each store crate (sets `CARGO_BIN_EXE_<store>-crash-target`) | Mirrors the established `CARGO_BIN_EXE_*` + `Child::kill` pattern (`log-query-api` slice_07/08). Self-contained per crate; no extra workspace member. DEVOPS flagged the helper as a DELIVER concern — scaffolded RED-ready here. |
| DWD-7 | pulse scope | **Snapshot-only** (3 scenarios, mechanism (a) only); no wal-fsync, no new seam scaffold | pulse's WAL is already crash-durable (ADR-0049); adding a refusal scenario would duplicate `v1_slice_03_fsync_probe.rs`. |
| DWD-8 | RED posture | Every scenario `#[ignore = "RED until DELIVER: … slice NN"]`; seams are panicking `__SCAFFOLD__` bodies | Mandate 7 RED-not-BROKEN. The pre-commit hook (`cargo test --workspace --all-targets --locked`) stays GREEN; never `--no-verify`. |
| DWD-9 | Slice numbering | next free per crate: lumen `v1_slice_04`, ray `v1_slice_04`, strata `v1_slice_03`, cinder `v1_slice_04`, sluice `v1_slice_03`, beacon `v1_slice_03`, pulse `v1_slice_06_snapshot_atomicity` | Follows the existing `v1_slice_NN` convention; picked the next free number per crate's `tests/` dir. |

## Determinism guarantees (C6)

No wall-clock thresholds. Mechanism (a) is the crash-at-any-point invariant
(timing-independent); mechanism (b) is a presence/absence assertion in-process.
The child signals readiness (`CRASH_TARGET_READY`) so the parent kills at a
controlled moment. This is the flake-free framing DEVOPS pinned.

## Green-suite confirmation

`cargo test --workspace --all-targets --locked` exits 0 with all 40 new
scenarios ignored and the panicking scaffold bins never run (a `fn main` bin
has no tests). Per-suite ignore counts: lumen 7, ray 6, strata 6, cinder 6,
sluice 6, beacon 6, pulse 3 = 40. The pre-commit hook passes.

## Self-review and reviewer dispatch

The `nw-acceptance-designer-reviewer` (Sentinel) Agent is not invocable from
this subagent context. A structured self-review against critique dimensions
1–9 is recorded in `self-review.md`. **An independent top-level
`nw-acceptance-designer-reviewer` run is recommended before DELIVER.**

## What this DISTILL wave does NOT do

- Does not write production durability logic (the seams panic; DELIVER wires
  `sync_all` + `atomic_write_snapshot` Outside-In, one scenario at a time).
- Does not lift any `#[ignore]` (DELIVER's outer-loop job).
- Does not change ADR-0060, the user stories, the architecture, or the WAL
  format (C8).
- Does not add a CI job (DEVOPS A1: every touched crate already has gate-5).
- Does not proceed into DELIVER.
