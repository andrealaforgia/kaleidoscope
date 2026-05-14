# Ray v0 — DISCUSS wave decisions

## Key decisions

- **[D1] Port + one adapter at v0**, mirroring Sluice, Lumen,
  and Pulse. The Phase 5 columnar substrate (Iceberg-on-Parquet,
  `trace_id`-partitioned) lands at v1 behind the same trait.

- **[D2] OTLP-shaped types at the trait boundary**. `Span`
  field set mirrors `opentelemetry-proto::trace::v1::Span`
  exactly: trace/span/parent IDs, name, kind, start/end times,
  status, attributes, events, links, plus the carrying
  resource attributes. v0 round-trips every field byte-stable.

- **[D3] Two queries at v0**: by `trace_id` (returns the full
  trace) and by `(service.name, time range)` (returns every
  span for that service in the range). Slice 02 adds predicate
  filtering on span name / kind / status, composing with the
  service + range query.

- **[D4] Tenant on every call**. Same shape as every prior
  storage engine.

- **[D5] Dual index at v0**: `HashMap<(TenantId, TraceId),
  Vec<Span>>` for the `get_trace` query, plus `HashMap<(TenantId,
  ServiceName), Vec<Span>>` for the service + range query.
  Spans live in both maps (cloned on ingest). Memory cost is
  O(2N) but lookup is O(1) on the index for both query shapes.
  v1's columnar adapter will hoist this into a proper
  trace-id-partitioned columnar layout with secondary indices.

- **[D6] No sampling at v0**. Aperture forwards every span to
  Ray. Sieve v1 — head-based probabilistic sampling at
  Aperture, error-biased (100% of error traces retained, CI-
  enforced invariant per the Phase 5 exit criteria) — is a
  separate feature.

- **[D7] No exemplars at v0**. The `MetricPoint` ↔ `trace_id`
  link is a v1 concern.

- **[D8] In-memory only at v0**. Restart loses spans.

- **[D9] `MetricsRecorder` seam carries forward verbatim from
  Lumen + Pulse + Sluice**.

- **[D10] AGPL-3.0-or-later**.

- **[D11] Two carpaccio slices, both ≤1 day, in one
  implementation commit**, per the Aegis / Sluice / Lumen /
  Pulse precedent.

## Slicing

- **Slice 01 — walking skeleton** (US-RA-01). Trait + adapter
  + `get_trace` + `query` by service+range + KPI 1.
- **Slice 02 — structured query** (US-RA-02). `Predicate` +
  `query_with` + span_name / kind / status filters + KPI 2.

## Constraints established

- v1 disk-backed adapter must be drop-in compatible.
- v0 dual-index pays 2× memory for O(1) lookup on both axes —
  deliberate trade-off for v0; v1 chooses a single columnar
  layout.
- Ray depends on `aegis` (for `TenantId`) only.

## DESIGN handoff

DESIGN collapses into the implementation commit.
