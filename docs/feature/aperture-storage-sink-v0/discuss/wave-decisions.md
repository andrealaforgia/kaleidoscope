# Wave Decisions: aperture-storage-sink-v0 (DISCUSS)

## Configuration (decided, not asked)
- Feature type: Backend.
- Walking skeleton: No — brownfield; aperture (aperture/v0.1.0) and the pillars
  (lumen, ray, pulse) are complete. Slice 01 is the de-facto topology skeleton.
- UX research depth: Lightweight (operator-facing backend; no GUI/TUI surface
  beyond config + structured stderr events + pillar query results).
- JTBD: skipped per configuration — straight to journey + story map + requirements.

## Risks noted
- **No DIVERGE artifacts** exist for this feature. There is no
  `diverge/recommendation.md` or `diverge/job-analysis.md`. The job is grounded
  directly from the brief (close the pillars-have-no-consumer gap with a third
  OtlpSink). Persona (Priya Nair) was constructed in DISCUSS, not inherited from a
  validated job analysis. Risk: low — the job is concrete and the brief is precise.

## Key decisions
- Storage sink modelled as a THIRD OtlpSink (sibling of StubSink/ForwardingSink),
  honouring aperture's port. Not a contradiction of the forwarding design.
- Tenant-resolution rule surfaced as an explicit cross-cutting constraint:
  resource attribute `tenant.id` if present, else configured `default_tenant`,
  else **refuse** the record (never mis-file). Stories make this observable.
- Scope boundary honoured: SinkRecord has exactly three variants; **no profiles
  path** was invented (strata is out of scope).
- One signal per slice (logs, traces, metrics); each end-to-end and shippable.
- KPI latency budget (KPI-4) pinned to GitHub Actions ubuntu-latest; KPI-5 added
  as a correctness guardrail (accepted => queryable; refused => writes nothing).

## Open questions handed to DESIGN
- Q1: confirm the tenant-resolution attribute key name.
- Q2: crate placement (likely new `aperture-storage-sink` crate depending on
  aperture + lumen/ray/pulse; aperture must not gain a pillar dependency).

## Confirmed source facts (grounding)
- Ports: `OtlpSink::accept(record: SinkRecord) -> Pin<Box<dyn Future<Output = Result<(), SinkError>>>>`; `Probe::probe(...)`.
- SinkRecord variants: Logs(ExportLogsServiceRequest), Traces(ExportTraceServiceRequest), Metrics(ExportMetricsServiceRequest). No Profiles.
- Pillar ingest:
  - `LogStore::ingest(&TenantId, LogBatch) -> Result<IngestReceipt, LogStoreError>`
  - `TraceStore::ingest(&TenantId, SpanBatch) -> Result<IngestReceipt, TraceStoreError>`
  - `MetricStore::ingest(&TenantId, MetricBatch) -> Result<IngestReceipt, MetricStoreError>`
- Durability: `FileBacked{Log,Trace,Metric}Store::open(base_path, recorder)`.
- `aegis::TenantId(pub String)`.
