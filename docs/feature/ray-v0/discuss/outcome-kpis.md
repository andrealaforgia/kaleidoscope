# Ray v0 — outcome KPIs

## KPI 1 — Ingest latency

- **What**: `TraceStore::ingest(tenant, batch_of_100)` p95 ≤
  2 ms on the in-memory adapter.
- **Why**: Ray sits behind Aperture's exporter on the hot path.
- **Measured by**: `ray::tests::slice_01_walking_skeleton::
  ingest_p95_latency_under_two_milliseconds`. Warm up with 50
  ingests, time 1000 ingests of 100-span batches across 10
  trace_ids and 4 services (realistic OTLP batch shape).
- **Target**: 2 ms p95 over 1000 trials.
- **Why 2 ms and not 1 ms** (matching Lumen/Pulse): the v0
  adapter writes to **two** indices per span — `by_trace`
  (for `get_trace`) and `by_service` (for `query`/`query_with`).
  Each index is sorted-after-extend on every ingest, so Ray's
  per-batch ingest cost is roughly 2× Pulse's at the same
  accumulated state. The dual-index trade-off buys O(1)
  lookup on both axes — the alternative (single index +
  linear scan for the other axis) would shift this cost into
  every `get_trace` or `query` call. v1's columnar substrate
  (trace_id-partitioned Iceberg-on-Parquet) collapses this
  back into a single layout with proper secondary indices.

## KPI 2 — Query latency under predicate

- **What**: `TraceStore::query_with(tenant, service, range,
  predicate)` p95 ≤ 10 ms when scanning 10 000 ingested spans.
- **Why**: Riley's "show me errored spans for service X" must
  answer in under a second end-to-end.
- **Measured by**: `ray::tests::slice_02_structured_query::
  query_p95_latency_under_ten_milliseconds`. Ingest 10 000
  spans spanning multiple services / names / kinds / statuses.
  Warm up with 20 queries. Time 200 queries with a `span_name
  + status` predicate.
- **Target**: 10 ms p95 over 200 trials.

## Out-of-scope (deliberate)

- **Disk durability** (v1)
- **Sampling** (Sieve v1, separate feature)
- **Exemplars** (v1, cross-pillar)
- **TraceQL parsing** (v1)
- **Span events / link queries** (v1; v0 round-trips them
  byte-stable but does not let predicates dig into them)
- **trace_id-partitioned columnar storage** (v1)
- **Cross-tenant query**
