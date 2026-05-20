<!-- markdownlint-disable MD024 -->

# Pulse v1 — user stories

Fourth v0 to v1 carry-forward in the platform plane. After
Cinder v1, Sluice v1 and Lumen v1, the pattern of maturing an
in-memory store into a durable WAL-plus-snapshot adapter behind
the same trait is a settled property of the methodology. Pulse
is the metrics pillar; the job is obvious — durable metrics
survive a process restart — so this wave runs lightweight (no
JTBD, no walking skeleton; Pulse v0 already ships).

The operator here is the platform itself: the long-lived binary
that embeds Pulse to retain ingested metrics. Pulse has no CLI
surface yet, so the verifiable "After" is a library-level
round-trip — ingest points, drop the store, reopen at the same
path, query, observe the same points in the same order.

## System Constraints

- New durable adapter lives at `crates/pulse/src/file_backed.rs`;
  it implements the existing `MetricStore` trait verbatim
  alongside `InMemoryMetricStore`.
- v0 query contract preserved: per-`(tenant, metric_name)`
  isolation, ascending `time_unix_nano` ordering, half-open
  `[start, end)` range, `query` returns `Vec<(Metric, MetricPoint)>`.
- DISTILL writes `crates/pulse/tests/v1_slice_01_wal_durability.rs`
  and `crates/pulse/tests/v1_slice_02_snapshot.rs`. DELIVER writes
  the source. DISCUSS designs no implementation.
- Existing pulse v0 tests are not modified.
- AGPL-3.0-or-later.

## US-PV1-01 — WAL durability for metric ingest

### Elevator Pitch

- **Before**: Pulse v0 holds every ingested metric point in
  memory in an `InMemoryMetricStore`. A process crash or restart
  loses every point.
- **After**: run `cargo test -p pulse --test v1_slice_01_wal_durability`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests batches of points across tenants and metric names,
  drops the store, opens a new `FileBackedMetricStore` at the same
  path, queries, and asserts every point round-trips byte-stable
  in ascending `time_unix_nano` order with its owning `Metric`
  metadata intact.
- **Decision enabled**: the platform can embed Pulse in a
  long-lived process and trust that metrics ingested before a
  restart are still queryable after it — the operator decides to
  rely on Pulse for retention rather than re-scraping.

### Domain Examples

#### 1: Happy path — restart survives an ingest

Tenant `acme` ingests a `MetricBatch` carrying the gauge
`process.cpu.utilization` with three points at
`t=1_000`, `t=2_000`, `t=3_000` ns. The store is dropped. A fresh
`FileBackedMetricStore::open` at the same path then
`query(acme, process.cpu.utilization, TimeRange::all())` returns
the three points in ascending order, each paired with the gauge
`Metric` metadata (description, unit, resource attributes).

#### 2: Edge case — tenant and metric isolation across restart

Tenant `acme` ingests `http.server.duration.count` (a sum) while
tenant `globex` ingests `process.cpu.utilization` (a gauge).
After drop and reopen, querying `globex` for
`http.server.duration.count` returns an empty vector; each tenant
sees only its own series, and each `(tenant, metric_name)` series
is independent.

#### 3: Error / boundary — empty batch and corrupt WAL

Ingesting an empty `MetricBatch` for tenant `acme` is a no-op:
no WAL line is written and no state changes. Separately, a WAL
file with a truncated final line surfaces on `open` as
`MetricStoreError::PersistenceFailed` with the offending line
number, rather than a panic or silent data loss.

### UAT Scenarios

> Project uses Rust `#[test]` acceptance tests, not Gherkin. The
> scenarios below describe observable behaviours; DISTILL renders
> them as `#[test]` functions.

#### Scenario: Ingested points survive a restart

Given tenant `acme` has ingested a batch of 3 points under
`process.cpu.utilization` into a `FileBackedMetricStore` at path P
When the store is dropped and a new store is opened at path P
Then querying `acme` for `process.cpu.utilization` over the full
range returns the same 3 points in ascending time order with the
owning `Metric` metadata intact.

#### Scenario: Each tenant and metric stays isolated across restart

Given tenant `acme` ingested `http.server.duration.count` and
tenant `globex` ingested `process.cpu.utilization`
When the store is dropped and reopened
Then `globex` sees only `process.cpu.utilization` and `acme` sees
only `http.server.duration.count`.

#### Scenario: Recovered store honours the v0 query contract

Given a store reopened with points spanning `t=1_000` to `t=9_000`
When `query` is called with a half-open range `[2_000, 5_000)`
and when `query_with` adds a label predicate
Then only points inside the range (and matching the predicate)
are returned, in ascending order, exactly as the v0 adapter would.

#### Scenario: Empty batch is a no-op

Given an open `FileBackedMetricStore`
When an empty `MetricBatch` is ingested for tenant `acme`
Then no WAL record is written and a subsequent reopen recovers no
points for `acme`.

#### Scenario: Corrupt WAL fails loudly on open

Given a WAL file whose final line is truncated
When `FileBackedMetricStore::open` is called on that path
Then it returns `MetricStoreError::PersistenceFailed` naming the
offending line, and does not panic or silently drop records.

### Acceptance Criteria

- [ ] AC-1.1 — `FileBackedMetricStore::open(path, recorder)` opens
  or creates the WAL and replays existing records into per-series
  state.
- [ ] AC-1.2 — `ingest(tenant, batch)` appends an `Ingest` WAL
  record and updates in-memory per-`(tenant, metric_name)` state.
- [ ] AC-1.3 — A fresh `open` on the same path after drop recovers
  every prior point.
- [ ] AC-1.4 — Ascending `time_unix_nano` ordering within each
  series is preserved across restart (re-sorted on recovery).
- [ ] AC-1.5 — Byte-stable round-trip for every `MetricPoint`
  field (`time_unix_nano`, `start_time_unix_nano`, `attributes`,
  `value`) and every `Metric` field (`name`, `description`,
  `unit`, `kind`, `resource_attributes`).
- [ ] AC-1.6 — Per-tenant and per-metric-name isolation preserved
  across restart.
- [ ] AC-1.7 — `query` and `query_with` work against the recovered
  state with the v0 semantics (half-open range, predicate AND
  range, `Vec<(Metric, MetricPoint)>` shape).
- [ ] AC-1.8 — Corrupted WAL surfaces as
  `MetricStoreError::PersistenceFailed`.
- [ ] AC-1.9 — Empty batch ingest is a no-op (no WAL write, no
  state change).

### KPI anchor

- KPI 1 (Ingest latency): `ingest(batch_of_100)` p95 ≤ 2 ms on
  `FileBackedMetricStore`. The 2 ms budget is set with CI-realism
  margin from the first commit (see `outcome-kpis.md` § KPI 1 and
  the 2026-05-19 timing-bump lesson) — not a fast-workstation
  guess that fails on GitHub Actions.

## US-PV1-02 — Snapshot compaction for bounded recovery

### Elevator Pitch

- **Before**: the WAL grows linearly with every ingest, so
  recovery time grows linearly and unboundedly with the lifetime
  of the process.
- **After**: run `cargo test -p pulse --test v1_slice_02_snapshot`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests many points, calls `snapshot()`, ingests more,
  drops the store, reopens, and asserts the snapshot-plus-tail-WAL
  recovery yields exactly the same points as a pure-WAL recovery.
- **Decision enabled**: the platform sets a snapshot cadence in
  the embedding binary so recovery stays bounded regardless of how
  long the process has been ingesting.

### Domain Examples

#### 1: Happy path — compact then recover

Tenant `acme` ingests 10 000 points across several metrics. The
binary calls `snapshot()`; the WAL is truncated and the state is
written to the snapshot file. 100 more points are ingested into
the now-short WAL. After drop and reopen, every one of the 10 100
points is queryable — those from the snapshot and those from the
tail WAL.

#### 2: Edge case — snapshot-plus-WAL equals pure-WAL

A second store ingests the identical 10 100 points but never
snapshots. Querying both stores after reopen for the same
`(tenant, metric_name, range)` returns identical point sequences:
compaction changes the file layout, never the observable result.

#### 3: Error / boundary — idempotent snapshot

Calling `snapshot()` twice in succession with no intervening
ingest leaves a valid snapshot and an empty WAL; a reopen recovers
exactly the points present at the first snapshot, with no
duplication.

### UAT Scenarios

#### Scenario: Snapshot compacts the WAL without losing points

Given tenant `acme` has ingested 10 000 points and the binary has
called `snapshot()` then ingested 100 more points
When the store is dropped and reopened
Then all 10 100 points are queryable in ascending order.

#### Scenario: Snapshot-plus-WAL recovery matches pure-WAL recovery

Given one store that snapshotted mid-stream and one that never did,
both fed the identical 10 100 points
When both are reopened and queried over the same range
Then they return identical point sequences.

#### Scenario: Snapshot is idempotent

Given a store that has just snapshotted with no further ingest
When `snapshot()` is called again and the store is reopened
Then the recovered points are exactly those at the first snapshot,
with no duplication.

### Acceptance Criteria

- [ ] AC-2.1 — `snapshot()` writes current per-series state to the
  snapshot file and truncates the WAL.
- [ ] AC-2.2 — Recovery loads the snapshot first, then replays the
  remaining tail WAL on top.
- [ ] AC-2.3 — Snapshot-plus-WAL recovery matches pure-WAL recovery
  (parallel-store comparison over the same points).
- [ ] AC-2.4 — `snapshot()` is idempotent (no duplication on a
  second call without intervening ingest).

### KPI anchor

- KPI 2 (Recovery time): `open` p95 ≤ 2.5 s when recovering
  10 000 points from snapshot + WAL in a debug build. The 2.5 s
  budget matches post-bump Lumen v1 / Cinder v1 and carries the
  CI-realism margin from the first commit (see `outcome-kpis.md`
  § KPI 2).
