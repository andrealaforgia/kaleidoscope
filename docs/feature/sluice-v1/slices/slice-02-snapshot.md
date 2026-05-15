# Slice 02 — Snapshot compaction (US-SLV1-02)

## Goal

Bound recovery time. Explicit snapshot writes current
in-memory state and truncates the WAL.

## IN scope

- `snapshot()` method: write pending queues + in-flight
  ledger + `next_id` to a snapshot file, then truncate
  the WAL
- `open` reads snapshot first, then replays WAL
- Idempotence: snapshot with no intervening writes
  succeeds
- KPI 2

## OUT scope

- Atomic rename for snapshot (v2)
- Auto-trigger inside `enqueue` (v2)
- Compaction concurrency (v2)

## Learning hypothesis

Disproves "bounded recovery time can be added behind an
explicit `snapshot()` call across both Cinder and
Sluice without trait changes". Confirmation pins the
v0→v1 pattern as a generic capability, not a feature-
specific accident.

## Effort

≤1 day.
