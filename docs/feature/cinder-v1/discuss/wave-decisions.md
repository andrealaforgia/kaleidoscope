# Cinder v1 — DISCUSS wave decisions

## Key decisions

- **[D1] First v1 anywhere in the platform plane.** Every
  prior crate ships at v0 with an in-memory adapter; the
  trait shape carry-forward claim has never been
  validated. Cinder v1 validates it.

- **[D2] Same crate, new adapter**. Not a new crate. The
  `cinder` crate now exposes two adapters
  (`InMemoryTieringStore`, `FileBackedTieringStore`)
  behind the same `TieringStore` trait. v0 acceptance
  tests stay green.

- **[D3] NDJSON WAL format**. One JSON record per line.
  Human-readable, easy to inspect, easy to debug. The
  format is line-oriented so a partial last-line write
  is detectable (and ignored on recovery — see slice
  02's crash-tolerance posture).

- **[D4] Operation log, not state diff**. The WAL
  records each `place` and `migrate` operation, not the
  resulting state. Recovery replays the operations.
  This matches every classical WAL design (PostgreSQL,
  RocksDB, MySQL binlog).

- **[D5] Trait error enum extends additively**.
  `MigrateError` gains
  `PersistenceFailed { reason: String }`. The reason is
  a stringified `std::io::Error` so the enum keeps
  `Clone + PartialEq + Eq`. Callers that previously
  pattern-matched exhaustively on
  `MigrateError::UnknownItem` need to add a
  `_ => …` arm; this is a known cost of the v0 trait not
  being `#[non_exhaustive]`.

- **[D6] `BufWriter::flush` semantics at v1**, not
  fsync. A graceful drop flushes; a `kill -9` may lose
  the last record. v2 adds proper fsync. The acceptance
  test pins recovery from graceful shutdown.

- **[D7] Explicit `snapshot()` call**. Not auto-
  triggered. The operator binary decides cadence.

- **[D8] Snapshot is full state, not incremental**. v1
  ships the simplest snapshot semantics. v2 may add
  incremental snapshots.

- **[D9] WAL truncation after snapshot**. After
  `snapshot()` succeeds, the WAL file is reopened in
  truncate mode. New ops append to the truncated WAL.

- **[D10] No file locking at v1**. One process per WAL
  path.

- **[D11] serde + serde_json workspace deps used**.
  Same dependencies the rest of the workspace already
  carries.

- **[D12] AGPL-3.0-or-later**.

- **[D13] Two carpaccio slices in one implementation
  commit** per established precedent.

## Slicing

- **Slice 01 — WAL durability** (US-CV1-01).
  `FileBackedTieringStore::open` + `place` + `migrate`
  + replay-on-restart. The simplest possible WAL
  semantics; no snapshot yet.
- **Slice 02 — snapshot compaction** (US-CV1-02).
  `snapshot()` writes current state + truncates WAL.
  `open` reads snapshot first, then replays remaining
  WAL.

## Constraints established

- v0 acceptance tests for `InMemoryTieringStore`
  continue to pass unchanged.
- The trait shape (`TieringStore`) does not change. Only
  the `MigrateError` enum extends additively.
- v1's WAL format is the input contract for v2's S3 /
  Iceberg substrate. v2 will read WALs left by v1.
- Cinder depends on `aegis`, `serde`, `serde_json`. No
  new third-party deps beyond workspace deps.

## DESIGN handoff

DESIGN collapses into the implementation commit.
