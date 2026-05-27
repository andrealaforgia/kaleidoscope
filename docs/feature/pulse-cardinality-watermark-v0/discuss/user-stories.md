<!-- markdownlint-disable MD024 -->

# User Stories: pulse-cardinality-watermark-v0

British English. No em dashes. No emoji.

This feature is M-4 from the residuality analysis: a per-tenant
cardinality watermark on `pulse`'s ingest path. The current
`apply_ingest` (`crates/pulse/src/file_backed.rs:349`) inserts every
new `(tenant, SeriesKey)` into the index without any ceiling, and a
client (misconfigured or hostile) emitting metrics with growing-
cardinality labels (a timestamp, a UUID, a per-request ID) drives the
process to OOM. ADR-0045 made series identity the full label set;
the residuality analysis flagged the resulting RAM bomb as S04 and
the pulse cell of the incidence matrix reads "B OOM under enough
labels".

This feature closes that gap by adding ONE compile-time per-tenant
cap (`MAX_SERIES_PER_TENANT`) on the shared `apply_ingest` seam, with
a refused counter visible to the operator. EXISTING series keep
ingesting; one tenant's bomb does not contaminate another tenant; the
walking-skeleton entry point is the existing OTLP gRPC and HTTP-
protobuf gateway path the gateway already wires into pulse via
`aperture-storage-sink` (`crates/aperture-storage-sink/src/lib.rs:463`).

## System Constraints

- The cap rides INSIDE the `pulse` store implementation, NOT on the
  `MetricStore` trait. Trait method signatures (`ingest`, `query`,
  `query_with`) and their other callers are UNCHANGED. The cap check
  lives in `apply_ingest` (and the in-memory adapter's equivalent
  per-metric loop), the place ADR-0045 already established as the
  single shared seam for live ingest and WAL replay.
- The cap is per-tenant, NOT global. The per-tenant count is a
  natural projection of the existing `HashMap<(TenantId, SeriesKey),
  SeriesEntry>` key (count entries whose first tuple field equals
  the tenant). Cross-tenant isolation is the WHOLE POINT (one
  tenant's bomb does not contaminate another); a global cap would
  re-couple tenants and would violate A-D4.
- The cap applies to NEW `SeriesKey`s only. An EXISTING
  `SeriesKey` (any tenant, any cap state) continues to receive
  points normally. No series is ever EVICTED to make room; the cap
  refuses, it does not displace. (ADR-0040 Decision 2's
  append-and-sort discipline is preserved.)
- The refused counter is per-tenant. Each refused ingest of a NEW
  above-cap `SeriesKey` increments the tenant's counter by 1. The
  counter is monotonically non-decreasing per tenant per store
  instance.
- NEVER PANIC. NEVER SILENTLY DROP. The refusal is observable via
  the counter (FLAG 2: receipt field, self-observe metric, or
  both). No `unwrap_or_default`, no silently-discarded point, no
  panic on cap breach.
- Slice 01 cap is a COMPILE-TIME CONSTANT
  (`const MAX_SERIES_PER_TENANT: usize = ...;` in
  `crates/pulse/src/file_backed.rs` and mirrored in
  `crates/pulse/src/store.rs`). Env-driven configurability (e.g.
  `KALEIDOSCOPE_PULSE_MAX_SERIES_PER_TENANT`) is explicitly
  DEFERRED. A future slice lifts the cap to env-driven.
- The walking-skeleton entry point is the EXISTING OTLP gateway
  path: OTLP client -> `kaleidoscope-gateway` -> `aperture::
  transport` -> `aperture::app::ingest_metrics` ->
  `aperture-storage-sink::ingest_metrics`
  (`crates/aperture-storage-sink/src/lib.rs:463`) ->
  `pulse::FileBackedMetricStore::ingest`. No new HTTP path, no new
  gRPC method, no new query parameter, no new wire envelope. The
  observable change is at pulse's ingest seam.
- Acceptance is via in-process integration tests, mirroring the
  other pulse slices (`crates/pulse/src/file_backed.rs` and its
  test module): a real `FileBackedMetricStore` on a `TempDir`, the
  test calls `store.ingest(&tenant, batch)` directly, asserts the
  receipt and the refused counter.
- WAL replay shares `apply_ingest` (the SAME shared path the live
  ingest uses; `crates/pulse/src/file_backed.rs:158`). The cap is
  uniform by construction. Replay rebuilds EXISTING series (they
  were ingested before the cap fired, so they are matching keys at
  replay time, not new keys above the cap); the cap applies only
  to NEW series at post-replay LIVE ingest.
- FLAGGED to DESIGN, NOT decided here:
  (1) the exact `MAX_SERIES_PER_TENANT` value (1_000? 10_000?
      100_000? LIKELY recommendation: 10_000);
  (2) the counter location: `IngestReceipt` field, self-observe
      metric via the existing bridge, or BOTH (LIKELY: BOTH);
  (3) batch semantics: PARTIAL APPLY (existing-series points
      ingest, new-above-cap refused, counter increments)
      vs REJECT-WHOLE (LIKELY: PARTIAL APPLY);
  (4) ADR-0051 (new) vs amendment of ADR-0045 (LIKELY: NEW
      ADR-0051 that cites ADR-0045 and refines its open question).
- OUT of scope for slice 01 (deferred and declared): env-driven
  cap configurability; structured event log beyond the counter;
  any change to the `MetricStore` trait method signatures; global
  (cross-tenant) cap; eviction of existing series; per-(tenant,
  metric-name) sub-caps; per-tenant weighting (e.g. by resource-
  attribute count); cap on points per series; cap on resource-
  attribute size per key; cap on `MetricBatch` size (the gateway's
  backpressure already handles this).

---

## US-01: The (N+1)th new SeriesKey for a tenant at the cap is refused; the N existing series keep receiving points

### Elevator Pitch

- Before: Maya Kowalski operates the `kaleidoscope-gateway` binary
  for tenant "acme-prod". A misconfigured client (Hands-off Hannah's
  hand-edited OTLP exporter) attaches a per-request UUID as a label
  on `http.server.duration` and starts sending OTLP
  `ExportMetricsServiceRequest`s via gRPC. The aperture transport
  accepts them, routes through `aperture-storage-sink::ingest_metrics`
  (`crates/aperture-storage-sink/src/lib.rs:463`) into
  `pulse::FileBackedMetricStore::ingest`, which calls `apply_ingest`
  (`crates/pulse/src/file_backed.rs:349`), which inserts each new
  `(tenant, SeriesKey)` into the `HashMap<(TenantId, SeriesKey),
  SeriesEntry>` (`crates/pulse/src/file_backed.rs:84`) with NO cap.
  The map grows without bound. Within minutes the process OOMs and
  is killed by the kernel. The residuality analysis flagged this as
  S04 with the pulse cell reading "B OOM under enough labels"; the
  A-U1 "Silent data loss" attractor is realised.
- After: the same OTLP traffic to the same gateway endpoint
  reaches `apply_ingest`, which counts the distinct
  `(acme-prod, _)` entries in the index. When the count is at or
  above `MAX_SERIES_PER_TENANT` (the exact value FLAGGED to
  DESIGN, LIKELY 10_000), each NEW `SeriesKey` is REFUSED:
  `or_insert_with` is NOT called, the points carried for that
  new metric are NOT stored, the refused counter for "acme-prod"
  increments by 1 per refused metric. The EXISTING N series keep
  receiving points normally: a matching key takes the same
  `entry().points.extend(points)` path it always has. The receipt
  reports `count` (points ingested into matching series) honestly
  and (per FLAG 2 LIKELY) `refused_new_series` (new keys refused
  in this call). The process stays alive; the index width stays at
  `MAX_SERIES_PER_TENANT`; Maya sees a tenant whose index is at
  the cap and whose refused counter is climbing, not a process
  that died.
- Decision enabled: Maya knows her tenant has hit the cap (the
  counter is non-zero and rising) and that NEW series are being
  refused; she contacts the tenant's client owner with the
  refused count as evidence, or (in a future slice with env-driven
  config) tunes the cap. The S04 OOM surface for the pulse cell of
  the residuality matrix is closed.

### Problem

Maya Kowalski runs Kaleidoscope's `kaleidoscope-gateway` binary as
the OTLP ingress for tenant "acme-prod". A new microservice in
"acme-prod"'s estate, hand-deployed without review, sends
`http.server.duration` metrics with `attributes={"request_id":
<UUID-per-request>}` attached to the resource (not the point). Each
distinct UUID is a distinct `resource_attributes` map, hence a
distinct `SeriesKey` under ADR-0045's identity rule. The aperture
gateway accepts the OTLP batches (they are well-formed per the
conformance harness), routes them into pulse, and the
`HashMap<(TenantId, SeriesKey), SeriesEntry>` grows by one entry per
request. Within the first hour of traffic the map holds tens of
thousands of one-point series; within a few hours the resident set
size of the gateway process approaches the cgroup limit; the kernel
OOM-kills the process. Maya sees an empty `journalctl` (the process
went down before flushing its logs), an alert from the host-level
monitor, and an angry note from "globex-staging"'s operator whose
tenant was hosted in the same gateway process and lost ingest
through no fault of theirs (the A-U1 silent-loss attractor in
action). She needs the platform to refuse new series above a
per-tenant cap with a named counter, BEFORE OOM kills the whole
process, BEFORE other tenants are affected.

### Who

- Maya Kowalski - platform operator for "acme-prod" - runs
  `kaleidoscope-gateway` and is woken up when its host-level monitor
  alerts on the process being OOM-killed. Needs the platform to
  refuse cardinality bombs out loud, per tenant, before the kill.
- Hands-off Hannah - the tenant's client owner who hand-deployed
  the OTLP exporter with the per-request-UUID label - rarely sees
  the gateway itself; she sees a refused-ingest count rising on
  her dashboard (or in a future Prism panel) and knows the
  exporter needs fixing.
- A misconfigured client (or an attacker probing) - the feature
  must refuse predictably even when the request was not made in
  good faith; the counter is the residuality A-D6 "honest
  three-way outcomes" guarantee at the ingest side.

### Solution

Add a compile-time constant `MAX_SERIES_PER_TENANT` to
`crates/pulse/src/file_backed.rs` (the exact value is FLAGGED to
DESIGN; see `wave-decisions.md` FLAG 1). Inside `apply_ingest` at
`crates/pulse/src/file_backed.rs:349`, before
`series.entry(key).or_insert_with(...)` for a key that DOES NOT
match an existing entry, count the current `(tenant, _)` entries
in the map. If the count is at or above the cap, REFUSE: skip the
`or_insert_with`, drop the points for that metric without storing
them, increment the per-tenant refused counter by 1 (one refusal
per refused metric, not per point inside it). If the count is
below the cap, proceed as today.

A matching `(tenant, SeriesKey)` lookup ALWAYS takes the existing
extend-points path, regardless of the per-tenant count: the cap
is about INDEX WIDTH, not about the points a series can receive
once it exists.

The same edit applies in the in-memory adapter at
`crates/pulse/src/store.rs:147` (the per-metric loop in
`InMemoryMetricStore::ingest`); the two adapters' semantics stay
in lockstep. The `MetricStore` trait method signatures stay
byte-identical to the prior tag.

The refused count surfaces via FLAG 2's mechanism. DISCUSS's
LIKELY recommendation is BOTH a `refused_new_series: usize` field
on `IngestReceipt` and a `pulse.cardinality.refused.count`
self-observe metric via the existing bridge pattern; DESIGN owns
the pick.

### Domain Examples

#### 1: Happy path - the first 9_999 new series for "acme-prod" are accepted normally (cap is 10_000 per FLAG 1 LIKELY)

Maya's well-behaved metric source emits 9_999 distinct
`SeriesKey`s for tenant "acme-prod" over the course of a day. The
gateway accepts each OTLP batch, aperture-storage-sink routes
each into pulse, `apply_ingest` counts the per-tenant series
(starts at 0, rises to 9_999 entry by entry), each new key is
under the cap so each is inserted normally. The refused counter
for "acme-prod" stays at 0; the receipt's `count` reports the
points ingested per batch as today; the receipt's
`refused_new_series` (per FLAG 2 LIKELY) stays at 0.

#### 2: Refuse - the 10_001st distinct SeriesKey is REFUSED; the existing 10_000 keep ingesting

The misconfigured client emits the 10_000th distinct UUID
(filling the cap exactly), then the 10_001st. The 10_000th is
accepted (the cap is `>= MAX_SERIES_PER_TENANT` REFUSE, so the
10_000th is the boundary tick still within the cap). The
10_001st is REFUSED: `apply_ingest` counts 10_000 acme-prod
entries already in the map, sees the would-be new key would
push past the cap, skips `or_insert_with`, drops the metric's
points, increments the acme-prod refused counter to 1. The
receipt for the batch that contained the 10_001st reports
`refused_new_series: 1` (per FLAG 2 LIKELY).

Meanwhile, a well-behaved metric source for "acme-prod" sends a
point to an EXISTING SeriesKey (one of the 10_000 already in the
index). `apply_ingest` looks up the key, finds the entry,
extends its points vector, sorts. The point is stored; the
receipt's `count` reports 1; the refused counter stays at 1
(unaffected, because this was not a new-key insertion).

#### 3: Boundary - the cap fires at strictly above the limit, not at the limit

The cap check is half-open in the same sense the M-2 window cap
is: a count of EXACTLY `MAX_SERIES_PER_TENANT - 1` allows the
NEXT new key (the entry that brings the count to
`MAX_SERIES_PER_TENANT`) to be inserted; a count of EXACTLY
`MAX_SERIES_PER_TENANT` refuses the NEXT new key (the would-be
count of `MAX_SERIES_PER_TENANT + 1`). This boundary kills a `>=`
-> `>` mutant on the cap check the way the M-2 window cap kills
its boundary mutant.

### UAT Scenarios (BDD)

#### Scenario: A new SeriesKey is accepted while the tenant is under the cap

```gherkin
Given a FileBackedMetricStore opened on a temporary directory
And the cap MAX_SERIES_PER_TENANT is N (DESIGN-chosen, LIKELY 10_000)
And tenant "acme-prod" has 0 SeriesKeys in the index
When Maya ingests a batch containing 1 new metric named "cpu.utilisation" with resource_attributes={"service.name":"checkout"}
Then the ingest succeeds with receipt.count equal to the number of points in the batch
And the refused counter for "acme-prod" is 0
And the index width for "acme-prod" is 1
```

#### Scenario: The (N+1)th new SeriesKey for a tenant at the cap is refused; the refused counter increments

```gherkin
Given a FileBackedMetricStore opened on a temporary directory
And the cap MAX_SERIES_PER_TENANT is N
And tenant "acme-prod" has N distinct SeriesKeys already in the index (seeded by ingest)
When Maya ingests a batch containing 1 new metric "http.server.duration" with a previously-unseen resource_attributes map for "acme-prod"
Then the ingest call returns successfully (no panic, no MetricStoreError)
And the receipt's refused_new_series field equals 1 (per FLAG 2 LIKELY: receipt-field surface)
And the index width for "acme-prod" remains exactly N (the new key was NOT inserted)
And the points carried by that metric are NOT stored anywhere in the store
And the refused counter for "acme-prod" is 1
```

#### Scenario: An existing SeriesKey keeps receiving points after the tenant has reached the cap

```gherkin
Given a FileBackedMetricStore at the cap for tenant "acme-prod" with N distinct SeriesKeys
And one specific existing SeriesKey K (name="cpu.utilisation", resource_attributes={"service.name":"checkout"}) holds 5 points
When Maya ingests a batch containing 1 metric matching K with 3 new points
Then the ingest succeeds with receipt.count equal to 3
And K now holds 8 points sorted ascending by time_unix_nano
And the refused counter for "acme-prod" remains unchanged
And the index width for "acme-prod" remains exactly N
```

#### Scenario: The boundary at exactly N-1 admits one more new key, and exactly N refuses the next

```gherkin
Given a FileBackedMetricStore with tenant "acme-prod" at exactly N-1 distinct SeriesKeys
When Maya ingests a batch with 1 new metric whose SeriesKey is not yet in the index
Then the ingest succeeds, the new key is inserted, the index width for "acme-prod" is N
And the refused counter for "acme-prod" is 0
And when Maya ingests another batch with 1 different new metric whose SeriesKey is not yet in the index
Then the ingest call returns successfully, the new key is NOT inserted, the index width remains N
And the receipt's refused_new_series is 1
And the refused counter for "acme-prod" is 1
```

#### Scenario: The MetricStore trait signature is unchanged

```gherkin
Given the pulse workspace as of slice 01 of this feature
When the public-api diff is computed against the prior pulse tag
Then no method on MetricStore is added, removed, or re-signed
And the only public-api change (if any) is an additive field on IngestReceipt guarded by FLAG 2's DESIGN pick
```

### Acceptance Criteria

- [ ] A new SeriesKey is accepted while the tenant is under the cap (Scenario 1).
- [ ] The (N+1)th new SeriesKey at the cap is refused, the index width stays at N, the refused counter increments, the points are not stored (Scenario 2).
- [ ] An existing SeriesKey keeps receiving points after the tenant has reached the cap (Scenario 3).
- [ ] The boundary at exactly N-1 admits one more new key; exactly N refuses the next (Scenario 4).
- [ ] `pulse::MetricStore` trait method signatures are unchanged; the only public-api change is at most an additive field on `IngestReceipt` per FLAG 2 (Scenario 5).

### Outcome KPIs

- **Who**: an operator (Maya Kowalski) of the `kaleidoscope-gateway` binary at tenant "acme-prod".
- **Does what**: sees the `(N+1)`th NEW SeriesKey refused, the refused counter incrementing, the N existing series still ingesting, and the process staying alive, instead of an OOM kill that takes the whole gateway down.
- **By how much**: 100 percent of NEW-above-cap ingests in the acceptance suite refuse with the counter incrementing; 100 percent of EXISTING-series ingests post-cap succeed; the index width stays at exactly `MAX_SERIES_PER_TENANT`.
- **Measured by**: the slice-01 acceptance suite outcomes on `pulse`, plus 100 percent mutation kill on the changed files (ADR-0005 Gate 5).
- **Baseline**: 0 percent today. `apply_ingest` at `crates/pulse/src/file_backed.rs:349` inserts every new key without any cap; under enough labels the process OOMs (incidence-matrix S04 row, pulse cell "B OOM under enough labels").

### Technical Notes (DESIGN-flagged, NOT decided here)

- The exact value of `MAX_SERIES_PER_TENANT` is FLAGGED to DESIGN
  (`wave-decisions.md` FLAG 1). DISCUSS recommends 10_000 as a
  starting default.
- The cap lives in `apply_ingest`
  (`crates/pulse/src/file_backed.rs:349`) and the equivalent
  per-metric loop in `InMemoryMetricStore::ingest`
  (`crates/pulse/src/store.rs:147`). The two adapters' semantics
  stay in lockstep.
- The counter location is FLAGGED (FLAG 2). DISCUSS recommends
  BOTH a `refused_new_series: usize` field on `IngestReceipt` and a
  `pulse.cardinality.refused.count` self-observe metric.
- The per-tenant count is a natural projection of the existing map
  key: iterate `series.keys()` and count entries whose first tuple
  field equals the tenant. DESIGN MAY choose to maintain a
  shadow per-tenant counter for O(1) lookup; DISCUSS does not
  pin this.
- Dependencies: none beyond the existing `pulse` shape and the
  existing aperture-storage-sink wiring.

---

## US-02: Tenant A's cap breach does not affect tenant B's ingest

### Elevator Pitch

- Before: the gateway hosts two tenants, "acme-prod" and
  "globex-staging". A cardinality bomb on "acme-prod" (per US-01)
  grows the pulse index without bound and OOMs the WHOLE gateway
  process; "globex-staging"'s ingest stops too, even though
  "globex-staging" has done nothing wrong. Globex Steady (the
  operator of "globex-staging") sees an outage for which her
  tenant is not responsible. The A-D4 "Fail-closed tenancy at
  every plane boundary" attractor is violated at the resource
  level: per-tenant tenancy works at the auth seam, but per-tenant
  resource isolation is absent at the ingest seam.
- After: the cap is per-tenant. When "acme-prod" hits its cap,
  only "acme-prod"'s NEW SeriesKeys are refused; "globex-staging"
  can still ingest new SeriesKeys up to its OWN cap (which it is
  nowhere near). The pulse map width stops growing for
  "acme-prod" but continues to grow for "globex-staging". The
  process does not OOM. Globex Steady's ingest is unaffected. The
  A-D4 attractor is preserved at the resource axis too.
- Decision enabled: Globex Steady knows her tenant is safe from
  her neighbour's mistakes; she does not need to escalate. Maya
  contacts the tenant client owner for "acme-prod" alone; the
  problem is scoped to the tenant that caused it.

### Problem

Kaleidoscope is multi-tenant by design. The `aegis`-resolved
`TenantId` is the first element of the pulse map key, and the
per-tenant fail-closed posture (A-D4) is established at every
plane boundary. But resource consumption is implicitly global:
the SAME `HashMap` holds entries for ALL tenants, so an unbounded
growth on one tenant's keys grows the whole map and OOMs the
whole process. Pre-M-4 the platform's tenancy story is "fail
closed on auth, share resources at runtime"; M-4 closes the
resource-sharing leak for cardinality.

Globex Steady runs "globex-staging" through the same gateway
process Maya runs "acme-prod" through. She has nothing to do with
"acme-prod"'s misconfigured exporter, has no visibility into it,
and has no remedy when the gateway crashes from someone else's
bomb. She needs per-tenant resource isolation as an explicit
guarantee, not as an implicit property that holds until it does
not.

### Who

- Globex Steady - operator of tenant "globex-staging" - runs the
  same gateway process Maya runs "acme-prod" through. Needs her
  tenant's ingest to keep working when another tenant misbehaves.
- Maya Kowalski - operator of "acme-prod" - the upstream of the
  bomb. The per-tenant scoping localises her remediation to her
  own tenant.
- The platform itself - the A-D4 attractor "Fail-closed tenancy
  at every plane boundary" is the residue this story defends at
  the resource axis.

### Solution

The cap counts entries by tenant (per US-01). The check is a
per-tenant projection of the existing map; tenant A's count
does NOT include tenant B's entries and vice versa. When tenant
A is at or above the cap, NEW SeriesKeys for tenant A are
refused; NEW SeriesKeys for tenant B are accepted up to tenant
B's OWN cap (which tenant B is presumably far from). The refused
counter is per-tenant: tenant A's counter ticks; tenant B's
counter stays at 0.

This is implicit in US-01's solution (the cap is by definition
per-tenant; the count is over `(tenant, _)` entries), but the
test asserts it explicitly to kill a mutant that swaps the
per-tenant count for a global one (e.g. `series.len()` instead
of `series.keys().filter(|(t, _)| t == tenant).count()`).

### Domain Examples

#### 1: Tenant A at cap, tenant B fresh - tenant B's new SeriesKey is accepted

Tenant "acme-prod" has `MAX_SERIES_PER_TENANT` distinct
SeriesKeys (filled to the cap). Tenant "globex-staging" has 0
SeriesKeys. Maya attempts a NEW SeriesKey on "acme-prod"
(refused; counter on acme-prod ticks). Globex Steady attempts a
NEW SeriesKey on "globex-staging" (accepted; counter on
globex-staging stays at 0; the index width for globex-staging is
1).

#### 2: Both tenants below cap - both succeed independently

Tenant "acme-prod" has 5_000 SeriesKeys. Tenant "globex-staging"
has 3_000 SeriesKeys. Both ingest new SeriesKeys. Both succeed.
Both refused counters stay at 0.

#### 3: Tenant A reaches its cap, tenant B reaches its cap independently

Tenant "acme-prod" reaches `MAX_SERIES_PER_TENANT` first;
"acme-prod"'s next new key is refused, counter ticks to 1.
Tenant "globex-staging" continues normally; its index grows;
eventually it reaches `MAX_SERIES_PER_TENANT` independently; its
next new key is refused, ITS counter ticks to 1. The two
counters and the two index widths are tracked separately.

### UAT Scenarios (BDD)

#### Scenario: Tenant B is unaffected while tenant A is at the cap

```gherkin
Given a FileBackedMetricStore with cap MAX_SERIES_PER_TENANT equal to N
And tenant "acme-prod" has N distinct SeriesKeys in the index
And tenant "globex-staging" has 0 SeriesKeys in the index
When Globex Steady ingests a batch containing 1 new metric for "globex-staging"
Then the ingest succeeds with receipt.count equal to the number of points in the batch
And the index width for "globex-staging" is 1
And the refused counter for "globex-staging" is 0
And the refused counter for "acme-prod" is unchanged by this call
And the index width for "acme-prod" remains exactly N
```

#### Scenario: Both tenants ingest independently below the cap

```gherkin
Given a FileBackedMetricStore with cap MAX_SERIES_PER_TENANT equal to N
And tenant "acme-prod" has K_a distinct SeriesKeys where 0 < K_a < N
And tenant "globex-staging" has K_b distinct SeriesKeys where 0 < K_b < N
When both Maya and Globex Steady ingest one new SeriesKey each, on their own tenants
Then both ingests succeed
And the index width for "acme-prod" is K_a + 1
And the index width for "globex-staging" is K_b + 1
And both refused counters are 0
```

#### Scenario: The two tenants reach their caps independently and refuse independently

```gherkin
Given a FileBackedMetricStore with cap MAX_SERIES_PER_TENANT equal to N
And tenant "acme-prod" has N distinct SeriesKeys (at the cap)
And tenant "globex-staging" has N distinct SeriesKeys (also at the cap)
When Maya attempts to ingest 1 new SeriesKey on "acme-prod"
Then the ingest call returns successfully
And the refused counter for "acme-prod" is 1
And the refused counter for "globex-staging" is 0
And when Globex Steady attempts to ingest 1 new SeriesKey on "globex-staging"
Then the ingest call returns successfully
And the refused counter for "globex-staging" is 1
And the refused counter for "acme-prod" remains 1 (unchanged by Globex Steady's call)
```

### Acceptance Criteria

- [ ] Tenant B's new SeriesKey is accepted while tenant A is at the cap (Scenario 1).
- [ ] Both tenants ingest independently below the cap, with separate index widths and separate counters (Scenario 2).
- [ ] The two tenants reach their caps independently; each tenant's refused counter ticks only on its own refusals (Scenario 3).

### Outcome KPIs

- **Who**: the operators of two tenants sharing the same gateway process; specifically Globex Steady on "globex-staging" while Maya is on "acme-prod".
- **Does what**: sees tenant B's ingest unaffected when tenant A breaches its cap; the per-tenant counter ticks only on the offending tenant; the process stays alive for both.
- **By how much**: 100 percent of cross-tenant scenarios in the acceptance suite (US-02 Scenario 1, Scenario 2, Scenario 3) leave tenant B's index width, refused counter, and ingest behaviour unaffected by tenant A's cap state.
- **Measured by**: the slice-01 acceptance suite outcomes on `pulse`; 100 percent mutation kill on the changed files. A specific mutant to kill: replacing the per-tenant count with `series.len()` (a global count) is killed by Scenario 1.
- **Baseline**: 0 percent today. Without a cap at all, tenant A's bomb OOMs the whole process and takes tenant B's ingest down too; the A-D4 attractor is violated at the resource axis.

### Technical Notes (DESIGN-flagged, NOT decided here)

- The per-tenant count is a projection of the existing map key.
  Concretely, `series.keys().filter(|(t, _)| t == tenant).count()`
  is correct but O(N) in the size of the map; DESIGN MAY
  introduce an O(1) shadow per-tenant counter
  (a `HashMap<TenantId, usize>`) updated on insertion. DISCUSS
  does not pin the implementation; the contract is per-tenant
  semantics. The shadow counter is the natural choice once the
  map grows; the O(N) projection is fine for slice 01 tests.
- The per-tenant refused counter follows the same shape (a
  `HashMap<TenantId, usize>`).
- Dependencies: depends on US-01 establishing the cap pattern.

---

## US-03: The refused-ingest count is observable

### Elevator Pitch

- Before: a refused ingest is silent. Even with US-01's refusal
  in place, the operator has no way to see HOW MANY new series
  have been refused, WHEN they were refused, or by WHICH tenant.
  The refusal is honest at the code level (the points are not
  stored, the map width is bounded) but invisible at the operator
  level. From Maya's perspective, "new series stopped ingesting"
  is the same observation regardless of cause (cap fired, OTLP
  exporter died, network partition).
- After: each refused ingest of a NEW above-cap SeriesKey is
  observable via FLAG 2's mechanism. DISCUSS's LIKELY
  recommendation is BOTH: (A) a `refused_new_series: usize` field
  on `IngestReceipt` so the synchronous caller knows
  immediately, AND (B) a `pulse.cardinality.refused.count`
  self-observe metric via the existing bridge pattern (mirror of
  `LumenToPulseRecorder`) so the longitudinal view is queryable
  via `query-api` like any other pulse metric. The two surfaces
  answer different questions: the receipt-field is the per-call
  signal; the self-observe metric is the over-time signal.
- Decision enabled: Maya sees a counter rising and knows the cap
  fired (the residuality A-D6 "honest three-way outcomes" applied
  at the ingest side). She traces the bomb to the offending
  tenant (per-tenant counters; US-02) and the offending client
  (via existing per-service self-observe metrics). The S04 OOM
  surface is closed with a NAMED signal, not a silent ceiling.

### Problem

A cap that refuses silently is half a feature. The refusal IS
honest at the code level (the points are not stored, the map
width is bounded, the process does not OOM) but invisible to the
operator. Maya needs to know:

- How many new series have been refused for "acme-prod" since
  the last process restart? (a synchronous, immediate signal on
  each ingest call)
- How is the refusal rate changing over time, day-over-day or
  hour-over-hour? (a longitudinal, queryable signal)
- Which tenant is breaching the cap? (per-tenant, per US-02)

The synchronous question is best answered on the receipt itself
(the OTLP partial-success path the gateway already implements
can carry the refused count to the client too). The longitudinal
question is best answered via a self-observe metric, the same
pattern lumen and cinder already use to surface their events as
pulse metrics. Both surfaces compose with the rest of the
platform without bespoke event infrastructure.

### Who

- Maya Kowalski - operator persona - needs both the per-call
  signal (to see refusals in process logs as they happen) and the
  longitudinal signal (to see trends without scraping every log
  line).
- Hands-off Hannah - the tenant client owner - sees the
  longitudinal signal on her tenant dashboard (or in a future
  Prism panel) and knows the exporter needs fixing.
- A future security or reliability reviewer - reads the cap's
  observability surface as part of the residuality follow-up
  audit; the named counter is the residue against silent loss.

### Solution

FLAG 2 DISCUSS LIKELY recommendation: BOTH.

(A) Add `refused_new_series: usize` to `IngestReceipt`
(`crates/pulse/src/store.rs:30`). Each call to
`MetricStore::ingest` returns the count of NEW SeriesKeys refused
DURING THIS CALL (not cumulative). The default is 0; non-zero
indicates the cap fired during this batch. The receipt's `count`
field continues to report points ingested honestly (matching
existing series get their points; refused new series do not
contribute to `count`). DESIGN owns whether to add
`#[non_exhaustive]` to `IngestReceipt` for future additive
fields without a major-version bump.

(B) Add `PulseToPulseRecorder` to `crates/self-observe/`
mirroring `LumenToPulseRecorder` and `CinderToPulseRecorder`:
implement a thin recorder that emits one pulse metric per refused
ingest. Metric name `pulse.cardinality.refused.count`, value =
1, kind `Sum`, with optional `tenant` attribute on the point.
The metric is itself a pulse metric, subject to its own cap; the
operator picks the cap value above the natural self-observe
cardinality (FLAG 1's 10_000 LIKELY recommendation has plenty of
headroom for the self-observe traffic).

The two surfaces are independent and additive. DESIGN may pick
only (A), only (B), or both. DISCUSS's LIKELY recommendation is
BOTH because the surfaces answer different questions and both
are cheap.

### Domain Examples

#### 1: The receipt's refused_new_series field reports the count for the call

A batch of 3 metrics, 1 EXISTING SeriesKey and 2 NEW SeriesKeys
above the cap, is ingested. The receipt reports
`{ count: <points in the existing-key metric>,
   refused_new_series: 2 }`. The synchronous caller (aperture-
storage-sink or a test) sees the refusal immediately.

#### 2: The self-observe metric records the longitudinal refusal count

Over the course of an hour, 50 new SeriesKeys are refused for
"acme-prod" (one per minute, say). The self-observe bridge emits
50 single-point metrics named `pulse.cardinality.refused.count`,
each value=1, time_unix_nano set at emission, and (optionally)
a `tenant` point-attribute. A `query-api` query for
`pulse.cardinality.refused.count{tenant="acme-prod"}` over the
hour returns 50 points; the sum is 50; the rate is 50/hour or
~0.83/minute.

#### 3: Both surfaces compose - the receipt is the per-call signal, the metric is the trend

A test calls `store.ingest` 10 times. Five of the calls refuse a
new SeriesKey; five do not. The five refusing calls return
receipts with `refused_new_series: 1`; the five non-refusing
calls return receipts with `refused_new_series: 0`. The
self-observe bridge has emitted 5 `pulse.cardinality.refused.count`
metric points. A query for the metric returns 5 points; a sum
returns 5; the receipt-side total (summing
`refused_new_series` over the 10 calls) equals 5 too. The two
surfaces agree.

### UAT Scenarios (BDD)

#### Scenario: The receipt's refused_new_series field reflects the count of refusals in the call

```gherkin
Given a FileBackedMetricStore at the cap for tenant "acme-prod"
When Maya ingests a batch containing 2 new SeriesKeys above the cap
Then the ingest call returns successfully
And the receipt's refused_new_series field equals 2
And the receipt's count field equals 0 (no existing-series points in this batch)
```

#### Scenario: The self-observe metric records each refusal (FLAG 2 LIKELY: BOTH)

```gherkin
Given a FileBackedMetricStore at the cap for tenant "acme-prod"
And a PulseToPulseRecorder wired into the store (emitting into a second pulse instance)
When Maya ingests a batch containing 3 new SeriesKeys above the cap
Then the second pulse instance has received 3 points of metric "pulse.cardinality.refused.count"
And the points carry tenant="acme-prod" as an attribute (point-level, not series-level)
```

#### Scenario: The receipt's refused_new_series field is 0 when no refusal happens

```gherkin
Given a FileBackedMetricStore with tenant "acme-prod" below the cap
When Maya ingests a batch with 1 new SeriesKey and 1 point on an existing SeriesKey
Then the ingest call returns successfully
And the receipt's refused_new_series field equals 0
And the receipt's count field equals 2 (1 point on the new key + 1 point on the existing key, both stored)
```

#### Scenario: The self-observe metric is absent (no emission) when no refusal happens

```gherkin
Given a FileBackedMetricStore with tenant "acme-prod" below the cap
And a PulseToPulseRecorder wired into the store
When Maya ingests a batch with 0 refusals
Then the second pulse instance has 0 points of metric "pulse.cardinality.refused.count"
```

### Acceptance Criteria

- [ ] The receipt's `refused_new_series` field reports the per-call refusal count honestly (Scenario 1, Scenario 3).
- [ ] The self-observe metric records each refusal with the tenant attribute (Scenario 2), if FLAG 2 picks B or C.
- [ ] No emission and `refused_new_series = 0` when no refusal happens (Scenario 4, Scenario 3).

Per FLAG 2 LIKELY: BOTH. If DESIGN picks A only, drop Scenarios 2 and 4 (the metric-side ones); if DESIGN picks B only, drop Scenarios 1 and 3 (the receipt-side ones).

### Outcome KPIs

- **Who**: an operator (Maya Kowalski) or a queryable longitudinal consumer (a Prism panel, an automated audit) of the platform's pulse state.
- **Does what**: observes refused-ingest counts via the synchronous receipt field (immediate signal per call) and via the self-observe metric (longitudinal signal queryable through query-api), per tenant.
- **By how much**: 100 percent of refused-ingest scenarios in the acceptance suite expose the count via the chosen FLAG 2 surface; the receipt-side and metric-side counts agree.
- **Measured by**: the slice-01 acceptance suite on `pulse` (and `self-observe` if FLAG 2 picks B or C); the receipt-field assertion and the second-pulse-instance assertion respectively.
- **Baseline**: 0 percent today; no counter exists. An OOM kill was the only signal, and it killed the process.

### Technical Notes (DESIGN-flagged, NOT decided here)

- FLAG 2 picks the surface(s). DISCUSS recommends BOTH.
- If FLAG 2 picks (A), the receipt field name is
  `refused_new_series: usize` (DESIGN may pick a different
  name; DISCUSS pins the semantics, not the exact identifier).
  DESIGN owns `#[non_exhaustive]`.
- If FLAG 2 picks (B), the bridge name is `PulseToPulseRecorder`
  mirroring `LumenToPulseRecorder` and `CinderToPulseRecorder`;
  the metric name is `pulse.cardinality.refused.count` mirroring
  the naming convention `<source>.<event>.count`. The metric is
  a Sum with value=1 per emission, matching
  `cinder.place.count` / `cinder.migrate.count`.
- Dependencies: depends on US-01 (the refusal must exist for the
  count to mean anything) and US-02 (the per-tenant scoping
  determines the point attribute or per-tenant projection of the
  counter).

---

## US-04: WAL replay rebuilds existing series past the cap; the cap applies only to NEW series at post-replay live ingest

### Elevator Pitch

- Before: `apply_ingest` is shared between live ingest and WAL
  replay (`crates/pulse/src/file_backed.rs:158` calls
  `apply_ingest(&mut series, &tenant, metrics)` for every WAL
  record on `open`). Without a cap, both paths build the same
  index. With a cap added naively, replay would refuse to
  reconstruct a tenant that had crossed the cap during its
  history, and `FileBackedMetricStore::open` would re-create a
  store with FEWER series than it had pre-restart. That would
  silently lose data on every restart for any tenant near the
  cap, an A-U1 "silent data loss" attractor under a different
  name.
- After: the cap fires only on NEW SeriesKeys at post-replay live
  ingest, not on KEYS being rebuilt from the WAL. A `SeriesKey`
  present in the WAL is, BY DEFINITION, a series the platform
  has already accepted; replaying it cannot legitimately refuse
  it. The contract: on `open`, snapshot+WAL replay rebuilds
  whatever series were durable, regardless of count;
  post-replay, the cap fires from `current_count + 1` and
  refuses NEW keys. This pins the answer DESIGN might otherwise
  re-litigate.
- Decision enabled: Maya can restart the gateway process
  (planned, rolling deploy, or recovery) without losing series
  her tenant ingested before the cap value was decided. The cap
  is a forward-looking guard, not a retroactive truncation.

### Problem

The residuality analysis identifies the S04 row of the incidence
matrix, with pulse marked B (OOM possible). It also identifies
S21 (rolling restart) which interacts: on every restart, the
pulse adapter replays the WAL through `apply_ingest`
(`crates/pulse/src/file_backed.rs:158`); whatever cap fires
during replay defines the post-restart state. If the cap fires
during replay, restart becomes a silent truncation: a tenant
that legitimately held N+5 series before restart holds N after
restart, with no signal. This is A-U1 by another route.

The fix is to make the cap a property of the LIVE ingest path,
not the rebuild path. The WAL is the durable record of accepted
ingests; replaying it MUST reconstruct exactly what was accepted,
regardless of how the cap value relates to the count today. A
cap value change between deploys then has a clear interpretation:
"from now on, refuse new series above this cap"; it never
retroactively un-accepts already-stored data.

### Who

- Maya Kowalski - operator persona - restarts the gateway
  process for rolling deploys, recovery, and routine ops;
  needs restart to be lossless for already-accepted series.
- A future incident responder - reads the WAL replay logs and
  must see "rebuilt N+5 series for acme-prod" (the truth), not
  "rebuilt N series for acme-prod" (a silent truncation).
- The platform's residuality story - the A-U1 "silent data loss"
  attractor stays blocked across restart.

### Solution

`apply_ingest` consults the cap ONLY for NEW keys at LIVE ingest.
A clean way to express this:

- `apply_ingest` gains an `enforce_cap: bool` parameter (or two
  variants, `apply_ingest_live` and `apply_ingest_replay`;
  DESIGN owns the exact shape).
- On `open`, the snapshot rebuild and the WAL replay call the
  no-cap variant: every key is reconstructed, no refusal, no
  counter increment.
- On `FileBackedMetricStore::ingest` (live), the cap-enforcing
  variant is called: new keys above the cap are refused, the
  counter increments.

Alternative shape: the cap-enforcing arm is a separate function
called only from the live ingest path, and the per-metric loop in
`apply_ingest` itself never enforces. DESIGN picks the exact
shape; DISCUSS pins the semantics.

This pinning matches ADR-0040 Decision 2 (append-and-sort:
replay reconstructs what was accepted) and ADR-0045 (the WAL is
the single source of truth for the index; `apply_ingest` is the
shared seam that ingest and replay route through). The cap
refines the LIVE arm of that seam; it does NOT change the
REPLAY arm.

### Domain Examples

#### 1: Restart with a tenant exactly at the cap - all N series rebuild, the cap fires from N+1

Tenant "acme-prod" has `MAX_SERIES_PER_TENANT` distinct
SeriesKeys when the process restarts. `FileBackedMetricStore::
open` replays the WAL through the no-cap variant of
`apply_ingest`; all N series are reconstructed. Post-replay live
ingest of an EXISTING SeriesKey succeeds (matching key). Post-
replay live ingest of a NEW above-cap SeriesKey is REFUSED (the
cap fires; the counter increments).

#### 2: Restart with a tenant above what a tightened cap would now allow - all original series rebuild

Suppose the cap value was 10_000 in a prior deploy. Tenant
"acme-prod" legitimately accumulated 10_000 distinct SeriesKeys
(each accepted at the time). The cap value is then TIGHTENED to
5_000 for the next deploy. On restart with the tightened cap,
`open` replays the WAL through the no-cap variant; all 10_000
existing series are reconstructed. The tenant's index width is
10_000, which is above the new live cap of 5_000. Live ingest
of an EXISTING SeriesKey still succeeds (matching key). Live
ingest of any NEW SeriesKey is REFUSED (current count of 10_000
is above the new cap of 5_000; the cap fires; the counter
increments). The tenant cannot grow but does not shrink either;
the operator decides whether to shed series via a separate
mechanism (not in scope for slice 01).

#### 3: Restart with a tenant below the cap - replay completes, live ingest behaves as US-01

Tenant "acme-prod" has 3_000 distinct SeriesKeys at restart. The
WAL replay reconstructs them. Live ingest behaves as US-01: new
SeriesKeys are accepted until 10_000; the 10_001st is refused.

### UAT Scenarios (BDD)

#### Scenario: WAL replay reconstructs all N series for a tenant at the cap

```gherkin
Given a FileBackedMetricStore for tenant "acme-prod" populated to N distinct SeriesKeys (cap is N)
And the store has been snapshot()'d so the WAL contains the accepted records
When Maya closes the store and reopens it via FileBackedMetricStore::open
Then the reopened store's index width for "acme-prod" is exactly N
And the refused counter for "acme-prod" in the reopened store starts at 0 (refusals are not durable)
```

#### Scenario: Post-replay live ingest of a NEW SeriesKey above the cap is refused; existing series still ingest

```gherkin
Given a reopened FileBackedMetricStore with N distinct SeriesKeys for "acme-prod" and cap N
When Maya ingests a batch with 1 new SeriesKey for "acme-prod"
Then the ingest call returns successfully
And the receipt's refused_new_series field equals 1
And the index width for "acme-prod" remains exactly N
And when Maya ingests a batch with 1 point on an EXISTING SeriesKey for "acme-prod"
Then the ingest succeeds with receipt.count equal to 1
And the existing series has the new point
```

#### Scenario: Replay completes for a tenant above a tightened cap; no replay-time refusal

```gherkin
Given a FileBackedMetricStore previously deployed with cap 10_000 and tenant "acme-prod" at 10_000 SeriesKeys
When the store is reopened with cap 5_000 (the cap value tightened between deploys)
Then the reopened store's index width for "acme-prod" is 10_000 (replay did not refuse any of them)
And the refused counter for "acme-prod" starts at 0
And when Maya ingests a batch with 1 NEW SeriesKey for "acme-prod"
Then the ingest call returns successfully
And the receipt's refused_new_series field equals 1
And the index width remains 10_000
```

### Acceptance Criteria

- [ ] WAL replay reconstructs all N series for a tenant at the cap; the refused counter starts at 0 post-replay (Scenario 1).
- [ ] Post-replay live ingest of a NEW above-cap SeriesKey is refused; existing series still ingest (Scenario 2).
- [ ] A reopen with a tightened cap reconstructs the full pre-tightening series count without replay-time refusal; live ingest behaves per the new cap (Scenario 3).

### Outcome KPIs

- **Who**: an operator (Maya Kowalski) restarting the `kaleidoscope-gateway` for a routine deploy, a recovery, or after a cap-value tuning change.
- **Does what**: sees WAL replay rebuild all existing series for every tenant, regardless of count; sees the cap apply only to NEW series at post-replay live ingest.
- **By how much**: 100 percent of restart scenarios in the acceptance suite preserve existing series; 0 restart-time refusals; the cap fires only at live ingest post-replay.
- **Measured by**: the slice-01 acceptance suite on `pulse`; the test populates the store, snapshots, reopens, and asserts the index width.
- **Baseline**: 0 percent today; the cap does not exist, so the question is undefined. With a naive cap added that fires during replay, restart would silently truncate the index for any tenant near the cap (A-U1 by another route).

### Technical Notes (DESIGN-flagged, NOT decided here)

- The shape of `apply_ingest`'s cap-vs-no-cap distinction is a
  DESIGN call: a boolean parameter, two variants, or a separate
  function. DISCUSS pins the SEMANTICS (replay never refuses;
  live enforces) and leaves the shape to DESIGN.
- The refused counter is not durable: it counts refusals SINCE
  the current process started, not across restarts. (A cumulative
  durable counter would require a WAL of refusals, which is
  overkill for slice 01.)
- Dependencies: depends on US-01 establishing the cap pattern.

---

## US-05: An ingest batch with both existing-series points and new-series points above the cap is PARTIALLY applied; the whole batch is NEVER rejected

### Elevator Pitch

- Before: an ingest batch is a `MetricBatch` carrying a
  `Vec<Metric>`, each metric carrying its own points; the gateway
  routes the WHOLE batch into `FileBackedMetricStore::ingest`
  which calls `apply_ingest` once for the whole vector. Today, no
  cap fires, so the question is undefined. With a naive cap that
  rejects the whole batch on ANY new-above-cap metric, a single
  bomb-shaped metric inside an otherwise-legitimate batch would
  cause the WHOLE batch to be lost. That is silent loss of GOOD
  DATA (the existing-series points inside the same batch) and an
  A-U4 "fabricated empty" attractor by another route: the batch
  is reported as failed when the legitimate part of it could
  have been applied.
- After: the cap fires per-metric, not per-batch. A batch
  containing 3 metrics (one matching an EXISTING SeriesKey, one
  matching a different EXISTING SeriesKey, one carrying a NEW
  above-cap SeriesKey) is PARTIALLY applied: the two
  existing-series metrics have their points stored; the
  new-above-cap metric is refused; the refused counter
  increments by 1; the receipt's `count` reports points stored
  honestly; the receipt's `refused_new_series` reports 1.
- Decision enabled: Maya knows the batch was partly accepted
  (count > 0) and partly refused (refused_new_series > 0); she
  does not lose good points because a sibling metric happened to
  be a bomb. The OTLP partial-success contract the gateway
  already uses
  (`opentelemetry_proto::tonic::collector::metrics::v1::
  ExportMetricsPartialSuccess`) is the natural wire-side report;
  the client can act on the partial success.

### Problem

A `MetricBatch` is not atomic at the wire. OTLP itself supports
partial success (`ExportMetricsServiceResponse.partial_success`):
the server reports `rejected_data_points` and an
`error_message`, the client retains the rejected ones for
retry. M-4's cap must compose with this contract; rejecting the
whole batch when one metric breaches the cap would:

- Lose good data (points on existing series in the same batch
  the client cannot easily re-send for the partial reason).
- Misreport the situation (the receipt would show 0 stored, when
  in truth the cap is about NEW SERIES, not about EXISTING ones).
- Violate A-D6 (honest three-way outcomes): "rejected" is not
  the honest report; "partially applied, with these refused" is.

The partial-apply path is the natural shape of `apply_ingest`'s
per-metric loop: iterate metrics; per metric, decide accept or
refuse; never abort the loop. US-05 pins this semantics so a
DESIGN-time mutant that aborts the loop on first refusal is
killed by the acceptance test.

### Who

- Maya Kowalski - operator persona - needs partial-apply
  semantics so a single bomb-shaped metric does not poison a
  whole batch of legitimate ones.
- Hands-off Hannah - the tenant client owner - sees a
  partial-success response on her OTLP client and knows which
  metrics to fix.
- The platform's residuality story - A-D6 (honest three-way
  outcomes) and A-U4 (no fabricated empty) stay blocked at the
  ingest side.

### Solution

`apply_ingest`'s per-metric loop already iterates one metric at
a time. The cap check is inside the loop, per metric:

- If the metric's `SeriesKey` matches an existing entry: extend
  the points (existing path; never refuses).
- If the metric's `SeriesKey` is new and the tenant count is
  below the cap: insert (existing path; never refuses).
- If the metric's `SeriesKey` is new and the tenant count is at
  or above the cap: REFUSE this metric, increment the refused
  counter, drop the metric's points, CONTINUE THE LOOP.

The loop NEVER breaks early. The receipt's `count` accumulates
the points stored (matching plus new-below-cap); the receipt's
`refused_new_series` accumulates the refused metrics.

### Domain Examples

#### 1: Batch with 1 existing-series metric and 1 new-above-cap metric - partial apply

Tenant "acme-prod" is at the cap with one existing SeriesKey K
(name="cpu.utilisation", `resource_attributes={"service.name":
"checkout"}`) holding 5 points. Maya ingests a batch containing
two metrics:

- Metric A: matches K, carries 3 new points.
- Metric B: a NEW SeriesKey (name="cpu.utilisation",
  `resource_attributes={"service.name":"orders", "request_id":
  "<UUID>"}`), carries 2 points.

The receipt reports `{ count: 3, refused_new_series: 1 }`. K
now holds 8 points; Metric B's points are dropped; the index
width for "acme-prod" stays at the cap.

#### 2: Batch with 2 new-above-cap metrics and 1 existing - one accepted, one refused, one extends

Variant of (1) with two new metrics: one fits before the cap,
the other does not. Tenant "acme-prod" has cap-1 distinct
SeriesKeys. The batch contains three metrics: an existing
extend, a new-just-fits, and a new-above-cap. The receipt
reports `{ count: <existing-extend points> + <just-fits
points>, refused_new_series: 1 }`. The order of metrics in the
batch determines which fits and which refuses if there is only
ONE slot left and TWO new metrics; the test pins this with a
known order.

#### 3: Batch entirely of new-above-cap metrics - all refused, nothing rejected

Tenant "acme-prod" is at the cap. Maya ingests a batch of 5
metrics, all NEW SeriesKeys above the cap. The receipt reports
`{ count: 0, refused_new_series: 5 }`. The call returns
successfully (no `MetricStoreError`); the index width stays at
the cap; the counter ticks by 5.

### UAT Scenarios (BDD)

#### Scenario: A batch with both existing-series points and new-above-cap series is partially applied

```gherkin
Given a FileBackedMetricStore at the cap for tenant "acme-prod"
And one specific existing SeriesKey K with 5 points
When Maya ingests a batch containing 1 metric matching K with 3 new points AND 1 metric with a NEW above-cap SeriesKey carrying 2 points
Then the ingest call returns successfully
And the receipt's count equals 3 (the points on the matching metric)
And the receipt's refused_new_series equals 1
And K now holds 8 points sorted ascending by time_unix_nano
And the index width for "acme-prod" remains at the cap (unchanged)
And the points carried by the new-above-cap metric are NOT stored anywhere
```

#### Scenario: A batch of only new-above-cap metrics is refused per-metric, not as a whole

```gherkin
Given a FileBackedMetricStore at the cap for tenant "acme-prod"
When Maya ingests a batch containing 5 metrics, each with a previously-unseen NEW SeriesKey
Then the ingest call returns successfully (NO MetricStoreError is raised)
And the receipt's count equals 0
And the receipt's refused_new_series equals 5
And the index width for "acme-prod" remains at the cap
And the refused counter for "acme-prod" advances by 5
```

#### Scenario: A batch with a mix where some new SeriesKeys fit and others do not - ordering pins which fits

```gherkin
Given a FileBackedMetricStore with tenant "acme-prod" at exactly N-1 distinct SeriesKeys (cap N)
When Maya ingests a batch whose metrics, in order, are: existing-extend (2 points), new-just-fits (1 point), new-above-cap (3 points)
Then the ingest call returns successfully
And the receipt's count equals 3 (2 + 1 = 3 points stored)
And the receipt's refused_new_series equals 1
And the index width for "acme-prod" is exactly N (the just-fits new key was inserted)
And the new-above-cap metric's points are NOT stored
```

### Acceptance Criteria

- [ ] A batch with both existing-series points and new-above-cap series is partially applied honestly (Scenario 1).
- [ ] A batch of only new-above-cap metrics returns successfully with `count=0` and `refused_new_series=batch_size`; no `MetricStoreError` is raised (Scenario 2).
- [ ] A batch where some new SeriesKeys fit and others do not partial-applies per-metric in batch order (Scenario 3).

### Outcome KPIs

- **Who**: an operator (Maya Kowalski) or an OTLP client whose batches mix legitimate metrics with above-cap new SeriesKeys.
- **Does what**: sees partial-apply semantics: legitimate points land, above-cap new metrics are refused, the receipt reports honestly, the whole batch is never rejected.
- **By how much**: 100 percent of mixed-batch scenarios in the acceptance suite partial-apply honestly; 0 whole-batch rejections; 0 silent losses of legitimate points.
- **Measured by**: the slice-01 acceptance suite on `pulse`; the test inspects both the receipt and the stored state of the existing series.
- **Baseline**: 0 percent today; the cap does not exist, so the question is undefined. With a naive whole-batch-reject cap, every partial-bomb batch would lose its legitimate parts (an A-U4 attractor by another route).

### Technical Notes (DESIGN-flagged, NOT decided here)

- The per-metric loop never aborts. This is the kill condition
  for a "break-on-first-refuse" mutant; the acceptance test pins
  the loop's completion via Scenario 3 (the metric AFTER the
  refusal is the SAME refusal because there is only one new-
  above-cap metric in Scenario 3; an additional scenario where
  another existing-extend follows the new-above-cap metric would
  also kill the mutant; DESIGN may add).
- The receipt's `count` is the count of POINTS stored, NOT the
  count of metrics applied; matching the existing
  `IngestReceipt.count: usize` semantics
  (`crates/pulse/src/store.rs:30`).
- The wire-side OTLP partial-success report belongs to aperture,
  not pulse. The aperture-storage-sink helper at
  `crates/aperture-storage-sink/src/lib.rs:463` translates pulse's
  receipt into the OTLP partial-success path. DESIGN owns the
  wire-side translation; DISCUSS pins the pulse-side semantics.
- Dependencies: depends on US-01 (the per-metric refusal), US-02
  (the per-tenant scoping), US-03 (the counter surface).

---

## Story sizing summary

| Story | Scenarios | Effort | Right-sized? |
|---|---|---|---|
| US-01 (per-tenant cap on `apply_ingest`) | 5 | 0.3 days | Yes |
| US-02 (per-tenant isolation) | 3 | 0.15 days | Yes |
| US-03 (refused-count observability) | 4 | 0.25 days | Yes |
| US-04 (WAL-replay coherence) | 3 | 0.15 days | Yes |
| US-05 (batch partial-apply) | 3 | 0.15 days | Yes |

Total slice 01: roughly 1 day on `pulse`, matching the residuality
analysis's "~80 LOC" estimate plus the per-FLAG-2-LIKELY 1-file
addition to `self-observe`. All five stories live inside slice 01
(the walking skeleton). Slice 02 (deferred) lifts the cap from a
compile-time constant to env-driven configurability, per the
deferred "v1-roadmap" frame in the residuality analysis. None of
these stories renegotiates the OTLP wire contract, adds a
structured event log, evicts existing series, introduces a global
cap, or touches the `MetricStore` trait method signatures.
