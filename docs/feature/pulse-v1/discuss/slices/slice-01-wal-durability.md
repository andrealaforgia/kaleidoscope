# Slice 01 — WAL durability

Story: US-PV1-01. Targets KPI 1 (ingest p95 ≤ 2 ms) and the
pre-snapshot half of KPI 3 (100% durability).

## Outcome

The platform binary can embed `FileBackedMetricStore`, ingest
metric points, lose the process, reopen at the same path, and
query back every point in ascending `time_unix_nano` order with
its owning `Metric` metadata intact. This is the durability floor:
no compaction yet, just append-and-replay.

## In scope

- `FileBackedMetricStore::open(path, recorder)` — create or open
  the WAL, replay existing NDJSON records into per-`(tenant,
  metric_name)` state, re-sort each series on `time_unix_nano`.
- `ingest(tenant, batch)` — append one `Ingest` WAL record per
  batch, then update in-memory series (mirrors v0 sort-on-ingest).
- `query` / `query_with` against recovered state, preserving the
  v0 contract (half-open range, predicate AND range,
  `Vec<(Metric, MetricPoint)>` shape).
- Per-tenant and per-metric-name isolation across restart.
- Empty-batch ingest as a no-op.
- Corrupt WAL surfaces as `MetricStoreError::PersistenceFailed`
  naming the offending line.

## Out of scope (this slice)

- `snapshot()` and WAL truncation — Slice 02.
- Columnar layout, compression, retention, replication, fsync,
  atomic rename, file locking — all v2 (see outcome-kpis.md).

## Acceptance criteria

AC-1.1 through AC-1.9 in `user-stories.md` § US-PV1-01.

## Tests (written by DISTILL, not here)

`crates/pulse/tests/v1_slice_01_wal_durability.rs`:

- restart round-trip, ascending order, byte-stable fields
- tenant + metric-name isolation across restart
- recovered-state query / query_with honour v0 semantics
- empty batch is a no-op
- corrupt WAL → `PersistenceFailed`
- `ingest_p95_latency_under_two_milliseconds` (KPI 1)

## Notes

- On-disk format: NDJSON, one `Ingest` record per `MetricBatch`
  (per-batch, matching Lumen v1's natural unit). See wave-decisions
  D3.
- The `MetricsRecorder` seam is carried forward verbatim from v0;
  the file-backed adapter records ingest / query exactly as the
  in-memory adapter does.
- Existing pulse v0 tests are untouched.
