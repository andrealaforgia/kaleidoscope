# Shared Artifacts Registry: durable-stores-integration-v0

Every value that flows across journey steps or across crate boundaries, with
its single source of truth and consumers. Untracked shared artifacts are the
primary cause of horizontal integration failure.

## Registry

```yaml
shared_artifacts:
  tenant_id:
    source_of_truth: "crates/aegis/src/lib.rs (aegis::TenantId)"
    consumers:
      - "pulse::FileBackedMetricStore::ingest / query"
      - "ray::FileBackedTraceStore::ingest / get_trace / query"
      - "strata::FileBackedProfileStore::ingest / query"
      - "the integration test (one &TenantId threaded through all three)"
    owner: "aegis crate"
    integration_risk: "HIGH — if aegis changes TenantId's shape, the cross-crate identity contract breaks at compile time across all three pillars."
    validation: "Identity-contract test passes the same &TenantId reference to all three adapters with no conversion; compiles only if the shape is shared."

  test_target_name:
    source_of_truth: "crates/integration-suite/Cargo.toml ([[test]] name)"
    consumers:
      - "the cargo test invocation in journey-second-triad-durability-visual.md"
      - "story-map.md"
      - "slices/slice-01-*.md and slices/slice-02-*.md"
      - "outcome-kpis.md (KPI measurement command)"
    owner: "integration-suite crate"
    integration_risk: "MEDIUM — a rename makes the documented command and the KPI measurement command wrong."
    validation: "The [[test]] name in Cargo.toml must equal v1_three_durable_stores_compose, matching every documented cargo test command."

  base_paths:
    source_of_truth: "the test's temp_root() helper (mirrors the first-triad file)"
    consumers:
      - "pulse FileBackedMetricStore::open(base, recorder) — write then reopen"
      - "ray FileBackedTraceStore::open(base, recorder) — write then reopen"
      - "strata FileBackedProfileStore::open(base, recorder) — write then reopen"
    owner: "integration-suite test"
    integration_risk: "HIGH — the path used to write must be the exact path used to reopen, or recovery silently reads an empty store and the test gives a false PASS."
    validation: "Each store's reopen call must reuse the identical PathBuf produced in Phase 1; cleanup() removes the root after assertions."

  dev_dependencies:
    source_of_truth: "crates/integration-suite/Cargo.toml [dev-dependencies]"
    consumers:
      - "the new test file's use statements (pulse already present; ray + strata must be added)"
    owner: "integration-suite crate"
    integration_risk: "MEDIUM — ray and strata are not yet dev-deps of integration-suite; the test will not compile until they are added."
    validation: "[dev-dependencies] lists pulse, ray, strata (and aegis); the new test compiles."
```

## Consistency checks

- [x] Every `${variable}` in the TUI mockups has a documented source.
- [x] `tenant_id` flows write -> reopen -> read identically across all three pillars (single source: aegis).
- [x] The documented `cargo test` command, the KPI measurement command, and the `[[test]]` name all reference `v1_three_durable_stores_compose`.
- [x] No hardcoded tenant strings diverge: `acme`, `globex`, `shared` are test fixtures, not shared production artifacts.
- [x] Reopen path == write path for each store (false-PASS guard).

## Cross-crate vocabulary

| Concept | Canonical type | Crate |
|---------|----------------|-------|
| Tenant identity | `TenantId` | aegis |
| Metric ingest unit | `MetricBatch` of `Metric` / `MetricPoint` | pulse |
| Trace ingest unit | `SpanBatch` of `Span` | ray |
| Profile ingest unit | `ProfileBatch` of `Profile` | strata |
| Metric lookup key | `(TenantId, MetricName)` | pulse |
| Trace lookup keys | `(TenantId, TraceId)` and `(TenantId, ServiceName)` | ray |
| Profile lookup key | `(TenantId, ServiceName)` | strata |

CLI vocabulary is consistent: every documented invocation is
`cargo test -p integration-suite --test v1_three_durable_stores_compose`.
</content>
