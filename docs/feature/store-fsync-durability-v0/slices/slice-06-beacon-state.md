# Slice 06 — beacon rule-state store crash-durable end-to-end

- **Story**: US-06
- **Store**: beacon `state_store` (rule-state)
- **Priority**: P3 (internal-state store; alerting rule state)
- **Type**: vertical end-to-end slice (thin repeat of slice 01 pattern)

## Scope

1. **WAL fsync**: beacon state_store `append_wal`
   (`crates/beacon/src/state_store.rs:330-335`) gains `sync_all` per
   record (replaces `wal.flush()`-only at `:334`).
2. **Atomic snapshot**: beacon state_store `snapshot` replaces
   `File::create` at `:259` with write-tmp → fsync-tmp → rename →
   fsync-parent-dir.
3. **Probe wiring**: the rule-state store's composition root runs the
   fsync-honesty probe before opening the store for writes.
4. **Proving test**: out-of-process kill-9 (mid-write + mid-snapshot),
   asserting the acked rule-state transition is in the recovered state and
   the store opens.

## Observable outcome

After a simulated power loss, the rule-state store `open()` succeeds and
the acked rule transition (e.g. `r-payment-latency` → `firing`) is in the
recovered state.

## Acceptance criteria

See US-06 in `../discuss/user-stories.md` (4 UAT scenarios + 6 AC).

## Constraints

C1 (`RuleStateStore`), C2–C8 as per `user-stories.md` System Constraints.

## Dependencies

- US-01.
- ADR-0040 governs the rule-state store seam.
- **Flag**: beacon's rule-state store is NOT in ADR-0059's torn-tail
  recovery slice. DESIGN reconciles whether to extend torn-tail recovery
  here or assert the acked-prefix outcome without it.
