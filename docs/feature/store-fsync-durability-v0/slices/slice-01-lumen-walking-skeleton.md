# Slice 01 — lumen crash-durable end-to-end (WALKING SKELETON)

- **Story**: US-01
- **Store**: lumen (logs pillar)
- **Priority**: P1 (walking skeleton; derisks the fatal assumption)
- **Type**: vertical end-to-end slice (WAL fsync + atomic snapshot + proving test)

## Why this is the walking skeleton

It connects all five backbone activities (acknowledge → survive crash →
reopen → read back → trust) for ONE store, on the pillar with the most
observable read path (`GET /api/v1/logs`). It is the FIRST slice because
it validates the riskiest assumption: that an out-of-process kill-9
proving test can deterministically demonstrate crash-survival in CI
without `fork()`-in-tokio hazards. If that fails, every later slice is at
risk; we learn it here on one store.

## Scope

1. **WAL fsync**: lumen `append_wal`
   (`crates/lumen/src/file_backed.rs:277-283`) gains `sync_all` per record
   after the buffer flush, via the `FsyncBackend` seam proven in pulse
   (ADR-0049 §6). Replaces the current `wal.flush()`-only path at `:281`.
2. **Atomic snapshot**: lumen `snapshot`
   (`crates/lumen/src/file_backed.rs:141-175`) replaces `File::create`
   onto the canonical path (`:160`) with write-tmp → fsync-tmp → rename →
   fsync-parent-dir. This is the gap ADR-0049 left open even in pulse.
3. **Probe wiring**: lumen's composition root (the gateway, which opens
   `FileBackedLogStore`) runs the fsync-honesty probe before binding,
   reusing pulse's `fsync_probe` surface.
4. **The proving test (highest-value deliverable)**: an out-of-process
   kill-9 test — child process acks a write, `SIGKILL`, parent reopens,
   asserts acked write present via `GET /api/v1/logs` AND store opens;
   plus a mid-snapshot variant.

## Observable outcome

`curl http://localhost:8080/api/v1/logs?tenant=acme&from=...&to=...`
after a simulated power loss returns the exact record the exporter was
acked for before the crash; the collector started cleanly even though the
crash hit during a snapshot.

## Acceptance criteria

See US-01 in `../discuss/user-stories.md` (5 UAT scenarios + 7 AC). The
kill-9 proving test is AC #6 and is a FIRST-CLASS criterion, not an
afterthought.

## Constraints (DESIGN must honour)

- C1 (no `LogStore` trait change), C2 (durable = on disk), C3 (atomic
  snapshot), C4 (reuse `FsyncBackend` seam + refusal vocabulary), C5
  (out-of-process crash, not `fork()`-in-tokio), C6 (deterministic
  invariant, not timing), C7 (pairs with ADR-0059 torn-tail recovery,
  already covers lumen), C8 (no WAL format change). See `user-stories.md`
  System Constraints.

## Dependencies

- ADR-0049 (`FsyncBackend` seam — landed).
- ADR-0059 (lumen torn-tail recovery — landed; this slice produces the
  genuine torn tail it reads back).

## DESIGN open questions

- Lift the `fsync_probe` / `FsyncBackend` surface into a shared crate now,
  or keep per-pillar and reuse pulse's (ADR-0049 §8 deferred this to
  "successor slices will decide"; this is the first successor slice, so
  DESIGN decides here)?
- The exact out-of-process crash harness (test-only binary per store vs
  `std::process::Command` driving the gateway) — DESIGN/DEVOPS choice,
  constrained by C5 + C6.
