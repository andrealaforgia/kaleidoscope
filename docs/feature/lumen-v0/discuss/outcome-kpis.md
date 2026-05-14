# Lumen v0 — outcome KPIs

Two numeric, testable KPIs. Each has a measurement method that
is an acceptance test, not a vague "we'll watch it in prod".

## KPI 1 — Ingest latency

- **What**: `LogStore::ingest(tenant, batch_of_100)` p95 ≤ 1 ms
  on the in-memory adapter.
- **Why**: Lumen sits behind Aperture's exporter chain on the
  hot path. If ingest is slow, Aperture batches stall and OTLP
  back-pressures the producer.
- **Measured by**: `lumen::tests::slice_01_walking_skeleton::
  ingest_p95_latency_under_one_millisecond`. Warm up the
  adapter with 50 ingests, then time 1000 ingests of 100-record
  batches. Sort the samples. Read off index 950 as p95. Assert
  ≤ 1000 µs.
- **Target**: 1 ms p95 over 1000 trials.
- **Acceptable variance**: ±20% on CI hardware; the test
  asserts on the absolute target, not a relative one.

## KPI 2 — Query latency under predicate

- **What**: `LogStore::query_with(tenant, range, predicate)` p95
  ≤ 10 ms when scanning 10 000 ingested records on the
  in-memory adapter.
- **Why**: Riley's "grep yesterday's logs for correlation_id =
  …" question must answer in under 10 seconds end-to-end (per
  the Phase 3 exit criteria in the roadmap). The v0 adapter
  must already be fast enough that the bottleneck is the
  network and the UI, not the storage trait.
- **Measured by**: `lumen::tests::slice_02_structured_query::
  query_p95_latency_under_ten_milliseconds`. Ingest 10 000
  records spanning multiple services and severities. Warm up
  with 20 queries. Time 200 queries with a `service +
  min_severity` predicate. Read off p95.
- **Target**: 10 ms p95 over 200 trials.
- **Acceptable variance**: ±50% on CI hardware; the v0 adapter
  is intentionally unoptimised (linear scan). The KPI is a
  ceiling, not a stretch goal. v1's columnar substrate will
  tighten this.

## Out-of-scope (deliberate)

- **Disk durability**. v0 is in-memory only; a restart loses
  data. No KPI on persistence or recovery.
- **Cross-tenant query**. Lumen v0 is per-tenant by
  construction. No KPI on cross-tenant joins.
- **Full-text search**. Tantivy lands at v1. No KPI on body
  substring search.
- **Cardinality limits**. Bounded ingest with backpressure is a
  v1 concern; v0 happily ingests until the process runs out of
  memory.
