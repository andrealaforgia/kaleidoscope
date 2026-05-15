# Lumen v1 — user stories

Third v0→v1 carry-forward in the platform plane. After
Cinder v1 and Sluice v1, this is the slice that turns the
v0→v1 contract from "twice is a tradition" into
"settled property of the methodology".

## US-LV1-01 — WAL durability for log ingest

### Elevator Pitch

- **Before**: Lumen v0 holds every ingested log record in
  memory. A process crash loses every record.
- **After**: run `cargo test -p lumen --test v1_slice_01_wal_durability`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test ingests batches across tenants, drops
  the store, opens a new store at the same path, and
  asserts every record round-trips byte-stable in
  observed-time order.
- **Decision enabled**: Sasha can run Lumen in a
  long-lived process and survive restarts.

### Acceptance criteria

- AC-1.1 — `FileBackedLogStore::open(path, recorder)`
  opens or creates the WAL and replays existing records.
- AC-1.2 — `ingest(tenant, batch)` appends an `Ingest`
  record and updates in-memory state.
- AC-1.3 — A fresh `open` on the same path after drop
  recovers every prior record.
- AC-1.4 — Observed-time ordering is preserved across
  restart (records re-sorted on recovery).
- AC-1.5 — Byte-stable round-trip for every `LogRecord`
  field including `trace_id`, `span_id`, attributes,
  resource attributes.
- AC-1.6 — Tenant isolation preserved across restart.
- AC-1.7 — `query` and `query_with` work against the
  recovered state with the v0 semantics.
- AC-1.8 — Corrupted WAL surfaces as
  `LogStoreError::PersistenceFailed`.
- AC-1.9 — Empty batch ingest is a no-op (no WAL write,
  no state change).

### KPI anchor

- KPI 1 (Ingest latency): `ingest(batch_of_100)` p95 ≤
  1.5 ms on the file-backed adapter. (1.5 ms not 500 µs
  because the three v1 costs — batch clone for WAL,
  JSON encode of 100 records, BufWriter flush — settle
  around 1.1 ms in debug mode. See `outcome-kpis.md` §
  KPI 1 for the honesty rationale.)

## US-LV1-02 — Snapshot compaction

### Elevator Pitch

- **Before**: WAL grows linearly with ingest. Recovery
  time grows linearly.
- **After**: run `cargo test -p lumen --test v1_slice_02_snapshot`
  → sees `test result: ok. N passed; 0 failed`.
- **Decision enabled**: Sasha sets a snapshot cadence in
  the operator binary.

### Acceptance criteria

- AC-2.1 — `snapshot()` writes current state and
  truncates WAL.
- AC-2.2 — Recovery loads snapshot first then replays
  remaining WAL.
- AC-2.3 — Snapshot+WAL recovery matches pure-WAL
  recovery (parallel store comparison).
- AC-2.4 — Snapshot is idempotent.

### KPI anchor

- KPI 2 (Recovery time): `open` p95 ≤ 1 s when recovering
  10 000 records from snapshot + WAL in debug build.
  Same calibration philosophy as Cinder v1 (NDJSON parse
  cost in debug dominates).
