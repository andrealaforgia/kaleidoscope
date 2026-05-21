# Slice 01 — WAL durability

Story: US-SV1-01. Targets KPI 1 (ingest p95 ≤ 8 ms) and the
pre-snapshot half of KPI 3 (100% durability).

## Outcome

The platform binary can embed `FileBackedProfileStore`, ingest
profiles, lose the process, reopen at the same path, and query
back every profile — by service and range — in ascending
`time_unix_nano` order with the full pprof sample payload intact.
This is the durability floor: no compaction yet, just
append-and-replay.

## In scope

- `FileBackedProfileStore::open(path, recorder)` — create or open
  the WAL, replay existing NDJSON records into the single
  `per_service` index, re-sort each bucket on `time_unix_nano`.
- `ingest(tenant, batch)` — append one `Ingest` WAL record per
  batch, then update the in-memory index (mirrors v0 per-service
  population, including the empty-`service.name` rule: such
  profiles are dropped from the index).
- `query` / `query_with` against recovered state, preserving the
  v0 contract (half-open range, predicate AND range,
  `Vec<Profile>` shape).
- Per-tenant and per-service isolation across restart.
- Touched-bucket sort on the live ingest path (only buckets the
  batch touched are re-sorted), inherited from the v0 adapter.
- Empty-batch ingest as a no-op.
- Corrupt WAL surfaces as `ProfileStoreError::PersistenceFailed`
  naming the offending line.

## Out of scope (this slice)

- `snapshot()` and WAL truncation — Slice 02.
- Columnar layout, compression, retention, replication, fsync,
  atomic rename, file locking, sample-payload encoding
  optimisation — all v2 (see outcome-kpis.md).

## Acceptance criteria

AC-1.1 through AC-1.10 in `user-stories.md` § US-SV1-01.

## Tests (written by DISTILL, not here)

`crates/strata/tests/v1_slice_01_wal_durability.rs`:

- restart round-trip via `query`, ascending order, byte-stable
  fields including the full sample payload
- tenant + service isolation across restart
- profile with no `service.name` dropped from the index
- recovered-state `query` / `query_with` honour v0 semantics
- empty batch is a no-op
- corrupt WAL → `PersistenceFailed`
- `ingest_p95_latency_under_eight_milliseconds` (KPI 1)

## Notes

- On-disk format: NDJSON, one `Ingest` record per `ProfileBatch`
  (per-batch). See wave-decisions D3.
- Single-index replay: the WAL replay routes through a shared
  split routine that pushes each profile into the one `per_service`
  index and returns the touched buckets, mirroring
  `InMemoryProfileStore::ingest`, so the live ingest path and
  recovery cannot drift. See wave-decisions D5. This is simpler
  than Ray's dual-index replay.
- Touched-bucket sort from the first cut: the v0 adapter already
  tracks touched buckets and sorts only those (store.rs lines
  119-137); v1 inherits the discipline rather than learning it the
  hard way as Ray did. Recovery sorts all buckets once.
- The `MetricsRecorder` seam is carried forward verbatim from v0;
  the file-backed adapter records ingest / query exactly as the
  in-memory adapter does.
- The v0 profile types gain serde derives (D6); none derive serde
  today.
- Existing strata v0 tests are untouched.
