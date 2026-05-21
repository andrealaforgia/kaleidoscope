# Shared Artifacts Registry: aperture-storage-sink-v0

Every value that flows across journey steps, with a single source of truth.

```yaml
shared_artifacts:
  sink_kind:
    source_of_truth: "aperture config file (sink.kind)"
    consumers: ["startup log", "config validator selecting the storage sink"]
    owner: "aperture composition root"
    integration_risk: "MEDIUM - wrong value silently selects stub/forwarding instead of storage"
    validation: "Startup log prints sink=storage; probe_ok line names the storage sink"

  pillar_root:
    source_of_truth: "aperture config file (sink.storage.pillar_root)"
    consumers:
      - "startup probe (writable check)"
      - "FileBackedLogStore::open / FileBackedTraceStore::open / FileBackedMetricStore::open base_path"
      - "post-restart re-open and re-query"
    owner: "storage sink configuration"
    integration_risk: "HIGH - if ingest and post-restart re-open use different roots, durability silently breaks"
    validation: "Ingest then restart against the same root returns the identical record set"

  tenant_id:
    source_of_truth: "resource attribute tenant.id if present, else config default_tenant"
    consumers:
      - "LogStore/TraceStore/MetricStore ingest tenant key (aegis::TenantId)"
      - "query tenant key"
    owner: "storage sink tenant-resolution rule"
    integration_risk: "HIGH - tenant resolved at ingest must equal tenant queried, or records appear lost"
    validation: "Query under the resolved tenant returns the ingested records; a record with no tenant rule is refused, not mis-filed"

  service_name:
    source_of_truth: "OTLP resource attribute service.name"
    consumers:
      - "translated resource_attributes on lumen LogRecord / ray Span / pulse Metric"
      - "sink_accepted log line (resource.service.name field)"
      - "query result"
    owner: "per-signal translator"
    integration_risk: "MEDIUM - fidelity: value sent must equal value queried"
    validation: "Round-trip assertion compares sent service.name to queried resource_attributes['service.name']"

  signal:
    source_of_truth: "SinkRecord variant (Logs|Traces|Metrics)"
    consumers: ["sink_accepted log line", "per-signal translator dispatch", "per-signal count field name"]
    owner: "aperture ports::SinkRecord"
    integration_risk: "LOW - the harness type-path identity guarantee makes the variant authoritative"
    validation: "Match arm count == 3; no profiles arm exists"

  otlp_endpoint:
    source_of_truth: "gateway listen address (sink-independent, existing aperture config)"
    consumers: ["client export call", "startup listening log"]
    owner: "aperture transport layer (pre-existing)"
    integration_risk: "LOW - unchanged by this feature"
    validation: "Existing aperture behaviour; not modified here"
```

## Consistency checks

- Every `${variable}` in the journey TUI mockups maps to an entry above.
- `pillar_root` and `tenant_id` are HIGH risk: the durability and the
  not-lost guarantees both depend on these matching across ingest, query,
  and restart. The journey integration_validation block enforces both.
- No value is hardcoded in two places: `tenant_id` has a single resolution
  rule (resource attribute then default), used by both ingest and query.
