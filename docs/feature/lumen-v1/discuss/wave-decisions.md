# Lumen v1 — DISCUSS wave decisions

## Key decisions

- **[D1] Third v0→v1 carry-forward**. After Cinder v1
  and Sluice v1. The pattern is now a settled property
  of the methodology.

- **[D2] Same crate, new adapter**. `FileBackedLogStore`
  joins `InMemoryLogStore` behind the same `LogStore`
  trait.

- **[D3] NDJSON WAL with per-batch records**. Unlike
  Sluice v1 (one record per op) and Cinder v1 (one record
  per state change), Lumen's natural unit is the batch.
  One `Ingest` record per `LogBatch`. Reduces WAL size
  and recovery overhead at the cost of slightly larger
  individual lines.

- **[D4] LogStoreError grows from empty to one variant**.
  The v0 `enum LogStoreError {}` had a `match *self {}`
  Display impl using the never-type idiom. v1 adds
  `PersistenceFailed { reason: String }` and rewrites
  Display to match it. v0 callers that pattern-matched
  on the empty enum need an explicit arm.

- **[D5] All v0 record types derive Serialize +
  Deserialize**. `LogRecord`, `SeverityNumber`,
  `BTreeMap<String, String>` already serde-friendly.
  `Option<[u8; 16]>` and `Option<[u8; 8]>` serialise as
  JSON arrays of integers — verbose but correct.

- **[D6] Recovery re-sorts every tenant bucket**.
  Snapshot may have un-sorted records (depending on v2's
  layout); WAL ingest records carry batches that may be
  out-of-order. Recovery re-sorts every bucket once at
  the end, before exposing the state.

- **[D7] `BufWriter::flush` semantics**, same as Cinder
  v1 and Sluice v1.

- **[D8] Explicit `snapshot()` call**, same as the
  other v1s.

- **[D9] AGPL-3.0-or-later**.

- **[D10] Two carpaccio slices in one implementation
  commit**.

## Slicing

- **Slice 01 — WAL durability** (US-LV1-01)
- **Slice 02 — snapshot compaction** (US-LV1-02)

## DESIGN handoff

DESIGN collapses into the implementation commit.
