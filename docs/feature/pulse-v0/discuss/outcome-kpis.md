# Pulse v0 — outcome KPIs

Two numeric, testable KPIs. Each has a measurement method that
is an acceptance test, not a vague "we'll watch it in prod".

## KPI 1 — Ingest latency

- **What**: `MetricStore::ingest(tenant, batch_of_100)` p95 ≤ 1 ms
  on the in-memory adapter.
- **Why**: Pulse sits behind Aperture's exporter chain on the
  hot path. Slow ingest back-pressures the producer.
- **Measured by**: `pulse::tests::slice_01_walking_skeleton::
  ingest_p95_latency_under_one_millisecond`. Warm up with 50
  ingests, then time 1000 ingests of 100-point batches. Sort,
  read off index 950 as p95. Assert ≤ 1000 µs.
- **Target**: 1 ms p95 over 1000 trials.
- **Acceptable variance**: ±20% on CI hardware; the test
  asserts on the absolute target.

## KPI 2 — Query latency under predicate

- **What**: `MetricStore::query_with(tenant, metric, range,
  predicate)` p95 ≤ 10 ms when scanning 10 000 ingested points
  on the in-memory adapter.
- **Why**: Riley's "show me `http.server.duration` for service
  `checkout`" question must answer in under a second
  end-to-end. The v0 adapter must already be fast enough that
  the bottleneck is the network and the UI.
- **Measured by**: `pulse::tests::slice_02_structured_query::
  query_p95_latency_under_ten_milliseconds`. Ingest 10 000
  points spanning multiple metrics and services. Warm up with
  20 queries. Time 200 queries with a `service + label`
  predicate. Read off p95.
- **Target**: 10 ms p95 over 200 trials.
- **Acceptable variance**: ±50% on CI hardware; the v0 adapter
  is intentionally unoptimised (linear scan). The KPI is a
  ceiling, not a stretch goal.

## Out-of-scope (deliberate)

- **Disk durability**. v0 is in-memory only.
- **Histogram + exponential histogram + summary**. v0 ships
  gauge + sum (number points) only. Other metric shapes land
  at v1 alongside the columnar substrate.
- **PromQL parsing**. v1.
- **Exemplars**. v1 (linking Pulse data points to Ray trace
  IDs, per Phase 5 in the roadmap).
- **Cardinality limits**. v1 (cardinality budget enforcement
  at Aperture).
- **Cross-tenant query**. Pulse v0 is per-tenant by
  construction.
