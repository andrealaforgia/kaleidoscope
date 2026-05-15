# Slice 01 — `FileBackedLogStore` + WAL replay (US-LV1-01)

WAL durability for log batch ingest. Recovery preserves the
v0 query-ordering invariant by re-sorting every tenant
bucket after replay.

## IN scope

- `FileBackedLogStore::open(path, recorder)` constructor
- Per-batch NDJSON WAL records
- Recovery re-sorts on observed_time
- `LogStoreError::PersistenceFailed { reason }` new
  variant
- KPI 1

## OUT scope

- Snapshot (slice 02), fsync, file locking — v2.

## Learning hypothesis

Disproves "the v0→v1 carry-forward depends on the v0 trait
having an already-populated error enum". Lumen's v0
`LogStoreError` was empty; v1 grows it from zero variants
to one. If that transition needs more than the additive
change Cinder/Sluice managed, the methodology has a
hidden cost.

## Effort

≤1 day.
