# Slice 05 — sluice crash-durable end-to-end

- **Story**: US-05
- **Store**: sluice (durable queue)
- **Priority**: P3 (internal-state store; fallible-apply nuance)
- **Type**: vertical end-to-end slice (thin repeat of slice 01 pattern)

## Scope

1. **WAL fsync**: sluice `append_wal`
   (`crates/sluice/src/file_backed.rs:387-392`) gains `sync_all` per
   record (replaces `wal.flush()`-only at `:391`).
2. **Atomic snapshot**: sluice `snapshot` replaces `File::create` at
   `:243` with write-tmp → fsync-tmp → rename → fsync-parent-dir.
3. **Probe wiring**: sluice's composition root runs the fsync-honesty
   probe before opening the store for writes.
4. **Proving test**: out-of-process kill-9 (mid-write + mid-snapshot +
   in-flight-item variant), asserting the acked enqueue is dequeuable in
   sluice's recovered queue and sluice opens.

## Observable outcome

After a simulated power loss, sluice `open()` succeeds and the acked
enqueue is present and dequeuable; an in-flight item is recovered to its
pre-crash state.

## Acceptance criteria

See US-05 in `../discuss/user-stories.md` (4 UAT scenarios + 6 AC).

## Constraints

C1 (sluice queue store), C2–C8 as per `user-stories.md` System Constraints.

## Dependencies

- US-01.
- **Flag**: sluice's replay applies records through a FALLIBLE
  `apply_record` (`crates/sluice/src/file_backed.rs:176`, `?`-propagated),
  unlike the infallible-apply stores. The fsync addition on `append_wal`
  is orthogonal to apply fallibility. If torn-tail recovery is extended to
  sluice here, DESIGN must provide the fallible-`apply` seam variant
  ADR-0059 §5 describes. Sequenced after the infallible-apply stores so
  the simpler shape is proven first.
