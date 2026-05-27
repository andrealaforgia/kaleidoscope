# Wave Decisions: earned-trust-fsync-probe-v0 (DESIGN)

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.
Interaction mode: propose. Scope: application. British English. No em dashes.

This wave resolves the four flags DISCUSS handed to DESIGN
(`discuss/wave-decisions.md` FLAGs 1-4): the probe mechanism, the
slice-01 pillar, the test seam, and the new ADR. It also lands one
critical cross-crate cleanup Luna verified during DISCUSS: the WAL
append path in `crates/pulse/src/file_backed.rs:354` calls
`BufWriter::flush()` (user-space buffer to kernel) but NEVER `sync_data`
or `sync_all` on the underlying `File`. Without that fix, an honest
fsync probe is theatre: the probe would refuse on a lying substrate but
the write path the probe vouches for would still depend on a fsync
that the code never asks for. Slice 01 ships BOTH the missing fsync in
the WAL append (and on the snapshot rename's parent-directory durability)
AND the probe at startup. The full rationale, alternatives, and
consequences are in
`docs/product/architecture/adr-0049-earned-trust-honour-fsync.md`.

## Reads checklist

- [x] `docs/feature/earned-trust-fsync-probe-v0/discuss/user-stories.md`
- [x] `docs/feature/earned-trust-fsync-probe-v0/discuss/wave-decisions.md`
- [x] `docs/feature/earned-trust-fsync-probe-v0/discuss/story-map.md`
- [x] `docs/feature/earned-trust-fsync-probe-v0/discuss/outcome-kpis.md`
- [x] `docs/feature/earned-trust-fsync-probe-v0/slices/slice-01-fsync-probe-walking-skeleton.md`
- [x] `docs/product/architecture/residuality-analysis.md` (M-1 section,
  S02 row of the incidence matrix, A-U1 "silent data loss" attractor,
  "Honest gaps in the platform's claimed invariants")
- [x] `docs/residuality-followups-roadmap.md` (this feature is item 1
  of 3; ground rules; full nWave per feature; no 1.0.0 bump)
- [x] `docs/product/architecture/adr-0042-query-api-contract-and-promql-subset.md`
  (Decision 8 "Earned-Trust probe (wire-then-probe-then-use)", the
  `health.startup.refused` event name; cited as the precedent this
  feature REFINES, NOT modified)
- [x] `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md`
  (Decision 6 reproducing the same probe posture for logs; cited as
  precedent, NOT modified)
- [x] `docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`
  (Decision 8 reproducing the same probe posture for traces; cited as
  precedent, NOT modified)
- [x] `crates/log-query-api/src/composition.rs` (the current `probe()`
  at lines 73-84; the `LyingLogStore` shape in `#[cfg(test)] mod tests`
  at lines 97-130; the structured `event=health.startup.refused`
  vocabulary)
- [x] `crates/trace-query-api/src/composition.rs` (the symmetric
  `probe()` for traces; same posture)
- [x] `crates/pulse/src/file_backed.rs` (around line 354 in
  `append_wal`: `wal.write_all(...); wal.flush(...)` and NO
  `sync_data` / `sync_all` on the underlying file; the snapshot path
  at lines 162-196: `File::create(&snapshot_path)`, serde write,
  `writer.flush()`, then truncate-and-recreate the WAL; no parent
  directory fsync after the snapshot or after the WAL recreate)
- [x] `crates/pulse/src/lib.rs` (the public surface; `mod file_backed`
  is private; the crate is library-only with no `main.rs`)
- [x] `crates/pulse/Cargo.toml` (no `[[bin]]`; pulse is library-only;
  `serde` and `serde_json` are present; no fsync dep is needed because
  `File::sync_data` and `File::sync_all` live in `std::fs`)
- [x] `crates/kaleidoscope-gateway/src/main.rs` (the WRITER binary that
  opens `FileBackedMetricStore` via `pillar_root.join(PULSE_SUBDIR)`;
  the composition root where the new pulse fsync probe is wired)
- [x] `docs/product/architecture/brief.md` (per-feature section
  convention; the most recent feature section
  `## Application Architecture — ray-query-api-v0` at line 3016 sets
  the shape and length this feature's section matches)
- [x] `ls crates/pulse/src/`: lib.rs, file_backed.rs, metric.rs,
  metrics.rs, predicate.rs, store.rs. No existing `composition.rs`;
  the probe lands as a new `fsync_probe.rs` module (justified CREATE
  NEW, small)
- [x] `ls docs/product/architecture/adr-*.md`: highest is 0048; 0049
  is the next free number (confirmed)

## Key Decisions

| # | Decision | Resolution | Rationale (short) |
|---|----------|-----------|-------------------|
| 1 | Probe mechanism (FLAG 1) | **Write sentinel + fsync + drop handle + reopen + read**, run at startup BEFORE the listener binds | Portable (no platform-specific syscalls), no fork inside tokio (which is unsafe), catches the fsync no-op class of failure (Docker overlayfs without persistent volume, certain tmpfs configs, mount options that disable sync); cheapest behavioural test for the dominant failure mode |
| 1a | True crash (fork + SIGKILL + reopen) | **REJECTED for slice 01**, RESERVED as a documented escalation if (1) leaves field false negatives | Fork inside a tokio runtime is unsafe (the parent's worker threads, mutexes, file handles, and tokio reactor are duplicated into the child with undefined behaviour); test setup is heavy; the marginal honesty over (1) for the fsync-no-op class is small at v0/v1 |
| 1b | `statfs` / `fstatfs` inspection | **REJECTED**, not portable | Fragmented across Linux/macOS, returns CLAIMS about the filesystem rather than BEHAVIOUR; the residuality analysis's preference is behaviour-tests over claim-reads, and a probe that asks the filesystem to PROVE itself is the only honest one |
| 2 | Pillar for slice 01 (FLAG 2) | **`pulse`** | Most recent pillar change in the ADR-0045 series (series identity); the WRITE path lives at `file_backed.rs:354` (the exact line Luna flagged as missing fsync); the lie hurts most on the WRITE path because the pillar persists state there; reusing the read-API `probe()` shape on a write owner closes both halves (the missing fsync AND the probe) in one slice |
| 2a | `kaleidoscope-gateway` as the pillar | **REJECTED as the pillar**: the probe lands AS PART of the gateway's composition root (its `main.rs`), because pulse is library-only and has no binary of its own; but the probe LOGIC, the seam, and the fsync addition live in the `pulse` crate so later read-binary call sites reuse them | The gateway is the writer's composition root; the fsync-honesty logic belongs to the pillar that owns the write path; coupling the probe to the gateway only would leave the read-side binaries (query-api) without coverage |
| 2b | `log-query-api` or `trace-query-api` for slice 01 | **REJECTED for slice 01**: they already have `probe()` covering open-and-read but they are READ APIs, not the write owners; the missing fsync Luna found is on the WAL append, not on a read | Read APIs do not write a WAL; landing the fsync probe on a read API would leave the write path uncovered (the pillar's actual durability claim) |
| 3 | Add real fsync to the WRITE path (the Luna finding) | **`sync_all` per record on the WAL append**, AND `sync_all` on the snapshot file plus a parent-directory fsync on the rename path | `sync_all` syncs file data AND metadata: the WAL grows over time and recovery reads its length, so the file-size metadata MUST survive a crash to recover correctly; `sync_data` would skip metadata and could leave a recovered WAL appearing shorter than it is; per-record (NOT batched) at slice 01 because durability comes before optimisation, and the project's residue is correctness over capacity (residuality analysis "Limits" section); batched fsync is a documented later optimisation behind the same call site |
| 3a | `sync_data` vs `sync_all` | **`sync_all`** chosen | See rationale row above; on POSIX, `sync_all` is `fsync(2)` (data + metadata) versus `sync_data` which is `fdatasync(2)` (data only); for a growing WAL the file-length metadata is part of the durability promise |
| 3b | Per-record vs batched | **Per-record** at slice 01 | Slice 01 prioritises durability honesty over throughput; per-batch fsync is recorded as a successor optimisation under its own ADR (residuality follow-up roadmap rules) |
| 3c | Snapshot rename durability | **`sync_all` on the snapshot file before truncating the WAL**, plus a parent-directory fsync after the snapshot create | On POSIX, a rename (or a file create followed by a WAL truncate) is durable only if the parent directory is fsynced; without this, a crash can leave the snapshot present but invisible after reboot, or the WAL truncated without the snapshot landing; this preserves the recovery invariant of ADR-0040 (snapshot wins, WAL replays on top) |
| 4 | Test seam for hostile filesystem (FLAG 3) | **`FsyncBackend` trait** with a real implementation and a lying double | Mirrors `LyingLogStore` / `LyingTraceStore` in `crates/log-query-api/src/composition.rs:97` and `crates/trace-query-api/src/composition.rs:106`; the only genuine polymorphism in slice 01; keeps "data + free functions + traits where polymorphism is genuinely needed" (CLAUDE.md) honest; the trait is private to the pulse crate, not part of the public surface |
| 4a | Path injection (point a probe directory at a tmpfs / overlayfs) | **REJECTED** | Less controllable than a trait double; platform-dependent (a tmpfs on macOS vs Linux behaves differently); CI cannot reliably arrange a hostile filesystem; a trait double can deterministically simulate three lie modes (no-op fsync, truncating fsync, byte-flipping fsync) in a unit test |
| 4b | Tempdir double overriding fsync | **REJECTED** | Effectively path injection plus a wrapper; the same controllability problem, and Rust's `std::fs::File::sync_all` cannot be intercepted without a seam (no `LD_PRELOAD` portability); the trait IS the seam, so the wrapper is redundant |
| 5 | New ADR (FLAG 4) | **ADR-0049** records the refinement of the Earned-Trust discipline from "open and read" to "honour fsync"; cites ADR-0042 Decision 8, ADR-0047 Decision 6, ADR-0048 Decision 8 as the precedents this REFINES, NOT modifies | Per the residuality follow-up roadmap's "DESIGN via Morgan, with an ADR if the change is load-bearing" rule, and the project's ADR-immutability convention; ADR-0049 is the next free number (highest existing is 0048, verified by `ls docs/product/architecture/adr-*.md`) |
| 6 | Reuse of the existing `event=health.startup.refused` event | **REUSED VERBATIM**, with a new payload field `substrate=<descriptor>` (e.g. `substrate=fsync-noop`, `substrate=fsync-truncating`) | No new event name, no new metric, no new dashboard; the substrate descriptor is informational and rides on the same event the read APIs already emit |
| 7 | Storage trait surface | **UNCHANGED** | `MetricStore`, `LogStore`, `TraceStore`, beacon `RuleStateStore` trait signatures are byte-identical to the prior tag; the probe rides in a new `pulse::fsync_probe` module and the WAL fsync rides INSIDE the existing `append_wal` free function, neither touching the trait |
| 8 | Probe placement | **`crates/pulse/src/fsync_probe.rs`** (new module), exposed as `pub fn fsync_probe(backend: &dyn FsyncBackend, probe_dir: &Path) -> Result<(), String>`; called from `crates/kaleidoscope-gateway/src/main.rs` BEFORE the gateway binds its listener | Pulse is library-only (no `main.rs`), so the probe LOGIC must live in the library; the wiring lives in the binary that owns the pulse store; the read-side composition roots can later call the same free function (slice 02+) |

## Architecture Summary

A new private module `crates/pulse/src/fsync_probe.rs` in the pulse
library exposes one free function over a small `FsyncBackend` trait:

- `pub trait FsyncBackend` with three methods: `open_for_write`,
  `write_and_sync`, `reopen_and_read` (or a smaller surface; the
  crafter owns the internal decomposition during GREEN, this design
  fixes only the public shape).
- `pub struct RealFsyncBackend` implementing the trait via
  `std::fs::File` + `File::sync_all`.
- `pub fn fsync_probe(backend: &dyn FsyncBackend, probe_dir: &Path) -> Result<(), String>`:
  1. Write a 64-byte sentinel (e.g. the literal
     `b"kaleidoscope-fsync-probe-<unix-ns>"`) to
     `probe_dir/.fsync-probe`.
  2. Call `sync_all` on the file handle.
  3. Drop the handle.
  4. Open a fresh handle on the same path.
  5. Read all bytes back.
  6. Compare to what was written; on mismatch (including "file gone"
     and "file present but shorter or different"), return an `Err`
     naming the substrate descriptor.
- Internally, `apply_ingest` and `snapshot` in `file_backed.rs` gain
  the missing `sync_all` calls on the WAL handle (per record) and on
  the snapshot path plus the parent directory fsync after the
  snapshot create and after the WAL truncate.

The gateway's composition root wires the probe before binding its
listener:

```text
let probe_dir = pillar_root.join(PULSE_SUBDIR);
if let Err(reason) = pulse::fsync_probe(&pulse::RealFsyncBackend, &probe_dir) {
    // emit event=health.startup.refused with substrate descriptor
    // exit non-zero, do NOT bind the listener
}
```

The existing `composition::probe()` in the read APIs continues to
run unchanged (it asserts open-and-read; the new probe asserts
survive-via-fsync). Both probes are independent: open-and-read can
pass while survive-via-fsync fails (the very class US-01 Scenario 2
covers).

### Probe outcome mapping

| Outcome | Condition | Behaviour |
|---|---|---|
| Pass | bytes read back match bytes written | Probe returns `Ok(())`; gateway proceeds to bind the listener |
| Fail (file gone) | reopen succeeds but file is empty / 0 bytes | Probe returns `Err("fsync no-op at <path>: bytes vanished")`; gateway emits `event=health.startup.refused` with `substrate=fsync-noop`; exits non-zero; listener never binds |
| Fail (file truncated) | reopen succeeds but file is shorter than sentinel | Probe returns `Err("fsync truncated at <path>: bytes lost")`; same event with `substrate=fsync-truncating`; same exit |
| Fail (bytes differ) | reopen succeeds, file length matches, bytes differ | Probe returns `Err("fsync corrupted at <path>: bytes differ")`; same event with `substrate=fsync-corrupting`; same exit |
| Fail (no permission / IO) | open or write or sync_all errors | Probe returns `Err("fsync probe IO error at <path>: <reason>")`; same event with `substrate=fsync-io`; same exit |

The substrate descriptor names the LIE class, not the filesystem;
this is honest because the probe observes behaviour, not mount
options.

## Reuse Analysis (MANDATORY)

The verdict is REUSE the existing probe PATTERN and the existing
event; CREATE NEW only the small additions justified below.

| Asset | Where today | Verdict for slice 01 | Lines |
|---|---|---|---|
| `event=health.startup.refused` structured event | `crates/log-query-api/src/composition.rs` (the read APIs emit it on `probe()` failure); ADR-0042 Decision 8 vocabulary | **REUSE VERBATIM**: no new event name. New payload field `substrate=<descriptor>` for context (informational only). | 0 (existing) |
| `composition::probe()` open-and-read shape | `crates/log-query-api/src/composition.rs:73`, `crates/trace-query-api/src/composition.rs:77` | **REUSE PATTERN**: the new fsync probe is a free function with the same shape (data + free function), invoked from a composition root, with a Lying* test double in `#[cfg(test)] mod tests`. NOT shared code (logs/traces stay on open-and-read; the fsync probe is a SECOND independent probe at the same composition root). | 0 (pattern only) |
| `LyingLogStore` / `LyingTraceStore` test double pattern | `crates/log-query-api/src/composition.rs:97`, `crates/trace-query-api/src/composition.rs:106` | **REUSE PATTERN**: a `LyingFsyncBackend` test double in the pulse `#[cfg(test)] mod tests` block, with three modes (no-op, truncating, byte-flipping). NOT shared code (different trait). | 0 (pattern only) |
| `std::fs::File::sync_all` | std | **REUSE STD**: no new dependency for the fsync addition or the probe; `File::sync_all` and `File::open` cover the entire surface. | 0 (existing in std) |
| `BufWriter::flush` on the WAL | `crates/pulse/src/file_backed.rs:358` (inside `append_wal`) | **EXTEND**: add `wal.get_ref().sync_all()` (or `wal.flush()` then `wal.get_ref().sync_all()`) after the existing flush, per record; one effective line plus error mapping. | 1-3 in `append_wal` |
| `BufWriter::flush` on the snapshot | `crates/pulse/src/file_backed.rs:184` (inside `snapshot`) | **EXTEND**: add `writer.get_ref().sync_all()` after the existing flush; add a parent-directory `File::open(parent).and_then(File::sync_all)` after the snapshot create AND after the WAL truncate (POSIX rename durability). | 3-6 in `snapshot` |
| `FsyncBackend` trait | DOES NOT EXIST | **CREATE NEW small, justified**: the only genuine polymorphism needed for slice 01; private to the pulse crate; mirrors `LogStore` / `TraceStore` in shape (a trait + a real impl + a Lying* double in tests); not part of the public surface unless slice 02+ needs it. | ~15-30 |
| `pulse::fsync_probe` free function | DOES NOT EXIST | **CREATE NEW small, justified**: lives in `crates/pulse/src/fsync_probe.rs` (new module file, since pulse has no `composition.rs`); the crafter owns the internal decomposition. | ~30-60 |
| Wiring in `kaleidoscope-gateway/src/main.rs` | The gateway opens the pulse store at `pillar_root.join(PULSE_SUBDIR)` | **EXTEND**: one call to `pulse::fsync_probe(&pulse::RealFsyncBackend, &pulse_path)` before the listener binds, with the existing event emission on `Err`. | ~5-10 |
| New crate | n/a | **NONE**: no new workspace crate; the entire change is inside `crates/pulse` and the existing `crates/kaleidoscope-gateway`. | 0 |
| New external dependency | n/a | **NONE**: `std::fs::File::sync_all` and `File::open` are sufficient; no `nix`, no `libc`, no `tempfile` in non-test code. | 0 |
| New CI job | n/a | **NONE**: the existing `gate-5-mutants-pulse` job covers `crates/pulse/src/` (including the new `fsync_probe.rs` and the additions to `file_backed.rs`) via `--in-diff`; the existing per-crate Gate 5 catches the change without a new workflow. | 0 |

**Verdict.** Zero new crate. Zero new external dependency. Zero new
CI job. Zero new event name. One new small module
(`fsync_probe.rs`), one new small private trait (`FsyncBackend`),
one new free function (`fsync_probe`), and three small surgical
additions to the WRITE path (one in `append_wal`, two in
`snapshot`). Every CREATE NEW is justified and small. The probe
PATTERN and the event VOCABULARY are reused from ADR-0042 / 0047 /
0048 precedents.

## Constraints

- **No storage trait change**: `MetricStore`, `LogStore`,
  `TraceStore`, beacon `RuleStateStore` trait signatures stay
  byte-identical to the prior tag. The probe rides OUTSIDE the trait;
  the fsync addition rides INSIDE the existing `append_wal` /
  `snapshot` free functions, which are not trait methods. Gate 2
  (`cargo public-api` diff) catches any regression.
- **No new event name**: refusal rides on the existing
  `event=health.startup.refused`.
- **Library-only pulse**: pulse remains library-only; no `main.rs`
  added; the wiring lives in `kaleidoscope-gateway/src/main.rs`.
- **No fork**: the probe must not call `fork(2)` or `posix_spawn`;
  the walking skeleton uses drop-handle + reopen instead.
- **No new external dependency**: `std::fs::File` covers the entire
  fsync surface; `serde` and `serde_json` are already pulled in for
  the existing WAL / snapshot.
- **Per-record fsync**: slice 01 fsyncs per WAL record; batched
  fsync is deferred to a successor feature under its own ADR.
- **Probe runs BEFORE the listener binds**: the wire-then-probe-then-use
  invariant of ADR-0042 Decision 8 is preserved; a failed probe
  refuses to start, never binds half-up.

## C4 — Levels 1, 2 — earned-trust-fsync-probe-v0

See `design/application-architecture.md` for the Mermaid diagrams.
L1 (System Context): the platform operator runs the gateway; the
gateway opens the pulse FileBackedMetricStore on `pillar_root/pulse`;
before binding the listener, the gateway calls `pulse::fsync_probe`,
which writes a sentinel, fsyncs, reopens, and reads back; on success,
the gateway binds; on failure, the gateway emits
`event=health.startup.refused` and exits non-zero, never binding.
L2 (Probe path): the in-process flow inside `pulse::fsync_probe`
from `write_sentinel` -> `fsync` -> `drop_handle` -> `reopen` ->
`read` -> `refuse_or_bind`. L3 is NOT produced: the probe is one
free function over one trait with one real implementation and a
Lying* test double, not a multi-component subsystem.

## Quality attribute coverage (ISO 25010)

| Attribute | How addressed |
|---|---|
| Reliability | The probe runs BEFORE the listener binds (wire-then-probe-then-use, ADR-0042 Decision 8 preserved); a fsync-lying substrate refuses to start rather than serving fabricated durability; on the WAL append, the new `sync_all` makes the durability claim honest at the byte level (the Luna finding closed). |
| Functional Suitability | Three lie classes (no-op, truncating, byte-corrupting) are distinguished in the substrate descriptor; the probe is deterministic over identical inputs (same sentinel, same path). |
| Maintainability | One new small module in pulse; one small private trait; three surgical additions in `file_backed.rs`; no trait change; the probe pattern mirrors the read-API `probe()` shape so future contributors recognise it without re-reading this ADR. Per-feature mutation testing at 100% kill on the changed files (ADR-0005 Gate 5, CLAUDE.md) covers `fsync_probe.rs` and the additions in `file_backed.rs` via the existing `gate-5-mutants-pulse` workflow scoped by `--in-diff`. |
| Security | The probe path `probe_dir/.fsync-probe` is FIXED and overwritten on every run; no accumulating state across restarts; the path is under `pillar_root` (the operator-controlled directory), not under `/tmp` or a global location. The event payload's substrate descriptor names the LIE class, not the filesystem options or any credential. |
| Performance Efficiency | The probe runs ONCE at startup: one small file write (64 bytes), one `sync_all`, one reopen, one read. Cost is bounded and unobservable in operational latency. The per-record `sync_all` on the WAL append is a real cost; ADR-0049 records it as a known trade with batched fsync available as a successor optimisation under its own ADR. |
| Portability | No platform-specific syscalls; `std::fs::File::sync_all` is the portable surface; the probe works on Linux/macOS/Windows alike with the same semantics. |
| Compatibility | No change to the WAL or snapshot file formats; recovery semantics under ADR-0040 are preserved (the WAL is now actually durable, which the recovery was implicitly assuming). |

## DEVOPS Handoff Annotation

For `@nw-platform-architect` (Apex):

- **No new crate**: the change is inside `crates/pulse` and the
  existing `crates/kaleidoscope-gateway`. No workspace `Cargo.toml`
  members edit.
- **No new external dependency**: `std::fs::File::sync_all` and
  `File::open` are sufficient; `serde` / `serde_json` already exist
  in pulse for the WAL / snapshot; no `nix`, no `libc`, no
  `tempfile` in non-test code.
- **No new CI job**: the existing `gate-5-mutants-pulse` job covers
  the changed files (`crates/pulse/src/fsync_probe.rs` and the
  additions to `crates/pulse/src/file_backed.rs`) via `--in-diff` at
  the 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md). Primary
  mutation targets: the probe's bytes-differ branch (`!=` -> `==`
  must be killed), the per-record `sync_all` on `append_wal` (the
  call must not be deletable without a surviving test), the
  parent-directory fsync on snapshot rename, and the three lie
  classes in the substrate descriptor mapping (no-op vs truncating
  vs corrupting must remain distinguishable).
- **No new event name**: refusal rides on the existing
  `event=health.startup.refused`. The new payload field is
  `substrate=<descriptor>` (informational); no dashboard or
  alert work needed at v0/v1 (the platform has no live
  observability stack of its own yet, per `outcome-kpis.md`
  "Handoff to DEVOPS").
- **External integrations**: NONE. The probe reads/writes the
  in-process filesystem under `pillar_root`, not a network service.
  No consumer-driven contract test recommendation.
- **Earned Trust enforcement (three orthogonal layers, reproduced
  from ADR-0042 Decision 8 / ADR-0047 Decision 6 / ADR-0048
  Decision 8)**: (a) subtype check at the gateway's composition
  root (the probe consumes the `FsyncBackend` port; `RealFsyncBackend`
  satisfies it by `impl` at the boundary); (b) AST structural
  pre-commit check that the gateway's `main.rs` calls
  `pulse::fsync_probe` BEFORE `axum::serve` / the listener bind;
  (c) behavioural gold-test in `crates/pulse/tests/` exercising the
  three lie classes (no-op, truncating, corrupting `FsyncBackend`
  double) and asserting the probe returns `Err` and a synthetic
  composition-root caller emits `event=health.startup.refused`.
  A single-layer bypass is caught by at least one of the other two.
- **Forward-looking scope**: slice 01 covers ONE pillar (pulse).
  Successor slices extend the same `FsyncBackend` and
  `fsync_probe` shape to the remaining pillars (lumen, ray, cinder,
  strata, sluice) and to the beacon state store. Each successor
  slice reuses the trait and the probe (so the shape stabilises in
  slice 01) and adds the missing fsync to that pillar's WAL append
  / snapshot path symmetrically.
- **DELIVER paradigm**: Rust idiomatic (data + free functions + a
  small trait where polymorphism is genuinely needed per CLAUDE.md);
  the crafter owns GREEN / REFACTOR internals; this design fixes
  only the public `fsync_probe` free function signature, the
  `FsyncBackend` trait surface (as a minimum), the substrate
  descriptor classes, and the exact lines in `file_backed.rs` to
  add `sync_all` (per the Changes Per File table in
  `design/application-architecture.md`).

## Upstream Changes

None. The three precedent ADRs (0042 / 0047 / 0048) are CITED, NOT
modified. ADR-0049 RECORDS the refinement of the principle as a NEW
ADR per the project's ADR-immutability convention. The
`event=health.startup.refused` vocabulary is reused verbatim.

## Honest contradiction check

The DISCUSS framing flagged FLAG 2 with three candidates (`pulse`,
`kaleidoscope-gateway`, a read API). DESIGN observed during the
reads checklist that **pulse is library-only**: there is no
`crates/pulse/src/main.rs` and the Cargo manifest has no `[[bin]]`
section. The candidate "pulse" in DISCUSS therefore needs a small
clarification: the probe LOGIC lives in `pulse` (where it belongs,
because pulse owns the write path), and the probe WIRING lives in
the consumer binary `kaleidoscope-gateway/src/main.rs` (the gateway
opens the pulse store as a writer). This is captured in Decision 2
and Decision 8 and explained in `design/application-architecture.md`.
This is a refinement, not a reversal: pulse remains the
slice-01 pillar; the gateway is its composition root because pulse
has no binary of its own.

No other DISCUSS framing was contradicted. The Luna finding (missing
`sync_data` / `sync_all` on `append_wal`) is fully reproduced from
`crates/pulse/src/file_backed.rs:354`. The four DISCUSS flags are
resolved as recommended by the prompt with documented rejections
for the alternatives.

## Changelog

- 2026-05-27: feature `earned-trust-fsync-probe-v0` DESIGN wave
  artefacts written by Morgan. Four flags resolved (probe mechanism
  = write+fsync+drop+reopen+read; slice-01 pillar = `pulse` with the
  wiring in `kaleidoscope-gateway/src/main.rs`; seam = `FsyncBackend`
  trait + lying double; new ADR = ADR-0049). One critical
  cross-crate cleanup pinned: `sync_all` on `append_wal` per record
  and on the snapshot rename's parent directory, the missing fsync
  Luna verified during DISCUSS.
