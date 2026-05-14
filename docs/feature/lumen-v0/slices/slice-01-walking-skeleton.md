# Slice 01 — `LogStore` trait + in-memory adapter (US-LU-01)

## Goal

Ship the trait that the v1 disk-backed adapter will implement,
plus one in-memory adapter that proves the contract carries
real OTLP-shaped log payloads end-to-end.

## IN scope

- `LogStore` trait with `ingest` + `query` methods
- `InMemoryLogStore` adapter using `HashMap<TenantId,
  Vec<LogRecord>>` keyed by tenant, sorted-on-ingest by
  `observed_time_unix_nano`
- `LogRecord`, `LogBatch`, `IngestReceipt`, `TimeRange`,
  `LogStoreError` types
- Tenant isolation by construction
- KPI 1 acceptance test: 1000 ingests of 100-record batches,
  assert p95 ≤ 1 ms
- Byte-stable field preservation across ingest + query

## OUT scope

- Predicates beyond time range (slice 02)
- Disk durability (v1)
- Aperture retrofit (v1)
- HTTP / gRPC query surface (v1)
- Tantivy / Parquet / Arrow / DataFusion (v1)

## Learning hypothesis

Disproves "an in-memory `Vec<LogRecord>` per tenant is fast
enough to sit on the OTLP hot path at v0 latencies". If KPI 1
fails on the in-memory adapter, the v1 columnar substrate is
the only path forward and we re-slice. If it passes, the trait
shape is validated and v1's work is well-bounded.

## Effort

≤1 day. Trait + in-memory adapter is mechanical; the time goes
into the acceptance test design and KPI 1 measurement.
