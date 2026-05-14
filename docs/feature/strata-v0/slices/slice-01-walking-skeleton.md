# Slice 01 — `ProfileStore` trait + in-memory adapter (US-ST-01)

## Goal

Ship the trait the v1 disk-backed adapter will implement, plus
one in-memory adapter that proves the contract carries
pprof-shaped profiles end-to-end.

## IN scope

- `ProfileStore` trait with `ingest` + `query`
- `InMemoryProfileStore` adapter using
  `HashMap<(TenantId, ServiceName), Vec<Profile>>` sorted by
  `time_unix_nano`
- `Profile`, `Sample`, `Location`, `Function`, `Mapping`,
  `SampleType`, `ValueType`, `ProfileBatch`,
  `IngestReceipt`, `TimeRange`, `ProfileStoreError`,
  `ServiceName` types
- Tenant isolation
- Byte-stable field round-trip including string table +
  locations + functions + mappings
- KPI 1

## OUT scope

- Predicates (slice 02)
- Symbolisation (v1)
- Flame graph rendering (Prism v1)
- Disk durability (v1)

## Learning hypothesis

Disproves "a single `HashMap` index keyed by `(tenant,
service)` is fast enough at v0 latencies despite the heavy
per-profile clone cost". If KPI 1 fails, we either drop to a
slimmer profile shape at the trait boundary, or push directly
into the columnar substrate.

## Effort

≤1 day.
