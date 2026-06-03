# Slice 03 — strata crash-durable end-to-end

- **Story**: US-03
- **Store**: strata (profiles)
- **Priority**: P3 (internal-state store; outcome observed at reopen)
- **Type**: vertical end-to-end slice (thin repeat of slice 01 pattern)

## Scope

1. **WAL fsync**: strata `append_wal`
   (`crates/strata/src/file_backed.rs:329-334`) gains `sync_all` per
   record (replaces `wal.flush()`-only at `:333`).
2. **Atomic snapshot**: strata `snapshot` replaces `File::create` at
   `:170` with write-tmp → fsync-tmp → rename → fsync-parent-dir.
3. **Probe wiring**: strata's composition root runs the fsync-honesty
   probe before opening the store for writes.
4. **Proving test**: out-of-process kill-9 (mid-write + mid-snapshot),
   asserting the acked profile is in strata's recovered state and strata
   opens. strata has no HTTP read path — the outcome is observed at
   `open()` and via the store's in-process query after the child is
   killed.

## Observable outcome

After a simulated power loss, strata `open()` succeeds and its recovered
state contains the acked profile.

## Acceptance criteria

See US-03 in `../discuss/user-stories.md` (4 UAT scenarios + 6 AC).

## Constraints

C1 (`ProfileStore`), C2–C8 as per `user-stories.md` System Constraints.

## Dependencies

- US-01.
- **Flag**: strata is NOT in ADR-0059's torn-tail recovery slice
  (ADR-0059 §5 lists it out). DESIGN reconciles whether this slice also
  extends torn-tail recovery to strata (one `apply`-closure addition per
  ADR-0059 §5) or asserts the acked-prefix outcome without it.
