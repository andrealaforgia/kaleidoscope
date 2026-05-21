<!-- markdownlint-disable MD024 -->
# User Stories: durable-stores-integration-v0

## System Constraints

- **Brownfield**: `crates/integration-suite` already exists with a working
  first-triad precedent (`tests/v1_three_adapters_compose_under_restart.rs`).
  These stories extend that crate; they do not bootstrap it.
- **No new user surface**: the value is exercised through the integration-suite
  crate's `cargo test` target. No CLI subcommand, no metric-ingest path, no GUI
  is introduced (honest backend/quality framing).
- **Read-only collaborators**: pulse, ray, strata, aegis are consumed via their
  public surfaces only. This feature modifies no production crate; it adds one
  test file and two `[dev-dependencies]` + one `[[test]]` block to
  `crates/integration-suite/Cargo.toml`.
- **Crafter boundary**: per project CLAUDE.md, only `@nw-software-crafter` writes
  source under `crates/*/src/`. The integration test under
  `crates/integration-suite/tests/` is test code authored in the DELIVER wave.
- **CI realism**: any timing budget is set against GitHub Actions ubuntu-latest
  with generous headroom; correctness (recover-and-isolate) is the gate.
- **Public surfaces are fixed** (confirmed by reading the crates):
  - `pulse::FileBackedMetricStore::open(base_path, Box::new(NoopRecorder))`;
    `ingest(&TenantId, MetricBatch)`; `query(&TenantId, &MetricName, TimeRange)`.
  - `ray::FileBackedTraceStore::open(base_path, Box::new(NoopRecorder))`;
    `ingest(&TenantId, SpanBatch)`; `get_trace(&TenantId, &TraceId)`;
    `query(&TenantId, &ServiceName, TimeRange)`.
  - `strata::FileBackedProfileStore::open(base_path, Box::new(NoopRecorder))`;
    `ingest(&TenantId, ProfileBatch)`; `query(&TenantId, &ServiceName, TimeRange)`.

---

## US-01: Second triad recovers identically across a platform restart

### Problem

Priya Nair is the platform reliability engineer who owns the storage plane's
operational trust. The first triad (cinder + sluice + lumen) is already proven
to compose under one tenant and survive a drop-and-reopen. The second triad —
metrics (pulse), traces (ray), profiles (strata) — has durable v1 adapters and
its own per-crate suites, but nothing proves the three COMPOSE under one shared
tenant on the durable path. She finds it untrustworthy to declare the durable
plane sound when half the signal pillars have no composed restart evidence; her
only workaround is to reason about three separate per-crate suites in her head
and hope they hold together.

### Who

- Platform reliability engineer | restarting the platform process (deploy, reboot, OOM kill) | needs proof that all signal pillars recover together under one tenant with no leakage.

### Solution

Add `crates/integration-suite/tests/v1_three_durable_stores_compose.rs` with a
test that ingests metrics, spans and profiles for tenant `acme` (and parallel
data for tenant `globex`) into the three FileBacked durable stores, drops the
process (scope exit flushes), reopens all three from the same paths, and asserts
that `acme`'s data recovers identically in each pillar while `globex`'s data
never leaks into `acme`'s view. Mirrors the shape, helpers (`temp_root`,
`cleanup`, `tenant`) and structure of the first-triad file.

### Elevator Pitch

- **Before**: Priya cannot prove metrics, traces and profiles survive a restart
  together; she reasons across three disconnected per-crate suites and hopes.
- **After**: she runs
  `cargo test -p integration-suite --test v1_three_durable_stores_compose` and
  sees `test result: ok. 2 passed` — proving metrics + traces + profiles recover
  identically across a restart under one tenant, with zero cross-tenant leakage.
- **Decision enabled**: she can declare the durable storage plane trustworthy
  end-to-end (second triad at parity with the first) and green-light the next
  milestone, instead of holding it back on unproven composition.

### Domain Examples

#### 1: Happy Path — acme's three signals recover after reopen

Priya ingests, for tenant `acme`: a `MetricBatch` with a `Gauge` metric
`process.cpu.utilization` carrying three `MetricPoint`s at times 100/200/300; a
`SpanBatch` with two spans on service `checkout` sharing one `TraceId`; a
`ProfileBatch` with one `cpu` profile for service `checkout`. The three
FileBacked stores drop and reopen from the same temp paths. After reopen:
`pulse.query(&acme, &MetricName("process.cpu.utilization"), TimeRange::all())`
returns three points in time order; `ray.get_trace(&acme, &trace)` returns both
spans start-time ascending; `strata.query(&acme, &ServiceName("checkout"),
TimeRange::all())` returns the one profile.

#### 2: Edge Case — globex runs in parallel and stays isolated

Tenant `globex` ingests its own metric `billing.requests`, a span on service
`billing`, and a `heap` profile for service `billing`, into the same three
stores. After reopen, `acme`'s metric query contains no `billing.requests`
point, `acme`'s `checkout` trace contains no `billing` span, and
`strata.query(&acme, &ServiceName("billing"), TimeRange::all())` is empty. Each
tenant sees exactly its own data in every pillar.

#### 3: Error / Boundary — a pillar that loses data on reopen fails loudly

If `ray::FileBackedTraceStore` reopened and returned an empty trace for
`acme` (a WAL not flushed, or replay incomplete), the recovered span count
would be 0 instead of 2 and the assertion `assert_eq!(spans.len(), 2)` fails
with a clear count mismatch — the test catches partial-recovery asymmetry
rather than letting a half-durable triad ship.

### UAT Scenarios (BDD)

#### Scenario: Metrics, traces and profiles all survive a platform restart under one tenant

Given Priya has ingested metric points, two spans and one profile for tenant "acme" into the pulse, ray and strata durable stores
When the platform process drops and the three stores are reopened from their original paths
Then tenant "acme"'s metric points are recovered in ascending time order
And tenant "acme"'s spans are recovered, queryable by trace id and by service
And tenant "acme"'s profile is recovered for service "checkout"

#### Scenario: A parallel tenant's data never leaks across pillars

Given tenant "globex" has ingested its own metric, span and profile into the same three stores
When the stores are reopened and queried under tenant "acme"
Then no "globex" metric point appears under "acme"
And no "globex" span appears in "acme"'s trace or service query
And "acme"'s profile query for service "billing" returns empty

#### Scenario: A pillar that loses data on reopen is caught

Given tenant "acme"'s spans were ingested before the restart
When a pillar reopens and returns fewer records than were written
Then the test reports a recovery count mismatch and fails
And the half-durable triad is prevented from shipping green

### Acceptance Criteria

- [ ] After reopen, pulse returns exactly the metric points written for `acme`, in ascending `time_unix_nano` order. (Scenario 1)
- [ ] After reopen, ray returns the full `acme` trace by `TraceId` and the `acme` spans by `(tenant, ServiceName)`. (Scenario 1)
- [ ] After reopen, strata returns the `acme` profile for `(acme, "checkout")`. (Scenario 1)
- [ ] No `globex` record is visible under `acme` in any of the three pillars; `acme`'s query for `globex`'s service returns empty. (Scenario 2)
- [ ] A recovered count lower than the written count makes an assertion fail with a clear count mismatch. (Scenario 3)
- [ ] The test target is `v1_three_durable_stores_compose` and reports `test result: ok` on ubuntu-latest.

### Outcome KPIs

- **Who**: Priya (storage-plane trust owner).
- **Does what**: trusts that metrics, traces and profiles recover under one tenant after a restart, proven by a composed durable-path test.
- **By how much**: 100% compose-and-recover fidelity, zero cross-bucket leakage.
- **Measured by**: `cargo test -p integration-suite --test v1_three_durable_stores_compose` -> `test result: ok`.
- **Baseline**: 0% — no composed durable-path evidence for the second triad exists today.

### Technical Notes

- Add `ray` and `strata` to `[dev-dependencies]` in
  `crates/integration-suite/Cargo.toml` (pulse and aegis are already present);
  add a `[[test]]` block named `v1_three_durable_stores_compose`.
- Reuse the first-triad helpers verbatim in shape: `temp_root(test_name)`,
  `cleanup(root)`, `tenant(id)`. The write path must equal the reopen path
  (false-PASS guard, see shared-artifacts-registry.md).
- `NoopRecorder` from each crate; `Box::new(...)` into each `open`.
- Depends on US-02 sharing the same dev-deps and `[[test]]` wiring (both tests
  live in one file). No external blockers.

---

## US-02: Cross-crate tenant-identity contract holds across the signal pillars `@infrastructure`

> Labelled `@infrastructure` honestly. This story enables no new operator
> decision on its own; it is a compile-time regression guard. Per review
> Dimension 0, slice-level value is satisfied because the slice it belongs to
> also contains US-01 (a user-visible story). No Elevator Pitch is claimed —
> manufacturing a fake user surface here would be dishonest.

### Problem

A maintainer could change `aegis::TenantId`'s shape (the cross-crate identity
type) and not realise it has silently diverged the way pulse, ray and strata
key their per-tenant state. The first triad has an analogous contract test;
the signal triad does not. Without one, identity drift would surface as a
confusing runtime isolation bug rather than an immediate build failure.

### Who

- Maintainer touching `aegis` or any signal-pillar store | wants identity drift to fail at build time, not at 3am in production.

### Solution

Add a second test in the same file,
`tenant_id_is_the_cross_crate_identity_contract_for_signals`, that holds one
`aegis::TenantId` and passes the same `&TenantId` reference to all three
durable adapters with no conversion, then reads each back under it. If
`aegis::TenantId`'s shape drifts, the test fails to compile — the desired
early-warning behaviour. Mirrors test 2 of the first-triad file.

### Domain Examples

#### 1: Happy Path — one identity, three pillars

Priya holds `tenant("shared")`. She ingests one metric point, one span and one
profile, passing `&shared` to `pulse.ingest`, `ray.ingest` and
`strata.ingest`. Each store then reads exactly one record back under `&shared`:
`pulse.query(...).len() == 1`, `ray.get_trace(...).len() == 1`,
`strata.query(...).len() == 1`.

#### 2: Edge Case — no per-adapter tenant type

The same binding `let shared: aegis::TenantId = tenant("shared");` is the only
tenant value in the test. No adapter requires a conversion, a wrapper, or an
adapter-local tenant type; the test exercises that single-identity property by
construction.

#### 3: Error / Boundary — shape drift breaks the build

If `aegis::TenantId` changed (e.g. became a struct with extra fields or a
different constructor), `tenant("shared")` or the `&shared` passes would fail to
compile, alerting the maintainer that the cross-crate contract has shifted —
before any runtime isolation bug can occur.

### UAT Scenarios (BDD)

#### Scenario: One tenant identity is honoured by all three signal stores

Given Priya holds a single aegis::TenantId value "shared"
When she ingests one metric, one span and one profile using that same reference for all three durable stores
Then pulse reads its one metric point back under "shared"
And ray reads its one span back under "shared"
And strata reads its one profile back under "shared"

#### Scenario: No adapter required a tenant conversion

Given the test holds exactly one aegis::TenantId binding
When that same reference is passed to all three adapters
Then no adapter-specific tenant type or conversion is used
And the test compiles only because TenantId is shared verbatim across the three crates

### Acceptance Criteria

- [ ] One `aegis::TenantId` binding is passed by reference to pulse, ray and strata with no conversion. (Scenario 1, 2)
- [ ] Each store reads exactly one record back under that tenant. (Scenario 1)
- [ ] The test compiles only while `aegis::TenantId`'s shape is shared across the three crates; a shape change breaks compilation. (Scenario 2, boundary example 3)

### Outcome KPIs

- **Who**: any maintainer touching aegis or a signal-pillar store.
- **Does what**: is alerted at build time when the cross-crate `TenantId` contract drifts across the three signal pillars.
- **By how much**: 100% — drift is a compile failure, never a silent runtime divergence.
- **Measured by**: the test target compiles and passes; a shape change fails to compile.
- **Baseline**: no signal-pillar identity tripwire exists today.

### Technical Notes

- Co-located with US-01 in `v1_three_durable_stores_compose.rs`; shares the same
  dev-deps and `[[test]]` block. No separate wiring.
- Uses the same `temp_root`/`cleanup`/`tenant` helpers as US-01.
- Depends on US-01's dev-dependency additions; otherwise no external blockers.
</content>
