# Ray v0 — user stories

Two LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`.

The principal user is **Sasha, a platform engineer** who needs
the third first-party storage engine to land behind a stable
trait, completing the three signal pillars: logs (Lumen),
metrics (Pulse), traces (Ray). Sasha's job at v0 is to ingest
OTLP spans from Aperture and answer two queries: "fetch this
specific trace" and "fetch traces for service X that match
this predicate".

The secondary user is **Riley, an SRE** debugging a slow
request. Riley copies a `trace_id` from a Prism log entry and
asks "show me every span in this trace" — the bedrock query of
distributed tracing.

System constraints (apply to every story):

1. Library at v0. Ray ships as a Rust crate (`ray`) exposing
   the trace-store trait and one in-memory adapter. The
   columnar adapter (Iceberg-on-Parquet, trace-id-partitioned,
   per the Phase 5 roadmap) lives behind the same trait at v1.
2. AGPL-3.0-or-later. Same posture as every platform crate.
3. **OTLP-shaped types at the trait boundary**. Ray consumes
   `opentelemetry-proto::trace::v1::ResourceSpans`-shaped data.
   v0 ships `Span` (including W3C trace and span IDs, kind,
   status, parent linkage, attributes, events, links — the
   full set).
4. **Per-tenant isolation**. Ray keys every ingested span by
   `aegis::TenantId`.
5. **Two queries at v0**: by `trace_id` (returns every span
   in the trace, ordered by `start_time_unix_nano`); by
   `(service.name, time range)` (returns every span belonging
   to that service in the range — the "what was running" query).
   Slice 02 adds predicates (span name, kind, status).
6. **No sampling at v0**. Sieve v1 will install head-based
   probabilistic sampling at Aperture; Ray sees whatever
   Aperture forwards. Sieve v1 is out of scope for Ray v0.
7. **No exemplars at v0**. Linking Pulse data points to Ray
   trace IDs is a v1 concern.
8. **No telemetry-on-telemetry**. The `MetricsRecorder` seam
   carries forward verbatim from Lumen + Pulse.
9. **In-memory only at v0**. Restart loses spans. The v1
   adapter implements durability behind the same trait.

---

## US-RA-01 — Walking skeleton: ingest + query by trace_id

### Elevator Pitch

- **Before**: Sasha has no first-party trace storage. Traces
  forward to an external Tempo / Jaeger / Datadog backend. The
  "we built it ourselves" claim for the trace pillar is empty.
- **After**: run `cargo test -p ray --test slice_01_walking_skeleton`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests a batch of OTLP spans, queries one trace by
  `trace_id`, and asserts every span in that trace round-trips
  byte-stable in `start_time_unix_nano` order.
- **Decision enabled**: Sasha can credibly claim Ray is the
  first-party trace engine even at v0 because the trait shape
  is pinned by the acceptance suite. The v1 disk-backed adapter
  inherits the same contract.

### Acceptance criteria

- AC-1.1 — `TraceStore::ingest(tenant, batch)` accepts a
  `SpanBatch` and returns `Ok(IngestReceipt { count })`.
- AC-1.2 — `TraceStore::get_trace(tenant, trace_id)` returns
  every span sharing that `trace_id`, ordered by ascending
  `start_time_unix_nano`.
- AC-1.3 — `TraceStore::query(tenant, service_name, range)`
  returns every span belonging to that service whose
  `start_time_unix_nano` falls within `[start, end)`.
- AC-1.4 — Two tenants' spans are isolated.
- AC-1.5 — Roundtrip preserves every field on `Span`:
  `trace_id`, `span_id`, `parent_span_id`, `name`, `kind`,
  `start_time_unix_nano`, `end_time_unix_nano`, `status`,
  `attributes`, `events`, `links`, `resource_attributes`.
- AC-1.6 — `get_trace` on an unknown `trace_id` returns
  `Ok(Vec::new())`, not an error.
- AC-1.7 — `query` on an unknown service returns
  `Ok(Vec::new())`, not an error.

### KPI anchor

- KPI 1 (Ingest latency): p95 ≤ 2 ms per 100-span batch on the
  in-memory adapter. (2ms not 1ms because of the dual
  `by_trace` + `by_service` index — see `outcome-kpis.md` for
  rationale.)

---

## US-RA-02 — Structured query: span-name + kind + status

### Elevator Pitch

- **Before**: Riley can pull a trace by id, but cannot filter
  "show me only the database spans in this trace" or "show me
  every span across the service that errored in the last 5
  minutes". v0 cannot answer either.
- **After**: run `cargo test -p ray --test slice_02_structured_query`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests a mixed batch (multiple span names, kinds,
  statuses) and asserts that
  `query_with(tenant, service, range, Predicate { span_name,
  kind, status })` returns exactly the matching spans.
- **Decision enabled**: Riley narrows traces by structural
  predicates without learning TraceQL. Full TraceQL surface
  lands at v1.

### Acceptance criteria

- AC-2.1 — `Predicate::span_name(name)` filters to spans whose
  `name == name`.
- AC-2.2 — `Predicate::kind(SpanKind)` filters to spans whose
  `kind == kind`.
- AC-2.3 — `Predicate::status(StatusCode)` filters to spans
  whose status code equals the given code.
- AC-2.4 — Predicates compose: any combination intersects.
- AC-2.5 — Empty predicate is equivalent to the slice-01
  service + range query.
- AC-2.6 — Predicates that match nothing return
  `Ok(Vec::new())`.

### KPI anchor

- KPI 2 (Query latency under predicate): p95 ≤ 10 ms when
  scanning 10 000 ingested spans on the in-memory adapter.
