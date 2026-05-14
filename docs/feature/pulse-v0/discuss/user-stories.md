# Pulse v0 — user stories

Two LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`.

The principal user is **Sasha, a platform engineer** who needs
the second first-party storage engine to land behind a stable
trait, so the pattern established by Lumen carries through to
the metrics pillar. Sasha's job at v0 is to ingest OTLP metric
points from Aperture and answer simple time-range queries
keyed by metric name.

The secondary user is **Riley, an SRE** investigating a
latency regression. Riley needs to ask "show me
`http.server.duration` for service `checkout` over the last 30
minutes" without leaving Kaleidoscope. The full PromQL surface
is a v1 concern; at v0 Riley exercises the trait directly via
the Rust API and the acceptance tests pin the contract.

System constraints (apply to every story):

1. Library at v0. Pulse ships as a Rust crate (`pulse`)
   exposing the metric-store trait and one in-memory adapter.
   The columnar + durable adapter (Arrow + Parquet + DataFusion
   + RocksDB, per the Prometheus TSDB block format) lives
   behind the same trait at v1.
2. AGPL-3.0-or-later. Same licensing posture as every platform
   component.
3. **OTLP-shaped types at the trait boundary**. Pulse consumes
   `opentelemetry-proto::metrics::v1::ResourceMetrics`-shaped
   data. v0 ships **gauge / sum number points only** — the
   simplest metric shape. Histogram, exponential histogram,
   and summary land at v1 alongside the columnar substrate.
4. **Per-tenant isolation**. Pulse keys every ingested point
   by `aegis::TenantId`. No cross-tenant query at v0.
5. **Time-range query at v0; label predicates in slice 02**.
   Slice 01 answers "points for metric X for tenant Y between
   t1 and t2". Slice 02 adds `service.name` and label-match
   predicates. PromQL parsing waits for v1.
6. **No telemetry-on-telemetry at v0**. The `MetricsRecorder`
   seam carries forward verbatim from Lumen and Sluice.
7. **In-memory only at v0**. No durability; restart loses
   points. The v1 adapter implements durability behind the same
   trait.
8. **No exemplars at v0**. Linking Pulse data points to Ray
   trace IDs (Phase 5 in the roadmap) is a v1 concern. The v0
   `MetricPoint` type carries no `exemplars` field; v1 adds it
   non-breaking.
9. **No PromQL / OTLP query API at v0**. Pulse exposes a Rust
   API: a trait. The HTTP / gRPC query surface belongs at v1
   bundled with Aperture v1's exporter retrofit.

---

## US-PU-01 — Walking skeleton: ingest + query by metric name and time range

### Elevator Pitch

- **Before**: Sasha has no first-party metric storage. Metrics
  forward through Aperture to an external backend (Mimir /
  Prometheus / Datadog). The "we built it ourselves" claim for
  the metrics pillar is empty.
- **After**: run `cargo test -p pulse --test slice_01_walking_skeleton`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests a batch of OTLP metric points, queries them back
  by metric name and time range, asserts the points round-trip
  byte-stable in observed-time order.
- **Decision enabled**: Sasha can credibly claim Pulse is the
  first-party metric engine even at v0 because the trait shape
  is pinned by the acceptance suite. The v1 disk-backed adapter
  will inherit the same trait.

### Acceptance criteria

- AC-1.1 — `MetricStore::ingest(tenant, batch)` accepts a
  `MetricBatch` and returns `Ok(IngestReceipt { count })`.
- AC-1.2 — `MetricStore::query(tenant, metric_name, range)`
  returns every point whose `time_unix_nano` falls within
  `[start, end)` and whose owning metric's name equals
  `metric_name`.
- AC-1.3 — Points are returned in ascending `time_unix_nano`
  order within a metric.
- AC-1.4 — Two tenants' points are isolated: query on tenant A
  never returns tenant B's points.
- AC-1.5 — Roundtrip preserves every field on `MetricPoint`
  and its enclosing `Metric` byte-for-byte: `name`,
  `description`, `unit`, `kind`, `time_unix_nano`,
  `start_time_unix_nano`, `value`, `attributes`,
  `resource_attributes`.
- AC-1.6 — Querying an unknown metric name returns
  `Ok(Vec::new())`, not an error.

### KPI anchor

- KPI 1 (Ingest latency): p95 ≤ 1 ms per 100-point batch on the
  in-memory adapter. Aperture sits on the hot path; ingest
  cannot be a bottleneck.

---

## US-PU-02 — Structured query: service + label match

### Elevator Pitch

- **Before**: Riley can ask Pulse "give me the points for
  metric M between t1 and t2" but cannot narrow by service or
  by label. A typical production question is "show me
  `http.server.duration` for service `checkout` with route
  `/api/checkout` over the last 5 minutes"; v0 cannot answer
  it.
- **After**: run `cargo test -p pulse --test slice_02_structured_query`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests a mixed batch (multiple services, multiple
  label sets) and asserts that
  `query_with(tenant, metric, range, Predicate { service, label
  matches })` returns exactly the matching points.
- **Decision enabled**: Riley can narrow by service and by
  label without learning PromQL. The full PromQL surface
  lands at v1; the contract for "I want a subset" is already
  written down.

### Acceptance criteria

- AC-2.1 — `Predicate::service(name)` filters to points whose
  resource attribute `service.name == name`.
- AC-2.2 — `Predicate::label_eq(key, value)` filters to points
  whose point-level attribute `key == value`.
- AC-2.3 — Predicates compose: `service` + multiple
  `label_eq` filters intersect.
- AC-2.4 — An empty predicate is equivalent to the slice-01
  range-only query.
- AC-2.5 — Predicates that match nothing return
  `Ok(Vec::new())`, not an error.

### KPI anchor

- KPI 2 (Query latency under predicate): p95 ≤ 10 ms when
  scanning 10 000 ingested points across multiple metrics on
  the in-memory adapter. v1's columnar substrate will tighten
  this dramatically.
