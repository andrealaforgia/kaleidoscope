# ADR-0060 — Earned-Trust store durability: per-record fsync + atomic snapshot across all seven pillars, proven two ways

- **Status**: Accepted
- **Date**: 2026-06-04
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `store-fsync-durability-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0049 (`earned-trust-fsync-probe-v0` — the pulse fsync
  discipline this feature generalises: per-record `sync_all` on WAL append,
  parent-directory fsync around the snapshot truncate, the `FsyncBackend`
  trait + `RealFsyncBackend` + `LyingFsyncBackend` test double, the
  `fsync_probe` free function, the `event=health.startup.refused` /
  `substrate=<descriptor>` refusal vocabulary; **§8 explicitly RESERVED the
  successor scope this feature is**, and §3/alt-A RESERVED the
  out-of-process true-crash test this feature now uses; cited as the
  precedent this ADR EXTENDS, NOT modified). ADR-0059
  (`wal-torn-tail-recovery-v0` — the read-back mirror that recovers the
  intact acked prefix past a torn final WAL line; this feature produces the
  genuine torn tail it reads back, and introduced the `crates/wal-recovery`
  shared leaf crate that is the rule-of-three extraction precedent for the
  shared-helper decision below; cited, NOT modified). ADR-0040
  (`beacon-rule-state-store-seam` — the WAL + snapshot + replay recovery
  discipline whose durability the missing fsync silently violated; cited,
  NOT modified). ADR-0050 (`earned-trust-read-side-caps` — the Earned-Trust
  lineage on the read side; cited as lineage, NOT modified). ADR-0054
  (`query-http-common` — the rule-of-three shared-crate extraction precedent
  governing the helper decision; cited, NOT modified). ADR-0005 (the five CI
  gates including Gate 5 100% mutation kill on modified files and Gate 2
  `cargo public-api` byte identity).

## Context

Six file-backed storage pillars acknowledge a write as durable after only
`BufWriter::flush()`, which empties the user-space buffer into the kernel
**page cache** and does NOT put bytes on stable storage. A power loss or
kernel panic after an acknowledged write loses that write silently. The gap
was verified in code by Luna in DISCUSS (2026-06-03) and is the #1 item on
the four-quadrants implementer backlog:

| Store | WAL append today | Snapshot write today | Verified at |
|-------|------------------|----------------------|-------------|
| lumen | `wal.flush()` only | `File::create` onto canonical path | `crates/lumen/src/file_backed.rs:281`, `:160` |
| ray | `wal.flush()` only | `File::create` | `crates/ray/src/file_backed.rs:392`, `:171` |
| strata | `wal.flush()` only | `File::create` | `crates/strata/src/file_backed.rs:333`, `:170` |
| cinder | `wal.flush()` only | `File::create` | `crates/cinder/src/file_backed.rs:383`, `:207` |
| sluice | `wal.flush()` only (fallible `apply_record`) | `File::create` | `crates/sluice/src/file_backed.rs:391`, `:243` |
| beacon state_store | `wal.flush()` only | `File::create` | `crates/beacon/src/state_store.rs:334`, `:259` |
| **pulse** | **per-record `sync_all` (ADR-0049)** | **`File::create` — still NOT atomic** | `crates/pulse/src/file_backed.rs:511`, `:257` |

Two distinct defects are confirmed:

1. **WAL fsync gap** — six stores never call `sync_all`. Pulse alone was
   fixed by ADR-0049. The other six never received that discipline.

2. **Snapshot atomicity gap — present in EVERY store, pulse included** — the
   snapshot is written with `File::create` straight onto the canonical path,
   with no temp-file-plus-rename. A crash midway through a snapshot leaves a
   torn file at the path the next `open()` reads; `serde_json` fails on it
   and the whole store refuses to open. Pulse's per-file `sync_all` cannot
   save a file that is itself torn. This is **total data loss**, distinct
   from and worse than the WAL gap, and it is the one residue ADR-0049 §5
   left open even in its own pillar (it fsynced the snapshot file and the
   parent directory, but wrote onto the canonical path, so a crash before
   the `sync_all` completes still tears the live file).

3. **False-confidence root cause** — all 1194 tests reopen the store IN THE
   SAME PROCESS, exercising only a graceful restart, never page-cache loss.
   No test does a `kill -9` / power-loss mid-write then reopen. The green
   suite overstates durability the same way the README does.

This feature is the ADR-0049 §8 successor work ("extend the same
`FsyncBackend` and `fsync_probe` surface to lumen, ray, cinder, strata,
sluice, and the beacon rule-state store, each from its own composition
root"), plus the snapshot-atomicity gap ADR-0049 left open even in pulse. It
pairs with ADR-0059: this feature's per-record fsync makes the torn final
line the EXACT residue ADR-0059's recovery reads back.

ADRs in this repository are immutable (superseded, never edited). ADR-0040 /
0049 / 0050 / 0054 / 0059 are Accepted and referenced as precedents; they
are NOT modified. ADR-0060 is the next free number (the highest existing was
0059, verified by `ls docs/product/architecture/adr-*.md`).

## Decision

### 1. The two-mechanism proving split (the load-bearing decision)

**A `kill -9` / SIGKILL process-kill test CANNOT prove the WAL-fsync
half.** This is verified reasoning from the black-box verifier, not opinion,
and it shapes the entire proving strategy. The reason: `flush()` writes
bytes into the kernel **page cache**, and the page cache SURVIVES the
process dying. The kernel still owns those bytes and will write them back to
disk on its own schedule. A child process killed with SIGKILL mid-write,
then reopened by the parent on the SAME host, will STILL find the acked
record — **even on the buggy `flush()`-only code** — because the OS still
has it in cache. The distinction between `flush()` and `sync_all()` is only
observable when the unsynced data is actually DISCARDED, which a
same-host process kill does not do. **A SIGKILL test of the WAL-fsync AC
would pass on the bug and prove nothing.** Inheriting such a test into
DISTILL would be the false-confidence finding reproduced one level up.

Therefore the proving strategy is SPLIT into two semantically distinct
mechanisms, one per defect:

| # | Property to prove | Mechanism | Why this mechanism (and not the other) | KPI |
|---|-------------------|-----------|----------------------------------------|-----|
| **(a)** | **Atomic-snapshot correctness** — a crash mid-snapshot never leaves a torn file at the canonical path that the next `open()` fails to parse | **Real out-of-process process-kill mid-snapshot** (a child PROCESS is `SIGKILL`ed while writing the snapshot; the parent reopens) | A torn snapshot file is a PHYSICAL artefact that lands on disk and survives the kill. The page cache cannot hide it: `open()` either finds the partial temp file (never renamed) or the previous/new complete canonical file. The process-kill is the right shape here — and it is the test ADR-0049 §3/alt-A RESERVED. | K3 |
| **(b)** | **WAL-fsync correctness** — an acked write is on stable storage, not merely in the page cache | **In-suite lying-substrate probe** (the `FsyncBackend` seam injects a substrate that DROPS unsynced writes / observably distinguishes `sync_all` from `flush`) | The `flush()` vs `sync_all()` distinction is ONLY observable when unsynced data is discarded. A lying substrate (the `LyingFsyncBackend` `no_op`/`truncating` modes proven in pulse under ADR-0049) discards exactly the unsynced bytes a power cut would. This is deterministic, in-process, and reuses the mechanism ADR-0049 already built — no new seam. | K2, K4 |

**This split is mandatory and explicit.** The DISCUSS acceptance criteria
(US-01..US-07) conflate the two: every per-store AC asserts WAL-durability
("the acked write survives `SIGKILL`") via the same process-kill mechanism
as snapshot atomicity. Per (a)/(b) above, the process-kill proves snapshot
atomicity but proves NOTHING about WAL fsync. An `upstream-changes.md` note
(authored alongside this ADR) asks the product owner to SEPARATE the two ACs
per store into a **snapshot-atomicity AC** (proven by process-kill
mid-snapshot) and a **wal-fsync AC** (proven by the lying-substrate probe).
DISTILL MUST NOT inherit a single SIGKILL test claiming to prove both; the
"For Acceptance Designer" note in the brief names, per AC, which mechanism
to use.

A note on what the process-kill DOES still prove for the WAL path: it proves
the read-back tolerates the torn tail (ADR-0059 path) and that the
acked-prefix-present invariant holds on a graceful-cache-writeback restart.
That is a real regression guard and is kept — but it is labelled as a
**recovery/read-back** assertion, NOT a fsync-durability assertion. The
fsync-durability claim rests solely on mechanism (b).

### 2. Atomic-snapshot procedure (precise, POSIX-correct)

Every store's `snapshot()` replaces `File::create(canonical_path)` with the
following sequence, which makes the snapshot **whole-or-absent, never torn**
at the canonical path (constraint C3):

1. Serialise the snapshot to a **temp file in the SAME directory** as the
   canonical path (same directory so the subsequent `rename` is atomic on
   POSIX — `rename(2)` is atomic only within a filesystem; a temp in `/tmp`
   could cross a mount boundary and degrade to copy+unlink). Temp name is
   `{canonical}.tmp` (fixed; overwritten each snapshot, no accumulation).
2. `flush()` the `BufWriter`, then **`fsync_backend.fsync_file(tmp)`** so the
   temp file's bytes are on stable storage BEFORE it is renamed into place.
3. **`rename(tmp, canonical)`** — atomic on POSIX; the canonical path points
   at the old OR the new whole file at every instant, never a torn one.
4. **`fsync_backend.fsync_dir(parent)`** so the directory entry now pointing
   at the new file is durable (rename durability — a rename is not crash-safe
   until the parent directory is fsynced).
5. Then truncate-and-recreate the WAL (the existing post-snapshot step) and
   `fsync_dir(parent)` again so the WAL recreate is durable (this second
   dir-fsync is the discipline ADR-0049 §5 already pinned for pulse; it is
   carried unchanged).

A crash at ANY point leaves a recoverable state: before step 3, the
canonical path is the previous complete snapshot and the temp is ignored on
reopen (reopen reads only the canonical path; a stray `.tmp` is never read);
at step 3 the rename is atomic; after step 4 the new snapshot is durable.
This closes the gap ADR-0049 §5 left open even in pulse: ADR-0049 fsynced
the snapshot file and parent but wrote onto the canonical path; this ADR
writes to a temp and renames, so a crash before the file's own `sync_all`
completes tears only the temp, never the live canonical file.

**Where it lives**: in the shared durability helper (Decision 4), as
`atomic_write_snapshot(...)`, so the tmp+fsync+rename+fsync-dir sequence is
written ONCE and cannot drift across seven stores.

### 3. WAL fsync procedure + the probe seam (precise)

Each un-hardened store's `append_wal` gains `sync_all` after the buffered
flush and before returning success, exactly as pulse's `append_wal` already
does (`crates/pulse/src/file_backed.rs:499-513`):

```text
wal.write_all(line.as_bytes())?;   // record bytes
wal.write_all(b"\n")?;             // newline (the ADR-0059 torn-tail boundary)
wal.flush()?;                      // user-space buffer -> kernel page cache
fsync_backend.fsync_file(wal.get_ref())?;  // kernel page cache -> stable storage
```

`sync_all` (not `sync_data`) per ADR-0049 §4: the WAL grows append-only,
recovery reads its length, the file-size metadata is part of the durability
promise. Per-record (not batched) per ADR-0049 §4: correctness over capacity
at v0; batched fsync is a documented successor under its own ADR.

**The seam that makes the wal-fsync AC testable in-suite (mechanism (b))**:
each store gains an `open_with_fsync_backend(base_path, recorder,
fsync_backend: Arc<dyn FsyncBackend + Send + Sync>)` constructor mirroring
pulse's existing one (`crates/pulse/src/file_backed.rs:124`). The public
`open(...)` delegates to it with `Arc::new(RealFsyncBackend)`, preserving
the C1 trait-signature byte-identity (the constructor is an inherent method
on the concrete adapter, NOT part of the `LogStore`/`TraceStore`/… trait).
The acceptance suite injects a `LyingFsyncBackend` (or a discarding backend)
through the explicit constructor: it acks a write, the lying substrate
DISCARDS the unsynced bytes (`no_op`/`truncating` mode), the store reopens,
and the acked write is observably ABSENT on the buggy `flush()`-only path
and PRESENT once `sync_all` is wired. This is the only mechanism that
distinguishes `flush` from `sync_all`, and it is deterministic and
in-process (no host-level power-cut needed).

The fsync-honesty **startup probe** (the `fsync_probe` free function) is
wired per store at its composition root, against THAT store's pillar root,
so each store refuses to start on a lying substrate and emits
`event=health.startup.refused` with `substrate=<descriptor>` (constraint C4;
K4). Today the gateway probes only the pulse pillar root
(`crates/kaleidoscope-gateway/src/composition.rs:102`); this feature extends
`probe_or_refuse` to run the fsync probe against each opened store's root
(lumen, ray, … as each lands), reusing the same `fsync_probe` function and
refusal vocabulary verbatim — no new event, no new dashboard.

### 4. Shared durability helper, not seven copies (the Reuse decision)

The fsync-and-atomic-snapshot logic is IDENTICAL across all seven stores.
Per the MANDATORY Reuse Analysis (below, and in the feature
`wave-decisions.md`), the `FsyncBackend` seam and the atomic-snapshot
procedure are **EXTRACTED into the existing shared leaf crate
`crates/wal-recovery`** (renaming its public role from "WAL recovery" to
"WAL + snapshot durability"; the crate node is reused, not multiplied),
rather than (a) copy-pasted into seven `append_wal`/`snapshot` paths, or (b)
consumed from `pulse`.

The decisive evidence is an **architectural layering fact**: the
`FsyncBackend` family currently lives in `crates/pulse` (the metrics
pillar). Six of the seven consumers are OTHER pillars (lumen=logs, ray=
traces, …). Routing `lumen -> pulse` to reuse `FsyncBackend` would make the
**logs pillar depend on the metrics pillar** — a sibling-pillar dependency
and a layering inversion with no domain justification. Crucially, **`lumen`
already depends on `crates/wal-recovery`** (`crates/lumen/Cargo.toml:25`) and
does NOT depend on pulse; so does every other pillar (ADR-0059). The
durability seam belongs in the SAME leaf crate the pillars already depend on
INWARD, exactly as ADR-0059 placed the recovery seam there and ADR-0054
placed the read-tier seam in `query-http-common`. This is the rule of three
satisfied seven times over.

The DESIGN-pinned seam (crafter owns exact signatures in DELIVER):

```text
// crate: wal-recovery  (leaf; gains the FsyncBackend family + atomic snapshot)
//   - already depended on INWARD by all six pillars (ADR-0059)
//   - depends only on serde_json + tracing + std (+ now the fsync seam, std-only)

pub trait FsyncBackend {
    fn fsync_file(&self, file: &File) -> io::Result<()>;   // sync_all on POSIX
    fn fsync_dir(&self, dir: &Path) -> io::Result<()>;     // parent-dir fsync (rename durability)
}
pub struct RealFsyncBackend;                 // delegates to File::sync_all
pub struct LyingFsyncBackend { /* no_op | truncating | byte_flipping */ }
pub enum FsyncProbeError { FsyncIgnored, BytesLost, BytesMismatch, Io(io::Error) }
pub fn fsync_probe(root: &Path, backend: &dyn FsyncBackend) -> Result<(), FsyncProbeError>;

/// Atomic snapshot: write `bytes` to {canonical}.tmp in the same dir,
/// fsync the tmp, rename onto `canonical`, fsync the parent dir.
/// Whole-or-absent at `canonical` across a crash at any point.
pub fn atomic_write_snapshot(
    canonical: &Path,
    backend: &dyn FsyncBackend,
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()>;
```

**Migration is mechanical and behaviour-preserving for pulse.** The
`FsyncBackend` family moves from `crates/pulse/src/fsync_probe.rs` into
`crates/wal-recovery`; pulse re-exports it from its own `lib.rs`
(`pub use wal_recovery::{FsyncBackend, RealFsyncBackend, ...}`) so pulse's
public surface and the gateway's `pulse::{fsync_probe, ...}` imports stay
byte-identical (C1). The gateway's import path is preserved by the re-export;
no downstream edit beyond the crate the symbols resolve to. This re-export
shim is the only pulse change in the helper-extraction step, and it is a
pure move (the byte-flipping/truncating/no_op modes and the
`substrate_descriptor` mapping are carried verbatim).

### 5. Per-store rollout order (lumen walking skeleton first)

The rollout is one store per slice, ordered by operator-visible blast radius
and read-path observability, with the **riskiest assumption first**:

| Order | Slice | Store | WAL fsync | Atomic snapshot | Proving mechanisms |
|-------|-------|-------|-----------|-----------------|---------------------|
| 1 (WS) | US-01 | **lumen** | add `sync_all` per record (via seam) | add `atomic_write_snapshot` | (a) process-kill mid-snapshot → reopen + `GET /api/v1/logs`; (b) lying-substrate probe on `append_wal` |
| 2 | US-02 | ray | add `sync_all` per record | add `atomic_write_snapshot` | (a) process-kill mid-snapshot → reopen + `GET /api/v1/traces`; (b) lying-substrate probe |
| 3 | US-03 | strata | add `sync_all` per record | add `atomic_write_snapshot` | (a) process-kill mid-snapshot → reopen; (b) lying-substrate probe |
| 4 | US-04 | cinder | add `sync_all` per record | add `atomic_write_snapshot` | (a) process-kill mid-snapshot → reopen; (b) lying-substrate probe |
| 5 | US-05 | sluice | add `sync_all` per record | add `atomic_write_snapshot` | (a) process-kill mid-snapshot → reopen; (b) lying-substrate probe |
| 6 | US-06 | beacon state_store | add `sync_all` per record | add `atomic_write_snapshot` | (a) process-kill mid-snapshot → reopen; (b) lying-substrate probe |
| 7 | US-07 | pulse | **already done (ADR-0049)** | add `atomic_write_snapshot` (snapshot-only) | (a) process-kill mid-snapshot → reopen + `GET /api/v1/metrics` |

lumen is the walking skeleton because it validates the fatal assumption (can
the out-of-process process-kill be made deterministic in CI without
`fork()`-in-tokio, constraint C5) end-to-end on the most observable read
path, AND it is the first to land the shared `atomic_write_snapshot` helper
that all seven then reuse. The shared helper is extracted in slice 01 (the
first store to use it), not before — no abstraction ships before a consumer
(brief guidance). US-07 (pulse) is snapshot-only: its WAL is already
crash-durable, and it has NO wal-fsync AC, so its only mechanism is (a).

### 6. Torn-tail recovery reconciliation per store (C7)

ADR-0059 migrated lumen, ray, cinder, pulse to the shared
`replay_wal_tolerating_torn_tail`. sluice and strata still carry the inline
parse-or-die loop (`crates/sluice/src/file_backed.rs:167-177` confirmed;
sluice's `apply_record` is FALLIBLE, propagated with `?`). For the four
already-migrated stores, this feature's per-record fsync simply produces the
genuine torn tail their recovery already handles — no recovery change. For
**strata (US-03) and sluice (US-05)**, DESIGN reconciles per ADR-0059 §5:
this feature's proving test asserts the acked-prefix-present outcome; the
torn-tail-recovery migration for these two is the one-line-closure follow-up
ADR-0059 §5 already scoped (and sluice needs the fallible-`apply` seam
variant ADR-0059 §5 describes). **The fsync addition on `append_wal` is
orthogonal to apply fallibility** and lands regardless. If a slice's store
is not yet on the shared recovery routine, its process-kill mid-write
variant asserts the acked-prefix outcome via that store's existing replay;
extending it to the shared routine is recorded as the ADR-0059 follow-up,
not a blocker. This feature does NOT widen the torn-tail tolerance (G1) and
does NOT change the WAL format (C8).

## Alternatives considered

### Shared-helper A (rejected): copy-paste the seam into seven stores

For: no crate change; each store self-contained; matches ADR-0049's
slice-01 per-pillar `FsyncBackend` choice. Against: seven identical
`append_wal` fsync additions and seven identical tmp+rename+fsync-dir
snapshot sequences are exactly the copy-paste divergence ADR-0054 was
extracted to end and ADR-0040 case B warned a recovery copy-paste produces.
The tmp+rename+fsync-dir ordering is subtle (same-dir temp for atomic
rename; fsync the dir AFTER rename) and a drifted copy silently reintroduces
the torn-snapshot bug on one store. Mutation signal would be split across
seven suites. ADR-0049 kept its trait per-pillar because only ONE pillar was
in scope and the shape was unproven; here SEVEN are in scope and the shape
is fully proven (the residue is identical in all seven). Rejected; the rule
of three (satisfied sevenfold) mandates the share.

### Shared-helper B (rejected): consume `FsyncBackend` from `crates/pulse`

For: zero code movement; the family already lives and is tested in pulse.
Against: it makes the logs/traces/profiles/tiering/queue/rule-state pillars
depend on the METRICS pillar — a sibling-pillar dependency and a layering
inversion with no domain justification. lumen already depends on
`crates/wal-recovery` and NOT on pulse; pointing six pillars at pulse would
invert the dependency graph the workspace deliberately keeps acyclic and
leaf-ward. Rejected. The seam belongs in the leaf crate the pillars already
depend on inward.

### Shared-helper C (rejected): a brand-new `store-durability` crate

For: a clean name for the fsync + snapshot seam, separate from WAL recovery.
Against: it is a THIRD leaf crate the same six pillars must depend on,
alongside `wal-recovery`, when the two seams are the two halves of the same
crash-durability story (fsync the write; tolerate the torn read-back) and
ship to the same consumers. ADR-0059's `wal-recovery` is the natural home;
broadening its charter from "WAL recovery" to "WAL + snapshot durability" is
cheaper than a new node and keeps the durability seam in one place. Rejected
in favour of broadening `wal-recovery` (Decision 4).

### Proving A (rejected): a single SIGKILL test proves both halves

For: one mechanism, one test harness per store, matches the DISCUSS AC
wording. Against: **it is wrong** — a same-host process kill leaves unsynced
bytes in the page cache, which the OS writes back, so the test passes on the
`flush()`-only bug and proves nothing about fsync (Decision 1). Accepting it
would reproduce the false-confidence finding one level up and let DISTILL
inherit a test that cannot fail on the very defect it claims to guard.
Rejected hard; this is the load-bearing correction of the whole feature.

### Proving B (rejected): a true power-cut on real hardware / a VM with disk-cache-drop

For: the most faithful reproduction of a power loss; discards the page cache
for real. Against: not reproducible deterministically in CI without
privileged VM/`echo 3 > drop_caches`/blkdiscard orchestration that flakes,
needs root, and is platform-fragmented; constraint C6 demands a
deterministic invariant, not a host-dependent one. The lying-substrate probe
discards exactly the unsynced bytes a power cut would, deterministically and
in-process, and is the mechanism ADR-0049 already built. Rejected for the
in-suite wal-fsync AC; the lying substrate is the portable, deterministic
equivalent.

### Proving C (rejected): `fork()` + SIGKILL inside the test's tokio runtime

For: a true in-process crash. Against: `fork(2)` inside a tokio runtime is
unsafe (worker threads, mutexes, file handles, and the reactor are
duplicated into the child with undefined behaviour); ADR-0049 §3 rejected it
and RESERVED a SEPARATE child PROCESS instead. Constraint C5 pins this.
Rejected; the proving test for mechanism (a) is a separate child PROCESS
(`std::process::Command` or a small test-only binary), not a `fork()`.

### fsync strategy (rejected): `sync_data` / batched fsync

Same rejection as ADR-0049 §4: `sync_data` skips the file-size metadata the
append-only WAL recovery depends on; batched fsync trades correctness for
throughput. Per-record `sync_all` at v0; batched fsync is a documented
successor under its own ADR. Rejected for this feature.

## Consequences

### Positive

- **The durability promise becomes true AND provable across all seven
  stores**. K1 moves 0/7 → 7/7 stores with a passing proving test; K2/K3 hit
  100%; K4 extends the lying-substrate refusal from 1/7 (pulse) to 7/7.
- **The proving split is honest**. The wal-fsync claim rests on the only
  mechanism that can falsify it (a lying substrate), not on a SIGKILL test
  that passes on the bug. The snapshot-atomicity claim rests on a real
  process-kill that lands a torn file on disk. Each AC is proven by the
  mechanism that can actually fail it.
- **One durability seam, one mutation site**. The fsync calls, the rename,
  and the parent-dir fsync live once in `crates/wal-recovery`; the
  tmp+rename+fsync-dir ordering cannot drift across seven stores; a single
  `cargo mutants` run kills the snapshot-atomicity mutants meaningfully (G2).
- **No trait change, no blast radius**. `LogStore`, `TraceStore`,
  `TieringStore`, `MetricStore`, beacon `RuleStateStore` signatures stay
  byte-identical; the fsync backend is injected through an inherent
  `open_with_fsync_backend` constructor, not the trait (C1; Gate 2; K5).
- **No new event, no new dashboard**. Refusal reuses
  `event=health.startup.refused`; torn-tail recovery reuses
  `event=wal.recovery.torn_tail_dropped`. The substrate descriptors are
  carried verbatim from ADR-0049.
- **pulse's snapshot gap closes**. The one residue ADR-0049 §5 left open in
  its own pillar (non-atomic snapshot write onto the canonical path) is
  closed by the shared `atomic_write_snapshot`.

### Negative

- **Per-record `sync_all` is a real throughput cost** on six more stores. At
  v0/v1 acceptable (correctness over capacity, no production load); batched
  fsync is a documented successor (ADR-0049 §4 alt B).
- **`FsyncBackend` moves crates**. The family moves from `crates/pulse` to
  `crates/wal-recovery`; pulse re-exports it so the gateway's import path is
  unchanged, but the move is a real (mechanical, behaviour-preserving) edit
  to pulse and the gateway's resolved symbol source. Bounded; the lie modes
  and descriptor mapping are carried verbatim and remain mutation-covered.
- **strata and sluice stay on parse-or-die recovery** until the ADR-0059 §5
  follow-up. Their fsync + atomic-snapshot lands now; their torn-tail
  recovery migration (sluice needs the fallible-`apply` seam) is the tracked
  follow-up, not a blocker. The proving test asserts the acked-prefix
  outcome via their existing replay.
- **The wal-fsync AC needs the lying-substrate seam per store**. Each store
  gains an `open_with_fsync_backend` constructor (one inherent method). This
  is the same shape pulse already carries; the cost is one constructor per
  store, justified because it is the only seam that makes the wal-fsync AC
  testable in-suite.

### Trade-off summary

The feature is intentionally narrow: add the missing `sync_all` to six WAL
append paths, make seven snapshots atomic via one shared helper, and prove
each defect with the mechanism that can actually falsify it — a real
process-kill for snapshot atomicity, a lying substrate for WAL fsync. The
trade-off is "one SIGKILL test for both (simpler, but proves nothing about
fsync)" against "two mechanisms (honest, each falsifiable)". v0 takes the
latter, which is the entire point of a durability-hardening feature.

## Verification

- **The two-mechanism split is the headline verification**: (a) a real
  out-of-process process-kill mid-snapshot proves snapshot atomicity (K3);
  (b) an in-suite lying-substrate probe proves WAL fsync (K2/K4). A SIGKILL
  test is NEVER the sole proof of a wal-fsync AC.
- **Earned-Trust enforcement (three orthogonal layers, per the methodology
  and the ADR-0049 / 0059 precedent)**: (a) **subtype/type check** — each
  store consumes the `FsyncBackend` port through its `open_with_fsync_backend`
  constructor; `RealFsyncBackend` satisfies it by `impl FsyncBackend`;
  removing the impl fails the build (`cargo check` is the `mypy`-equivalent).
  (b) **structural check** — an AST pre-commit check asserts each in-scope
  store's `append_wal` calls `fsync_backend.fsync_file(...)` after the flush,
  its `snapshot` calls `wal_recovery::atomic_write_snapshot` (and retains NO
  inline `File::create(canonical)` snapshot write), and its composition root
  calls `fsync_probe` against that store's root BEFORE the listener binds.
  `import-linter` was investigated and rejected (its contracts are
  import-graph only, no API for method/call-presence enforcement on classes);
  the AST hook covers the structural layer. (c) **behavioural gold-test** —
  the lying-substrate proving test per store exercises the catalogued
  substrate lie (the `no_op`/`truncating` discard) and asserts the acked
  write is absent on `flush()`-only and present once `sync_all` is wired,
  plus the composition-root `event=health.startup.refused` emission on a
  lying substrate. A single-layer bypass is caught by at least one of the
  other two.
- **Self-application of Earned-Trust**: the lying-substrate gold-test is the
  probe that verifies each store actually fsyncs (not merely claims to); the
  AST structural layer is the probe that verifies each store actually calls
  the shared atomic-snapshot helper and the per-store fsync probe (not merely
  that they exist). Asking "what happens when the substrate lies?" is
  answered per store by mechanism (b).
- **Mutation testing**: `cargo mutants --in-diff` at the 100% kill-rate gate
  (ADR-0005 Gate 5; CLAUDE.md). Primary targets: the per-record `sync_all` on
  each `append_wal` (the call must not be deletable without a surviving
  test); the `rename` and the parent-dir fsync in `atomic_write_snapshot`
  (each non-deletable); the temp-then-rename ordering. Covered by each store's
  existing `gate-5-mutants-*` job plus the `gate-5-mutants-wal-recovery`
  expectation (DEVOPS wires the CI; the expectation is annotated).
- **Gate 2 (`cargo public-api`)** confirms `LogStore`, `TraceStore`,
  `TieringStore`, `MetricStore`, beacon `RuleStateStore` signatures are
  byte-identical to the prior tag (C1; K5). The `open_with_fsync_backend`
  constructor is an inherent method on the concrete adapter, not a trait
  member, so the trait surface is unchanged.

## External-integration handoff

None. Every store reads and writes the in-process filesystem under
`pillar_root`, not a network service. No consumer-driven contract test
recommendation. The refusal and torn-tail-recovery events ride the existing
structured `tracing` stream (`read-api-tracing-subscriber-v0`); no new
metric, no new dashboard.
