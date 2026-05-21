# Journey (Visual): Second-Triad Durability Guarantee

## Persona

Priya Nair, platform reliability engineer. She owns the Kaleidoscope storage
plane's operational trust. When she restarts the platform process (a deploy, a
host reboot, an OOM kill), she must be able to trust that every signal a tenant
has sent is still there afterwards, with no cross-tenant or cross-pillar
bleed. Her trust is earned by green integration evidence, not by hope.

## Goal

Prove, in compiled and exercised code, that the second triad of durable stores
(metrics via pulse, traces via ray, profiles via strata) composes under one
shared `aegis::TenantId` and recovers identically across a process restart,
mirroring the guarantee the first triad already gives.

## Trigger

The storage-plane milestone is complete: all six pillars have durable v1
adapters. The first triad is proven to compose-and-recover. The second triad is
not. Priya wants the same evidence for metrics + traces + profiles before she
will call the durable plane trustworthy end-to-end.

## Emotional arc (lightweight)

Problem Relief pattern. Start: uneasy (the second triad is unproven on the
composed durable path). Middle: focused (running the new test). End: relieved
and confident (a visible `test result: ok` is the trust artifact).

## ASCII flow

```
[Trigger: 2nd triad      [Run the new           [Read the PASS line]      [Goal: durable plane
 unproven on composed     compose+restart                                  trusted end-to-end]
 durable path]            integration test]
   Feels: uneasy            Feels: focused          Sees: test result: ok    Feels: relieved
   Sees: a gap in           Sees: build +           proving 3 pillars        Sees: parity with
   the trust matrix         test compiling          recover under 1 tenant   the first triad
   Artifacts:               Artifacts:              Artifacts:               Artifacts:
   first-triad file         v1_three_durable_       stdout PASS +            trust matrix
   (the precedent)          stores_compose test     2 passing tests          fully green
```

## Step 1 — Run the second-triad composition test

```
+-- Step 1: prove metrics+traces+profiles compose and survive restart -------+
| $ cargo test -p integration-suite \                                        |
|       --test v1_three_durable_stores_compose                               |
|                                                                            |
|    Compiling integration-suite v0.1.0                                      |
|     Running tests/v1_three_durable_stores_compose.rs                       |
|                                                                            |
| running 2 tests                                                            |
| test pulse_ray_strata_compose_under_shared_tenant_id_and_survive_restart   |
|   ... ok                                                                   |
| test tenant_id_is_the_cross_crate_identity_contract_for_signals ... ok     |
|                                                                            |
| test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured                 |
+----------------------------------------------------------------------------+
```

Mockup variables and their single source of truth are tracked in
`shared-artifacts-registry.md`. The `${tenant_id}` flowing through all three
adapters is `aegis::TenantId`, owned by the `aegis` crate. The test command
itself (`v1_three_durable_stores_compose`) is the `[[test]]` name declared in
`crates/integration-suite/Cargo.toml`.

## What "PASS" proves (the trust outcome)

1. `pulse::FileBackedMetricStore` recovers tenant `acme`'s metric points after
   reopen, in ascending time order, keyed by `(tenant, MetricName)`.
2. `ray::FileBackedTraceStore` recovers tenant `acme`'s spans after reopen,
   queryable both by `TraceId` and by `(tenant, ServiceName)`.
3. `strata::FileBackedProfileStore` recovers tenant `acme`'s profiles after
   reopen, keyed by `(tenant, ServiceName)`.
4. Tenant `globex`'s parallel state never leaks into `acme`'s view in any of
   the three pillars (zero cross-bucket leakage).
5. The same `&aegis::TenantId` reference threads through all three adapters
   with no conversion — the cross-crate identity contract holds.

## Failure modes (feed DISTILL error-scenario generation)

- A pillar loses points/spans/profiles across reopen (WAL not flushed or replay
  incomplete) — recovery count mismatch.
- A pillar returns another tenant's data under the queried tenant
  (cross-tenant leakage) — isolation breach.
- One pillar recovers but another silently returns empty — partial-recovery
  asymmetry across the triad.
- `aegis::TenantId` shape changes and a pillar no longer accepts the shared
  reference — compile-time contract break (desired early-warning behaviour).

## Material honesty note

This is a backend/quality journey. The medium is `cargo test`, not a UI. The
honest entry point is the test command and its stdout PASS line. We do not
manufacture a CLI subcommand or a metric-ingest path; the trust value lives in
the composed durability guarantee, observed through the integration suite.
</content>
