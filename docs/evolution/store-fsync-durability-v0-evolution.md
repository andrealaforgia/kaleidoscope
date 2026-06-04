# Evolution archive — store-fsync-durability-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`, which established the
per-file convention: one file per feature, named `<feature-id>-evolution.md`,
with the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0060
  (`docs/product/architecture/adr-0060-earned-trust-store-fsync-durability.md`),
  extending ADR-0049 (`adr-0049-earned-trust-honour-fsync.md`), in
  particular ADR-0049 section 8.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `1653a0d` | the fsync family, with the test that proves it |
| design | `88d5a3f` | ADR-0060, two-mechanism proof, shared durability seam |
| devops | `486f510` | slim wave, existing gates cover, proving-test determinism |
| distill | `ac45f12` | 40 crash-durability scenarios, two-mechanism proof |
| distill | `9d7446d` | wal-fsync proven by counting not lying-survival (DELIVER-found correction) |
| feat | `ec82270` | lumen walking skeleton + shared FsyncBackend / atomic_write_snapshot seam in wal-recovery |
| feat | `033707f` | ray crash-durable WAL fsync + atomic snapshot |
| feat | `aa73210` | strata crash-durable WAL fsync + atomic snapshot |
| feat | `c714403` | cinder crash-durable WAL fsync + atomic snapshot |
| feat | `6d81c29` | sluice crash-durable WAL fsync + atomic snapshot |
| feat | `fb43b27` | beacon rule-state crash-durable WAL fsync + atomic snapshot |
| feat | `16ceb4b` | pulse atomic snapshot via wal-recovery (snapshot-only) |
| docs | `9631d6b` | narrative + slide closure |

## The problem, in Earned-Trust framing

This was the number one finding of the per-module four-quadrants
assessment. Two distinct durability lies were live across the durable
stores.

First, six stores (lumen, ray, strata, cinder, sluice and beacon's
`state_store`) acknowledged a write after only `flush()`. `flush()` pushes
the bytes out of the program's buffer into the operating system page
cache; it does not push them to the physical medium. An acknowledgement
made after `flush()` alone is a claim of durability the substrate has not
honoured: a power loss or kernel panic between the `flush()` and the
eventual writeback loses an acked record. The store told the caller "your
write is safe" while the bytes were still only in volatile memory.

Second, every store including pulse wrote its snapshot non-atomically:
`File::create` on the canonical snapshot path, truncating it in place and
streaming the new contents over it, with no temp-plus-rename. A crash
mid-snapshot therefore left a half-written file at the canonical path,
and the next `open` read that torn snapshot as authoritative. A crash
during snapshotting bricked the next open.

Both are claims contradicted by code, the exact class of substrate lie
the project's Earned-Trust principle exists to forbid. The feature is the
next foot of that lineage: ADR-0060 generalises pulse's ADR-0049 fsync
discipline (ADR-0049 section 8) from one store into shared infrastructure
and routes all seven durable stores through it.

The false-confidence root cause it remedied: every prior restart test
reopened the store in-process. An in-process reopen reads from the same
page cache the writes were never flushed past, so the durability gap was
invisible to a fully green suite. A passing restart test proved nothing
about disk because no disk round trip ever occurred. This feature adds
the out-of-process `kill -9` crash-durability tests that close that gap.

## The architecture decision

### Shared FsyncBackend family extracted into wal-recovery by the rule of three

pulse already carried the ADR-0049 fsync discipline. With six more stores
needing the identical mechanism, the rule of three is satisfied and then
some. Rather than replicate the fsync-honest append and the atomic
snapshot seven times, the discipline was extracted into the existing
shared leaf crate `crates/wal-recovery` (the same crate the torn-tail
feature created), at `crates/wal-recovery/src/lib.rs`. This is the same
single-mutation-site benefit ADR-0054 and the torn-tail feature relied
on: the durability logic lives at exactly one site.

The extracted surface:

- `trait FsyncBackend` with `fsync_file` and `fsync_dir`, the seam every
  store calls instead of touching `sync_all` directly.
- `RealFsyncBackend`: the production implementation, real `sync_all` on
  the file and on the parent directory.
- `LyingFsyncBackend`: a test double that lies about persistence, with
  `no_op`, `truncating` and `byte_flipping` modes. Used to drive the
  refusal acceptance criteria.
- `CountingFsyncBackend`: wraps a `RealFsyncBackend` and counts each
  `fsync_file` and `fsync_dir` call (`file_fsync_count`,
  `dir_fsync_count`), so a test can assert that fsync was actually
  invoked on the write path. This is the wired-ness assertion that a
  lying double cannot make.
- `fsync_probe`: a startup-time probe that detects a substrate that does
  not honour fsync, returning `FsyncProbeError`.
- `atomic_write_snapshot`: the snapshot primitive, temp-plus-fsync-plus-
  rename-plus-fsync-dir. It writes to a temp file, fsyncs the temp file,
  renames it onto the canonical path (atomic on the platforms in scope),
  then fsyncs the containing directory so the rename itself is durable.

All seven stores delegate to this family. pulse, which already owned the
discipline, re-exports the surface from `crates/pulse/src/fsync_probe.rs`
(`pub use wal_recovery::{fsync_probe, FsyncBackend, FsyncProbeError,
LyingFsyncBackend, RealFsyncBackend}`) so pulse's public surface stays
byte-identical after the move. The implementation relocated; the surface
did not change.

This is the Rust-idiomatic shape per CLAUDE.md: data plus free functions
plus a trait where polymorphism is genuinely needed (the FsyncBackend seam
that lets test doubles substitute for the real substrate), no inheritance,
no `dyn` where direct monomorphisation suffices.

### Two mechanisms per store

Each store now does two distinct things through the shared family:

- WAL: a per-record `sync_all` (via `FsyncBackend::fsync_file`) on append,
  so an acknowledged WAL record is on the physical medium before the ack
  returns.
- Snapshot: `atomic_write_snapshot`, so a snapshot is either wholly the
  old one or wholly the new one, never a torn intermediate, even across a
  crash mid-write.

pulse is snapshot-only in this feature: its WAL fsync discipline was
already in place from ADR-0049, so its commit (`16ceb4b`) closes only the
snapshot-atomicity gap that ADR-0049 had left open. The other six stores
received both mechanisms.

## The two-mechanism proving split, and why a SIGKILL cannot prove fsync

The load-bearing design point is that the two durability properties
require two different proving mechanisms, because one crime cannot be
proven by the other's evidence.

Snapshot atomicity is proven by an out-of-process `SIGKILL` mid-snapshot.
A child process is launched, made to begin writing a snapshot, and
`kill -9`d partway through. The parent then reopens the store and asserts
the snapshot is the prior intact one (the rename had not yet happened) and
never a torn intermediate. A real signal-delivered kill at an arbitrary
point is the honest model of a crash, so this is a faithful proof of the
atomicity property.

WAL fsync CANNOT be proven by a process kill. The reason is physical: when
a process is `kill -9`d, the operating system page cache survives the kill.
The kernel keeps running; only the process dies. So bytes that reached only
the page cache (the precise failure this feature fixes) are still readable
by the next open after a process kill, exactly as if they had been fsynced.
A process-kill crash test of WAL durability is therefore vacuously green
whether or not fsync is wired: it cannot distinguish a flushed-but-not-
synced write from a synced one, because both survive the kill. To prove
fsync you must defeat the page cache, which a process kill does not do.

So WAL fsync is proven two other ways:

- A lying-substrate refusal, out of process: `LyingFsyncBackend` models a
  substrate that does not persist, and the acceptance criterion asserts
  the store's behaviour under it, distinguishing an honest fsync path from
  a no-op.
- An in-suite `CountingFsyncBackend` count: the test asserts that the
  write path actually invoked `fsync_file` (the count is non-zero and
  matches the records written). This proves the fsync call is wired into
  the append path, a property no process-kill test can establish.

The split is the feature's central correctness argument: atomicity by
real crash, fsync-wiring by counting and by lying-substrate refusal,
because the page cache makes a kill blind to fsync.

## The seven-store rollout, lumen as the walking skeleton

lumen was delivered first (`ec82270`) as the walking skeleton: its commit
both rewired lumen and extracted the shared FsyncBackend family and
`atomic_write_snapshot` into wal-recovery, proving the full seam end to
end on one store before the other six followed the established pattern.
The remaining stores then adopted the proven seam in turn: ray
(`033707f`), strata (`aa73210`), cinder (`c714403`), sluice (`6d81c29`),
beacon rule-state (`fb43b27`) and pulse snapshot-only (`16ceb4b`).

Each store's pillar-specific work survives around the shared seam; the
shared family absorbs only the fsync-and-atomic-snapshot mechanism, not
the per-store record types or replay logic.

## The DELIVER-found acceptance-design correction

During DELIVER the crafter found a conflation in the original
acceptance design: a lying double had been placed in the WRITE path to
prove WAL fsync by survival, which is the same blindness described above.
A lying double in the write path proves a refusal property, not a
wired-ness property, and using survival-under-lying as the fsync proof
conflated the two.

The crafter escalated this rather than weakening the test to make it pass.
The resolution, committed as `9d7446d`, replaced the survival assertion
with a `CountingFsyncBackend` assertion that mirrors pulse's existing
slice-03 pattern: prove fsync is wired by counting the calls, not by
inferring it from survival under a lie. This is the acceptance design
made honest under DELIVER pressure rather than bent to a green bar, and
it is the reason the proving split above is as sharp as it is.

## Verification

- 40 acceptance scenarios across the seven stores, all real local I/O:
  real WAL and snapshot files on a real tmp directory, real child
  processes for the crash path, real reopen in the parent.
- The crash tests are deterministic and out of process. The snapshot
  atomicity scenarios `kill -9` a child mid-snapshot and assert the
  reopened store is intact. The crash-at-any-point invariant is asserted
  structurally (either the prior whole snapshot or the new whole snapshot,
  never a torn one); it is NOT timed, so it does not depend on hitting a
  particular instant in the write and is not a wall-clock-flaky test.
- WAL fsync wiring is proven by `CountingFsyncBackend` call counts and by
  `LyingFsyncBackend` refusal, per the proving split, not by process-kill
  survival.
- `crates/wal-recovery`: 100% mutation kill (ADR-0005 Gate 5; CLAUDE.md
  per-feature 100%) on the new FsyncBackend family and
  `atomic_write_snapshot`, including the rename mutant (a second snapshot
  must replace the first wholesale) and the fsync-call mutants (the
  Counting backend's counters and the Real backend's `fsync_file` /
  `fsync_dir`).
- Per-store call-site mutation: each store's existing
  `gate-5-mutants-<store>` `--in-diff` job mutates its own changed
  call-site lines automatically; those thin call-site mutants are killed
  by the crash-durability scenarios. The shared durability logic is
  mutation-killed once at the wal-recovery site (the single-site benefit).
- Gate 1 (cargo test --workspace) and Gate 4 (cargo deny, workspace-wide)
  auto-cover the extended crate. No trait signature on the seven store
  traits changed; pulse re-exports the relocated surface so its public
  surface stays byte-identical.

## The honest finding

The durability gap was invisible for as long as it was precisely because
every restart test reopened in process and read back through the same
page cache the writes never left. The suite was green and the stores were
not durable. The work of this feature was less in the fsync call itself
(a `sync_all`) than in building the out-of-process crash harness and the
counting-versus-lying proving split that can actually see the gap, and in
holding that split honest when the original acceptance design conflated
the two mechanisms. The value of recording this is the same as the
torn-tail archive's: the difficulty was in the proof that the claim is
true, not in the one-line mechanism the claim is about.

## Known follow-ups (open)

1. cinder swallowed `append_wal` errors. `place()`
   (`crates/cinder/src/file_backed.rs:270`, `if let Err(_e) = append_wal`)
   and `evaluate_at()` (`:364`, `let _ = append_wal`) discard the result
   of the WAL append rather than surfacing it. A failed durable append on
   these two paths is silently dropped, which is itself a residual
   substrate lie now that the append is fsync-honest. Tracked as
   `cinder-wal-error-surfacing-v0`. Open.

2. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap is not yet covered by this feature's durability work and needs
   its own slice. Open.

3. The AST structural check from ADR-0059 Decision 8 layer b remains
   unwired. ADR-0059 specified a structural pre-commit check asserting
   each in-scope store delegates to the shared wal-recovery routine and
   retains no inline replay loop; the tool choice was deferred to DELIVER
   and remains DEFERRED. It is feedback, not a gate, consistent with the
   pure trunk-based, no-required-checks posture; when wired it belongs in
   the local pre-commit stage. Carried forward unchanged from the
   torn-tail archive. Open.

4. Stale `__SCAFFOLD__` doc markers. The doc comments in
   `crates/query-http-common/src/lib.rs` (and the corresponding aperture
   surface) still carry `__SCAFFOLD__ ... RED` markers describing the
   functions as unimplemented scaffolding, which no longer reflects the
   shipped code. These belong to the claims-honesty-pass and are not part
   of this feature's scope; recorded here so the pass picks them up. Open.
