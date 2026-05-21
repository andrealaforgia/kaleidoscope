# Application Architecture: durable-stores-integration-v0

> **Author**: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-21.
> **Feature**: second-triad durable composition guarantee. Test-only, inside the
> existing `crates/integration-suite` crate. No production `src/` change.

## What this feature is, architecturally

This feature adds no new component. It adds one piece of **integration
evidence**: a test that wires the three signal-pillar durable adapters
(`pulse::FileBackedMetricStore`, `ray::FileBackedTraceStore`,
`strata::FileBackedProfileStore`) under a single `aegis::TenantId`, exercises a
drop-and-reopen, and asserts composed durability and tenant isolation. It is the
exact peer of the first-triad test that already proves cinder + sluice + lumen
compose under restart.

The "architecture" of a test-only feature is the composition graph it exercises:
which adapters share which identity, and which durable path each one writes to.

## Component view (C4 Component, Mermaid)

The diagram shows the new test binary as the driver composing three driven
durable adapters, each owning its own base path under one shared temp root, all
keyed by one `aegis::TenantId`. Arrows are labelled with verbs.

```mermaid
C4Component
  title Component view — v1_three_durable_stores_compose (integration-suite)

  Container_Boundary(suite, "integration-suite (test crate, no src logic)") {
    Component(test, "v1_three_durable_stores_compose", "Rust integration test (2 tests)", "Composes the second triad under one tenant; drops and reopens; asserts recover-and-isolate")
  }

  System_Ext(aegis, "aegis::TenantId", "Shared identity type", "The single cross-crate tenant key")

  Container_Boundary(pillars, "Signal-pillar durable adapters (first-party libraries)") {
    Component(pulse, "pulse::FileBackedMetricStore", "Rust lib, WAL+snapshot", "Metrics, keyed by (tenant, MetricName)")
    Component(ray, "ray::FileBackedTraceStore", "Rust lib, WAL+snapshot", "Traces, dual index (tenant,trace_id)+(tenant,service)")
    Component(strata, "strata::FileBackedProfileStore", "Rust lib, WAL+snapshot", "Profiles, keyed by (tenant, ServiceName)")
  }

  ContainerDb(pulse_fs, "pulse-store/", "filesystem (temp_root)", "WAL + snapshot")
  ContainerDb(ray_fs, "ray-store/", "filesystem (temp_root)", "WAL + snapshot")
  ContainerDb(strata_fs, "strata-store/", "filesystem (temp_root)", "WAL + snapshot")

  Rel(test, aegis, "keys all three adapters with one")
  Rel(test, pulse, "ingests metrics into, then reopens and queries")
  Rel(test, ray, "ingests spans into, then reopens and queries")
  Rel(test, strata, "ingests profiles into, then reopens and queries")
  Rel(pulse, pulse_fs, "persists to and recovers from")
  Rel(ray, ray_fs, "persists to and recovers from")
  Rel(strata, strata_fs, "persists to and recovers from")
```

## Composition invariant exercised

```mermaid
flowchart LR
  A["open three FileBacked stores<br/>under one temp_root"] --> B["ingest acme + globex<br/>metrics, spans, profiles"]
  B --> C["drop scope<br/>(BufWriter flushes)"]
  C --> D["reopen all three<br/>from the SAME base paths"]
  D --> E["assert: acme recovers identically<br/>in each pillar"]
  E --> F["assert: globex never leaks<br/>into acme in any pillar"]
```

The write path equals the reopen path (false-PASS guard). The drop is the only
durability event under test; no explicit flush call is made, mirroring the
first-triad precedent.

## Earned Trust note (probing the durable boundary)

Each adapter depends on the **filesystem**, the highest-risk external dependency
in this design. This test IS the empirical probe for the composed durable path:
it does not assume the three stores honour their durability contract together, it
demonstrates it by writing, dropping the process scope, and reopening from disk.
The per-pillar v1 suites already probe each adapter's WAL+snapshot+replay in
isolation; this feature probes that the three honour it **simultaneously under one
tenant** with **no cross-bucket leakage** — the property no per-crate suite can
observe alone. The error/boundary scenario (a pillar returning fewer records than
written) is the catalogued substrate-lie this probe is designed to catch via a
clear count-mismatch assertion.

## Boundaries and ownership

- This DESIGN wave produces specifications only. The test source under
  `crates/integration-suite/tests/` is authored in the DELIVER wave by
  `@nw-software-crafter` (per project CLAUDE.md).
- No production crate is modified. pulse, ray, strata, aegis are consumed
  read-only through their public surfaces.
- Cargo.toml gains two path dev-deps (`ray`, `strata`) and one `[[test]]` block
  (`v1_three_durable_stores_compose`); see DD2.
