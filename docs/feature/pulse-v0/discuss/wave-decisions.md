# Pulse v0 — DISCUSS wave decisions

## Key decisions

- **[D1] Port + one adapter at v0**, mirroring Sluice and
  Lumen. The Arrow + Parquet + DataFusion + Prometheus-TSDB-
  format substrate that the architecture roadmap names for
  Phase 4 lands at v1 behind the same `MetricStore` trait.
  Pulse v0 ships one in-memory adapter and one acceptance
  suite that pins the trait.

- **[D2] OTLP-shaped types at the trait boundary**. The trait
  takes `Metric` and `MetricPoint` types whose field set
  matches `opentelemetry-proto::metrics::v1` exactly. No
  Pulse-specific projections. v1 disk-backed adapter's job is
  mechanical.

- **[D3] Gauge + sum (number points) only at v0**. Histogram,
  exponential histogram, and summary need a different point
  shape and a different query semantics (bucket interpolation,
  rate calculation). They land at v1 together with PromQL.
  Choosing one point shape at v0 keeps the trait small and the
  acceptance criteria sharp.

- **[D4] Tenant on every call**. `MetricStore::ingest(&TenantId,
  …)`, `MetricStore::query(&TenantId, &MetricName, …)`. No
  `set_tenant_context()` mode. Same shape as Aegis, Sluice, and
  Lumen.

- **[D5] Metric-name-first lookup**. `query` takes both
  `tenant` and `metric_name`. Internally the store keys by
  `(TenantId, MetricName)` so single-metric lookups are O(1)
  on the map; the scan is over the per-metric point list, not
  every record. This matches how Prometheus and Mimir organise
  their storage.

- **[D6] Time-range query at v0; predicates in slice 02**.
  Slice 01 ships range-only. Slice 02 adds `service` and
  `label_eq(key, value)`. PromQL operators (`rate`, `sum`,
  `histogram_quantile`) wait for v1.

- **[D7] In-memory only at v0**. No durability. Restart loses
  points. Communicated in every story and KPI.

- **[D8] No exemplars at v0**. v1 adds the field non-breaking
  (it appears as `Option<Exemplar>` or similar on
  `MetricPoint`).

- **[D9] `MetricsRecorder` seam carries forward verbatim from
  Lumen + Sluice**. `record_ingest` and `record_query` events.
  NoopRecorder + CapturingRecorder ship at v0.

- **[D10] AGPL-3.0-or-later**. Same as every platform crate.

- **[D11] Two carpaccio slices, both ≤1 day, in one
  implementation commit per the Aegis + Sluice + Lumen
  precedent**.

## Slicing

- **Slice 01 — walking skeleton** (US-PU-01). `MetricStore`
  trait + `InMemoryMetricStore` adapter + ingest +
  metric-name + time-range query + KPI 1. Acceptance test
  asserts ascending-time order, tenant + metric isolation,
  byte-stable field preservation, and ingest p95 ≤ 1 ms.
- **Slice 02 — structured query** (US-PU-02). `Predicate`
  value type + `query_with` method + service / label-eq
  filters + KPI 2. Acceptance test asserts predicate
  composition (intersection), and query p95 ≤ 10 ms over 10k
  points.

## Constraints established

- The v1 disk-backed adapter MUST be drop-in compatible with
  the v0 trait. If v1 reveals a trait-shape miss (e.g. async
  ingest, exemplars threading through `query_with`), it lands
  as a deliberate v0 → v1 breaking change with its own
  wave-decisions note.
- The v0 adapter's linear-scan query is *intentional*. The
  KPI ceiling accommodates it. v1's columnar substrate will
  tighten the ceiling.
- Pulse depends on `aegis` (for `TenantId`) only. The
  dependency graph stays acyclic.

## DESIGN handoff

DESIGN collapses into the implementation commit per the
established precedent.
