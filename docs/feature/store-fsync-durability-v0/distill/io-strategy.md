# I-O Strategy Decision — store-fsync-durability-v0 (DISTILL)

## Decision: Strategy C (real local I/O + real child process)

The acceptance suite drives **real** filesystem I/O on a real per-test tmp
directory and, for the snapshot-atomicity mechanism, a **real OS child
process**. No InMemory doubles for the storage substrate; no containers, no
network, no external services.

## Why C, not A/B/D

- This feature's entire point is on-disk crash durability. An InMemory double
  cannot exhibit a torn snapshot, a page-cache loss, or the
  rename-durability boundary — the very phenomena under test. InMemory would
  be Fixture Theater: green without proving anything.
- The two proving mechanisms (ADR-0060 §1) are inherently I/O-shaped:
  - **(a) snapshot atomicity** needs a torn snapshot to physically land on
    disk and survive a process death — only a real file on a real filesystem
    written by a real child process that is `SIGKILL`ed does this. `fork()`-in-
    tokio is rejected (UB; ADR-0049 §3, C5/D7); the crash is a separate child
    PROCESS via `std::process::Command` against a per-store `*-crash-target`
    `[[bin]]`.
  - **(b) WAL fsync** is proven in-process but still over real files: the
    `LyingFsyncBackend` injected through `open_with_fsync_backend` discards
    exactly the unsynced bytes a power cut would, then the store reopens with
    a real `RealFsyncBackend` and reads the real WAL back. This is the only
    mechanism that distinguishes `flush` from `sync_all` (a SIGKILL cannot —
    the page cache survives the kill).
- DEVOPS (`environments.yaml`) pins exactly this realism: "real local
  filesystem I/O on a per-test tmp dir (tempfile / TempDir), never a fixed
  path" for both mechanisms, and "out-of-process child … SIGKILL
  mid-snapshot" for (a).

## Determinism (C6; avoids the overnight p95-flake class)

- No wall-clock thresholds anywhere. Mechanism (a) asserts the crash-at-ANY-
  point invariant (canonical path whole-or-absent, never torn → `open()`
  always succeeds), which is timing-independent. Mechanism (b) is a
  presence/absence assertion, fully deterministic in-process.
- The child signals readiness on stdout (`CRASH_TARGET_READY`) so the parent
  kills at a controlled moment; the any-point invariant holds regardless of
  exactly when the kill lands.

## Per-test isolation

Each test writes under its own tmp root (`std::env::temp_dir()` + a label +
PID + nanos, the established v1 file-backed convention), cleaned up at the
end. Concurrent test runs and the two target environments (clean, ci) never
collide on a fixed path.

## Environment matrix (DEVOPS environments.yaml)

| Environment | Runs the suite via | Platform | SIGKILL faithful? |
|-------------|--------------------|----------|-------------------|
| clean (local pre-commit hook) | `cargo test --workspace --all-targets --locked` | macOS | yes (signal 9 uncatchable, immediate) |
| ci (GitHub Actions gate-1-test) | identical invocation | ubuntu-latest | yes (signal 9 uncatchable/unblockable) |

Both environments run the IDENTICAL command; both proving mechanisms execute
in each. No new CI job (every touched crate already owns a path-filtered
`gate-5-mutants-<crate>` job; DEVOPS A1).

## Walking-skeleton boundary proof (Dim 9)

Strategy C is declared in `wave-decisions.md` (DWD-1). The walking skeleton
(lumen slice 01) uses real files + a real child process for both mechanisms;
no `@in-memory` tag appears on any walking-skeleton scenario. Litmus test "if
I deleted the real adapter, would the WS still pass?" — NO: the WS opens a
real `FileBackedLogStore` against a real crashed pillar root and reads the
real WAL/snapshot back; there is no InMemory substitute in the path.

## Adapter integration coverage

Every store's real file-backed adapter is exercised with real I/O by at least
one `@real-io @adapter-integration` scenario (the snapshot-atomicity reopen +
the lying-substrate reopen both read real on-disk state back). The shared
`wal_recovery::atomic_write_snapshot` / `fsync_probe` helpers are NOT entered
directly (the brief's prohibition) — they are exercised indirectly through the
store seams; their own gold-test is a DELIVER unit/integration concern.
