# Wave Decisions — store-fsync-durability-v0 (DESIGN)

- **Wave**: DESIGN (nWave)
- **Architect**: Morgan (`nw-solution-architect`)
- **Date**: 2026-06-04
- **Mode**: Autonomous overnight run, **propose mode**. All decisions made
  by Morgan and recorded here.
- **Scope**: Application/components — storage durability hardening across
  seven existing crates (lumen, ray, strata, cinder, sluice, beacon
  state_store, pulse).
- **ADR**: ADR-0060 (`docs/product/architecture/adr-0060-earned-trust-store-fsync-durability.md`).

## Multi-architect context

`docs/product/architecture/brief.md` has no `## System Architecture`
(Titan) or `## Domain Model` (Hera) sections for this feature — both are
absent by prior decision (platform architecture lives in
`docs/architecture/kaleidoscope-architecture.md`, reused as-is). Morgan's
output appends under `## Application Architecture — store-fsync-durability-v0`.
Built on prior Application Architecture sections, especially
`earned-trust-fsync-probe-v0` (ADR-0049) and `wal-torn-tail-recovery-v0`
(ADR-0059). No conflicts with prior decisions; this feature is their named
successor.

## Key Decisions

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| D1 | Proving strategy | **Two distinct mechanisms, one per defect** | A SIGKILL cannot prove WAL-fsync (page cache survives the kill). Snapshot atomicity needs a real process-kill; WAL fsync needs a lying substrate. See Two-Mechanism Split below. **Load-bearing.** |
| D2 | Atomic-snapshot procedure | tmp-in-same-dir → fsync-tmp → rename → fsync-parent-dir | POSIX-correct whole-or-absent at the canonical path (C3). Closes the gap ADR-0049 §5 left open even in pulse. |
| D3 | Shared durability helper | **EXTRACT into the existing `crates/wal-recovery` leaf crate** (broaden its charter to "WAL + snapshot durability") | Rule of three satisfied sevenfold; all pillars already depend on `wal-recovery` INWARD; consuming from pulse would invert the dependency graph. See Reuse Analysis. |
| D4 | WAL-fsync seam | per-store `open_with_fsync_backend` inherent constructor (mirrors pulse) | Makes the wal-fsync AC testable in-suite with a `LyingFsyncBackend`, WITHOUT touching the store trait (C1). |
| D5 | Startup probe wiring | per-store `fsync_probe` at each composition root, against that store's own pillar root | Each store refuses on a lying substrate (K4: 1/7 → 7/7). Reuses `fsync_probe` + `event=health.startup.refused` verbatim (C4). |
| D6 | Rollout order | lumen (WS) → ray → strata → cinder → sluice → beacon → pulse | Riskiest-assumption-first (the deterministic out-of-process crash on the most observable read path); lumen extracts the shared helper the other six reuse. |
| D7 | Crash harness | separate child PROCESS (`std::process::Command` or test-only binary), NOT `fork()`-in-tokio | C5. `fork()` inside tokio is UB (ADR-0049 §3); ADR-0049 §3/alt-A RESERVED exactly this out-of-process escalation. |
| D8 | strata/sluice torn-tail recovery | fsync + atomic snapshot land NOW; torn-tail-recovery migration is the tracked ADR-0059 §5 follow-up | The fsync addition on `append_wal` is orthogonal to apply fallibility. sluice's fallible `apply_record` needs the ADR-0059 §5 fallible-`apply` seam variant. |
| D9 | Upstream AC correction | request product owner split each conflated crash AC into AC-snapshot-atomicity + AC-wal-fsync | `upstream-changes.md`. DISTILL is unblocked by the For-Acceptance-Designer note regardless. |

## The Two-Mechanism Proving Split (D1, ADR-0060 Decision 1)

**A `SIGKILL` / process-kill on the same host CANNOT prove the WAL-fsync
half.** `flush()` writes into the kernel **page cache**, which **survives
the process dying**: the OS still writes those bytes back. A child killed
mid-write and reopened by the parent on the same host STILL finds the acked
record — even on the buggy `flush()`-only code. The `flush` vs `sync_all`
distinction is observable ONLY when unsynced data is DISCARDED, which a
same-host process kill does not do. A SIGKILL test of a wal-fsync AC would
pass on the bug and prove nothing.

| Property | Mechanism | Why this and not the other | KPI |
|----------|-----------|----------------------------|-----|
| **(a) Atomic-snapshot correctness** | **Real out-of-process process-kill mid-snapshot**, parent reopens | A torn snapshot file is a physical on-disk artefact that survives the kill; the page cache cannot hide it. Right shape; the test ADR-0049 §3/alt-A RESERVED. | K3 |
| **(b) WAL-fsync correctness** | **In-suite lying-substrate probe** (`LyingFsyncBackend` `no_op`/`truncating` through `open_with_fsync_backend`) | The lying substrate DISCARDS exactly the unsynced bytes a power cut would; deterministic, in-process; reuses the ADR-0049 mechanism. Only this falsifies the bug. | K2, K4 |

The kept `SIGKILL`+read-API assertion is RE-LABELLED a recovery/read-back
regression guard (pairs with ADR-0059), NOT the wal-fsync proof.

## Reuse Analysis (MANDATORY)

Question: where does the fsync-and-atomic-snapshot logic live — copied into
seven stores, consumed from pulse, a new crate, or extracted into the
existing `wal-recovery` leaf crate?

| Option | Dependency shape | Drift risk | Mutation signal | Verdict |
|--------|------------------|------------|-----------------|---------|
| **A. Copy-paste into 7 stores** | none new | **HIGH** — 7 copies of subtle tmp+rename+fsync-dir ordering; one drift reintroduces the torn-snapshot bug | split across 7 suites | **REJECT** — the copy-paste divergence ADR-0054 was extracted to end, ADR-0040 case B warned of, sevenfold |
| **B. Consume `FsyncBackend` from `crates/pulse`** | `lumen→pulse`, `ray→pulse`, … (six sibling-pillar edges) | low | one site | **REJECT** — logs/traces/… pillars depending on the METRICS pillar is a layering inversion; pillars are siblings, not layered. lumen already depends on `wal-recovery`, NOT pulse. |
| **C. New `store-durability` crate** | six new `→store-durability` edges | low | one site | **REJECT** — a THIRD leaf crate the same six pillars import alongside `wal-recovery`, when fsync-the-write and tolerate-the-torn-read-back are two halves of one crash-durability story shipping to the same consumers |
| **D. EXTRACT into existing `crates/wal-recovery`** | reuses edges the pillars ALREADY have (ADR-0059) | low | **one site** | **ACCEPT** |

### Decisive evidence (architectural fact, verified in code)

- `crates/lumen/Cargo.toml:25` — lumen **already depends on
  `wal-recovery`** and does **NOT** depend on pulse. Same for every pillar
  (ADR-0059 migrated lumen/ray/cinder/pulse; sluice/strata are pending but
  will join).
- `crates/pulse/src/lib.rs:52,66` — the `FsyncBackend` family currently
  lives in pulse and is re-exported. Pointing six other pillars at pulse
  would make six sibling pillars depend on the metrics pillar.
- ADR-0059 already placed the shared recovery seam in `wal-recovery`; ADR-0054
  placed the read-tier seam in `query-http-common`. Same rule-of-three
  discipline; here satisfied SEVEN times over.

### Decision: EXTRACT (Option D)

Move the `FsyncBackend` / `RealFsyncBackend` / `LyingFsyncBackend` /
`FsyncProbeError` / `fsync_probe` family from `crates/pulse` into
`crates/wal-recovery`, and add `atomic_write_snapshot(...)` there. pulse
**re-exports** the family from its own `lib.rs` so pulse's public surface
and the gateway's `pulse::{fsync_probe, ...}` imports stay byte-identical
(C1). This is a pure, behaviour-preserving move (lie modes + descriptor
mapping carried verbatim). The seam is extracted in **slice 01 (lumen, the
first consumer)** — no abstraction ships before a consumer uses it.

## Constraints honoured (C1..C8)

- **C1** No trait signature change — fsync backend injected via inherent
  `open_with_fsync_backend`, NOT the trait. Gate 2 (`cargo public-api`)
  enforces byte-identity (K5).
- **C2** Durable = on stable storage — `sync_all` per WAL record; the
  wal-fsync AC is proven by the lying substrate that discards page-cached
  bytes, not by a process kill.
- **C3** Atomic snapshot whole-or-absent — tmp+fsync+rename+fsync-dir;
  proven by a real process-kill mid-snapshot.
- **C4** Reuse the proven seam — `FsyncBackend`, `RealFsyncBackend`,
  `LyingFsyncBackend`, `event=health.startup.refused`, `substrate=<descriptor>`
  all reused verbatim (moved crate, identical API). No new event/dashboard.
- **C5** Out-of-process crash, not `fork()`-in-tokio — D7; child PROCESS.
- **C6** Deterministic invariant, not a timing threshold — both mechanisms
  assert presence/absence + open-succeeds, never a wall-clock p95 (avoids the
  overnight p95-flake class in project memory; G3).
- **C7** Pairs with ADR-0059, does not duplicate it — D8; the per-record
  fsync produces the torn tail ADR-0059 recovers. strata/sluice recovery
  migration is the tracked ADR-0059 §5 follow-up.
- **C8** No WAL format change — no checksums, no length prefixes; NDJSON
  stays human-readable. The atomic-snapshot change is on the snapshot file,
  not the WAL framing.

## Upstream changes

One: split each per-store conflated crash AC into AC-snapshot-atomicity
(process-kill) + AC-wal-fsync (lying-substrate probe). See
`upstream-changes.md`. Requested of the product owner; DISTILL is unblocked
by the For-Acceptance-Designer note in `brief.md` regardless.

## Risk register (DESIGN deltas to DISCUSS)

| Risk | Mitigation |
|------|------------|
| DISTILL inherits a SIGKILL test that proves nothing about fsync | The two-mechanism split is explicit in ADR-0060 §1 and the For-Acceptance-Designer note; `upstream-changes.md` requests the SSOT correction. |
| `FsyncBackend` move breaks the gateway import | pulse re-exports the family; gateway's `pulse::{...}` path is preserved; Gate 2 + `cargo check` catch any drift. |
| tmp+rename crosses a mount boundary (rename degrades to copy) | The temp is created in the SAME directory as the canonical path (ADR-0060 §2 step 1), so the rename is intra-filesystem and atomic. |
| sluice fallible `apply_record` complicates recovery | fsync addition is orthogonal to apply fallibility (D8); recovery migration deferred to ADR-0059 §5 follow-up with its fallible-`apply` seam. |
| Per-record `sync_all` throughput cost on six more stores | Known; correctness over capacity at v0 (ADR-0049 §4 alt B successor). |

## What this DESIGN does NOT do

- Does not change any store trait signature (C1).
- Does not introduce batched fsync (ADR-0049 §4 alt B successor).
- Does not change the WAL on-disk format (C8).
- Does not migrate strata/sluice to the shared torn-tail recovery routine
  (ADR-0059 §5 follow-up; only the fsync + atomic snapshot land here).
- Does not write production code (crafter owns DELIVER); pins observable
  behaviour and the seams only.
- Does not proceed into DEVOPS/DISTILL.

## Peer review

Self-review against the SA critique dimensions recorded in `self-review.md`
(reviewer Agent tool unavailable from this subagent context; an independent
top-level `nw-solution-architect-reviewer` run is recommended before
DISTILL).
