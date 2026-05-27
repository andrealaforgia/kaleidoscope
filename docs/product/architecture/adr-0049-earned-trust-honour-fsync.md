# ADR-0049 — Earned-Trust at startup must honour fsync, not just open-and-read

- **Status**: Accepted
- **Date**: 2026-05-27
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `earned-trust-fsync-probe-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0042 (the originating Earned-Trust probe at the query-api
  composition root, the `event=health.startup.refused` vocabulary, the
  wire-then-probe-then-use invariant; cited as the precedent this ADR
  REFINES, NOT modified). ADR-0047 (Decision 6 reproduced the same probe
  posture for the log read API; cited as precedent, NOT modified).
  ADR-0048 (Decision 8 reproduced the same probe posture for the trace
  read API; cited as precedent, NOT modified). ADR-0040 (the WAL +
  snapshot + replay recovery discipline whose durability the missing
  fsync silently violated; cited, NOT modified). ADR-0005 (the five CI
  gates including Gate 5 100% mutation kill on modified files).

## Context

The platform claims Earned-Trust at startup. ADR-0042 Decision 8
("Earned-Trust probe, wire-then-probe-then-use") established the
discipline: every composition root runs `probe()` BEFORE binding its
listener and emits `event=health.startup.refused` on failure. ADR-0047
Decision 6 and ADR-0048 Decision 8 reproduced the discipline for the
log and trace read APIs.

The discipline as recorded in those three ADRs and as implemented in
`crates/log-query-api/src/composition.rs:73` and
`crates/trace-query-api/src/composition.rs:77` means **"open and read"**:
the probe calls `store.query(...)` against a sentinel range and asserts
`Ok(())`. It does NOT mean **"survive a crash via fsync"**.

The residuality analysis
(`docs/product/architecture/residuality-analysis.md`, commit 50e20b5)
flagged this as the single biggest residue gap in the incidence matrix,
in the S02 row ("fsync silently no-op on the substrate"): six storage
pillars (`pulse`, `lumen`, `ray`, `cinder`, `strata`, `sluice`) and the
beacon rule-state store all rely on fsync that the WAL recovery
discipline (ADR-0040) implicitly assumes; none of them probe it. The
matrix cell for every storage column under S02 reads **B silent loss
possible (A-U1)**.

During the DISCUSS wave of `earned-trust-fsync-probe-v0` (Luna,
2026-05-25), a workspace grep verified the gap. `fsync`, `sync_data`,
`sync_all`, `FsyncProbe` return zero hits in `crates/`. Worse: the WAL
append in `crates/pulse/src/file_backed.rs:354` calls
`wal.write_all(...)` then `wal.flush()` on a `BufWriter<File>`. `flush`
empties the user-space buffer to the kernel; it does NOT call
`sync_data` or `sync_all` on the underlying file. The WAL is therefore
durable in the buffered sense (the bytes left the process) and NOT
durable in the crash-survival sense (the bytes have not yet reached
stable storage and may be lost on a power failure, a kernel panic, or
an unclean container stop). The snapshot path is similar: it flushes
the snapshot file's `BufWriter`, then truncates and recreates the WAL,
without `sync_all` on either file and without a parent-directory fsync
to make the snapshot's directory entry durable before the WAL
truncation that depends on it.

The Earned-Trust claim in three ADRs is paper, not code. This ADR
refines the discipline to mean **"honour fsync"**: every storage
pillar's composition root runs an fsync-honesty probe BEFORE binding
the listener, AND the WAL append and snapshot rename paths actually
call `sync_all` on the underlying file. The two halves are atomic in
slice 01 for one pillar (`pulse`); successor slices extend the same
shape to the remaining pillars.

ADRs in this repository are immutable (superseded, never edited).
ADR-0042 / 0047 / 0048 are Accepted and referenced as precedents; they
are NOT modified. ADR-0049 is the next free number (the highest
existing was 0048, verified by `ls docs/product/architecture/adr-*.md`).

## Decision

### 1. Refined Earned-Trust principle: probe must honour fsync, not just open-and-read

Every storage pillar's composition root MUST run an fsync-honesty probe
BEFORE binding its listener. The probe writes a sentinel under
`pillar_root`, calls `sync_all` on the file handle, drops the handle,
opens a fresh handle, reads the bytes back, and compares. On any
mismatch (file gone, file shorter, file present with different bytes,
or IO error at any step), the probe MUST refuse to start: the composition
root emits `event=health.startup.refused` with a `substrate=<descriptor>`
payload field and exits non-zero, NEVER binding the listener. The
"open and read" probe established by ADR-0042 Decision 8 continues to
run unchanged; the fsync-honesty probe is a SECOND independent check at
the same composition root. Open-and-read can pass while
survive-via-fsync fails; that is the very class of substrate lie this
refinement exists to catch.

### 2. Write paths must actually call fsync

Every WAL append path MUST call `sync_all` on the underlying file
AFTER the `BufWriter::flush` and BEFORE returning success to the caller.
Every snapshot persistence path MUST call `sync_all` on the snapshot
file before the dependent WAL truncate, AND fsync the parent directory
both between snapshot create and WAL truncate AND after the WAL
recreate. `BufWriter::flush` is NOT a substitute for `sync_all`:
`flush` empties the user-space buffer to the kernel; `sync_all` instructs
the kernel to make the data durable on stable storage. The two are not
the same and conflating them is the gap this refinement closes.

### 3. Probe mechanism for slice 01: write + fsync + drop + reopen + read

The probe at slice 01 is the cheapest portable behavioural test for the
fsync-no-op class of substrate failure: write a 64-byte sentinel, call
`sync_all`, drop the file handle (which closes it), open a fresh
handle, read the bytes back, compare. A fork + SIGKILL + reopen true
crash test is REJECTED for slice 01 because `fork(2)` inside a tokio
runtime is unsafe (worker threads, mutexes, file handles, and the
reactor are duplicated into the child with undefined behaviour); it is
RESERVED as a documented escalation if the cheaper probe leaves field
false negatives. A `statfs` / `fstatfs` syscall inspection is REJECTED
because it is fragmented across Linux/macOS and reads CLAIMS about the
filesystem rather than BEHAVIOUR; an honest probe makes the substrate
PROVE itself.

### 4. fsync strategy on the write path: `sync_all` per record

`sync_all` is chosen over `sync_data`: on POSIX, `sync_all` is `fsync(2)`
(data + metadata) versus `sync_data` which is `fdatasync(2)` (data
only). The WAL grows append-only over time; recovery reads its
length; the file-size metadata is therefore part of the durability
promise. `sync_data` would skip metadata and could leave a recovered
WAL appearing shorter than it is. Per-record (NOT batched) fsync at
slice 01: durability honesty before throughput optimisation, in line
with the residuality analysis's framing of correctness residues over
capacity residues at v0/v1. Batched fsync is a documented successor
optimisation behind the same call site, under its own ADR.

### 5. Snapshot rename durability

On POSIX, a file that has been created and written is not durable
across crashes until both the file's bytes AND the parent directory's
entry pointing to the file are fsynced. The snapshot path in
`crates/pulse/src/file_backed.rs:163-195` MUST:

1. Write the snapshot file with `BufWriter::flush` AND
   `writer.get_ref().sync_all()`.
2. fsync the parent directory once between snapshot persistence and
   WAL truncate (so the snapshot's directory entry is durable before
   the WAL truncation that depends on it).
3. Truncate-and-recreate the WAL.
4. fsync the parent directory a second time after the WAL recreate
   (so the WAL truncation is durable).

Without these, a crash can leave the snapshot present but invisible
after reboot, or the WAL truncated without the snapshot landing, both
of which silently corrupt the ADR-0040 recovery invariant.

### 6. Test seam: `FsyncBackend` trait + lying double

Slice 01 introduces one small private trait `FsyncBackend` in the pulse
crate, with one real implementation `RealFsyncBackend` (delegating to
`std::fs::File::sync_all`) and a `LyingFsyncBackend` test double in
`#[cfg(test)] mod tests` covering three lie modes: no-op fsync (file
vanishes on reopen), truncating fsync (file present but shorter), and
byte-flipping fsync (file present, length matches, bytes differ). The
trait mirrors the shape of `LogStore` / `TraceStore` and the double
mirrors `LyingLogStore` / `LyingTraceStore` in
`crates/log-query-api/src/composition.rs:97` and
`crates/trace-query-api/src/composition.rs:106`, so contributors
recognise the pattern. The trait is private to the pulse crate at slice
01 (later pillars may need to share it; successor slices will decide).

### 7. Refusal vocabulary reuses the existing event

The composition root emits `event=health.startup.refused` verbatim,
with a new informational payload field `substrate=<descriptor>` where
`<descriptor>` is one of `fsync-noop`, `fsync-truncating`,
`fsync-corrupting`, `fsync-io`. No new event name, no new metric, no
new dashboard. The substrate descriptor names the LIE class observed
by the probe, not the filesystem or mount options (an honest probe
reports behaviour, not claims).

### 8. Slice 01 covers one pillar; successor slices extend symmetrically

Slice 01 covers `pulse` only. The probe LOGIC lives in `crates/pulse`
(the pillar that owns the write path); the probe WIRING lives in
`crates/kaleidoscope-gateway/src/main.rs` (the binary that opens
`FileBackedMetricStore`, because pulse is library-only and has no
`main.rs` of its own). Successor slices extend the same `FsyncBackend`
and `fsync_probe` surface to `lumen`, `ray`, `cinder`, `strata`,
`sluice`, and the beacon rule-state store, each from its own composition
root. Each successor slice reuses the trait and the probe (so the shape
stabilises in slice 01) and adds the missing `sync_all` to that pillar's
WAL append / snapshot path symmetrically.

## Alternatives considered

### Probe mechanism A (rejected): fork + SIGKILL the child + reopen

A true crash test: the probe writes the sentinel, forks, the child
fsyncs and is SIGKILL'd before exit, the parent reopens and reads.
For: catches a wider class of substrate lies including buffered fsync
that lies only across process boundaries. Against: `fork(2)` inside a
tokio runtime is unsafe (the parent's worker threads, mutexes, file
handles, and reactor are duplicated into the child with undefined
behaviour); the test setup is heavy; the marginal honesty over the
drop-handle-and-reopen approach for the fsync-no-op class is small at
v0/v1. Rejected for slice 01; RESERVED as a documented escalation if
the cheaper probe leaves field false negatives in a successor slice.

### Probe mechanism B (rejected): `statfs` / `fstatfs` syscall inspection

Inspect filesystem options to detect known-lying substrates
(overlayfs without persistent volume, tmpfs with `sync=no`, network
filesystems with delayed fsync). For: detects the substrate before any
write. Against: `statfs` and `fstatfs` are fragmented across
Linux/macOS (different struct layouts, different fields, missing on
some platforms); they return CLAIMS the filesystem makes about itself,
not behaviour the probe can verify; an honest probe asks the substrate
to PROVE itself rather than reading its self-description. Rejected.

### fsync strategy A (rejected): `sync_data` (fdatasync)

`sync_data` is faster than `sync_all`: it skips metadata syncs. For: a
real performance gain in the steady-state ingest path. Against: the WAL
grows append-only and recovery reads its length; the file-size metadata
is part of the durability promise; `sync_data` would skip it and could
leave a recovered WAL appearing shorter than it is. Rejected for the
WAL append. The snapshot path is similar (the snapshot file's size and
the parent directory's entry are both part of the recovery invariant).

### fsync strategy B (rejected): batched fsync (one fsync per N records)

Batch multiple WAL appends behind one `sync_all` call. For: throughput
gain proportional to N. Against: at slice 01 the project's residues are
correctness residues over capacity residues (residuality analysis
"Limits" section); per-record fsync is the strongest durability
contract and the simplest correctness invariant to test; batched fsync
is a documented successor optimisation behind the same call site,
under its own ADR. Rejected for slice 01; recorded for a successor
feature.

### Seam A (rejected): path injection

Configure a probe directory the test can point at a tmpfs (different
fsync semantics) or an overlayfs simulation. For: tests the real
filesystem, not a double. Against: less controllable (a tmpfs on
macOS vs Linux behaves differently); CI cannot reliably arrange a
hostile filesystem without containers and special privileges; a trait
double can deterministically simulate three lie modes (no-op,
truncating, byte-flipping) in a unit test. Rejected.

### Seam B (rejected): tempdir double overriding fsync

A tempdir owned by the test plus a wrapper that intercepts `sync_all`
calls. For: works on the real filesystem. Against: effectively path
injection plus a wrapper; the same controllability problem;
`std::fs::File::sync_all` cannot be intercepted in safe Rust without a
seam (no portable `LD_PRELOAD` equivalent); the trait IS the seam, so
the wrapper is redundant. Rejected.

### Place the probe in a read-API crate instead of pulse (rejected)

Land the fsync probe in `log-query-api` or `trace-query-api`, where the
existing `composition::probe()` already lives. For: smallest blast
radius for the first probe; reuses the file the precedent lives in.
Against: the read APIs do not write a WAL; the missing fsync Luna
verified is on the WAL append (a write owner's path); landing the
fsync probe on a read API would leave the write path uncovered (the
pillar's actual durability claim). Rejected for slice 01.

### Place the probe in the gateway crate (rejected as the slice-01 pillar)

Land the probe LOGIC inside `crates/kaleidoscope-gateway`. For: the
gateway is the writer's composition root and a natural caller. Against:
the probe LOGIC belongs to the pillar that owns the write path (so
later read-side binaries can reuse it); landing the logic in the
gateway only would leave query-api and the future per-pillar binaries
without coverage and would duplicate the trait and free function
across crates. The probe LOGIC lives in pulse; the WIRING lives in the
gateway. Rejected as the slice-01 pillar; ACCEPTED as the slice-01
composition root (the binary that calls `pulse::fsync_probe`).

## Consequences

### Positive

- **The Earned-Trust claim becomes code, not paper**. The principle
  recorded in ADR-0042 Decision 8 and reproduced in ADR-0047 / 0048
  is now demonstrable by a behavioural probe, with a deterministic
  refusal path on a lying substrate. The S02 row of the residuality
  matrix transitions from "B silent loss possible" to "S substrate
  refused at startup" for pulse (and, in successor slices, for the
  remaining pillars).
- **The WAL is actually durable**. The missing `sync_all` on
  `append_wal` is closed; the recovery invariant of ADR-0040 (snapshot
  wins, WAL replays on top) is now backed by a substrate the platform
  has verified honours fsync.
- **The snapshot rename is durable**. On POSIX, the parent-directory
  fsyncs around the snapshot create and WAL truncate make the recovery
  invariant survive a crash between the two operations.
- **No new event name, no new dashboard**. The refusal rides on the
  existing `event=health.startup.refused`; the substrate descriptor is
  an informational payload field; the existing operator vocabulary is
  preserved.
- **No trait change, no blast radius**. `MetricStore`, `LogStore`,
  `TraceStore`, beacon `RuleStateStore` trait signatures are
  byte-identical to the prior tag; Gate 2 (`cargo public-api`) catches
  any regression.
- **The probe pattern is reusable**. The `FsyncBackend` trait, the
  `fsync_probe` free function, and the `event=health.startup.refused`
  emission shape stabilise in slice 01 so successor slices extend the
  same surface to other pillars cheaply.

### Negative

- **Per-record `sync_all` is a real cost**. The WAL append path now
  pays a fsync per record. At v0/v1 this is acceptable (no production
  load, correctness over capacity); a successor optimisation (batched
  fsync) is recorded as a future feature under its own ADR.
- **The probe writes a sentinel file**. `pillar_root/pulse/.fsync-probe`
  is created and overwritten on every gateway start. The path is fixed
  and overwritten (no accumulating state); the file is small (64
  bytes); the cost is bounded. Documented as a known artefact in
  `pillar_root` for operators inspecting the directory.
- **Slice 01 covers only one pillar**. The remaining pillars (lumen,
  ray, cinder, strata, sluice, beacon state store) keep their existing
  un-probed fsync until successor slices extend the pattern. Recorded
  in the residuality follow-up roadmap; the S02 row of the incidence
  matrix transitions pillar by pillar, not in one slice.
- **The probe is not a true crash test**. The drop-handle-and-reopen
  mechanism catches the fsync-no-op class of failure (overlayfs without
  persistent volume, tmpfs with `sync=no`, mount options that disable
  sync) but does not catch lies that manifest only across process
  boundaries. The fork+SIGKILL escalation is RESERVED behind the same
  seam in a successor slice if the cheaper probe leaves documented
  field false negatives.
- **The probe is platform-portable but coarse-grained on Windows**.
  `std::fs::File::sync_all` maps to `FlushFileBuffers` on Windows; the
  semantics are close enough to POSIX `fsync` for the probe to work,
  but the substrate descriptor classes (no-op vs truncating vs
  corrupting) reflect POSIX behaviour and may be less informative on
  Windows. Documented as a known portability limit.

### Trade-off summary

The refinement is intentionally narrow: it changes the meaning of the
Earned-Trust probe from "open and read" to "honour fsync" and adds the
missing `sync_all` calls in pulse's write path. The trade-off is
"throughput optimisation now" against "an honest, testable, deployable
durability claim now". v0/v1 takes the latter and records every
deferral.

## Verification

- A workspace grep for `fsync`, `sync_data`, `sync_all`, `FsyncBackend`
  in `crates/` returns non-zero hits after slice 01 lands (it returned
  zero before, per the residuality analysis and Luna's DISCUSS
  verification). The minimum surface: `FsyncBackend` in
  `crates/pulse/src/fsync_probe.rs`, `sync_all` calls in
  `crates/pulse/src/file_backed.rs` (the `append_wal` line and the
  snapshot path), and `pulse::fsync_probe` in
  `crates/kaleidoscope-gateway/src/main.rs`.
- An acceptance test in `crates/pulse/tests/slice_01_fsync_probe.rs`
  exercises the honest path (the probe returns `Ok(())` over a real
  tempdir) AND the three lie classes via `LyingFsyncBackend` (the
  probe returns `Err` with the matching substrate descriptor on each).
- A regression test in the existing read APIs (`log-query-api`,
  `trace-query-api`, `query-api`) confirms their `composition::probe()`
  continues to refuse on an unreadable store (US-01 Scenario 4); the
  existing `LyingLogStore` / `LyingTraceStore` shape is preserved.
- Gate 2 (`cargo public-api`) confirms `MetricStore`, `LogStore`,
  `TraceStore`, beacon `RuleStateStore` trait signatures are
  byte-identical to the prior tag.
- **Earned-Trust enforcement (three orthogonal layers)**: (a) subtype
  check at the gateway's composition root (the probe is consumed
  through the `FsyncBackend` port; `RealFsyncBackend` satisfies it by
  `impl FsyncBackend for RealFsyncBackend`; removing the implementation
  fails the build); (b) an AST structural pre-commit check that
  `crates/kaleidoscope-gateway/src/main.rs` calls `pulse::fsync_probe`
  BEFORE `axum::serve` / the listener bind; (c) a behavioural gold-test
  exercising the three lie classes via `LyingFsyncBackend` and
  asserting the probe returns `Err` and the synthetic composition-root
  caller emits `event=health.startup.refused`. A single-layer bypass is
  caught by at least one of the other two. `import-linter` was
  investigated and rejected (its contracts are import-graph only, no
  API for method-presence enforcement on classes); the AST pre-commit
  hook covers the structural layer.
- Mutation testing: `cargo mutants` scoped to the modified files
  (`crates/pulse/src/fsync_probe.rs` and the additions in
  `crates/pulse/src/file_backed.rs`) at the 100% kill-rate gate
  (ADR-0005 Gate 5; CLAUDE.md). Primary targets: the bytes-differ
  branch (`!=` -> `==` must be killed); the per-record `sync_all` on
  `append_wal` (the call must not be deletable without a surviving
  test); the parent-directory fsync calls on the snapshot rename; the
  three substrate descriptor classes (no-op vs truncating vs corrupting
  must remain distinguishable). Covered by the existing
  `gate-5-mutants-pulse` workflow via `--in-diff` (no new CI job).

## External-integration handoff

None. The probe reads and writes the in-process filesystem under
`pillar_root`, not a network service. No consumer-driven contract test
recommendation. (The existing read-API contracts under ADR-0042 /
0047 / 0048 are unaffected; their `composition::probe()` continues to
run unchanged.)
