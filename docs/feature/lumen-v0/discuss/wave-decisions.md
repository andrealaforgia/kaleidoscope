# Lumen v0 — DISCUSS wave decisions

## Key decisions

- **[D1] Port + one adapter at v0**, mirroring the Sluice
  precedent. The Arrow + Parquet + DataFusion + Tantivy
  substrate that the architecture roadmap names for Phase 3 all
  lands at v1 behind the same `LogStore` trait. Lumen v0 ships
  one in-memory adapter and one acceptance suite that pins the
  trait. This buys us a useful contract today and keeps the
  high-difficulty substrate work bounded as a separate piece.

- **[D2] OTLP-shaped types at the trait boundary**. The trait
  takes `LogRecord` and `LogBatch` types whose field set
  matches `opentelemetry-proto::logs::v1` exactly. No
  Lumen-specific projections. This makes the v1 disk-backed
  adapter's job mechanical: same input, same output, different
  storage substrate underneath.

- **[D3] Tenant on every call**. `LogStore::ingest(&TenantId,
  …)`, `LogStore::query(&TenantId, …)`, `LogStore::query_with(
  &TenantId, …, &Predicate)`. There is no
  `set_tenant_context()` mode; tenant isolation is enforced by
  the type system, not by discipline. Same shape Aegis and
  Sluice already use.

- **[D4] Time-range query at v0; structured predicates in
  slice 02**. Slice 01 ships range-only query. Slice 02 adds
  `Predicate { service, min_severity }`. Full-text search and
  attribute-path predicates wait for v1's substrate.

- **[D5] In-memory only at v0**. No durability. The trait shape
  does not preclude durability (the v1 adapter can implement it
  trivially), but the v0 adapter does not pay for it. Restart
  loses data. This is communicated in every story and KPI.

- **[D6] Observability via `MetricsRecorder` seam**, copied
  verbatim from Sluice. `IngestRecord` and `QueryRecord` events
  cover the hot paths. NoopRecorder for production v0;
  CapturingRecorder for tests. The OTLP-binding recorder
  belongs in the operator's binary at v1.

- **[D7] No PromQL / LogQL / OTLP query API at v0**. Lumen v0
  exposes a Rust API: a trait. The HTTP / gRPC query surface
  belongs at v1 — bundled with Aperture v1's exporter retrofit
  and Prism's log-panel.

- **[D8] No Aperture retrofit at v0**. Lumen ships as a
  library. Aperture continues to forward logs to whatever
  external backend the operator has wired in. Aperture learns
  about Lumen at v1.

- **[D9] AGPL-3.0-or-later**. Same as every platform crate.

- **[D10] Two carpaccio slices, both ≤1 day, in one
  implementation commit per the Aegis + Sluice precedent**.

## Slicing

- **Slice 01 — walking skeleton** (US-LU-01). `LogStore` trait +
  `InMemoryLogStore` adapter + ingest + time-range query +
  KPI 1. Acceptance test asserts FIFO-by-observed-time within
  a tenant, isolation between tenants, byte-stable field
  preservation, and ingest p95 ≤ 1 ms.
- **Slice 02 — structured query** (US-LU-02). `Predicate` value
  type + `query_with` method + service / severity filters +
  KPI 2. Acceptance test asserts predicate composition,
  intersection semantics, and query p95 ≤ 10 ms over 10k
  records.

## Constraints established

- The v1 disk-backed adapter MUST be drop-in compatible with
  the v0 trait. No trait-shape changes at v1 — only an
  additional adapter implementation. If v1 reveals a
  trait-shape miss (e.g. async ingest is required), it lands
  as a deliberate v0 → v1 breaking change with its own
  wave-decisions note.
- The v0 adapter's linear-scan query is *intentional*. The KPI
  ceiling is loose enough to accommodate it. v1's columnar
  substrate will tighten the ceiling.
- Lumen does NOT depend on Aperture or Beacon or Sieve. It
  depends on Aegis only for `TenantId`. The dependency graph
  stays acyclic and easy to reason about.

## DESIGN handoff

DESIGN collapses into the implementation commit per the Aegis
and Sluice precedents. The architecture decisions above are
already concrete enough to drive `crafty`-style implementation
work without a separate ADR.
