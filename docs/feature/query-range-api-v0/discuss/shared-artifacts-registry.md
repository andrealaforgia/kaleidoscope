# Shared Artifacts Registry: query-range-api-v0

Every value that crosses a boundary between Prism's client, the query service, and
Pulse. Untracked artifacts are the primary cause of horizontal integration failure;
here the highest risk is the response shape, which is pinned by Prism's own validator.

## Registry

```yaml
shared_artifacts:
  prometheus_matrix_response:
    source_of_truth: "apps/prism/src/lib/promql/queryRange.ts (isPromSuccess / isPromError) + ADR-0027 §2"
    consumers:
      - "query service response serialiser (this feature)"
      - "Prism queryRange validator -> kind:'success'|'empty'|'parse-error'"
      - "ECharts series builder"
      - "QueryPanel footer 'N series, M points, K ms'"
    owner: "Prism v0 (contract pinned upstream; this feature is the provider)"
    integration_risk: "HIGH - any drift renders in Prism as transport-error:shape"
    validation: "Round-trip a real response through Prism's isPromSuccess/isPromError in a contract test"

  query_param_raw_promql:
    source_of_truth: "apps/prism/src/lib/promql/queryRange.ts buildUrl() -> URLSearchParams({query})"
    consumers: ["query service selector parser", "Prism query input (operator free text)"]
    owner: "this feature (parser); Prism (producer)"
    integration_risk: "MEDIUM - Prism sends a RAW PromQL string, not structured params; parser must reject unsupported forms with status:error"
    validation: "Parser accepts bare metric name; rejects operators/functions/aggregations with HTTP 400 status:error"

  start_end_seconds:
    source_of_truth: "apps/prism/src/lib/promql/queryRange.ts resolveRange() -> epoch SECONDS (float)"
    consumers: ["query service TimeRange construction (must convert to nanoseconds)"]
    owner: "this feature"
    integration_risk: "MEDIUM - seconds vs nanoseconds unit error would silently return wrong/empty data"
    validation: "Boundary example: start=1716200000s must map to 1716200000000000000ns; half-open [start,end)"

  tenant_id:
    source_of_truth: "crates/aegis (TenantId newtype). Write path uses KALEIDOSCOPE_DEFAULT_TENANT (gateway main.rs)"
    consumers: ["query service tenant resolution", "pulse query key (TenantId, MetricName)"]
    owner: "DESIGN (mechanism unresolved - RED CARD 1)"
    integration_risk: "HIGH - wrong tenant returns another tenant's data or none; must fail closed"
    validation: "Slice-01: configured single tenant, fail-closed if unset, mirroring gateway. Header path deferred."

  metric_name:
    source_of_truth: "crates/pulse/src/metric.rs MetricName(String)"
    consumers: ["query service (parsed from query param)", "pulse query", "matrix __name__ label"]
    owner: "pulse"
    integration_risk: "LOW - direct string mapping"
    validation: "Bare selector string becomes MetricName::new(name); echoed as __name__ in the matrix labels"

  label_set:
    source_of_truth: "crates/pulse/src/metric.rs Metric.resource_attributes + MetricPoint.attributes"
    consumers: ["query service matrix grouping (RED CARD 3)", "matrix metric{} object"]
    owner: "this feature (grouping policy); pulse (data)"
    integration_risk: "MEDIUM - grouping key choice changes how many series Prism plots"
    validation: "One matrix series per distinct merged label set; recommended to include __name__"

  point_value_encoding:
    source_of_truth: "apps/prism/src/lib/promql/queryRange.ts parseValue() ('NaN' -> NaN; else parseFloat)"
    consumers: ["query service value serialiser", "ECharts"]
    owner: "this feature"
    integration_risk: "MEDIUM - values MUST be JSON strings, not numbers; NaN encodes as the string 'NaN'"
    validation: "Each values pair is [seconds:number, value:string]; f64 NaN serialises as \"NaN\""

  header_redaction:
    source_of_truth: "ADR-0027 §6 (Prism redacts forwarded header values from any outcome)"
    consumers: ["query service status:error serialiser (must not echo auth/tenancy header values)"]
    owner: "this feature (provider-side discipline)"
    integration_risk: "MEDIUM - a backend that echoes a forwarded secret in an error body leaks it"
    validation: "status:error messages never contain a forwarded header/credential value"
```

## Consistency checks (for DISTILL integration validation)

1. Does the serialised response pass `isPromSuccess` for non-empty and `data.result: []` for empty?
2. Does a `status:error` body pass `isPromError` (status==='error' AND typeof error==='string')?
3. Is start=1716200000 (seconds) converted to 1716200000000000000 (nanoseconds) before pulse query?
4. Is every `values` value a JSON string, with NaN as `"NaN"`?
5. Does the tenant used for the query come from the same aegis TenantId vocabulary the write path uses?
