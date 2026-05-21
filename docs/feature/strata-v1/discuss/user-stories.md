<!-- markdownlint-disable MD024 -->

# Strata v1 — user stories

Sixth v0 to v1 carry-forward in the platform plane, and the LAST
storage pillar to gain a durable v1. After Cinder v1, Sluice v1,
Lumen v1, Pulse v1 and Ray v1, every pillar — lumen, pulse, ray,
strata, sluice, cinder — has a durable WAL-plus-snapshot adapter
behind the same trait. Strata is the profiles (pprof-shaped)
pillar; the job is obvious — durable profiles survive a process
restart — so this wave runs lightweight (no JTBD, no walking
skeleton; Strata v0 already ships `InMemoryProfileStore`).

The operator here is the platform itself: the long-lived binary
that embeds Strata to retain ingested profiles. Strata has no CLI
surface yet, so the verifiable "After" is a library-level
round-trip — ingest profiles, drop the store, reopen at the same
path, query, observe the same profiles in the same order.

Strata's index is the SIMPLEST of the v1 set: a single
per-service index, `HashMap<(TenantId, ServiceName),
Vec<Profile>>` sorted by `time_unix_nano` (store.rs lines 87-90,
confirmed in lib.rs line 44). This is closest to Pulse v1's single
keyed-series model and simpler than Ray's dual index. Recovery
rebuilds the one index from the WAL via a shared split routine
that returns the touched buckets, so only those buckets are
re-sorted (the v0 adapter already does this — store.rs lines
119-137 — and Strata inherits the touched-bucket discipline from
the first cut rather than learning it the hard way as Ray did).

The one wrinkle that drives the KPI budget is payload weight. A
`Profile` is the heaviest payload of any pillar: it carries
`samples` (each a stack with a location-id vector, a values
vector and an attribute map), plus `locations`, `functions`,
`mappings`, a `string_table` of every name / unit / filename /
build-id, and two resource/profile attribute maps. See
wave-decisions D5 and outcome-kpis.md KPI 1 for the budget
reasoning.

## System Constraints

- New durable adapter lives at `crates/strata/src/file_backed.rs`;
  it implements the existing `ProfileStore` trait verbatim
  alongside `InMemoryProfileStore`.
- v0 query contract preserved: per-tenant + per-service isolation
  keyed by `(TenantId, ServiceName)`, ascending `time_unix_nano`
  ordering within a service bucket, half-open `[start, end)`
  range, `query` / `query_with` return `Vec<Profile>`.
- A single index is rebuilt on recovery: `per_service` keyed on
  `(tenant, service_name)`. Profiles whose `service.name` resource
  attribute is empty are dropped from the index, mirroring v0
  ingest (store.rs lines 122-125).
- DISTILL writes
  `crates/strata/tests/v1_slice_01_wal_durability.rs` and
  `crates/strata/tests/v1_slice_02_snapshot.rs`. DELIVER writes
  the source. DISCUSS designs no implementation.
- Existing strata v0 tests are not modified.
- AGPL-3.0-or-later.

## US-SV1-01 — WAL durability for profile ingest

### Elevator Pitch

- **Before**: Strata v0 holds every ingested profile in memory in
  an `InMemoryProfileStore` (single per-service index). A process
  crash or restart loses every profile, including the heavy pprof
  sample payload.
- **After**: run
  `cargo test -p strata --test v1_slice_01_wal_durability`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests batches of profiles across tenants and services,
  drops the store, opens a new `FileBackedProfileStore` at the
  same path, queries by service and range, and asserts every
  profile round-trips byte-stable in ascending `time_unix_nano`
  order with the full sample payload intact.
- **Decision enabled**: the platform can embed Strata in a
  long-lived process and trust that profiles ingested before a
  restart are still queryable after it — the operator decides to
  rely on Strata for profile retention rather than losing
  in-flight profiles on restart.

### Domain Examples

#### 1: Happy path — restart survives an ingest

Tenant `acme` ingests a `ProfileBatch` carrying three `cpu`
profiles emitted by service `checkout`, with `time_unix_nano` at
`t=1_000`, `t=2_000`, `t=3_000`, each carrying a non-trivial
sample payload (several `Sample`s with location-id stacks, a
`string_table`, `locations`, `functions`). The store is dropped. A
fresh `FileBackedProfileStore::open` at the same path then
`query(acme, "checkout", TimeRange::all())` returns the three
profiles in ascending time order with every field intact
(`time_unix_nano`, `duration_nanos`, `profile_type`,
`sample_type`, `samples`, `locations`, `functions`, `mappings`,
`string_table`, `resource_attributes`, `attributes`).

#### 2: Edge case — service and tenant isolation across restart

Tenant `acme` ingests a `payments` heap profile while tenant
`globex` ingests a `checkout` cpu profile. After drop and reopen,
`query(globex, "payments", all)` is empty and each tenant sees
only its own services. A profile whose `service.name` resource
attribute is absent is dropped from the index, exactly as the v0
adapter behaves (store.rs lines 122-125), and never appears in any
query.

#### 3: Error / boundary — empty batch and corrupt WAL

Ingesting an empty `ProfileBatch` for tenant `acme` is a no-op: no
WAL line is written and no state changes. Separately, a WAL file
with a truncated final line surfaces on `open` as
`ProfileStoreError::PersistenceFailed` naming the offending line
number, rather than a panic or silent data loss.

### UAT Scenarios

> Project uses Rust `#[test]` acceptance tests, not Gherkin. The
> scenarios below describe observable behaviours; DISTILL renders
> them as `#[test]` functions.

#### Scenario: Ingested profiles survive a restart

Given tenant `acme` has ingested three `cpu` profiles from service
`checkout` (with full sample payloads) into a
`FileBackedProfileStore` at path P
When the store is dropped and a new store is opened at path P
Then `query` for `checkout` over the full range returns the same
three profiles in ascending `time_unix_nano` order with every
profile field intact, including the sample payload.

#### Scenario: Services and tenants stay isolated across restart

Given tenant `acme` ingested a `payments` profile and tenant
`globex` ingested a `checkout` profile
When the store is dropped and reopened
Then each tenant sees only its own services via `query`, with no
cross-tenant leakage.

#### Scenario: Profiles without a service name are dropped from the index

Given a profile whose resource attributes carry no `service.name`
When it is ingested and the store is dropped and reopened
Then it never appears in any `query` by service, exactly as the v0
adapter behaves.

#### Scenario: Recovered store honours the v0 query contract

Given a store reopened with profiles spanning `t=1_000` to
`t=9_000`
When `query` is called with a half-open range `[2_000, 5_000)`
and when `query_with` adds a `profile_type` predicate
Then only profiles inside the range (and matching the predicate)
are returned, in ascending time order, exactly as the v0 adapter
would.

#### Scenario: Empty batch is a no-op

Given an open `FileBackedProfileStore`
When an empty `ProfileBatch` is ingested for tenant `acme`
Then no WAL record is written and a subsequent reopen recovers no
profiles for `acme`.

#### Scenario: Corrupt WAL fails loudly on open

Given a WAL file whose final line is truncated
When `FileBackedProfileStore::open` is called on that path
Then it returns `ProfileStoreError::PersistenceFailed` naming the
offending line, and does not panic or silently drop records.

### Acceptance Criteria

- [ ] AC-1.1 — `FileBackedProfileStore::open(path, recorder)`
  opens or creates the WAL and replays existing records into the
  single `per_service` index.
- [ ] AC-1.2 — `ingest(tenant, batch)` appends an `Ingest` WAL
  record and updates the in-memory index (mirrors v0 per-service
  population, including dropping profiles with an empty
  `service.name`).
- [ ] AC-1.3 — A fresh `open` on the same path after drop recovers
  every prior profile into the index.
- [ ] AC-1.4 — Ascending `time_unix_nano` ordering within each
  service bucket is preserved across restart (touched buckets
  re-sorted on the live path; all buckets re-sorted once on
  recovery).
- [ ] AC-1.5 — Byte-stable round-trip for every `Profile` field
  (`time_unix_nano`, `duration_nanos`, `profile_type`,
  `sample_type`, `samples`, `locations`, `functions`, `mappings`,
  `string_table`, `resource_attributes`, `attributes`), including
  the full nested sample payload.
- [ ] AC-1.6 — Per-tenant and per-service isolation preserved
  across restart.
- [ ] AC-1.7 — A profile with no `service.name` is dropped from
  the index and never appears in a query.
- [ ] AC-1.8 — `query` and `query_with` work against the recovered
  state with v0 semantics (half-open range, predicate AND range,
  `Vec<Profile>` shape).
- [ ] AC-1.9 — Corrupted WAL surfaces as
  `ProfileStoreError::PersistenceFailed`.
- [ ] AC-1.10 — Empty batch ingest is a no-op (no WAL write, no
  state change).

### KPI anchor

- KPI 1 (Ingest latency): `ingest(batch_of_100)` p95 ≤ 8 ms on
  `FileBackedProfileStore`. The 8 ms budget is set deliberately
  higher than Ray's 5 ms because a `Profile` is the heaviest
  payload of any pillar (see outcome-kpis.md § KPI 1 and
  wave-decisions D5). CI-realism margin is baked in from the first
  commit per the 2026-05-19 timing-bump lesson.

## US-SV1-02 — Snapshot compaction for bounded recovery

### Elevator Pitch

- **Before**: the WAL grows linearly with every ingest, and
  because each profile is a heavy payload the WAL grows fast, so
  recovery time grows linearly and unboundedly with the lifetime
  of the process.
- **After**: run
  `cargo test -p strata --test v1_slice_02_snapshot`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests many profiles, calls `snapshot()`, ingests more,
  drops the store, reopens, and asserts the snapshot-plus-tail-WAL
  recovery yields exactly the same profiles as a pure-WAL recovery.
- **Decision enabled**: the platform sets a snapshot cadence in the
  embedding binary so recovery stays bounded regardless of how long
  the process has been ingesting heavy profile payloads.

### Domain Examples

#### 1: Happy path — compact then recover

Tenant `acme` ingests 2 000 profiles across several services. The
binary calls `snapshot()`; the WAL is truncated and the state is
written to the snapshot file. 100 more profiles are ingested into
the now-short WAL. After drop and reopen, every one of the 2 100
profiles is queryable — those from the snapshot and those from the
tail WAL — via `query`.

#### 2: Edge case — snapshot-plus-WAL equals pure-WAL

A second store ingests the identical 2 100 profiles but never
snapshots. Querying both stores after reopen for the same
`(service, range)` returns identical profile sequences: compaction
changes the file layout, never the observable result.

#### 3: Error / boundary — idempotent snapshot

Calling `snapshot()` twice in succession with no intervening
ingest leaves a valid snapshot and an empty WAL; a reopen recovers
exactly the profiles present at the first snapshot, with no
duplication in the index.

### UAT Scenarios

#### Scenario: Snapshot compacts the WAL without losing profiles

Given tenant `acme` has ingested 2 000 profiles and the binary has
called `snapshot()` then ingested 100 more profiles
When the store is dropped and reopened
Then all 2 100 profiles are queryable in ascending time order via
`query`.

#### Scenario: Snapshot-plus-WAL recovery matches pure-WAL recovery

Given one store that snapshotted mid-stream and one that never did,
both fed the identical 2 100 profiles
When both are reopened and queried over the same `(service, range)`
Then they return identical profile sequences.

#### Scenario: Snapshot is idempotent

Given a store that has just snapshotted with no further ingest
When `snapshot()` is called again and the store is reopened
Then the recovered profiles are exactly those at the first
snapshot, with no duplication in the index.

### Acceptance Criteria

- [ ] AC-2.1 — `snapshot()` writes the current per-service index
  state to the snapshot file and truncates the WAL.
- [ ] AC-2.2 — Recovery loads the snapshot first, then replays the
  remaining tail WAL on top, rebuilding the index.
- [ ] AC-2.3 — Snapshot-plus-WAL recovery matches pure-WAL recovery
  (parallel-store comparison over the same profiles).
- [ ] AC-2.4 — `snapshot()` is idempotent (no duplication on a
  second call without intervening ingest).

### KPI anchor

- KPI 2 (Recovery time): `open` p95 ≤ 2.5 s when recovering 2 000
  heavy profiles from snapshot + WAL in a debug build. The 2.5 s
  budget matches the post-bump Pulse v1 / Ray v1 / Lumen v1 /
  Cinder v1 figure and carries the CI-realism margin from the
  first commit (see outcome-kpis.md § KPI 2). The profile count
  (2 000) is lower than Ray's 10 000 spans because each profile is
  a far heavier payload — the count is chosen to be representative
  of the parse cost, not to inflate it.
