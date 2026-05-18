# Shared Artefacts Registry — `cinder-to-pulse-bridge-v0`

Every cross-step variable in `journey-observe-cinder-tier-transitions.yaml`,
its single source of truth, its consumers, and the integration risk if it
drifts.

## Registry

```yaml
shared_artifacts:

  tenant_id:
    source_of_truth: aegis::TenantId (constructed by operator binary; in tests, by the acceptance harness)
    consumers:
      - cinder API calls (place, migrate, evaluate_at)
      - bridge.record_place / record_migrate / record_evaluate forwarders
      - pulse.ingest (first argument)
      - pulse.query (first argument)
    owner: aegis crate
    integration_risk: |
      HIGH. Tenant identity is the partition key on both emission and
      query sides. The bridge MUST forward `&TenantId` unchanged. Any
      silent transform (interning, lowercase, trim) leaks data across
      tenants when the query side does not apply the same transform.
    validation: |
      Slice 02 + Slice 03 each include a two-tenant test asserting the
      acme/globex isolation invariant.

  pulse_store:
    source_of_truth: |
      The operator binary constructs ONE Arc<dyn MetricStore + Send + Sync>
      at startup (post-v0 follow-up feature). At v0 the acceptance tests
      construct it in-test.
    consumers:
      - CinderToPulseRecorder::new (sink for emissions)
      - operator's query path (read surface)
    owner: operator binary (or test harness at v0)
    integration_risk: |
      MEDIUM. If the operator wires two different MetricStore instances
      (one for the bridge, one for queries), the symptom is silent "no
      points found". v0 tests share one Arc explicitly; the post-v0 CLI
      wiring feature must enforce the shared-Arc invariant.
    validation: |
      All slices' acceptance tests clone the Arc and use it for both
      construction and query, mirroring the Lumen bridge test pattern.

  metric_name:
    source_of_truth: |
      String literals in self_observe::cinder_bridge.rs (DESIGN wave
      will decide whether they live as `const`s or inline). Three names:
        - "cinder.place.count"
        - "cinder.migrate.count"
        - "cinder.evaluate.migrated.count"
    consumers:
      - bridge emission (each record_* method)
      - operator's pulse.query calls
      - acceptance tests (string-literal asserts)
      - operator runbooks (post-v0)
    owner: self-observe crate
    integration_risk: |
      HIGH. The metric name is the contract between emission and query.
      A typo on either side returns silent empty results. Pulse does not
      did-you-mean.
    validation: |
      Acceptance tests in Slices 01/02/03 each assert the exact metric
      name string. Slice changelogs lock the names against drift.

  tier_value:
    source_of_truth: cinder::Tier enum (Hot / Warm / Cold)
    consumers:
      - cinder.place argument
      - cinder.migrate arguments (from + to)
      - bridge point attribute "tier" on cinder.place.count
      - bridge point attributes "from"/"to" on cinder.migrate.count
      - operator queries filtering by tier (post-v0, via pulse Predicate)
    owner: cinder crate
    integration_risk: |
      MEDIUM. The bridge must serialise Tier consistently. Convention:
      lowercase string. Tier::Hot -> "hot", Tier::Warm -> "warm",
      Tier::Cold -> "cold". Any deviation (uppercase, Debug repr "Hot",
      numeric) breaks operator queries that filter by tier value.
    validation: |
      Acceptance tests in Slice 01 assert the exact lowercase string
      values for tier attribute on place events. Slice 02 asserts the
      same convention for from/to on migrate events.

  migrated_count:
    source_of_truth: |
      Internal to cinder::InMemoryTieringStore::evaluate_at, computed
      per tenant as the count of items moved for that tenant in this
      evaluate call (see crates/cinder/src/store.rs lines 218-230).
    consumers:
      - record_evaluate(tenant, migrated) — Cinder calls the bridge
      - bridge emits cinder.evaluate.migrated.count with value=migrated as f64
      - operator's sum/avg queries over the metric (post-v0)
    owner: cinder crate
    integration_risk: |
      LOW. Pure arithmetic. The `as f64` cast is exact for counts up
      to 2^53, operationally impossible to exceed per evaluate.
    validation: |
      Slice 03 asserts the exact f64 value matches the per-tenant
      eligible-item count (5.0 for acme, 2.0 for globex).

  emission_timestamp:
    source_of_truth: |
      SystemTime::now() inside the bridge at emission time, converted
      to nanos-since-Unix-epoch as u64. Matches the LumenToPulseRecorder
      pattern.
    consumers:
      - MetricPoint.time_unix_nano (sort key for time-range queries)
    owner: bridge implementation
    integration_risk: |
      LOW. The timestamp is set by the bridge, not by Cinder. Operator
      queries by TimeRange::all() in v0 acceptance tests, sidestepping
      precision questions. v1 may revisit if operators want event-time
      vs ingest-time distinctions.
    validation: |
      Acceptance tests do not pin specific timestamps. They assert
      count of points + values + attributes. The Lumen bridge
      precedent uses the same approach.
```

## Consistency check (DISCUSS wave gate)

| Artefact | Source documented | Consumers documented | Risk classified | Validation pointer |
|----------|-------------------|---------------------|-----------------|-------------------|
| tenant_id | yes | yes | HIGH | Slices 02 + 03 |
| pulse_store | yes | yes | MEDIUM | all slices |
| metric_name | yes | yes | HIGH | Slices 01/02/03 |
| tier_value | yes | yes | MEDIUM | Slices 01 + 02 |
| migrated_count | yes | yes | LOW | Slice 03 |
| emission_timestamp | yes | yes | LOW | inherited from Lumen pattern |

All six artefacts have a single source of truth, documented consumers, a
risk classification, and an acceptance-test validation pointer. The
DISCUSS-wave horizontal-coherence gate **passes**.

## Cross-feature artefact interactions (none)

This bridge is library-only at v0. It does not interact with:

- `docs/product/journeys/incident-response.yaml` (orthogonal journey)
- `docs/product/jobs.yaml` (no new job promoted at v0)
- any pre-existing SSOT artefact

The post-v0 CLI follow-up will likely promote a new SSOT job
(`operator-observes-platform-internals`) and a new SSOT journey
once an operator-visible surface exists.
