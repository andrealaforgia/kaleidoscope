<!-- markdownlint-disable MD024 -->

# User Stories — `cinder-to-pulse-bridge-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. No `dyn Trait` where direct
  monomorphisation suffices, except at the existing trait-port boundaries
  (`cinder::MetricsRecorder` and `pulse::MetricStore`) which the bridge
  must honour by definition.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The bridge MUST be `Send + Sync` so that `Box<dyn cinder::MetricsRecorder + Send + Sync>`
  accepts it.
- Best-effort emission posture: `let _ = pulse.ingest(...)` matches the
  shipped `LumenToPulseRecorder`. `pulse::MetricStoreError` is empty at
  v0 so no real error path is exercised; the explicit discard documents
  forward compatibility.
- The bridge MUST NOT modify the tenant id, MUST serialise `Tier` as
  lowercase strings (`"hot"`/`"warm"`/`"cold"`), and MUST emit exactly
  one `MetricBatch` containing exactly one `Metric` with exactly one
  `MetricPoint` per Cinder event.
- File layout: bridge in `crates/self-observe/src/cinder_bridge.rs`;
  re-export from `crates/self-observe/src/lib.rs`; acceptance tests in
  `crates/self-observe/tests/cinder_to_pulse.rs`;
  `crates/self-observe/Cargo.toml` declares the new `cinder` dependency
  and the new `[[test]]` entry.

## Note on the `@infrastructure` slice rule

Per `nw-po-review-dimensions` Dimension 0 item 5: if every story in a
slice is `@infrastructure`, the slice has no release value and is
BLOCKED at slice level.

**This feature is library-only at v0.** Andrea decided (recorded in the
task brief and in `wave-decisions.md`) that the operator-visible CLI
surface is deferred to a separate follow-up feature. The slices here are
intentionally library-substrate slices.

The Elevator Pitch for each story therefore references the **Rust public
API entry point** (`self_observe::CinderToPulseRecorder`) used by the
operator binary, and the **Pulse `MetricStore::query` API** as the
operator-observable surface. This is a real entry point invoked by a
real downstream consumer (the post-v0 CLI feature) — it is not a
test-runner command or an internal helper.

The reviewer's blocking rule on all-infrastructure slices is acknowledged
and deliberately overridden by feature scope. The downstream CLI feature
will carry the operator-visible Elevator Pitches.

---

## US-01: Cinder `place` events land as queryable Pulse points

### Elevator Pitch

- **Before**: Priya wires Cinder with `NoopRecorder` (the default) and
  runs `cinder.place(&acme, &item, Tier::Hot, t)`. She queries Pulse
  expecting to see `acme`'s placement; Pulse returns an empty Vec
  because Cinder told nobody. Her only diagnostic is `println!` in
  Cinder source.
- **After**: Priya replaces `NoopRecorder` with
  `self_observe::CinderToPulseRecorder::new(pulse.clone())`. The exact
  same `cinder.place` call now produces a point queryable via
  `pulse.query(&acme, &MetricName::new("cinder.place.count"), TimeRange::all())`.
  She sees `[(Metric { name: "cinder.place.count", kind: Sum, ... },
  MetricPoint { value: 1.0, attributes: {"tier": "hot"}, ... })]`.
- **Decision enabled**: Priya can decide "is `acme`'s Hot-tier
  placement rate normal today?" without modifying Cinder source.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope deployment.
Cinder ships with `NoopRecorder` as the default `MetricsRecorder`, so
every `cinder.place` call evaporates into a noop. Priya finds it
operationally hostile to answer "did tenant `acme` just place an item in
Hot?" because her only workaround is patching Cinder source with
`println!` and rebuilding.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope deployment
for a fintech | already queries Pulse for Lumen events via the
`MetricStore::query` API and wants the same idiom for Cinder.

### Solution

A `CinderToPulseRecorder` struct in `crates/self-observe/src/cinder_bridge.rs`
that implements `cinder::MetricsRecorder`. The `record_place` method emits
one Pulse `MetricBatch` per call: one `Metric` named `cinder.place.count`
(Sum kind, unit `"1"`) with one `MetricPoint` (value 1.0, attribute
`tier = lowercase(input_tier)`).

### Domain Examples

#### 1. Happy path — Priya inspects fresh placement
Priya wires the bridge. `cinder.place(&TenantId("acme".into()),
&ItemId::new("trade-2026-05-18-001"), Tier::Hot, SystemTime::now())` runs.
Priya queries `pulse.query(&TenantId("acme".into()),
&MetricName::new("cinder.place.count"), TimeRange::all())`. She receives
a Vec of length 1 with `MetricPoint { value: 1.0,
attributes: { "tier": "hot" }, ... }`.

#### 2. Multi-tier — Priya verifies tier serialisation across all three tiers
Priya places three items for tenant `acme`: `trade-001` in Hot,
`trade-002` in Warm, `trade-003` in Cold. She queries
`cinder.place.count` for `acme` and receives 3 points with `tier`
attribute values exactly `{"hot", "warm", "cold"}` (as a set).

#### 3. Two-tenant isolation — Priya verifies `acme` and `globex` do not bleed
Priya places one item for `acme` (Hot) and two items for `globex` (both
Hot). Querying `cinder.place.count` for `acme` returns 1 point; for
`globex` returns 2 points. No cross-tenant leak.

### UAT Scenarios (BDD)

#### Scenario: Place under a tenant produces one queryable point under that tenant
```gherkin
Given Priya has constructed a CinderToPulseRecorder around a shared Pulse store
And tenant acme has no prior tier metadata
When cinder.place(&acme, &item("trade-2026-05-18-001"), Tier::Hot, t0) is called
Then pulse.query for acme on cinder.place.count returns exactly 1 point
And that point's value is 1.0
And that point's attributes contain tier="hot"
```

#### Scenario: Tier attribute reflects each of the three tier values
```gherkin
Given Priya has wired the bridge
When cinder.place is called for tenant acme with Tier::Hot, Tier::Warm, Tier::Cold (three distinct items)
Then pulse.query for acme on cinder.place.count returns 3 points
And the set of "tier" attribute values across the three points is exactly {"hot", "warm", "cold"}
```

#### Scenario: Per-tenant isolation under simultaneous placements
```gherkin
Given Priya has wired the bridge
When cinder.place is called once for tenant acme (Hot) and twice for tenant globex (both Hot)
Then pulse.query for acme on cinder.place.count returns exactly 1 point
And pulse.query for globex on cinder.place.count returns exactly 2 points
```

#### Scenario: No place call means no Pulse point
```gherkin
Given Priya has wired the bridge but called nothing on Cinder
When pulse.query is called for any tenant on cinder.place.count
Then the result is an empty Vec
```

### Acceptance Criteria

- [ ] `self_observe::CinderToPulseRecorder` exists and is publicly exported from `crates/self-observe/src/lib.rs`.
- [ ] `CinderToPulseRecorder` implements `cinder::MetricsRecorder`.
- [ ] On every `record_place(tenant, tier)` call, exactly one `MetricBatch` with one `Metric` named `"cinder.place.count"` (kind Sum, unit `"1"`) with one `MetricPoint` (value 1.0) is ingested into the Pulse store under `tenant`.
- [ ] The `MetricPoint.attributes` map contains the entry `"tier" -> lowercase(tier)`, with `Tier::Hot -> "hot"`, `Tier::Warm -> "warm"`, `Tier::Cold -> "cold"`.
- [ ] Two-tenant isolation test passes: acme and globex placements land in disjoint Pulse buckets.
- [ ] An unused bridge produces zero Pulse points.
- [ ] The bridge is `Send + Sync` (compile-time `assert_send_sync` test).

### Outcome KPIs

- **Who**: platform operator (Priya)
- **Does what**: receives a queryable `cinder.place.count` point per `place` call, partitioned by tenant
- **By how much**: 100% of `place` calls produce exactly one point (acceptance-test level)
- **Measured by**: green tests in `crates/self-observe/tests/cinder_to_pulse.rs` (Slice 01 block)
- **Baseline**: 0% (NoopRecorder)

Maps to OK1 in `outcome-kpis.md`.

### Technical Notes

- File: `crates/self-observe/src/cinder_bridge.rs` (new).
- Re-export: `pub use cinder_bridge::CinderToPulseRecorder;` in `crates/self-observe/src/lib.rs`.
- Dependency to add in `crates/self-observe/Cargo.toml`: `cinder = { path = "../cinder", version = "0.1.0" }`.
- Test file: `crates/self-observe/tests/cinder_to_pulse.rs` (new). Add a `[[test]]` entry in `Cargo.toml`.
- Timestamp source: `SystemTime::now()` -> nanos since Unix epoch as u64, mirroring `lumen_bridge.rs:53-56`.
- Slice tag: `@infrastructure` (library-level; user-visible CLI is a post-v0 feature).

### Dependencies

- `cinder` crate v0.1.0 (already shipped; `MetricsRecorder` trait is stable).
- `pulse` crate v0.1.0 (already shipped; `MetricStore` trait + `InMemoryMetricStore` adapter are stable).
- `aegis` crate (already a self-observe dependency; provides `TenantId`).
- No new external crates required.

### Slice

`slices/slice-01-place-events-land-in-pulse.md`

---

## US-02: Cinder `migrate` events land as queryable Pulse points with direction attributes

### Elevator Pitch

- **Before**: Priya needs to know how many Hot->Warm migrations tenant
  `acme` saw in the last hour. With `NoopRecorder` her only path is
  manual `println!` instrumentation. Even Cinder's
  `CapturingRecorder` (in-process test helper) cannot be queried
  through the same idiom as her Lumen dashboards.
- **After**: Priya runs the bridge. After workload execution she
  queries
  `pulse.query(&acme, &MetricName::new("cinder.migrate.count"), TimeRange::all())`
  and receives a Vec of `MetricPoint`s, each with `attributes: { "from": "hot", "to": "warm" }`.
  Counting matching attributes answers her question in one query.
- **Decision enabled**: Priya can decide "is `acme`'s Hot->Warm
  migration rate consistent with the configured tier policy?" without
  modifying Cinder source.

### Problem

Priya needs to see the **direction** of every tier migration per tenant.
A successful `cinder.migrate(&acme, &item, Tier::Warm, t)` (called when
the item was Hot) carries the information `from=hot, to=warm` — but
that information dies inside `NoopRecorder`. A failed migrate (the
item was never placed, returns `MigrateError::UnknownItem`) MUST NOT
produce a spurious Pulse point — otherwise Priya cannot distinguish
real migrations from bookkeeping errors.

### Who

Priya the platform operator | wants direction-resolved migration counts
per tenant | wants failed migrations to leave no trace in the metric
(`UnknownItem` is a caller bug, not a tier event).

### Solution

The bridge's `record_migrate(tenant, from, to)` method emits one Pulse
`MetricBatch` per call: one `Metric` named `cinder.migrate.count` (Sum
kind, unit `"1"`) with one `MetricPoint` (value 1.0, attributes
`from = lowercase(from)`, `to = lowercase(to)`). Because Cinder's
`InMemoryTieringStore::migrate` only calls `record_migrate` on success
(see `crates/cinder/src/store.rs` lines 174-188), a failed migrate
naturally produces no bridge call and therefore no Pulse point.

### Domain Examples

#### 1. Happy path — Priya tracks a Hot->Warm migration on `acme`
Priya places `trade-2026-05-18-001` for `acme` in Hot. She migrates it
to Warm. Querying `cinder.migrate.count` for `acme` returns one point
with attributes `{from: "hot", to: "warm"}`.

#### 2. Quiescence — failed migrate emits nothing
Priya calls `cinder.migrate(&acme, &item("ghost-item"), Tier::Warm, t)`
without first placing `ghost-item`. Cinder returns
`Err(MigrateError::UnknownItem)`. Priya queries `cinder.migrate.count`
for `acme` and receives an empty Vec.

#### 3. Per-tenant isolation under simultaneous opposite-direction migrations
Priya places `a1` for `acme` in Hot and `g1` for `globex` in Hot. She
migrates `a1` to Warm and `g1` to Cold. Querying `cinder.migrate.count`
for `acme` returns 1 point with `{from: hot, to: warm}`; for `globex`
returns 1 point with `{from: hot, to: cold}`. The wrong-tenant attributes
do not bleed.

### UAT Scenarios (BDD)

#### Scenario: Successful migrate emits one point with direction attributes
```gherkin
Given Priya has wired the bridge
And tenant acme has placed item("trade-2026-05-18-001") in Tier::Hot
When cinder.migrate(&acme, &item("trade-2026-05-18-001"), Tier::Warm, t1) returns Ok(())
Then pulse.query for acme on cinder.migrate.count returns exactly 1 point
And that point's value is 1.0
And that point's attributes contain from="hot" and to="warm"
```

#### Scenario: Failed migrate (UnknownItem) emits no point
```gherkin
Given Priya has wired the bridge
And tenant acme has placed nothing
When cinder.migrate(&acme, &item("ghost-item"), Tier::Warm, t1) returns Err(UnknownItem)
Then pulse.query for acme on cinder.migrate.count returns an empty Vec
```

#### Scenario: Per-tenant isolation under simultaneous migrations
```gherkin
Given Priya has wired the bridge
And tenant acme has placed item("a1") in Hot
And tenant globex has placed item("g1") in Hot
When cinder.migrate(&acme, &item("a1"), Tier::Warm, t) and cinder.migrate(&globex, &item("g1"), Tier::Cold, t) both succeed
Then pulse.query for acme on cinder.migrate.count returns 1 point with attrs {from: hot, to: warm}
And pulse.query for globex on cinder.migrate.count returns 1 point with attrs {from: hot, to: cold}
```

### Acceptance Criteria

- [ ] On every successful `record_migrate(tenant, from, to)` call, exactly one `MetricBatch` with one `Metric` named `"cinder.migrate.count"` (kind Sum, unit `"1"`) with one `MetricPoint` (value 1.0) is ingested into the Pulse store under `tenant`.
- [ ] The `MetricPoint.attributes` map contains exactly `"from" -> lowercase(from)` and `"to" -> lowercase(to)`.
- [ ] A failed migrate (Cinder returns `Err(UnknownItem)`) leaves zero points under `cinder.migrate.count` (Cinder does not invoke `record_migrate` on failure, which the bridge inherits).
- [ ] Two-tenant isolation test passes: acme and globex migrations land in disjoint Pulse buckets with their respective direction attributes.

### Outcome KPIs

- **Who**: platform operator (Priya)
- **Does what**: receives a queryable `cinder.migrate.count` point per successful `migrate` call, with `from`/`to` attributes
- **By how much**: 100% of successful migrate calls produce one correct point; 0% of failed migrates produce any point
- **Measured by**: green tests in `crates/self-observe/tests/cinder_to_pulse.rs` (Slice 02 block)
- **Baseline**: 0% (NoopRecorder)

Maps to OK2 in `outcome-kpis.md`.

### Technical Notes

- Adds one method body to `CinderToPulseRecorder` (the one introduced in US-01).
- No new file, no new dependency beyond US-01.
- The lowercase serialisation helper introduced in US-01 is reused.
- Slice tag: `@infrastructure`.

### Dependencies

- US-01 (the bridge struct + the lowercase tier helper).

### Slice

`slices/slice-02-migrate-events-land-in-pulse-with-direction.md`

---

## US-03: Cinder `evaluate` events land as queryable Pulse points with per-tenant migrated counts

### Elevator Pitch

- **Before**: Priya runs a periodic `cinder.evaluate_at(now, &policy)`
  in her tier-management loop. With `NoopRecorder` she has no way to
  answer "did the last evaluate run actually migrate anything for
  `acme`?" except by diffing `cinder.get_tier(&acme, &item)` for every
  known item before and after — which itself is unobservable through
  the same query idiom.
- **After**: Priya queries
  `pulse.query(&acme, &MetricName::new("cinder.evaluate.migrated.count"), TimeRange::all())`
  and gets one point per evaluate run with a non-zero migrated count for
  `acme`, with the point value equal to the migrated count. She also
  sees `N` corresponding points on `cinder.migrate.count` for the
  per-item migrations (one per migrated item, with `from`/`to`
  attributes), so she can cross-check totals.
- **Decision enabled**: Priya can decide "did the last hourly evaluate
  run produce the expected migration volume for `acme`?" via a single
  Pulse query.

### Problem

The `evaluate_at` operation is the periodic policy-driven migration
trigger. It is fundamentally **per-tenant aggregate**: one call sweeps
the entire store and may migrate dozens of items across multiple
tenants. The metric needed for Priya's question is **per (tenant,
evaluate-call)**, not per item. AND the per-item migrations themselves
need to remain visible on `cinder.migrate.count` (US-02), so the bridge
must not deduplicate or merge.

Additionally, `cinder::InMemoryTieringStore::evaluate_at` does NOT call
`record_evaluate` for tenants with zero migrations (see `store.rs` lines
218-230 — only tenants present in the `per_tenant` map after migration
get a `record_evaluate` call). This is upstream behaviour; the bridge
inherits it without modification.

### Who

Priya the platform operator | wants per-tenant per-evaluate aggregated
migration counts | wants the dual emission (per-item migrate + per-
tenant evaluate) to remain visible and unsurprising | does NOT want
ghost evaluate points for tenants that had nothing eligible to migrate.

### Solution

The bridge's `record_evaluate(tenant, migrated)` method emits one Pulse
`MetricBatch` per call: one `Metric` named
`cinder.evaluate.migrated.count` (Sum kind, unit `"1"`) with one
`MetricPoint` (value = `migrated as f64`, empty attributes). The
per-item `record_migrate` calls that Cinder also makes from
`evaluate_at` are handled by the US-02 path and produce normal
`cinder.migrate.count` points.

### Domain Examples

#### 1. Happy path — `acme` has 5 items eligible for Hot->Warm migration
Priya places 5 items for `acme` in Hot at `t0`. The policy is
"Hot items older than 24h migrate to Warm." At `t0 + 25h` she calls
`cinder.evaluate_at(t_now, &policy)`. Cinder migrates all 5 items.
Priya queries:
- `cinder.evaluate.migrated.count` for `acme` -> 1 point with value 5.0.
- `cinder.migrate.count` for `acme` -> 5 points, each with `{from: hot, to: warm}`.

#### 2. Zero-eligible tenant — no evaluate point emitted
Priya places 3 items for `acme` in Hot at `t0`. At `t0 + 1h` (before the
24h threshold) she calls `cinder.evaluate_at(t_now, &policy)`. Cinder
migrates nothing. Priya queries:
- `cinder.evaluate.migrated.count` for `acme` -> empty Vec.
- `cinder.migrate.count` for `acme` -> empty Vec.

#### 3. Mixed tenants in one evaluate — per-tenant counts split correctly
Priya places 5 items for `acme` in Hot at `t0` and 2 items for `globex`
in Hot at `t0`. At `t0 + 25h` she calls `cinder.evaluate_at(t_now, &policy)`.
Priya queries:
- `cinder.evaluate.migrated.count` for `acme` -> 1 point with value 5.0.
- `cinder.evaluate.migrated.count` for `globex` -> 1 point with value 2.0.
- `cinder.migrate.count` for `acme` -> 5 points; for `globex` -> 2 points.

### UAT Scenarios (BDD)

#### Scenario: Evaluate that migrates N items for one tenant emits N migrate points AND 1 evaluate point
```gherkin
Given Priya has wired the bridge
And tenant acme has placed 5 items in Hot at t0
And the tier policy migrates Hot items older than 24h to Warm
When cinder.evaluate_at(t0 + 25h, &policy) is called
Then cinder.evaluate_at returns 5
And pulse.query for acme on cinder.migrate.count returns exactly 5 points, each with attrs {from: hot, to: warm}
And pulse.query for acme on cinder.evaluate.migrated.count returns exactly 1 point with value 5.0
```

#### Scenario: Evaluate with zero eligible items emits no evaluate point for that tenant
```gherkin
Given Priya has wired the bridge
And tenant acme has placed 3 items in Hot at t0
And the tier policy migrates Hot items older than 24h to Warm
When cinder.evaluate_at(t0 + 1h, &policy) is called
Then cinder.evaluate_at returns 0
And pulse.query for acme on cinder.evaluate.migrated.count returns an empty Vec
And pulse.query for acme on cinder.migrate.count returns an empty Vec
```

#### Scenario: Two-tenant evaluate emits per-tenant evaluate points
```gherkin
Given Priya has wired the bridge
And tenant acme has placed 5 items in Hot at t0
And tenant globex has placed 2 items in Hot at t0
And the tier policy migrates Hot items older than 24h to Warm
When cinder.evaluate_at(t0 + 25h, &policy) is called
Then pulse.query for acme on cinder.evaluate.migrated.count returns 1 point with value 5.0
And pulse.query for globex on cinder.evaluate.migrated.count returns 1 point with value 2.0
And pulse.query for acme on cinder.migrate.count returns 5 points
And pulse.query for globex on cinder.migrate.count returns 2 points
```

### Acceptance Criteria

- [ ] On every `record_evaluate(tenant, migrated)` call, exactly one `MetricBatch` with one `Metric` named `"cinder.evaluate.migrated.count"` (kind Sum, unit `"1"`) with one `MetricPoint` (value = `migrated as f64`, empty attributes) is ingested into the Pulse store under `tenant`.
- [ ] Tenants with zero migrations in a given `evaluate_at` call produce zero `cinder.evaluate.migrated.count` points (Cinder does not call `record_evaluate` for them; the bridge inherits this).
- [ ] The per-item `cinder.migrate.count` points from US-02 remain emitted by the same `evaluate_at` call (the dual-emission contract is preserved).
- [ ] Multi-tenant `evaluate_at` produces per-tenant evaluate points with values equal to that tenant's individual migration count.

### Outcome KPIs

- **Who**: platform operator (Priya)
- **Does what**: receives a queryable `cinder.evaluate.migrated.count` point per (tenant, evaluate-call) pair with at least one migration, value = migrated count
- **By how much**: 100% of qualifying (tenant, evaluate) pairs produce one point; 0% of zero-migration pairs produce a point
- **Measured by**: green tests in `crates/self-observe/tests/cinder_to_pulse.rs` (Slice 03 block)
- **Baseline**: 0% (NoopRecorder)

Maps to OK3 in `outcome-kpis.md`.

### Technical Notes

- Adds the third method body to `CinderToPulseRecorder`.
- The `migrated_count as f64` cast is exact for any operationally-
  meaningful count.
- The dual-emission test in this slice is the highest-information-
  density test in the suite — DESIGN wave should preserve the test's
  cross-event-type assertion shape.
- Slice tag: `@infrastructure`.

### Dependencies

- US-01 (bridge struct).
- US-02 (the migrate emission path — the dual-emission test in this slice cross-asserts both metrics).

### Slice

`slices/slice-03-evaluate-events-land-in-pulse-with-per-tenant-counts.md`
