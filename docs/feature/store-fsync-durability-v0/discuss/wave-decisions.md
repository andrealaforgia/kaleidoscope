# Wave Decisions — store-fsync-durability-v0 (DISCUSS)

- **Wave**: DISCUSS (nWave)
- **Analyst**: Luna (nw-product-owner)
- **Date**: 2026-06-03
- **Feature type**: Backend (storage durability / correctness hardening)
- **Mode**: Autonomous overnight run. All interactive decisions made by Luna and recorded here.

## Origin and verification

The per-module four-quadrants assessment
(`~/dev/kaleidoscope-4-quadrants-theory`) named this the single most
important real code defect; it is the #1 item on the implementer
backlog. Luna verified the defect in code on 2026-06-03 by direct
archaeology rather than assumption:

| Store | WAL append | Snapshot write | Verified at |
|-------|-----------|----------------|-------------|
| lumen | `wal.flush()` only, no `sync_all` | `File::create` onto canonical path | `crates/lumen/src/file_backed.rs:281`, `:160` |
| ray | `wal.flush()` only | `File::create` | `crates/ray/src/file_backed.rs:392`, `:171` |
| strata | `wal.flush()` only | `File::create` | `crates/strata/src/file_backed.rs:333`, `:170` |
| cinder | `wal.flush()` only | `File::create` | `crates/cinder/src/file_backed.rs:383`, `:207` |
| sluice | `wal.flush()` only (fallible `apply_record`) | `File::create` | `crates/sluice/src/file_backed.rs:391`, `:243` |
| beacon state_store | `wal.flush()` only | `File::create` | `crates/beacon/src/state_store.rs:334`, `:259` |
| **pulse** | **per-record `sync_all` (ADR-0049)** | **`File::create` — still NOT atomic** | `crates/pulse/src/file_backed.rs:511`, `:257` |

Two distinct defects confirmed:

1. **WAL fsync gap** — six stores call only `BufWriter::flush()`, which
   empties the user-space buffer into the kernel page cache and does
   NOT put bytes on stable storage. A power loss or kernel crash after
   an acknowledged write loses that write silently. Pulse alone was
   fixed by ADR-0049 (per-record `sync_all` + parent-dir fsync around
   snapshot truncate). The other six never received that discipline.

2. **Snapshot atomicity gap — present in EVERY store, pulse included**
   — the snapshot is written with `File::create` straight onto the
   canonical path (e.g. `crates/pulse/src/file_backed.rs:257`,
   `crates/lumen/src/file_backed.rs:160`), with no temp-file-plus-rename.
   A crash midway through a snapshot leaves a torn file at the path the
   next `open()` reads; `serde_json` fails on it and the whole store
   refuses to open. Pulse's per-file `sync_all` cannot save a file that
   is itself torn. This is **total data loss**, distinct from and worse
   than the WAL gap.

3. **False-confidence root cause** — every per-store restart test and
   the integration suite reopen the store IN THE SAME PROCESS, which
   exercises only a graceful restart and never page-cache loss. No test
   does a `kill -9` / power-loss mid-write then reopen. So 1194 tests
   are green and the durability gap is structurally undetectable by the
   current suite. The README and roadmap describe these stores as
   "durable / survives restart". The green suite overstates durability
   the same way the README does.

## The operator job (JTBD, Earned-Trust framing)

> "After a power loss or an OS crash, my collector restarts and still
> has every write it acknowledged as durable, and the store opens
> cleanly even if the crash hit mid-snapshot."

Today the honest answer is "only if the shutdown was graceful, and only
if no crash hit a snapshot." This feature closes the gap the
survives-a-restart promise has carried since v1. It extends ADR-0049
(pulse's fsync discipline) and pairs with ADR-0059 (torn-tail recovery
on the read-back) to make the durability promise whole.

## Lineage (prior-wave SSOT, read and grounded)

- **ADR-0049** (`earned-trust-fsync-probe-v0`) — the pulse fsync
  discipline this feature generalises: per-record `sync_all` on WAL
  append; parent-directory fsync around the snapshot truncate; the
  `FsyncBackend` trait + `RealFsyncBackend` + `LyingFsyncBackend` test
  double; the `fsync_probe` free function refusing to start on a
  substrate that lies about syncing; `event=health.startup.refused`
  with `substrate=<descriptor>`. ADR-0049 §8 EXPLICITLY names the
  successor scope: "extend the same `FsyncBackend` and `fsync_probe`
  surface to lumen, ray, cinder, strata, sluice, and the beacon
  rule-state store, each from its own composition root." **This feature
  is that successor work, plus the snapshot-atomicity gap ADR-0049 left
  open even in pulse.**
- **ADR-0059** (`wal-torn-tail-recovery-v0`) — the read-back mirror:
  recovers the intact acked prefix past a torn WAL tail. ADR-0059 §Context
  states the torn final line is "the EXPECTED post-crash shape" of a
  fsync-honest, append-only WAL. **This feature produces exactly that
  shape** (per-record fsync, append-then-newline-then-fsync), so the two
  ADRs interlock: fsync makes the prefix durable; torn-tail recovery
  reads it back. The kill-9 proving test in this feature is the first
  test in the codebase that actually generates a real torn tail (rather
  than a hand-crafted one), so it also validates ADR-0059's recovery on
  a genuine crash residue.
- **ADR-0040** (`beacon-rule-state-store-seam`) — the WAL + snapshot +
  replay recovery discipline whose durability the missing fsync silently
  violated. Cited, not modified.
- **ADR-0050** (`earned-trust-read-side-caps`) — the Earned-Trust lineage
  on the read side. Cited as lineage.

## Decisions (autonomous, per the overnight brief)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Feature type | Backend (storage durability) | No UI; the operator-visible surface is a store reopen after a crash. |
| Walking skeleton | Yes, brownfield-style | The stores exist; the skeleton hardens ONE store (lumen) end-to-end AND proves it with the kill-9 test. |
| First slice / WS pillar | **lumen** | The logs pillar; its read path (`GET /api/v1/logs`) is the easiest crash-survival outcome to observe end-to-end. Matches the brief's slicing guidance. |
| UX research | Lightweight | Operator restarting a crashed collector; one persona (Priya). |
| JTBD | Earned-Trust durability job (above) | Recorded as the single job all stories trace to. |
| Prose posture | Honest hardening/correctness framing | The whole project thesis is structural honesty against vendor overstatement; the prose says "today the honest answer is X" rather than overclaiming. |
| Snapshot atomicity scope | ALL stores **including pulse** | The snapshot gap is present in pulse too (`File::create` at `:257`); pulse's WAL slice is therefore snapshot-only. |
| Carpaccio width | One store per slice | Do not ship all six in one slice; do not ship the abstraction before a store uses it (brief guidance). |

## Risk register

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| No DIVERGE artifacts present (`docs/feature/store-fsync-durability-v0/diverge/` absent) | High (confirmed absent) | Low | Job is grounded directly in ADR-0049 §8 successor scope + verified code residue + the four-quadrants #1 ranking. JTBD recorded above; no re-run of ODI needed for a correctness-hardening feature. |
| `kill -9` proving test could be flaky under load (cf. p95 overnight flakes memory) | Medium | Medium | The proving test asserts a deterministic invariant (acked write present after reopen, store opens), not a timing threshold. It does NOT use wall-clock p95. Recorded as a guardrail: the test must be deterministic, not timing-based. |
| `fork(2)` inside tokio is unsafe (ADR-0049 §3 rejected fork+SIGKILL for the in-process probe) | High if naively done | High | The proving test is a SEPARATE child PROCESS (a small test-only binary or `std::process::Command` child) that writes-acks-then-is-killed, NOT a `fork()` inside the test's tokio runtime. The store under test does no tokio fork. This is the documented escalation ADR-0049 §3 RESERVED; it is now justified because a real crash test is the whole point of this feature. Pinned as a constraint, design choice deferred to DESIGN. |
| sluice's `apply_record` is fallible (ADR-0059 §5) unlike the other five | Medium | Low | Flagged per-slice; sluice's WAL-fsync slice notes the fallible-apply seam. The fsync addition is on the append path (`append_wal`), orthogonal to apply fallibility. |
| Snapshot atomicity interacts with ADR-0040 recovery ordering | Medium | Medium | The temp-then-rename + parent-dir fsync is the POSIX-correct ordering ADR-0049 §5 already pins for the truncate; this feature applies the SAME ordering discipline to the snapshot WRITE. DESIGN reconciles the exact sequence. |
| Per-record `sync_all` is a real throughput cost (ADR-0049 §Negative) | Known | Low at v0 | Correctness over capacity at v0/v1; batched fsync is a documented successor under its own ADR (ADR-0049 §4 alt B). Recorded, not addressed here. |

## What this feature does NOT do

- Does not change any store trait signature (`LogStore`, `TraceStore`,
  `TieringStore`, `MetricStore`, beacon `RuleStateStore`). Gate 2
  (`cargo public-api`) enforces byte-identity. Pinned as a guardrail.
- Does not introduce batched fsync (deferred to a successor ADR).
- Does not change the WAL on-disk format (no checksums, no length
  prefixes — ADR-0059 alt B/C rejected those for the same reasons).
- Does not re-run ODI / opportunity scoring (correctness-hardening
  feature with a pre-validated job from ADR-0049 §8).

## Peer review

Peer review (nw-product-owner-reviewer) run at end of DISCUSS; result
recorded in `dor-validation.md`. Handoff to DESIGN is NOT performed by
this wave (brief: "Do NOT proceed into DESIGN").
