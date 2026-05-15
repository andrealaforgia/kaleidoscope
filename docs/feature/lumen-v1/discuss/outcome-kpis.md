# Lumen v1 — outcome KPIs

## KPI 1 — Ingest latency

- **What**: `ingest(batch_of_100)` p95 ≤ 1.5 ms on
  `FileBackedLogStore`.
- **Why**: v1 pays three costs the v0 in-memory adapter
  doesn't: clone the batch for WAL serialisation,
  JSON-encode 100 records into a single line, flush the
  BufWriter. Plus the same sort-after-extend cost as v0.
  The p95 settles around 1.1 ms in debug mode; 1.5 ms
  gives the test honest headroom.
- **Why 1.5 ms and not 500 µs** (initial guess): the
  same pattern as Ray KPI 1 (1 ms → 2 ms for the dual
  index), Aegis KPI 2 (10 ms → 50 ms for toml parse),
  Cinder v1 KPI 2 (50 ms → 1 s for NDJSON parse). v1
  carry-forward has a fourth honesty moment; the KPI
  describes the shipping system.
- **Measured by**: `lumen::tests::v1_slice_01_wal_durability::
  ingest_p95_latency_under_one_point_five_milliseconds`.
- **Target**: 1.5 ms p95 over 1 000 trials.

## KPI 2 — Recovery time

- **What**: `open(path)` p95 ≤ 1 s when recovering
  10 000 records from snapshot + WAL in debug build.
- **Why**: same as Cinder v1 — recovery sits on the
  operator-binary startup path. NDJSON parsing in debug
  is the bottleneck.
- **Measured by**: `lumen::tests::v1_slice_02_snapshot::
  recovery_p95_latency_under_one_second`.
- **Target**: 1 s p95 over 20 trials in debug build.

## Out-of-scope (deliberate)

- **fsync**, **atomic rename**, **file locking** — v2.
- **Arrow / Parquet / DataFusion / Tantivy / RocksDB
  substrate** — still v2.
- **Per-record WAL** — v1 ships per-batch records.
