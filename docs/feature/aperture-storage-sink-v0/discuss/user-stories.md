<!-- markdownlint-disable MD024 -->
# User Stories: aperture-storage-sink-v0

## System Constraints

- The storage sink is a THIRD `OtlpSink` implementation, sibling of `StubSink`
  and `ForwardingSink`, using aperture's port exactly as designed:
  `fn accept(&self, record: SinkRecord) -> Pin<Box<dyn Future<Output = Result<(), SinkError>>>>`.
  It MUST also implement `Probe` (Earned-Trust startup contract).
- `SinkRecord` has exactly three variants: `Logs`, `Traces`, `Metrics`. There is
  **no Profiles variant**; strata/profiles is out of scope. No profiles path.
- Pillar ingest signatures (confirmed from source):
  - `LogStore::ingest(&self, tenant: &TenantId, batch: LogBatch) -> Result<IngestReceipt, LogStoreError>`
  - `TraceStore::ingest(&self, tenant: &TenantId, batch: SpanBatch) -> Result<IngestReceipt, TraceStoreError>`
  - `MetricStore::ingest(&self, tenant: &TenantId, batch: MetricBatch) -> Result<IngestReceipt, MetricStoreError>`
- Durability is provided by the `FileBacked*Store` adapters
  (`FileBackedLogStore::open(base_path, recorder)` and siblings). Restart survival
  uses the same `pillar_root`.
- `aegis::TenantId(pub String)`. OTLP has no native tenant.
  **Tenant-resolution rule (cross-cutting):** use the resource attribute
  `tenant.id` if present; otherwise the configured `default_tenant`; if neither
  is available the record is **refused** (`SinkError::Internal`), never mis-filed.
  DESIGN to confirm the attribute key name (open question Q1).
- Crate placement (likely a new `aperture-storage-sink` crate depending on
  aperture + lumen/ray/pulse) is a DESIGN decision (open question Q2); not pinned
  here. aperture itself must not gain a dependency on the pillars.
- Latency budgets (KPI-4) are measured on GitHub Actions ubuntu-latest, not a
  fast workstation.

---

## US-01: Logs persist to lumen and survive a restart

### Elevator Pitch
- **Before**: Priya sends OTLP logs to the gateway; the stub sink writes a stderr
  line and the data evaporates. lumen has no production consumer.
- **After**: Priya runs the gateway with `sink.kind=storage`, exports logs over
  gRPC `:4317`, restarts the process, and a lumen query for her tenant returns the
  records, field-faithful. She sees `event=sink_accepted sink=storage signal=logs`
  on accept, and the query yields the body, severity and `service.name` she sent.
- **Decision enabled**: Priya decides the logs pipeline is production-ready and
  can point real services' log exporters at the gateway.

### Problem
Priya Nair is a platform operator who has lumen compiled and durable and aperture
v0 listening, but nothing persists received logs. She finds it unacceptable to run
a platform where accepted telemetry silently disappears, so today she works around
it by not using the gateway for storage at all.

### Who
- Platform operator | self-hosted Kaleidoscope stack | wants the platform to run
  end to end and survive restarts.

### Solution
A storage `OtlpSink` that, for `SinkRecord::Logs`, translates
`ExportLogsServiceRequest` into `Vec<lumen::LogRecord>` (resource attributes incl.
`service.name`, observed timestamp, severity number + text, body, attributes,
trace/span id), resolves the tenant, and persists via `LogStore::ingest`. Also
implements `Probe` so startup fails fast if the pillar root is not writable.

### Domain Examples
#### 1: Happy path — checkout-api log
Priya exports one log for service `checkout-api`: body "order 1001 placed",
severity INFO (`SeverityNumber(9)`, severity_text "INFO"), observed at
1716240000000000000 ns, tenant resolved to "acme" via `default_tenant`. After
restart, `LogStore::query(tenant="acme", all)` returns that record unchanged.
#### 2: Edge case — explicit tenant attribute
A log batch from service `billing-worker` carries resource attribute
`tenant.id="globex"`. The sink files it under "globex", not the default "acme".
Querying "acme" returns nothing; querying "globex" returns the record.
#### 3: Error/boundary — no tenant resolvable
A log batch arrives with no `tenant.id` attribute and the operator configured no
`default_tenant`. The sink refuses the record with `SinkError::Internal` naming
the missing tenant rule; nothing is written to lumen.

### UAT Scenarios (BDD)
#### Scenario: Logs sent to the gateway are persisted to lumen
Given Priya runs the gateway with sink.kind "storage", pillar_root "./data" and default_tenant "acme"
When she exports one OTLP log "order 1001 placed" at severity INFO for service "checkout-api"
Then the gateway responds OK to the client
And it emits event=sink_accepted with sink=storage and signal=logs

#### Scenario: Persisted logs faithfully reflect what was sent
Given Priya has exported the "order 1001 placed" INFO log for "checkout-api" to tenant "acme"
When she queries lumen for tenant "acme" over all time
Then exactly one record is returned
And its body is "order 1001 placed", severity_text "INFO", and resource service.name "checkout-api"

#### Scenario: Persisted logs survive a gateway restart
Given Priya has persisted the "order 1001 placed" log for tenant "acme"
When the gateway process is restarted against the same pillar_root "./data"
And she queries lumen for tenant "acme" over all time
Then the same single record is returned, identical to before the restart

#### Scenario: An explicit tenant attribute overrides the default
Given the gateway is running with default_tenant "acme"
When Priya exports a log for "billing-worker" carrying resource attribute tenant.id "globex"
Then querying lumen for tenant "globex" returns the record
And querying lumen for tenant "acme" returns nothing

#### Scenario: A log with no resolvable tenant is refused, not mis-filed
Given the gateway is running with no default_tenant configured
When Priya exports a log carrying no tenant.id resource attribute
Then the gateway refuses the record with a sink error naming the missing tenant rule
And nothing is written to lumen

### Acceptance Criteria
- [ ] Exporting an OTLP log over the gateway returns OK and emits event=sink_accepted sink=storage signal=logs
- [ ] A queried record's body, severity_text and resource service.name equal what was sent
- [ ] The same record is returned after a process restart against the same pillar_root
- [ ] tenant.id resource attribute takes precedence over default_tenant
- [ ] A record with no resolvable tenant is refused with a clear sink error and nothing is persisted

### Outcome KPIs
- **Who**: Operator running the gateway with the storage sink
- **Does what**: Sends OTLP logs and finds them in lumen after a restart, field-faithful
- **By how much**: 100% of accepted log records queryable post-restart, zero loss (KPI-1)
- **Measured by**: Round-trip integration test (export -> restart -> query, assert field equality)
- **Baseline**: 0% — lumen has no production consumer today

### Technical Notes
- Translation: `ExportLogsServiceRequest` -> `Vec<lumen::LogRecord>`; severity maps
  to `lumen::SeverityNumber(i32)`; body to `String`; attributes/resource attributes
  to `BTreeMap<String,String>`; trace/span id to `Option<[u8;16]>` / `Option<[u8;8]>`.
- Durability via `FileBackedLogStore::open(pillar_root, recorder)`.
- Implements `Probe`: probe checks pillar_root is writable; startup uses
  wire_then_probe_then_use.
- Carries the cross-cutting setup (config, probe, tenant rule) reused by US-02/03.
- Depends on: aperture `OtlpSink`/`Probe` ports (available), lumen `LogStore`
  (available). Crate placement = open question Q2 (DESIGN).

---

## US-02: Traces persist to ray and survive a restart

### Elevator Pitch
- **Before**: Priya sends OTLP traces to the gateway and they evaporate; ray has
  no production consumer.
- **After**: Priya exports a trace over gRPC `:4317`, restarts the process, and a
  ray `get_trace`/`query` for her tenant returns the spans with trace id, parent,
  kind, status, events and links intact. She sees
  `event=sink_accepted sink=storage signal=traces span_count=N` on accept.
- **Decision enabled**: Priya decides the tracing pipeline is production-ready and
  points real services' trace exporters at the gateway.

### Problem
Priya has ray compiled and durable but nothing persists received spans. A platform
that accepts traces and then loses them is not one she can run in production, so
today she cannot use the gateway for trace storage.

### Who
- Platform operator | self-hosted Kaleidoscope stack | needs faithful, durable
  trace storage including span relationships.

### Solution
Extend the storage sink so that for `SinkRecord::Traces` it translates
`ExportTraceServiceRequest` into `Vec<ray::Span>` (trace id, span id, parent span
id, name, kind, start/end time, status, span attributes, resource attributes,
events, links), resolves the tenant (same rule as US-01), and persists via
`TraceStore::ingest`.

### Domain Examples
#### 1: Happy path — a two-span trace
Priya exports a trace for `checkout-api`: a root server span "POST /orders"
(no parent, kind Server, status Ok) and a child client span "charge-card"
(parent = root span id, kind Client). After restart, `get_trace` for that trace id
under tenant "acme" returns both spans with the parent relationship intact.
#### 2: Edge case — span with events and links
A span "process-payment" for `billing-worker` carries one event
"retry-attempted" at a timestamp and one link to a span in another trace. After
persistence the event name/timestamp and link trace/span ids are faithfully
queryable.
#### 3: Error/boundary — malformed trace id
A span arrives with a trace id that is not 16 bytes. The sink refuses the record
with a sink error naming the offending field; nothing is written to ray.

### UAT Scenarios (BDD)
#### Scenario: Traces sent to the gateway are persisted to ray
Given Priya runs the gateway with the storage sink against tenant "acme"
When she exports a trace for "checkout-api" with a root server span and a child client span
Then the gateway responds OK to the client
And it emits event=sink_accepted with sink=storage and signal=traces

#### Scenario: Persisted spans faithfully reflect the trace structure
Given Priya has exported the "POST /orders" root span and its "charge-card" child to tenant "acme"
When she queries ray for that trace id over all time
Then both spans are returned
And the child span's parent is the root span, with kinds Server and Client and status Ok

#### Scenario: Span events and links are persisted faithfully
Given Priya has exported a "process-payment" span carrying one event "retry-attempted" and one link
When she queries ray for that trace under tenant "acme"
Then the returned span has the event name and timestamp it was sent with
And the returned link carries the same trace id and span id

#### Scenario: Persisted traces survive a gateway restart
Given Priya has persisted the "POST /orders" trace for tenant "acme"
When the gateway is restarted against the same pillar_root
And she queries ray for that trace id
Then the same spans are returned, identical to before the restart

#### Scenario: A span with a malformed trace id is refused
Given the gateway is running with the storage sink
When Priya exports a span whose trace id is not 16 bytes
Then the gateway refuses the record with a sink error naming the field
And nothing is written to ray

### Acceptance Criteria
- [ ] Exporting a trace returns OK and emits event=sink_accepted sink=storage signal=traces
- [ ] Queried spans preserve trace id, span id, parent, kind and status
- [ ] Span events (name, timestamp, attributes) and links (trace/span id) are faithful
- [ ] The same trace is returned after a restart against the same pillar_root
- [ ] A span with a malformed (non-16-byte) trace id is refused and nothing is persisted

### Outcome KPIs
- **Who**: Operator running the gateway with the storage sink
- **Does what**: Sends OTLP traces and finds them in ray after a restart, field-faithful
- **By how much**: 100% of accepted spans queryable post-restart with faithful structure (KPI-2)
- **Measured by**: Round-trip trace integration test
- **Baseline**: 0% — ray has no production consumer today

### Technical Notes
- Translation: `ExportTraceServiceRequest` -> `Vec<ray::Span>`; ids to
  `ray::TraceId([u8;16])`/`ray::SpanId([u8;8])`; kind to `ray::SpanKind`; status to
  `ray::SpanStatus`/`StatusCode`; events to `Vec<SpanEvent>`; links to `Vec<SpanLink>`.
- Durability via `FileBackedTraceStore::open(pillar_root, recorder)`.
- Reuses US-01's config, probe and tenant rule.
- Depends on: US-01 (storage sink scaffold + tenant rule), ray `TraceStore` (available).

---

## US-03: Metrics persist to pulse and survive a restart

### Elevator Pitch
- **Before**: Priya sends OTLP metrics to the gateway and they evaporate; pulse has
  no production consumer.
- **After**: Priya exports gauge and sum metrics over gRPC `:4317`, restarts the
  process, and a pulse query for her tenant and metric name returns the points with
  value, unit and attributes intact. She sees
  `event=sink_accepted sink=storage signal=metrics data_point_count=N` on accept.
- **Decision enabled**: Priya decides the metrics pipeline is production-ready and
  points real services' metric exporters at the gateway, completing the platform.

### Problem
Priya has pulse compiled and durable but nothing persists received metric points.
Without a metrics consumer the platform still does not run end to end, so she
cannot rely on the gateway for metric storage.

### Who
- Platform operator | self-hosted Kaleidoscope stack | needs faithful, durable
  storage of gauge and sum metric points.

### Solution
Extend the storage sink so that for `SinkRecord::Metrics` it translates
`ExportMetricsServiceRequest` into `pulse::Metric` + `MetricPoint`s (name,
description, unit, kind gauge/sum, points with time/value/attributes, resource
attributes), resolves the tenant (same rule), and persists via
`MetricStore::ingest`.

### Domain Examples
#### 1: Happy path — a gauge
Priya exports a gauge `process.cpu.utilization` (unit "1") for `checkout-api`
with one point value 0.42 at 1716240000000000000 ns, tenant "acme". After restart,
querying pulse for `(tenant="acme", "process.cpu.utilization")` returns the point
with value 0.42 and kind Gauge.
#### 2: Edge case — a sum with point attributes
A sum `http.server.request.count` (unit "1") for `billing-worker` has a point
value 7.0 carrying attribute `http.route="/charge"`. After persistence the value,
kind Sum and the point attribute are faithfully queryable.
#### 3: Error/boundary — unsupported point type
A metric arrives as a histogram (pulse v0 supports gauge + sum only). The sink
refuses the record with a sink error naming the unsupported point type; nothing is
written to pulse.

### UAT Scenarios (BDD)
#### Scenario: Metrics sent to the gateway are persisted to pulse
Given Priya runs the gateway with the storage sink against tenant "acme"
When she exports a gauge "process.cpu.utilization" value 0.42 for "checkout-api"
Then the gateway responds OK to the client
And it emits event=sink_accepted with sink=storage and signal=metrics

#### Scenario: Persisted metric points faithfully reflect what was sent
Given Priya has exported the gauge "process.cpu.utilization" value 0.42 for "checkout-api" to tenant "acme"
When she queries pulse for tenant "acme" and metric "process.cpu.utilization" over all time
Then one point is returned with value 0.42 and kind Gauge
And its resource service.name is "checkout-api"

#### Scenario: Sum point attributes are persisted faithfully
Given Priya has exported a sum "http.server.request.count" value 7 with attribute http.route "/charge"
When she queries pulse for tenant "acme" and metric "http.server.request.count"
Then one point is returned with value 7 and kind Sum
And the point carries attribute http.route "/charge"

#### Scenario: Persisted metrics survive a gateway restart
Given Priya has persisted the gauge "process.cpu.utilization" for tenant "acme"
When the gateway is restarted against the same pillar_root
And she queries pulse for that metric under tenant "acme"
Then the same point is returned with value 0.42, identical to before the restart

#### Scenario: An unsupported metric point type is refused
Given the gateway is running with the storage sink
When Priya exports a histogram metric (unsupported at pulse v0)
Then the gateway refuses the record with a sink error naming the unsupported point type
And nothing is written to pulse

### Acceptance Criteria
- [ ] Exporting a gauge/sum returns OK and emits event=sink_accepted sink=storage signal=metrics
- [ ] A queried point preserves value, kind (Gauge/Sum), unit and resource service.name
- [ ] Point-level attributes are faithful
- [ ] The same point is returned after a restart against the same pillar_root
- [ ] A histogram (unsupported) metric is refused and nothing is persisted

### Outcome KPIs
- **Who**: Operator running the gateway with the storage sink
- **Does what**: Sends OTLP metrics and finds them in pulse after a restart, field-faithful
- **By how much**: 100% of accepted gauge/sum points queryable post-restart (KPI-3)
- **Measured by**: Round-trip metrics integration test
- **Baseline**: 0% — pulse has no production consumer today

### Technical Notes
- Translation: `ExportMetricsServiceRequest` -> `pulse::Metric` + `Vec<MetricPoint>`;
  kind to `pulse::MetricKind` (Gauge/Sum only at v0); value to `f64`; attributes to
  `BTreeMap<String,String>`.
- Durability via `FileBackedMetricStore::open(pillar_root, recorder)`.
- Reuses US-01's config, probe and tenant rule.
- Depends on: US-01 (scaffold + tenant rule), pulse `MetricStore` (available).

---

## Risk Assessment

| Risk | Category | Probability | Impact | Mitigation |
|------|----------|-------------|--------|------------|
| No DIVERGE artifacts for this feature (job statement not formally validated) | Project | High | Low | Brief grounds the job clearly; noted in wave-decisions.md |
| Tenant-resolution attribute key name unconfirmed | Technical | Medium | Medium | Open question Q1 to DESIGN; stories assume tenant.id-then-default-then-refuse |
| Crate placement could leak pillar deps into aperture | Technical | Low | High | Open question Q2 to DESIGN; constraint: new crate, aperture must not depend on pillars |
| Latency budget unrealistic if measured off-CI | Technical | Medium | Medium | KPI-4 pinned to GitHub Actions ubuntu-latest |
| Silent loss / partial persistence on translation error | Technical | Medium | High | KPI-5 correctness guardrail: accepted => queryable; refused => writes nothing |
