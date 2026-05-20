<!-- markdownlint-disable MD024 -->

# Ray v1 — user stories

Fifth v0 to v1 carry-forward in the platform plane. After Cinder
v1, Sluice v1, Lumen v1 and Pulse v1, the pattern of maturing an
in-memory store into a durable WAL-plus-snapshot adapter behind
the same trait is a settled property of the methodology. Ray is
the traces (spans) pillar; the job is obvious — durable spans
survive a process restart — so this wave runs lightweight (no
JTBD, no walking skeleton; Ray v0 already ships).

The operator here is the platform itself: the long-lived binary
that embeds Ray to retain ingested spans. Ray has no CLI surface
yet, so the verifiable "After" is a library-level round-trip —
ingest spans, drop the store, reopen at the same path, query,
observe the same spans in the same order.

The one shape difference from Pulse: Ray v0 keeps a **dual
index** — `by_trace` keyed on `(tenant, trace_id)` for
`get_trace`, and `by_service` keyed on `(tenant, service_name)`
for `query` / `query_with`. Spans are cloned into both maps on
ingest. Recovery must therefore rebuild **both** indices from the
WAL, so the WAL replay routes through a shared split routine
(like Pulse's `apply_ingest`, but populating two maps) rather than
the flat `extend` Lumen uses for its single per-tenant list. See
wave-decisions D5 for the full finding.

## System Constraints

- New durable adapter lives at `crates/ray/src/file_backed.rs`;
  it implements the existing `TraceStore` trait verbatim
  alongside `InMemoryTraceStore`.
- v0 query contract preserved: per-tenant isolation, ascending
  `start_time_unix_nano` ordering, half-open `[start, end)` range,
  `get_trace` returns the full trace, `query` / `query_with`
  return `Vec<Span>` for a service in a range.
- Both v0 indices are rebuilt on recovery: `by_trace` keyed on
  `(tenant, trace_id)` and `by_service` keyed on
  `(tenant, service_name)`. Spans with an empty `service.name`
  resource attribute populate only `by_trace`, mirroring v0
  ingest.
- DISTILL writes `crates/ray/tests/v1_slice_01_wal_durability.rs`
  and `crates/ray/tests/v1_slice_02_snapshot.rs`. DELIVER writes
  the source. DISCUSS designs no implementation.
- Existing ray v0 tests are not modified.
- AGPL-3.0-or-later.

## US-RV1-01 — WAL durability for span ingest

### Elevator Pitch

- **Before**: Ray v0 holds every ingested span in memory in an
  `InMemoryTraceStore` across two indices. A process crash or
  restart loses every span.
- **After**: run `cargo test -p ray --test v1_slice_01_wal_durability`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests batches of spans across tenants, traces and
  services, drops the store, opens a new `FileBackedTraceStore` at
  the same path, queries by trace and by service, and asserts
  every span round-trips byte-stable in ascending
  `start_time_unix_nano` order across both indices.
- **Decision enabled**: the platform can embed Ray in a long-lived
  process and trust that spans ingested before a restart are still
  queryable after it — the operator decides to rely on Ray for
  trace retention rather than losing in-flight traces on restart.

### Domain Examples

#### 1: Happy path — restart survives an ingest

Tenant `acme` ingests a `SpanBatch` carrying three spans of one
trace (`trace_id` `0x4bf9...`) emitted by service `checkout`, with
`start_time_unix_nano` at `t=1_000`, `t=2_000`, `t=3_000`. The
store is dropped. A fresh `FileBackedTraceStore::open` at the same
path then `get_trace(acme, 0x4bf9...)` returns the three spans in
ascending start-time order with every field intact (name, kind,
status, attributes, resource attributes, events, links), and
`query(acme, "checkout", TimeRange::all())` returns the same three
spans.

#### 2: Edge case — trace, service and tenant isolation across restart

Tenant `acme` ingests a `payments` span for trace `0xAAAA...`
while tenant `globex` ingests a `checkout` span for trace
`0xBBBB...`. After drop and reopen, `get_trace(globex, 0xAAAA...)`
returns an empty vector, `query(globex, "payments", all)` is
empty, and each tenant sees only its own traces and services.
A span whose `service.name` resource attribute is absent appears
in `get_trace` but never in any `query` by service.

#### 3: Error / boundary — empty batch and corrupt WAL

Ingesting an empty `SpanBatch` for tenant `acme` is a no-op: no
WAL line is written and no state changes. Separately, a WAL file
with a truncated final line surfaces on `open` as
`TraceStoreError::PersistenceFailed` with the offending line
number, rather than a panic or silent data loss.

### UAT Scenarios

> Project uses Rust `#[test]` acceptance tests, not Gherkin. The
> scenarios below describe observable behaviours; DISTILL renders
> them as `#[test]` functions.

#### Scenario: Ingested spans survive a restart

Given tenant `acme` has ingested a 3-span trace from service
`checkout` into a `FileBackedTraceStore` at path P
When the store is dropped and a new store is opened at path P
Then `get_trace` for that trace and `query` for `checkout` over
the full range both return the same 3 spans in ascending
start-time order with every span field intact.

#### Scenario: Traces, services and tenants stay isolated across restart

Given tenant `acme` ingested a `payments` span and tenant `globex`
ingested a `checkout` span, under different trace ids
When the store is dropped and reopened
Then each tenant sees only its own traces via `get_trace` and only
its own services via `query`, with no cross-tenant leakage.

#### Scenario: Spans without a service name recover into the trace index only

Given a span whose resource attributes carry no `service.name`
When the store is dropped and reopened
Then the span is returned by `get_trace` for its trace but never
by any `query` by service, exactly as the v0 adapter behaves.

#### Scenario: Recovered store honours the v0 query contract

Given a store reopened with spans spanning `t=1_000` to `t=9_000`
When `query` is called with a half-open range `[2_000, 5_000)`
and when `query_with` adds a span predicate
Then only spans inside the range (and matching the predicate) are
returned, in ascending start-time order, exactly as the v0
adapter would.

#### Scenario: Empty batch is a no-op

Given an open `FileBackedTraceStore`
When an empty `SpanBatch` is ingested for tenant `acme`
Then no WAL record is written and a subsequent reopen recovers no
spans for `acme`.

#### Scenario: Corrupt WAL fails loudly on open

Given a WAL file whose final line is truncated
When `FileBackedTraceStore::open` is called on that path
Then it returns `TraceStoreError::PersistenceFailed` naming the
offending line, and does not panic or silently drop records.

### Acceptance Criteria

- [ ] AC-1.1 — `FileBackedTraceStore::open(path, recorder)` opens
  or creates the WAL and replays existing records into both the
  `by_trace` and `by_service` indices.
- [ ] AC-1.2 — `ingest(tenant, batch)` appends an `Ingest` WAL
  record and updates both in-memory indices (mirrors v0 dual-index
  population).
- [ ] AC-1.3 — A fresh `open` on the same path after drop recovers
  every prior span into both indices.
- [ ] AC-1.4 — Ascending `start_time_unix_nano` ordering within
  each trace bucket and each service bucket is preserved across
  restart (re-sorted on recovery).
- [ ] AC-1.5 — Byte-stable round-trip for every `Span` field
  (`trace_id`, `span_id`, `parent_span_id`, `name`, `kind`,
  `start_time_unix_nano`, `end_time_unix_nano`, `status`,
  `attributes`, `resource_attributes`, `events`, `links`).
- [ ] AC-1.6 — Per-tenant, per-trace and per-service isolation
  preserved across restart.
- [ ] AC-1.7 — A span with no `service.name` recovers into
  `by_trace` only, never into `by_service`.
- [ ] AC-1.8 — `get_trace`, `query` and `query_with` work against
  the recovered state with v0 semantics (half-open range,
  predicate AND range, `Vec<Span>` shape).
- [ ] AC-1.9 — Corrupted WAL surfaces as
  `TraceStoreError::PersistenceFailed`.
- [ ] AC-1.10 — Empty batch ingest is a no-op (no WAL write, no
  state change).

### KPI anchor

- KPI 1 (Ingest latency): `ingest(batch_of_100)` p95 ≤ 2 ms on
  `FileBackedTraceStore`. The 2 ms budget is set with CI-realism
  margin from the first commit (see `outcome-kpis.md` § KPI 1 and
  the 2026-05-19 timing-bump lesson) — not a fast-workstation
  guess that fails on GitHub Actions.

## US-RV1-02 — Snapshot compaction for bounded recovery

### Elevator Pitch

- **Before**: the WAL grows linearly with every ingest, so
  recovery time grows linearly and unboundedly with the lifetime
  of the process.
- **After**: run `cargo test -p ray --test v1_slice_02_snapshot`
  → sees `test result: ok. N passed; 0 failed`. The acceptance
  test ingests many spans, calls `snapshot()`, ingests more, drops
  the store, reopens, and asserts the snapshot-plus-tail-WAL
  recovery yields exactly the same spans as a pure-WAL recovery
  across both indices.
- **Decision enabled**: the platform sets a snapshot cadence in
  the embedding binary so recovery stays bounded regardless of how
  long the process has been ingesting spans.

### Domain Examples

#### 1: Happy path — compact then recover

Tenant `acme` ingests 10 000 spans across several traces and
services. The binary calls `snapshot()`; the WAL is truncated and
the state is written to the snapshot file. 100 more spans are
ingested into the now-short WAL. After drop and reopen, every one
of the 10 100 spans is queryable — those from the snapshot and
those from the tail WAL — via both `get_trace` and `query`.

#### 2: Edge case — snapshot-plus-WAL equals pure-WAL

A second store ingests the identical 10 100 spans but never
snapshots. Querying both stores after reopen for the same trace
and for the same `(service, range)` returns identical span
sequences: compaction changes the file layout, never the
observable result.

#### 3: Error / boundary — idempotent snapshot

Calling `snapshot()` twice in succession with no intervening
ingest leaves a valid snapshot and an empty WAL; a reopen recovers
exactly the spans present at the first snapshot, with no
duplication in either index.

### UAT Scenarios

#### Scenario: Snapshot compacts the WAL without losing spans

Given tenant `acme` has ingested 10 000 spans and the binary has
called `snapshot()` then ingested 100 more spans
When the store is dropped and reopened
Then all 10 100 spans are queryable in ascending start-time order
via both `get_trace` and `query`.

#### Scenario: Snapshot-plus-WAL recovery matches pure-WAL recovery

Given one store that snapshotted mid-stream and one that never did,
both fed the identical 10 100 spans
When both are reopened and queried over the same trace and the same
`(service, range)`
Then they return identical span sequences.

#### Scenario: Snapshot is idempotent

Given a store that has just snapshotted with no further ingest
When `snapshot()` is called again and the store is reopened
Then the recovered spans are exactly those at the first snapshot,
with no duplication in either index.

### Acceptance Criteria

- [ ] AC-2.1 — `snapshot()` writes current dual-index state to the
  snapshot file and truncates the WAL.
- [ ] AC-2.2 — Recovery loads the snapshot first, then replays the
  remaining tail WAL on top, rebuilding both indices.
- [ ] AC-2.3 — Snapshot-plus-WAL recovery matches pure-WAL recovery
  (parallel-store comparison over the same spans, both indices).
- [ ] AC-2.4 — `snapshot()` is idempotent (no duplication on a
  second call without intervening ingest).

### KPI anchor

- KPI 2 (Recovery time): `open` p95 ≤ 2.5 s when recovering
  10 000 spans from snapshot + WAL in a debug build. The 2.5 s
  budget matches post-bump Pulse v1 / Lumen v1 / Cinder v1 and
  carries the CI-realism margin from the first commit (see
  `outcome-kpis.md` § KPI 2).
