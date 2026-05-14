# Slice 01 — `MetricStore` trait + in-memory adapter (US-PU-01)

## Goal

Ship the trait the v1 disk-backed adapter will implement, plus
one in-memory adapter that proves the contract carries
OTLP-shaped metric points end-to-end.

## IN scope

- `MetricStore` trait with `ingest` + `query` methods
- `InMemoryMetricStore` adapter using
  `HashMap<(TenantId, MetricName), MetricSeries>`,
  sorted-on-ingest by `time_unix_nano`
- `Metric`, `MetricPoint`, `MetricKind`, `MetricName`,
  `MetricBatch`, `IngestReceipt`, `TimeRange`,
  `MetricStoreError` types
- Tenant + metric isolation by construction
- KPI 1 acceptance test
- Byte-stable field preservation

## OUT scope

- Predicates beyond metric name + time range (slice 02)
- Histogram / exponential histogram / summary (v1)
- PromQL (v1)
- Exemplars (v1)
- Disk durability (v1)
- Aperture retrofit (v1)

## Learning hypothesis

Disproves "an in-memory `HashMap<(TenantId, MetricName),
Vec<MetricPoint>>` is fast enough to sit on the OTLP metrics
hot path at v0 latencies". If KPI 1 fails on the in-memory
adapter, the v1 columnar substrate is the only path forward
and we re-slice.

## Effort

≤1 day. Same shape as Lumen slice 01.
