# Slice 02 — snapshot compaction

Story: US-PV1-02. Targets KPI 2 (recovery p95 ≤ 2.5 s) and the
post-snapshot half of KPI 3 (snapshot + WAL equals pure WAL).

## Outcome

The platform binary can call `snapshot()` on a cadence so that
recovery stays bounded no matter how long the process has been
ingesting. Recovery loads the snapshot first, then replays the
tail WAL on top, yielding exactly the same points a pure-WAL
recovery would.

## In scope

- `snapshot()` — write the full per-`(tenant, metric_name)` state
  to the snapshot file, flush, then truncate the WAL.
- Recovery order in `open` — load snapshot first, replay remaining
  WAL on top, re-sort each series.
- Idempotent snapshot — a second `snapshot()` with no intervening
  ingest produces no duplication.
- Parallel-store equality: snapshot + tail WAL recovery matches a
  store that only ever used the WAL.

## Out of scope (this slice)

- Auto-triggered compaction (snapshot inside `ingest` when WAL
  exceeds a threshold) — v2.
- Columnar / Parquet snapshot format — v2.
- Atomic rename, fsync, file locking — v2 (see outcome-kpis.md).

## Acceptance criteria

AC-2.1 through AC-2.4 in `user-stories.md` § US-PV1-02.

## Tests (written by DISTILL, not here)

`crates/pulse/tests/v1_slice_02_snapshot.rs`:

- snapshot then ingest then reopen → all points present
- snapshot + WAL recovery equals pure-WAL recovery (parallel store)
- idempotent snapshot (no duplication)
- `recovery_p95_latency_under_two_and_a_half_seconds` (KPI 2)

## Notes

- Depends on Slice 01's `open` / replay path existing.
- Snapshot is an explicit call (no auto-compaction), matching
  Cinder v1, Sluice v1 and Lumen v1.
- JSON snapshot of `Vec<SeriesBucket>` keyed by `(tenant,
  metric_name)`. The 2.5 s recovery budget is the debug-mode
  NDJSON / JSON parse cost on CI hardware — see wave-decisions D6
  and outcome-kpis.md KPI 2.
