# Slice 01 — `FileBackedTieringStore` + WAL replay (US-CV1-01)

## Goal

Ship the simplest possible WAL-backed adapter that
implements the v0 `TieringStore` trait. Prove that
place + migrate persist and recover on restart.

## IN scope

- `FileBackedTieringStore` struct
- `open(path)` constructor: open or create the WAL,
  replay existing records into in-memory state
- `place` + `migrate` append NDJSON records
- `evaluate_at` works against in-memory state (no extra
  WAL records — replay reproduces state)
- `MigrateError::PersistenceFailed { reason: String }`
  new variant
- WAL record types: `Place`, `Migrate`
- KPI 1

## OUT scope

- Snapshot (slice 02)
- fsync (v2)
- File locking (v2)
- Atomic rename (v2)

## Learning hypothesis

Disproves "the v0 `TieringStore` trait shape carries
forward to a durable adapter without modification". If the
trait needs to change shape to accommodate I/O, the v0
work has to redo itself. If KPI 1 fails, NDJSON WAL is
too slow and we need a binary format earlier than v2.

## Effort

≤1 day.
