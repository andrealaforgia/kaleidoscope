# Slice 02 — snapshot compaction

Story: US-SV1-02. Targets KPI 2 (recovery p95 ≤ 2.5 s) and the
post-snapshot half of KPI 3 (snapshot + WAL equals pure WAL).

## Outcome

The platform binary can call `snapshot()` on a cadence so that
recovery stays bounded no matter how long the process has been
ingesting. This matters more for Strata than for the lighter
pillars: the heavy profile payload grows the WAL fast, so
unbounded WAL replay would dominate startup. Recovery loads the
snapshot first, then replays the tail WAL on top, yielding exactly
the same profiles a pure-WAL recovery would.

## In scope

- `snapshot()` — write the full per-service index state to the
  snapshot file, flush, then truncate the WAL.
- Recovery order in `open` — load snapshot first, replay remaining
  WAL on top, rebuild the index, re-sort each bucket.
- Idempotent snapshot — a second `snapshot()` with no intervening
  ingest produces no duplication.
- Parallel-store equality: snapshot + tail WAL recovery matches a
  store that only ever used the WAL, for `query`.

## Out of scope (this slice)

- Auto-triggered compaction (snapshot inside `ingest` when WAL
  exceeds a threshold) — v2.
- Columnar / Parquet snapshot format — v2.
- Compression of the snapshot payload — v2.
- Atomic rename, fsync, file locking — v2 (see outcome-kpis.md).

## Acceptance criteria

AC-2.1 through AC-2.4 in `user-stories.md` § US-SV1-02.

## Tests (written by DISTILL, not here)

`crates/strata/tests/v1_slice_02_snapshot.rs`:

- snapshot then ingest then reopen → all profiles present
- snapshot + WAL recovery equals pure-WAL recovery (parallel store)
- idempotent snapshot (no duplication)
- `recovery_p95_latency_under_two_and_a_half_seconds` (KPI 2)

## Notes

- Depends on Slice 01's `open` / replay path existing.
- Snapshot is an explicit call (no auto-compaction), matching the
  other five v1 pillars.
- The single per-service index serialises directly into the
  snapshot — no derived second index to rebuild (unlike Ray's
  by_service derivation), so the snapshot shape is the simplest of
  the v1 set. See wave-decisions D6. The 2.5 s recovery budget is
  the debug-mode JSON parse cost of 2 000 heavy profiles on CI
  hardware — see outcome-kpis.md KPI 2.
