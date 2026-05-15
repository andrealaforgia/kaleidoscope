# Cinder v1 — user stories

Two LeanUX user stories with mandatory Elevator Pitches.
This is the **first v1 feature anywhere in the platform
plane** — every prior crate ships at v0 with an in-memory
adapter. Cinder v1 introduces a file-backed adapter behind
the same v0 `TieringStore` trait, demonstrating that the
trait shape carries forward to a durable implementation
without retrofit.

The principal user is **Sasha, a platform engineer** who
shipped Cinder v0 in-memory and now needs to demonstrate
that tier metadata survives a process restart. The Phase 7
substrate (S3 + OpenDAL + Iceberg manifests) is still v2
work; v1 takes the most modest possible step: a local
append-only NDJSON WAL with optional snapshot compaction.
v1 proves the trait carries forward; v2 swaps the WAL for
proper Iceberg + object storage.

The secondary user is **Riley, an SRE** running a
disaster-recovery drill. Riley wants to assert that "I
kill the Cinder process at any point and the tier metadata
recovers correctly on restart". v1 ships that property
with a replay test.

System constraints (apply to every story):

1. Same crate (`cinder`). v1 ships a new adapter
   alongside `InMemoryTieringStore`, not a new crate.
2. **Same v0 trait**. `FileBackedTieringStore`
   implements the existing `TieringStore` trait verbatim.
   The v0 acceptance tests pass against the new adapter
   when run with appropriate constructors.
3. **NDJSON WAL**. Operations (place, migrate, snapshot)
   serialise as one JSON record per line. Human-readable,
   easy to inspect, easy to debug.
4. **Trait error enum extension**. The v0 `MigrateError`
   enum gains a new variant
   `PersistenceFailed { reason: String }` for I/O
   failures. Adding a variant is additive; v0 callers
   that pattern-match exhaustively get a compile warning
   only if they pattern-match without `_ =>`.
5. **Per-tenant + per-item isolation** preserved.
6. **No fsync at v1**. v2 adds proper fsync + atomic
   rename. v1 uses standard `BufWriter::flush` which is
   enough for graceful shutdown; a `kill -9` between
   flush and fsync loses the last record. The acceptance
   test pins recovery from graceful shutdown, not from
   `kill -9`.
7. **No concurrent writers at v1**. One
   `FileBackedTieringStore` instance per WAL path. v2
   adds file locking.
8. **AGPL-3.0-or-later**.

---

## US-CV1-01 — WAL durability: place + migrate survive restart

### Elevator Pitch

- **Before**: Sasha ships Cinder v0 in-memory. The
  `cinder/v0` story has a paragraph that says "v1
  persists tier metadata", but no v1 has shipped.
- **After**: run `cargo test -p cinder --test slice_01_wal_durability`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test creates a `FileBackedTieringStore` at
  a temp path, places several items, migrates some,
  drops the store, opens a new store at the same path,
  and asserts every tier + timestamp is restored.
- **Decision enabled**: Sasha removes the "v0 is
  in-memory only, restart loses data" qualifier from
  Cinder's story. The v0 trait shape has been validated
  to carry forward to a durable adapter.

### Acceptance criteria

- AC-1.1 — `FileBackedTieringStore::open(path)` opens
  or creates the WAL at `path` and replays any existing
  records into in-memory state.
- AC-1.2 — `place(tenant, item, tier, placed_at)`
  appends a Place record to the WAL and updates
  in-memory state.
- AC-1.3 — `migrate(tenant, item, to_tier,
  migrated_at)` appends a Migrate record and updates
  in-memory state. Unknown-item migrations return
  `MigrateError::UnknownItem` without touching the WAL.
- AC-1.4 — A fresh `FileBackedTieringStore::open` on
  the same path after `drop` recovers every prior
  placement and migration: `get_tier` returns the same
  values as before drop.
- AC-1.5 — Recovery preserves `placed_at` and
  `migrated_at` byte-stable across the WAL roundtrip.
- AC-1.6 — `evaluate_at(now, &policy)` works against the
  recovered state: ageing decisions are correct after
  restart.
- AC-1.7 — I/O failures (closed file, write error)
  surface as `MigrateError::PersistenceFailed { reason }`.
- AC-1.8 — Tenant isolation preserved across restart.

### KPI anchor

- KPI 1 (Write latency): `place` p95 ≤ 200 µs on the
  file-backed adapter. WAL writes flush to a buffered
  writer; the per-op latency must be tiny.

---

## US-CV1-02 — Snapshot compaction for bounded recovery time

### Elevator Pitch

- **Before**: the WAL grows unbounded. After a million
  place/migrate operations, recovery replays a million
  lines. Riley wants bounded recovery time.
- **After**: run `cargo test -p cinder --test slice_02_snapshot`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test places many items, calls `snapshot()`,
  places more items, restarts, and asserts that the WAL
  on disk has been truncated and the recovery state is
  complete.
- **Decision enabled**: Sasha sets a snapshot cadence in
  the operator binary (e.g. snapshot every 10 000 ops or
  every 10 minutes). v2's S3 / Iceberg substrate
  inherits the same snapshot semantics.

### Acceptance criteria

- AC-2.1 — `snapshot()` writes the current in-memory
  state to a snapshot file then truncates the WAL.
- AC-2.2 — `open(path)` after a snapshot loads the
  snapshot first then replays only the remaining WAL
  records.
- AC-2.3 — Recovery from snapshot + partial WAL produces
  the same in-memory state as recovery from full WAL.
- AC-2.4 — Snapshot is idempotent: calling `snapshot()`
  twice in a row (with no intervening writes) is a no-op
  on the second call.
- AC-2.5 — A snapshot error (I/O failure) returns
  `MigrateError::PersistenceFailed` and leaves the WAL
  intact (no partial state).

### KPI anchor

- KPI 2 (Recovery time): `open` p95 ≤ 1 s when
  recovering 10 000 placed items from snapshot + WAL in
  debug build. Recovery sits on the operator-binary
  startup path; bounded recovery means the operator
  binary boots within a few seconds even with a fully-
  loaded tier table. (1 s not 50 ms because NDJSON
  parsing of 10 000 entries in debug mode is dominated
  by `serde_json` token cost; v2's binary substrate will
  collapse this.)
