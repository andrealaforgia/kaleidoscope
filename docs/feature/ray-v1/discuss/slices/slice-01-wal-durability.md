# Slice 01 — WAL durability

Story: US-RV1-01. Targets KPI 1 (ingest p95 ≤ 2 ms) and the
pre-snapshot half of KPI 3 (100% durability).

## Outcome

The platform binary can embed `FileBackedTraceStore`, ingest
spans, lose the process, reopen at the same path, and query back
every span — by trace and by service — in ascending
`start_time_unix_nano` order with every field intact. This is the
durability floor: no compaction yet, just append-and-replay.

## In scope

- `FileBackedTraceStore::open(path, recorder)` — create or open
  the WAL, replay existing NDJSON records into both the `by_trace`
  and `by_service` indices, re-sort each bucket on
  `start_time_unix_nano`.
- `ingest(tenant, batch)` — append one `Ingest` WAL record per
  batch, then update both in-memory indices (mirrors v0
  dual-index population, including the empty-`service.name` rule:
  such spans land in `by_trace` only).
- `get_trace` / `query` / `query_with` against recovered state,
  preserving the v0 contract (half-open range, predicate AND
  range, `Vec<Span>` shape).
- Per-tenant, per-trace and per-service isolation across restart.
- Empty-batch ingest as a no-op.
- Corrupt WAL surfaces as `TraceStoreError::PersistenceFailed`
  naming the offending line.

## Out of scope (this slice)

- `snapshot()` and WAL truncation — Slice 02.
- Columnar layout, compression, retention, replication, fsync,
  atomic rename, file locking — all v2 (see outcome-kpis.md).

## Acceptance criteria

AC-1.1 through AC-1.10 in `user-stories.md` § US-RV1-01.

## Tests (written by DISTILL, not here)

`crates/ray/tests/v1_slice_01_wal_durability.rs`:

- restart round-trip via both `get_trace` and `query`, ascending
  order, byte-stable fields
- tenant + trace + service isolation across restart
- span with no `service.name` recovers into `by_trace` only
- recovered-state `query` / `query_with` honour v0 semantics
- empty batch is a no-op
- corrupt WAL → `PersistenceFailed`
- `ingest_p95_latency_under_two_milliseconds` (KPI 1)

## Notes

- On-disk format: NDJSON, one `Ingest` record per `SpanBatch`
  (per-batch). See wave-decisions D3.
- Dual-index replay: the WAL replay routes through a shared split
  routine that pushes each span into both `by_trace` and
  `by_service`, mirroring `InMemoryTraceStore::ingest`, so the
  live ingest path and recovery cannot drift. See wave-decisions
  D5. This is the one structural difference from Lumen's flat
  `extend`.
- The `MetricsRecorder` seam is carried forward verbatim from v0;
  the file-backed adapter records ingest / query exactly as the
  in-memory adapter does.
- Existing ray v0 tests are untouched.
