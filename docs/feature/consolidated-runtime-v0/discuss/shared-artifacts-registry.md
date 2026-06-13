# Shared Artifacts Registry — `consolidated-runtime-v0`

Shared artifacts are values (and one instance identity) that appear in multiple places across
the experiment loop and the composed process. In this feature the single most important shared
artifact is not a string at all — it is the **per-signal store instance**: ONE `Arc<Store>`
that the ingest sink and the query router must both hold. If they hold different instances the
whole feature fails silently (the reader sees a frozen snapshot), so it is tracked first and
as HIGH risk.

## Registry

```yaml
shared_artifacts:

  metric_store_instance:
    source_of_truth: "the composition root builds it once: Arc::new(FileBackedMetricStore::open(pulse_path, ...))"
    consumers:
      - "ingest sink: StorageSink::with_all_stores(.., Arc::clone(&metric_store), ..) (crates/kaleidoscope-gateway/src/main.rs:84-96)"
      - "query router: query_api::router(Arc::clone(&metric_store) as Arc<dyn MetricStore>, tenant, static_dir) (crates/query-api/src/lib.rs:122)"
    owner: "consolidated-runtime-v0 composition root (DESIGN decides new-binary vs extend-gateway)"
    integration_risk: "HIGH — if sink and router hold DIFFERENT instances, a write is never visible to a read; this is the exact bug C1 exists to fix. Must be the SAME Arc."
    validation: "US-01 + US-05 acceptance: ingest then query in one process returns the value; an integration test asserting send-then-immediately-query."

  log_store_instance:
    source_of_truth: "Arc::new(FileBackedLogStore::open(lumen_path, ...)) (crates/kaleidoscope-gateway/src/main.rs:76)"
    consumers:
      - "ingest sink: StorageSink::with_all_stores(Arc::clone(&log_store), ..)"
      - "query router: log_query_api::router(Arc::clone(&log_store) as Arc<dyn LogStore>, tenant) (crates/log-query-api/src/lib.rs:95)"
    owner: "consolidated-runtime-v0 composition root"
    integration_risk: "HIGH — same as metric_store_instance, for logs."
    validation: "US-03 acceptance."

  trace_store_instance:
    source_of_truth: "Arc::new(FileBackedTraceStore::open(ray_path, ...)) (crates/kaleidoscope-gateway/src/main.rs:80)"
    consumers:
      - "ingest sink: StorageSink::with_all_stores(.., Arc::clone(&trace_store))"
      - "query router: trace_query_api::router(Arc::clone(&trace_store) as Arc<dyn TraceStore>, tenant) (crates/trace-query-api/src/lib.rs:100)"
    owner: "consolidated-runtime-v0 composition root"
    integration_risk: "HIGH — same as above, for traces."
    validation: "US-04 acceptance."

  pillar_root:
    source_of_truth: "CLI arg 1, else KALEIDOSCOPE_PILLAR_ROOT, else default 'kaleidoscope-data' (crates/kaleidoscope-gateway/src/main.rs:180-188)"
    consumers: ["all three FileBacked*Store::open calls (one shared root, sub-dirs pulse/lumen/ray)"]
    owner: "consolidated-runtime-v0 composition root"
    integration_risk: "MEDIUM — in single-process there is one writer, so no cross-process WAL contention. Must NOT be co-run against a separate gateway on the same root (two writers corrupt the WAL — state assessment §4)."
    validation: "US-05; documented constraint that the consolidated runtime owns its pillar root."

  default_tenant:
    source_of_truth: "KALEIDOSCOPE_DEFAULT_TENANT (ingest, fail-closed if unset for records lacking tenant.id) (crates/kaleidoscope-gateway/src/main.rs:192-197)"
    consumers:
      - "ingest sink: StorageSinkConfig::with_default_tenant(tenant)"
      - "query routers: KALEIDOSCOPE_QUERY_TENANT / KALEIDOSCOPE_LOG_QUERY_TENANT / KALEIDOSCOPE_TRACE_QUERY_TENANT must equal it for the local single-tenant experiment"
    owner: "consolidated-runtime-v0 composition root; aegis owns TenantId"
    integration_risk: "HIGH — if the ingest default tenant and the query tenant differ, the experimenter ingests under acme and queries under (empty/other) and sees NOTHING. The minimal-friction posture sets them all to the same value (e.g. acme)."
    validation: "US-01 (same tenant => visible) + US-02 (different tenant => isolated/empty)."

  port_layout:
    source_of_truth: "DESIGN decision; defaults from existing binaries — ingest gRPC 4317 / HTTP 4318 (crates/aperture/src/lib.rs), metrics 9090, logs 9091, traces 9092 (the three query mains)"
    consumers: ["the OTLP push target (4318/4317)", "each query GET (9090/9091/9092)", "the run story C2 / docs C4"]
    owner: "consolidated-runtime-v0 composition root (DESIGN sets how they are configured)"
    integration_risk: "MEDIUM — all five must bind on ONE process without conflict; the fixed-port 4317/4318 flake (project memory) means tests should bind ephemeral ports and sweep+retry."
    validation: "US-01 / US-05 all-ports-bound scenarios."

  read_auth_validator:
    source_of_truth: "aegis::Validator, built once at composition when read-auth config is present (ADR-0074); audience kaleidoscope-query"
    consumers: ["router_with_auth(store, tenant, Some(validator), ..) on each query API"]
    owner: "read-path-query-api-auth-v0 (must NOT regress)"
    integration_risk: "MEDIUM — local experiment posture leaves auth OFF (validator None, env-tenant path). C1 must keep the OPTIONAL fail-closed bearer path available and unchanged when configured."
    validation: "US-02 technical notes; existing read-auth acceptance suites stay green."
```

## Consistency checks for DESIGN / DISTILL

- Does every `${port}` in the journey mockups resolve to a single configured source? (port_layout.)
- For each signal, is the sink's store and the router's store provably the SAME `Arc`? (the
  three `*_store_instance` artifacts — the load-bearing check.)
- Does the ingest default tenant equal the query tenant in the documented local posture?
  (default_tenant — a silent-empty trap if not.)
- Is there exactly ONE writer to the pillar root? (pillar_root.)
