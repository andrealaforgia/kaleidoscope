# Story Map: aperture-storage-sink-v0

## User: Priya Nair, platform operator (self-hosted Kaleidoscope stack)
## Goal: Send OTLP to the gateway, query the pillars, find the data faithfully present, and confirm it survives a restart.

This is the standard OpenTelemetry "collector with storage exporter" topology.
The storage sink is a THIRD `OtlpSink` implementation, a sibling of `StubSink`
and `ForwardingSink`, using aperture's port exactly as designed.

## Backbone

| Configure storage sink | Send OTLP over gateway | Persist into pillar | Query the pillar | Survive restart |
|------------------------|------------------------|---------------------|------------------|-----------------|
| Set sink.kind=storage  | Export logs (gRPC)     | Translate logs->lumen | Query lumen     | Re-open lumen, re-query |
| Set pillar_root        | Export traces          | Translate traces->ray | Query ray       | Re-open ray, re-query   |
| Set default_tenant     | Export metrics         | Translate metrics->pulse | Query pulse  | Re-open pulse, re-query |
| Probe pillars on start |                        | Resolve tenant       | Faithful fields  | Zero loss |

---

### Walking Skeleton

Brownfield: aperture and the pillars are complete (aperture/v0.1.0). No walking
skeleton is built across the whole platform. The thinnest end-to-end slice that
proves the topology is **Slice 01 (logs to lumen)**: configure -> send logs ->
query lumen -> restart -> still there. Slice 01 IS the de-facto skeleton for the
storage-sink topology; slices 02 and 03 replay the identical shape for the other
two signals.

### Slice 01: Logs persist to lumen and survive restart
Tasks: storage sink config + startup probe; `ExportLogsServiceRequest` ->
`Vec<lumen::LogRecord>` translation; tenant resolution; persist via
`LogStore::ingest`; query lumen; restart re-query.
Outcome KPI: KPI-1 (logs round-trip fidelity + durability). This slice
establishes the sink, the config, the probe, and the tenant rule — all reused by
02 and 03 — so it carries the cross-cutting setup.

### Slice 02: Traces persist to ray and survive restart
Tasks: `ExportTraceServiceRequest` -> `Vec<ray::Span>` translation (trace/span
ids, parent, name, kind, times, status, attributes, resource attributes, events,
links); persist via `TraceStore::ingest`; query ray; restart re-query.
Outcome KPI: KPI-2 (traces round-trip fidelity + durability).

### Slice 03: Metrics persist to pulse and survive restart
Tasks: `ExportMetricsServiceRequest` -> `pulse::Metric` + `MetricPoint`s
(name, description, unit, kind gauge/sum, points with time/value/attributes,
resource attributes); persist via `MetricStore::ingest`; query pulse;
restart re-query.
Outcome KPI: KPI-3 (metrics round-trip fidelity + durability).

## Priority Rationale

1. **Slice 01 (logs) first** — it is the riskiest-assumption-first slice: it
   proves the entire storage-sink topology (port wiring, config, Earned-Trust
   probe, tenant resolution, durable persistence, restart survival) end to end.
   Everything 02 and 03 need is validated here. Highest Value x Urgency / Effort
   because it derisks the whole feature.
2. **Slice 02 (traces)** — traces have the richest field mapping (events, links,
   parent ids, kind, status); doing it second means the sink scaffold is proven
   and the work is purely translation fidelity.
3. **Slice 03 (metrics)** — metrics complete the three OTLP-stable signals
   aperture receives. Pulse is gauge+sum only at v0, so the mapping is bounded.

Each slice is independently shippable end to end. Logs and metrics both have a
real production consumer gap today (the brief notes ray and pulse have no
consumer); ordering logs first delivers the thinnest faithful proof fastest.

## Scope Assessment: PASS — 4 stories (1 infra-shared + 3 value), 1 new crate (depends on aperture + 3 pillars), estimated 4-6 days
See `dor-validation.md` Scope Assessment section for the carpaccio taste tests.

## Out of scope (honest boundary)
`SinkRecord` has exactly three variants: Logs, Traces, Metrics. There is **no
Profiles variant**, so strata (profiles) is **not** covered by this feature, and
that is correct. No profiles path is invented.
