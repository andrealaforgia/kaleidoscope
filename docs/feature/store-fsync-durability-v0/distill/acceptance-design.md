# Acceptance Design — store-fsync-durability-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Agent**: Scholar (`nw-acceptance-designer`)
- **Date**: 2026-06-04
- **Mode**: Autonomous overnight run.
- **Inputs read**: `docs/product/architecture/brief.md`
  (§ Application Architecture — store-fsync-durability-v0, the C4 view, and
  the per-AC "For Acceptance Designer" note),
  `adr-0060-earned-trust-store-fsync-durability.md`,
  `design/upstream-changes.md`, `discuss/user-stories.md` (US-01..US-07),
  `devops/wave-decisions.md` + `environments.yaml`, and the template code
  (`crates/pulse/src/file_backed.rs`, `crates/pulse/src/fsync_probe.rs`,
  `crates/lumen/src/file_backed.rs`, the existing
  `crates/lumen/tests/v1_slice_03_torn_tail_recovery.rs` and
  `crates/pulse/tests/v1_slice_03_fsync_probe.rs`).

## The load-bearing decision DISTILL inherits: two proving mechanisms

ADR-0060 §1 / the brief / `upstream-changes.md` are unanimous and binding: a
single `SIGKILL` test CANNOT prove WAL fsync, because `flush()` leaves bytes
in the kernel **page cache**, which SURVIVES a process kill on the same host.
So each per-store crash AC is SPLIT (honouring `upstream-changes.md`) into
two ACs, each proven by the mechanism that can actually falsify it:

| AC | Property | Mechanism | Driving port |
|----|----------|-----------|--------------|
| **AC-snapshot-atomicity** | `open()` succeeds after a mid-snapshot crash; the canonical path is whole-or-absent, never torn | **(a)** real out-of-process child PROCESS `SIGKILL`ed mid-snapshot, parent reopens | store reopen + read path |
| **AC-wal-fsync** | an acked write is on stable storage, not merely the page cache | **(b)** in-suite `LyingFsyncBackend` (`no_op`/`truncating`) injected via `open_with_fsync_backend`, discarding the unsynced bytes a power cut would | `open_with_fsync_backend` seam |
| **AC-substrate-refusal** | the composition root refuses to start on a lying substrate, emitting `event=health.startup.refused substrate=<descriptor>` and exiting non-zero | **(b)** variant — drive the composition root (the crash-target `--probe-lying` mode) with a `LyingFsyncBackend` | process stderr (structured tracing) |
| **AC-recovery-regression** | the kept `SIGKILL`+read assertion, RE-LABELLED: torn never-acked tail dropped, acked prefix kept (pairs with ADR-0059) | **(a)** process-kill + reopen | store reopen + read path |

The two mechanisms are written as SEPARATE scenarios per store; a SIGKILL is
NEVER the sole proof of a wal-fsync AC (the explicit DISTILL prohibition).
pulse (US-07) carries ONLY AC-snapshot-atomicity (+ the recovery-regression
guard) — its WAL is already crash-durable under ADR-0049.

## Walking skeleton

**lumen, slice 01** is the walking skeleton: both mechanisms end to end,
observable through `FileBackedLogStore::open` reopen + the log read path for
AC-snapshot-atomicity, and through the lying-substrate `open_with_fsync_backend`
seam + the `--probe-lying` refusal for AC-wal-fsync / AC-substrate-refusal. It
validates the fatal assumption (a deterministic out-of-process crash on the
most observable read path) AND lands the shared `atomic_write_snapshot` helper
all seven then reuse. Rollout order per ADR-0060 §5: lumen → ray → strata →
cinder → sluice → beacon rule-state → pulse (snapshot-only).

## I-O strategy: C (real local I/O + real child process)

Real WAL/snapshot files on a real per-test tmp directory (std `temp_dir` +
PID + nanos, the established v1 file-backed convention; manual cleanup, no new
dev-dependency), and a real OS child process for mechanism (a). No external
services, no containers, no privilege, no `drop_caches`, no VM (matching
`devops/environments.yaml`). Every scenario is tagged `@real-io`. The decision
record is in `io-strategy.md`.

## Scenario inventory (40 scenarios across 7 stores)

| Store | Slice | Test file | Scenarios | Mechanisms |
|-------|-------|-----------|-----------|------------|
| lumen | 01 (WS) | `crates/lumen/tests/v1_slice_04_crash_durability.rs` | 7 | (a) + (b) + refusal + regression |
| ray | 02 | `crates/ray/tests/v1_slice_04_crash_durability.rs` | 6 | (a) + (b) + refusal + regression |
| strata | 03 | `crates/strata/tests/v1_slice_03_crash_durability.rs` | 6 | (a) + (b) + refusal + empty-store boundary |
| cinder | 04 | `crates/cinder/tests/v1_slice_04_crash_durability.rs` | 6 | (a) + (b) + refusal + regression |
| sluice | 05 | `crates/sluice/tests/v1_slice_03_crash_durability.rs` | 6 | (a) + (b) + refusal + in-flight boundary |
| beacon | 06 | `crates/beacon/tests/v1_slice_03_crash_durability.rs` | 6 | (a) + (b) + refusal + regression |
| pulse | 07 | `crates/pulse/tests/v1_slice_06_snapshot_atomicity.rs` | 3 | (a) ONLY (snapshot atomicity + boundaries + WAL-survival regression) |

Full per-AC → mechanism → test mapping in `ac-coverage.md`.

## Determinism (C6; DEVOPS A2/the overnight p95-flake class)

No scenario asserts a wall-clock p95. Mechanism (a) asserts a **deterministic
crash-at-ANY-point invariant**: after a kill at any instant the canonical
snapshot path holds the OLD or the NEW whole snapshot, never a torn one, so
`open()` always succeeds — timing-independent by construction (the robust
framing DEVOPS pinned). Mechanism (b) is fully deterministic and in-process
(presence/absence assertion). The child signals readiness on stdout
(`CRASH_TARGET_READY`) so the parent kills at a controlled moment; the
any-point invariant holds regardless.

## RED-ready scaffolding (Mandate 7)

Every scenario is `#[ignore = "RED until DELIVER: store-fsync-durability-v0
slice NN"]`. The production seams this feature adds do not exist yet, so the
tests reference RED scaffolds (panicking `__SCAFFOLD__` bodies, `// SCAFFOLD:
true` markers) so the suite COMPILES and is RED, not BROKEN. The full scaffold
inventory (what DELIVER replaces) is in `mandate-compliance.md`. The whole
workspace stays GREEN under `cargo test --workspace --all-targets --locked`
(every new test ignored; the panicking bins have no tests and never run) so
the pre-commit hook passes without `--no-verify`.

## Hexagonal boundary (Mandate 1; the brief's explicit instruction)

Entry is through each store's public driving ports only:
`open` / `open_with_fsync_backend` / `ingest` / `query` / `get_trace` /
`get_tier` / `dequeue` / `put` / `load_all`, plus the out-of-process
crash-target binary and its stderr. The brief's prohibition is honoured: NO
scenario enters through `wal_recovery::atomic_write_snapshot` or `fsync_probe`
directly — those are driven implementation details exercised indirectly
through the store seams. The shared crate's own gold-test (the behavioural
Earned-Trust layer) is a DELIVER unit/integration concern, not headline
acceptance.
