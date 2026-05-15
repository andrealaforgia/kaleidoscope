# Slice 01 — `FileBackedQueue` + WAL replay (US-SLV1-01)

## Goal

Ship the simplest WAL-backed queue adapter behind the
existing v0 `Queue` trait. Enqueue / dequeue / ack / nack
all persist and recover correctly.

## IN scope

- `FileBackedQueue` struct
- `open(path, cap, recorder)` constructor: open or create
  the WAL, replay records into in-memory state
- All four `Queue` methods append to WAL
- Hex-encoded payloads in WAL
- `MessageId` counter recovers from `max(id) + 1`
- `EnqueueError::PersistenceFailed { reason: String }`
  new variant
- KPI 1

## OUT scope

- Snapshot (slice 02)
- fsync (v2)
- File locking (v2)
- Binary WAL (v2)

## Learning hypothesis

Disproves "the v0→v1 carry-forward is Cinder-specific".
If Sluice v1 needs trait changes the Cinder claim does
not generalise. If KPI 1 fails, NDJSON WAL is too slow
for queue throughput and v2 needs to land sooner.

## Effort

≤1 day.
