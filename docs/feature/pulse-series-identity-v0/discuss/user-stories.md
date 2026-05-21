<!-- markdownlint-disable MD024 -->

# User Stories: pulse-series-identity-v0

British English. No em dashes.

Pulse is a library, not a daemon: it exposes no CLI or HTTP surface of its own. The
realistic invocable surface is the durable store API exercised by the acceptance test:
`FileBackedMetricStore::open` -> `ingest` (two same-named metrics differing by
`service.name`) -> `query` -> assert two distinct series. The Elevator Pitch "After" lines
reference this durable store API honestly, because it is the actual entry point a consumer
(query-api) and the acceptance suite drive. This is noted because the
[Elevator Pitch real-entry-point check] expects a user-invocable surface; for a storage
library the durable store API IS that surface.

## System Constraints

- **No `MetricStore` trait signature change.** `query` already returns
  `Vec<(Metric, MetricPoint)>` (`crates/pulse/src/store.rs` lines 77-82), where each point
  carries its owning `Metric` and that metric's `resource_attributes`. The shape already
  supports many series under a name. The fix is to the ingest keying and query fan-out
  beneath the trait, not the trait.
- **No migration story.** The durable snapshot format may change. Pulse is library-only with
  no production data (`crates/pulse/src/lib.rs`: "Library only at v0. No daemon, no
  network."), so a format change requires no migration, shim, or version negotiation. State
  this for DESIGN so it is not invented.
- **Point attributes are unchanged.** `MetricPoint.attributes` is already per-point and
  correct. Only the metric-level `resource_attributes` is currently lost. This feature does
  not touch point attributes.
- **Single fix point covers both paths.** Live ingest (`InMemoryMetricStore::ingest` and
  `FileBackedMetricStore::ingest`) and WAL recovery share `apply_ingest`
  (`crates/pulse/src/file_backed.rs`). Correcting series identity there is inherited by both
  the in-memory store and durable recovery; the two cannot drift.
- **Recovery discipline unchanged.** Pulse stays an append-and-sort pillar (points sorted by
  `time_unix_nano` after replay). This feature changes the series KEY, not the recovery
  discipline (see wave-decisions.md, "Relationship to ADR-0040").
- **Tenant isolation unchanged.** Series remain scoped per `aegis::TenantId`; full-label-set
  identity applies within a tenant.

---

## US-01: Two services sharing a metric name stay two distinct series

### Elevator Pitch

- Before: a platform ingests `http_requests_total` from checkout and from cart under tenant
  "acme-prod". Pulse keys both by `(acme-prod, http_requests_total)` and overwrites
  `resource_attributes` on the second ingest, so the two collapse into one series wearing
  whichever service ingested last. Querying the name returns one series labelled (say)
  `service.name="cart"` for points that actually came from checkout.
- After: an acceptance test drives `FileBackedMetricStore::open(base)` then `ingest` of two
  `http_requests_total` metrics differing only by `service.name` ("checkout" then "cart"),
  then `query(&tenant, "http_requests_total", TimeRange::all())`, and the returned
  `Vec<(Metric, MetricPoint)>` contains BOTH series: the checkout points each paired with a
  `Metric` carrying `resource_attributes{service.name: "checkout"}`, and the cart points each
  paired with a `Metric` carrying `service.name: "cart"`. Neither service's labels overwrite
  the other.
- Decision enabled: the operator (one layer up, in query-api/Prism) reads the checkout trend
  and the cart trend as two correctly-labelled lines and decides which service is the source
  of an incident, instead of being misled by one collapsed line wearing the wrong service's
  label.

### Problem

A platform team for tenant "acme-prod" emits `http_requests_total` from three services:
checkout, cart, and search, each with its own `service.name` resource attribute. Because
Pulse keys every series by `(tenant, metric_name)` alone and overwrites
`resource_attributes` on each ingest (`store.rs` line 161, `file_backed.rs` line 318), the
three services collapse into one series, and only the last-ingested service's labels
survive. Any query of `http_requests_total` returns one arbitrary service's labels applied
to every point, regardless of which service produced them. Per-service provenance is
destroyed at ingest, before any query runs. The team cannot tell their three services apart
in their own metric.

### Who

- Pulse's ingest/query consumer (query-api, and the on-call operator behind it) | ingesting
  a metric emitted by several services under one tenant, then querying it by name | needs
  each service's points to keep that service's own `resource_attributes`.
- The Pulse acceptance suite | drives `FileBackedMetricStore::open` -> `ingest` -> `query`
  and asserts the distinct-series outcome | is the concrete invocable surface for this
  library.

### Solution

Identify a metric series by its FULL label set (metric name + `resource_attributes`), not by
name alone. At ingest, two metrics sharing a name but differing by any `resource_attributes`
entry (for example `service.name`) become two distinct series. At query time, a query for a
metric name returns every series under that name, each `(Metric, MetricPoint)` pair carrying
its own series' `resource_attributes`. The `resource_attributes` overwrite at ingest is
removed: a series's resource attributes are part of its identity, not refreshable metadata.

### Domain Examples

### 1: Happy Path - two services, two series

Tenant "acme-prod". Ingest `http_requests_total` with `service.name="checkout"` and one
point (value 10 at t=100). Then ingest `http_requests_total` with `service.name="cart"` and
one point (value 20 at t=100). Query `http_requests_total` over `TimeRange::all()`. The
result has two pairs: (checkout metric, point value 10) and (cart metric, point value 20).
The checkout pair's `Metric.resource_attributes` is `{service.name: "checkout"}`; the cart
pair's is `{service.name: "cart"}`.

### 2: Edge Case - same name, same service, different point attributes is still ONE series

Tenant "acme-prod". Ingest `http_requests_total` with `service.name="checkout"` and two
points whose POINT attributes differ (`http.route="/a"` and `http.route="/b"`). Both points
share the same `resource_attributes{service.name: "checkout"}`, so they belong to ONE series.
Query returns both points, each paired with the same checkout `Metric`. Point attributes
remain per-point and do not split the series (they were never the bug).

### 3: Boundary - identical resource_attributes across two ingests merges, not duplicates

Tenant "acme-prod". Ingest `http_requests_total` with `service.name="checkout"` and a point
at t=100. Then ingest `http_requests_total` again with the SAME
`resource_attributes{service.name: "checkout"}` and a point at t=200. The two ingests target
the SAME series (identical full label set), so the result is one checkout series with two
points (t=100 then t=200, ascending). Distinct identity must not become accidental
duplication when the label sets are equal.

## UAT Scenarios (BDD)

### Scenario: Two services emitting the same metric name return two distinct series

Given tenant "acme-prod" has ingested "http_requests_total" with resource attribute service.name "checkout" and a point value 10 at time 100
And tenant "acme-prod" has then ingested "http_requests_total" with resource attribute service.name "cart" and a point value 20 at time 100
When the consumer queries "http_requests_total" over the full time range
Then two distinct series are returned
And one series carries resource attribute service.name "checkout" with the point value 10
And one series carries resource attribute service.name "cart" with the point value 20
And neither service's resource attributes overwrite the other's

### Scenario: Points differing only by point-level attributes stay in one series

Given tenant "acme-prod" has ingested "http_requests_total" with resource attribute service.name "checkout" and two points whose point attribute http.route is "/a" and "/b"
When the consumer queries "http_requests_total" over the full time range
Then one series is returned carrying resource attribute service.name "checkout"
And both points are present, each keeping its own http.route point attribute

### Scenario: Re-ingesting the same full label set merges points into one series

Given tenant "acme-prod" has ingested "http_requests_total" with resource attribute service.name "checkout" and a point at time 100
And tenant "acme-prod" has then ingested "http_requests_total" with the same resource attribute service.name "checkout" and a point at time 200
When the consumer queries "http_requests_total" over the full time range
Then one series is returned carrying resource attribute service.name "checkout"
And it contains both points in ascending time order

## Acceptance Criteria

- [ ] Ingesting two metrics with the same name but different `resource_attributes` produces two distinct series.
- [ ] Querying a metric name returns every series under that name, each `(Metric, MetricPoint)` pair carrying its own series' `resource_attributes`.
- [ ] A later ingest of one service no longer overwrites another service's `resource_attributes`.
- [ ] Two ingests with an identical full label set merge into one series (no accidental duplication), points in ascending time order.
- [ ] Point-level attributes remain per-point and do not split or merge series.
- [ ] The `MetricStore` trait signature is unchanged.

## Outcome KPIs

- **Who**: Pulse's ingest/query consumer (query-api and the operator behind it)
- **Does what**: ingests a metric emitted by several services and queries it back as one correctly-labelled series per service, instead of one collapsed series wearing the last-ingested service's labels
- **By how much**: 100% of distinct `resource_attributes` under a shared name are preserved as distinct series (correctness); 0 series whose `resource_attributes` were overwritten by a later ingest
- **Measured by**: acceptance test ingesting >= 2 services under one name and asserting each returned series carries its own `resource_attributes`
- **Baseline**: 0% (today every multi-service metric collapses to one series wearing the last-ingested service's labels)

### Learning Hypothesis

We believe keying a series by its full label set (name + `resource_attributes`) for a
multi-service metric will make per-service provenance survive ingest. We will know this is
true when an acceptance test ingesting checkout then cart under one name gets two distinct,
correctly-labelled series back from a single `query` call (one session, real durable store).

## Technical Notes (Optional)

- Fix point: `apply_ingest` (`crates/pulse/src/file_backed.rs`) and the matching upsert in
  `InMemoryMetricStore::ingest` (`crates/pulse/src/store.rs`). The map key must incorporate
  `resource_attributes`, and the `entry.metric.resource_attributes = ...` overwrite (store.rs
  line 161, file_backed.rs line 318) must go.
- Series key shape is a DESIGN call: `resource_attributes` is a `BTreeMap<String, String>`
  (deterministically orderable, so hashable/comparable as a key). A derived series-key type
  in `metric.rs` is one option. Out of DISCUSS scope.
- `MetricName` remains the query argument; the query now fans out across all series sharing
  that name within the tenant. The trait signature does not change (System Constraints).
- Depends on nothing; UNBLOCKS `query-api-label-matchers-v0` (six stashed scenarios).

---

## US-02: The distinct series survive a snapshot and reopen

### Elevator Pitch

- Before: even if ingest kept checkout and cart distinct in memory, a durable store that
  collapsed them on `snapshot()` + reopen, or replayed the WAL through the old name-only
  keying, would silently re-merge them on the next restart. A durable store that loses
  per-service identity across a restart is a half-fix.
- After: an acceptance test drives `FileBackedMetricStore::open(base)`, `ingest` of
  checkout then cart `http_requests_total`, then `snapshot()`, then drops the store and
  `FileBackedMetricStore::open(base)` again on the same path, then
  `query(&tenant, "http_requests_total", TimeRange::all())`, and STILL gets both distinct
  series back, each carrying its own `service.name`. The same holds when recovery happens via
  WAL replay (reopen without an intervening `snapshot()`).
- Decision enabled: an operator trusts that per-service series survive a Pulse restart, so
  incident triage after a process bounce is reading real per-service history, not a
  post-restart re-collapse.

### Problem

The same consumer relies on the durable `FileBackedMetricStore` across process restarts.
Recovery rebuilds series by replaying the WAL through `apply_ingest` and folding in the
snapshot; the snapshot stores series buckets keyed the same way the live map is keyed
(`crates/pulse/src/file_backed.rs`, `Snapshot`/`SeriesBucket`, lines 54-67 and 107-141). If
identity is corrected only on the live path but the snapshot bucketing or WAL replay still
keys by name alone, two services would be distinct until the next restart and then silently
re-merge. The durable angle is a distinct code path (reopen + replay) and needs its own
verification.

### Who

- Pulse's durable-store consumer | depends on per-service series surviving a process restart
  (snapshot + reopen, and WAL replay) | needs recovery to rebuild the SAME distinct series
  the live path produced.

### Solution

Apply the full-label-set identity consistently in the durable layer: the snapshot stores one
bucket per distinct series (by full label set), and WAL replay through `apply_ingest`
rebuilds the same distinct series. Because live ingest and recovery share `apply_ingest`, the
US-01 fix already covers replay; this story additionally pins the snapshot bucketing and the
reopen path so the distinct series provably survive both a snapshot+reopen and a WAL-only
reopen. The snapshot format may change freely (no production data, no migration; see System
Constraints).

### Domain Examples

### 1: Happy Path - distinct series survive snapshot + reopen

Tenant "acme-prod". Open a `FileBackedMetricStore` at `base`. Ingest checkout
`http_requests_total` (point 10 at t=100) and cart `http_requests_total` (point 20 at t=100).
Call `snapshot()`. Drop the store. Reopen at `base`. Query `http_requests_total`. Two
distinct series come back: checkout (point 10) and cart (point 20), each with its own
`service.name`.

### 2: Edge Case - distinct series survive a WAL-only reopen (no snapshot)

Tenant "acme-prod". Open at `base`. Ingest checkout then cart `http_requests_total`. Do NOT
snapshot. Drop the store. Reopen at `base` (recovery replays the WAL only). Query
`http_requests_total`. Both distinct series are rebuilt from WAL replay, each with its own
`service.name`. This proves replay, not just snapshot, honours full-label-set identity.

### 3: Boundary - re-ingest after reopen still targets the right service's series

Tenant "acme-prod". Open, ingest checkout (point at t=100) and cart (point at t=100),
`snapshot()`, reopen. After reopen, ingest checkout again with a point at t=200. Query
`http_requests_total`. The checkout series has two points (t=100, t=200) and the cart series
has one (t=100); the post-reopen checkout ingest joined the recovered checkout series, not
the cart series and not a third series.

## UAT Scenarios (BDD)

### Scenario: Distinct series survive a snapshot and reopen

Given a durable store at a path has ingested "http_requests_total" for service.name "checkout" with a point value 10 and for service.name "cart" with a point value 20
And the store has been snapshotted, dropped, and reopened on the same path
When the consumer queries "http_requests_total" over the full time range
Then two distinct series are returned
And one carries service.name "checkout" with the point value 10
And one carries service.name "cart" with the point value 20

### Scenario: Distinct series survive a WAL-only reopen with no snapshot

Given a durable store at a path has ingested "http_requests_total" for service.name "checkout" and for service.name "cart" without being snapshotted
And the store has been dropped and reopened on the same path so recovery replays the WAL
When the consumer queries "http_requests_total" over the full time range
Then two distinct series are returned, each carrying its own service.name

### Scenario: A re-ingest after reopen joins the correct recovered series

Given a durable store has ingested checkout and cart "http_requests_total", been snapshotted, and reopened
And the consumer ingests "http_requests_total" for service.name "checkout" again with a later point
When the consumer queries "http_requests_total" over the full time range
Then the checkout series carries both the recovered and the new point in ascending time order
And the cart series is unchanged with its single point

## Acceptance Criteria

- [ ] Two distinct series ingested before a `snapshot()` are both present, correctly labelled, after `snapshot()` + drop + reopen.
- [ ] Two distinct series ingested without a snapshot are both rebuilt, correctly labelled, by WAL replay on reopen.
- [ ] A re-ingest after reopen joins the matching recovered series by full label set, not name alone.
- [ ] The snapshot stores one bucket per distinct series (by full label set), not per metric name.
- [ ] Recovery and live ingest produce identical series identity (they share `apply_ingest`).

## Outcome KPIs

- **Who**: Pulse's durable-store consumer
- **Does what**: relies on per-service series surviving a process restart (snapshot+reopen and WAL replay)
- **By how much**: 100% of distinct series present before a restart are present and correctly labelled after it; 0 series re-merged or re-collapsed on recovery
- **Measured by**: acceptance test that ingests >= 2 services, restarts the store (both snapshot and WAL-only paths), and asserts the distinct series survive
- **Baseline**: n/a (multi-service series do not exist as distinct entities today, so nothing survives correctly)

### Learning Hypothesis

We believe that because live ingest and WAL recovery share `apply_ingest`, the US-01 identity
fix will carry into durable recovery, and that pinning the snapshot bucketing by full label
set will keep distinct series intact across a restart. We will know this is true when an
acceptance test that snapshots, reopens (and separately, WAL-replays), and queries gets the
same two distinct series the live path produced.

## Technical Notes (Optional)

- Snapshot bucketing: `SeriesBucket` (`crates/pulse/src/file_backed.rs` lines 59-67) and the
  `Snapshot` map keyed in `open` (lines 107-141) must bucket by full label set. The snapshot
  format may change (no migration; no production data).
- Recovery already shares `apply_ingest`, so the US-01 fix is inherited by replay; this story
  exists to PIN that with explicit reopen scenarios (snapshot path and WAL-only path are
  distinct code paths).
- The walking-skeleton acceptance scenario (tagged `@walking_skeleton`) is the US-01 happy
  path against a real `FileBackedMetricStore`; US-02 extends it across a restart.
- Depends on US-01. No external dependencies.
