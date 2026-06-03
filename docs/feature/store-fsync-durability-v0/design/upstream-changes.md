# Upstream changes — store-fsync-durability-v0 (DESIGN → product owner)

- **Wave**: DESIGN (nWave)
- **Architect**: Morgan (`nw-solution-architect`)
- **Date**: 2026-06-04
- **Audience**: product owner (Luna, `nw-product-owner`) — a request to
  amend the DISCUSS acceptance criteria so DISTILL does not inherit a test
  that proves nothing.

## The one upstream change: split each per-store crash AC into TWO ACs

### What the DISCUSS ACs say today

Every story US-01..US-06 asserts BOTH durability halves through the SAME
mechanism — an out-of-process `SIGKILL` of the collector process, then
reopen. For example US-01 AC-1/AC-2:

> - A log record acked (`200 OK`) before a `SIGKILL` of the collector
>   process is present in `GET /api/v1/logs` after restart.
> - No acked log record is lost across a `SIGKILL`-then-reopen cycle.

and AC-3 (snapshot atomicity) uses the same `SIGKILL` mechanism.

### Why this conflation must be separated (verified reasoning, not opinion)

A `SIGKILL` / process-kill, AS A PROCESS KILL ON THE SAME HOST, **cannot
prove the WAL-fsync half**. `flush()` writes bytes into the kernel **page
cache**, and the page cache **survives the process dying** — the OS still
owns those bytes and writes them back to disk on its own schedule. So a
child killed mid-write and reopened by the parent on the same host will
STILL find the acked record, **even on the buggy `flush()`-only code**. The
`flush()` vs `sync_all()` distinction is only observable when the unsynced
data is actually DISCARDED, which a same-host process kill does not do.

**Consequence**: AC-1/AC-2 (and the equivalents in US-02..US-06), as
worded, would PASS on the un-fixed bug. They prove the read-back tolerates
the torn tail and that a graceful cache-writeback restart recovers — a real
regression guard — but they prove NOTHING about fsync durability. Shipping
them as the wal-fsync proof reproduces the very false-confidence finding
(`wave-decisions.md` §3) that motivated this feature, one level up.

The snapshot-atomicity AC (US-xx "store opens cleanly after a mid-snapshot
SIGKILL") is DIFFERENT: a torn snapshot file is a physical artefact that
lands on disk and survives the kill, so a real process-kill mid-snapshot IS
the right mechanism there.

### The requested amendment (per store)

Replace the single conflated crash AC with TWO acceptance criteria, each
named with its proving mechanism (DESIGN ADR-0060 Decision 1):

| New AC | Property | Proving mechanism | KPI |
|--------|----------|-------------------|-----|
| **AC-snapshot-atomicity** | `open()` succeeds after a crash mid-snapshot; no torn file at the canonical path blocks the open | **Real out-of-process process-kill mid-snapshot**, then reopen (the SIGKILL test, kept and correctly scoped) | K3 |
| **AC-wal-fsync** | An acked write is on stable storage, not merely in the page cache | **In-suite lying-substrate probe**: inject a `LyingFsyncBackend` (`no_op`/`truncating`) through the store's `open_with_fsync_backend` seam; ack a write; the lying substrate discards the unsynced bytes; reopen; the acked write is ABSENT on `flush()`-only and PRESENT once `sync_all` is wired | K2, K4 |

The existing `SIGKILL`-then-reopen + read-API assertion is KEPT but
**re-labelled** as a recovery/read-back regression guard (it pairs with
ADR-0059 torn-tail recovery), NOT as the wal-fsync proof.

US-07 (pulse) needs NO wal-fsync AC (its WAL is already crash-durable under
ADR-0049); it keeps only the snapshot-atomicity AC. Its proving mechanism is
(a) only.

### Why this is a product-owner change, not a silent DESIGN override

The acceptance criteria are SSOT owned by DISCUSS. DESIGN cannot rewrite
them unilaterally; it flags the defect and proposes the corrected wording.
The "For Acceptance Designer" note in `brief.md` already names, per AC,
WHICH mechanism DISTILL must use, so DISTILL is unblocked even before the
SSOT edit lands — but the SSOT should be corrected so the two ACs are
distinct of record and the KPI mapping (K2 vs K3) is unambiguous.

### Recommended action

Amend US-01..US-06 in `discuss/user-stories.md` to split the conflated crash
AC into `AC-snapshot-atomicity` (process-kill) and `AC-wal-fsync`
(lying-substrate probe) per the table above; re-label the kept
`SIGKILL`+read-API assertion as a recovery regression guard. No change to
US-07 beyond confirming it carries only the snapshot-atomicity AC. No change
to the System Constraints (C1..C8 already pin out-of-process, deterministic,
reuse-the-seam), the KPIs (K2/K3/K4 already distinguish the two halves), or
the story map.

This is the only upstream change DESIGN requests.
