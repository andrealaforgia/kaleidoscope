# Cinder v1 — outcome KPIs

## KPI 1 — Write latency

- **What**: `place` p95 ≤ 200 µs on
  `FileBackedTieringStore`.
- **Why**: place sits on every storage-engine ingest
  path that consults Cinder. The WAL flush must be cheap.
  v2's S3 / Iceberg substrate will be slower per op but
  amortise via batching.
- **Measured by**: `cinder::tests::slice_01_wal_durability::
  place_p95_latency_under_two_hundred_microseconds`. Open
  a fresh WAL in a tempdir, warm up with 100 places, time
  1 000 places, read off p95.
- **Target**: 200 µs p95 over 1 000 trials.

## KPI 2 — Recovery time

- **What**: `open(path)` p95 ≤ 1 s when recovering
  10 000 placed items from snapshot + WAL on the debug-
  built `FileBackedTieringStore`.
- **Why**: recovery sits on the operator-binary startup
  path; bounded recovery time matters for operational
  responsiveness. 1 s for a 10 000-item recovery means
  the operator binary can boot within a few seconds even
  with a fully-loaded tier table.
- **Why 1 s and not 50 ms** (initial guess): NDJSON
  parsing of a 10 000-entry snapshot in debug mode hits
  ~550 ms end-to-end, dominated by `serde_json` token
  cost on a `Vec<SnapshotEntry>`. Release mode is
  several times faster; v2's binary snapshot format
  (Iceberg manifests + Parquet) will obliterate this
  number. Same honesty move as Ray's KPI 1 (1 ms → 2 ms
  for the dual index) and Aegis's catalogue-load (10 ms
  → 50 ms once `toml` parse was measured). The KPI
  describes the system that ships, not the system the
  architect imagines.
- **Measured by**: `cinder::tests::v1_slice_02_snapshot::
  recovery_p95_latency_under_one_second`. Place 10 000
  items, call `snapshot()`, place 100 more, drop the
  store. Time 20 reopens, read off p95.
- **Target**: 1 s p95 over 20 trials in debug build.

## Out-of-scope (deliberate)

- **fsync semantics**. v1 uses `BufWriter::flush`.
  Recovery from `kill -9` between flush and fsync is
  v2 work.
- **Atomic rename for snapshot**. v1 writes the snapshot
  in-place. Crash during snapshot leaves an
  inconsistent snapshot file; v2 adds the
  write-temp-then-rename pattern.
- **File locking**. v1 assumes one process per WAL path.
  v2 adds `fcntl` advisory locking.
- **Compaction inside `place`**. v1's `snapshot()` is an
  explicit call. v2 may auto-trigger when WAL exceeds a
  threshold.
- **S3 / OpenDAL / Iceberg substrate**. v2.
