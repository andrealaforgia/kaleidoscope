# Slice 02 — Snapshot compaction (US-CV1-02)

## Goal

Bound recovery time. After a `snapshot()` call the WAL
contains only operations newer than the snapshot, and
`open` reads the snapshot first.

## IN scope

- `snapshot()` method: write current in-memory state to a
  snapshot file, then truncate the WAL
- `open` reads snapshot first, then replays WAL
- Idempotence: snapshot with no intervening writes is a
  cheap no-op
- KPI 2 (recovery time bounded after snapshot)

## OUT scope

- Atomic rename for snapshot (v2)
- Incremental snapshots (v2)
- Auto-triggered snapshot inside `place` (v2)
- Compaction concurrency / background thread (v2)

## Learning hypothesis

Disproves "bounded-recovery-time snapshots are cheap to
add behind an explicit `snapshot()` call". If KPI 2 fails,
the snapshot format itself needs to change (e.g. binary
instead of NDJSON).

## Effort

≤1 day.
