# Slice 07 — pulse snapshot atomicity (snapshot-only)

- **Story**: US-07
- **Store**: pulse (metrics)
- **Priority**: P3 (narrow mid-snapshot window; low effort, seam owned)
- **Type**: vertical slice, SNAPSHOT-ONLY (WAL already done under ADR-0049)

## Why snapshot-only

pulse's WAL is already crash-durable under ADR-0049
(`sync_all`-per-record at `crates/pulse/src/file_backed.rs:511`). The one
durability residue ADR-0049 left open in its own pillar is the
snapshot: pulse still writes the snapshot with `File::create` straight
onto the canonical path (`:257`). A crash mid-snapshot tears that file;
the next `open()` fails to parse it and the whole metric store refuses to
open — total loss — because pulse's per-file `sync_all` cannot save a
file that is itself torn. This slice closes that gap.

## Scope

1. **Atomic snapshot**: pulse `snapshot`
   (`crates/pulse/src/file_backed.rs:235-284`) replaces `File::create` at
   `:257` with write-tmp → fsync-tmp → rename → fsync-parent-dir, reusing
   pulse's existing `FsyncBackend.fsync_file` / `fsync_dir` (already wired
   from ADR-0049). The existing parent-dir fsyncs around the WAL truncate
   (`:269`, `:281`) stay; the new work is making the snapshot WRITE itself
   atomic (temp + rename) rather than in-place.
2. **NO WAL change**: pulse's `append_wal` is unchanged.
3. **Proving test**: out-of-process kill-9 mid-snapshot test, asserting
   pulse `open()` succeeds and `GET /api/v1/metrics` serves the last
   consistent state.

## Observable outcome

After a simulated power loss during a snapshot, pulse `open()` succeeds
and `GET /api/v1/metrics?tenant=acme&metric=http_requests_total` serves
the acked series.

## Acceptance criteria

See US-07 in `../discuss/user-stories.md` (4 UAT scenarios + 6 AC).

## Constraints

C1 (`MetricStore`), C2 (durable), C3 (atomic snapshot), C5 (out-of-process
crash), C6 (deterministic). C4 reuse is already satisfied (pulse owns the
seam). See `user-stories.md` System Constraints.

## Dependencies

- US-01 (atomic-snapshot pattern proven there).
- ADR-0049 (pulse `FsyncBackend` seam + WAL fsync — landed; this slice
  reuses the seam and closes the snapshot gap ADR-0049 §5 left open).
