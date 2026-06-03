# Slice 04 — cinder crash-durable end-to-end

- **Story**: US-04
- **Store**: cinder (tiering/migration)
- **Priority**: P3 (internal-state store; large durable ledger)
- **Type**: vertical end-to-end slice (thin repeat of slice 01 pattern)

## Scope

1. **WAL fsync**: cinder `append_wal`
   (`crates/cinder/src/file_backed.rs:379-384`) gains `sync_all` per
   record (replaces `wal.flush()`-only at `:383`).
2. **Atomic snapshot**: cinder `snapshot` replaces `File::create` at
   `:207` with write-tmp → fsync-tmp → rename → fsync-parent-dir.
3. **Probe wiring**: cinder's composition root runs the fsync-honesty
   probe before opening the store for writes.
4. **Proving test**: out-of-process kill-9 (mid-write + mid-snapshot),
   asserting the acked migration is in cinder's recovered ledger and
   cinder opens.

## Observable outcome

After a simulated power loss, cinder `open()` succeeds and its recovered
ledger contains the acked migration.

## Acceptance criteria

See US-04 in `../discuss/user-stories.md` (4 UAT scenarios + 6 AC).

## Constraints

C1 (`TieringStore`), C2–C8 as per `user-stories.md` System Constraints.

## Dependencies

- US-01.
- ADR-0059 (cinder torn-tail recovery — landed; cinder doc already
  corrected under ADR-0059 §6).
