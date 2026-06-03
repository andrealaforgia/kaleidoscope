# Slice 02 — ray crash-durable end-to-end

- **Story**: US-02
- **Store**: ray (traces pillar)
- **Priority**: P2 (second-most-observable read path: `GET /api/v1/traces`)
- **Type**: vertical end-to-end slice (thin repeat of slice 01 pattern)

## Scope

1. **WAL fsync**: ray `append_wal`
   (`crates/ray/src/file_backed.rs:388-393`) gains `sync_all` per record
   (replaces `wal.flush()`-only at `:392`).
2. **Atomic snapshot**: ray `snapshot` replaces `File::create` at `:171`
   with write-tmp → fsync-tmp → rename → fsync-parent-dir.
3. **Probe wiring**: ray's composition root runs the fsync-honesty probe
   before binding.
4. **Proving test**: out-of-process kill-9 (mid-write + mid-snapshot),
   asserting the acked span is returned by `GET /api/v1/traces` after
   reopen and ray opens.

## Observable outcome

`curl http://localhost:8080/api/v1/traces?trace_id=4bf92f` after a
simulated power loss returns the acked span; ray opened cleanly.

## Acceptance criteria

See US-02 in `../discuss/user-stories.md` (4 UAT scenarios + 6 AC).

## Constraints

C1 (`TraceStore`), C2–C8 as per `user-stories.md` System Constraints.

## Dependencies

- US-01 (proves the crash mechanism and the pattern).
- ADR-0059 (ray torn-tail recovery — landed).
