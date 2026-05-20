# Slice 02 — snapshot compaction

Story: US-RV1-02. Targets KPI 2 (recovery p95 ≤ 2.5 s) and the
post-snapshot half of KPI 3 (snapshot + WAL equals pure WAL).

## Outcome

The platform binary can call `snapshot()` on a cadence so that
recovery stays bounded no matter how long the process has been
ingesting. Recovery loads the snapshot first, then replays the
tail WAL on top, yielding exactly the same spans a pure-WAL
recovery would, across both indices.

## In scope

- `snapshot()` — write the full dual-index state to the snapshot
  file, flush, then truncate the WAL.
- Recovery order in `open` — load snapshot first, replay remaining
  WAL on top, rebuild both indices, re-sort each bucket.
- Idempotent snapshot — a second `snapshot()` with no intervening
  ingest produces no duplication.
- Parallel-store equality: snapshot + tail WAL recovery matches a
  store that only ever used the WAL, for both `get_trace` and
  `query`.

## Out of scope (this slice)

- Auto-triggered compaction (snapshot inside `ingest` when WAL
  exceeds a threshold) — v2.
- Columnar / Parquet snapshot format — v2.
- Trace-aware compaction (dedup, late-span stitching) — v2.
- Atomic rename, fsync, file locking — v2 (see outcome-kpis.md).

## Acceptance criteria

AC-2.1 through AC-2.4 in `user-stories.md` § US-RV1-02.

## Tests (written by DISTILL, not here)

`crates/ray/tests/v1_slice_02_snapshot.rs`:

- snapshot then ingest then reopen → all spans present in both
  indices
- snapshot + WAL recovery equals pure-WAL recovery (parallel store)
- idempotent snapshot (no duplication)
- `recovery_p95_latency_under_two_and_a_half_seconds` (KPI 2)

## Notes

- Depends on Slice 01's `open` / replay path existing.
- Snapshot is an explicit call (no auto-compaction), matching
  Cinder v1, Sluice v1, Lumen v1 and Pulse v1.
- The snapshot stores spans once per tenant per trace; the
  `by_service` index is rebuilt from those spans on recovery
  rather than duplicated on disk (a span carries its own
  `service.name`), so the snapshot file does not pay the v0
  in-memory 2× cost. See wave-decisions D6. The 2.5 s recovery
  budget is the debug-mode JSON parse + dual-index rebuild cost on
  CI hardware — see outcome-kpis.md KPI 2.
