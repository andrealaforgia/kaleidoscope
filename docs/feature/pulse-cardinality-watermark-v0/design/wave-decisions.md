# Wave Decisions: pulse-cardinality-watermark-v0 (DESIGN)

British English. No em dashes. No emoji.

## Mode

`propose`. Decisions below resolve the four flags DISCUSS opened
(`docs/feature/pulse-cardinality-watermark-v0/discuss/wave-decisions.md`)
and pin the WAL-replay semantics, the enforcement seam, and the
recorder-hook shape. The companion document is
`design/application-architecture.md` (C4 L2 + Changes Per File table)
and the new ADR is `docs/product/architecture/adr-0051-pulse-per-tenant-cardinality-watermark.md`.

## Reads checklist

- [x] `docs/feature/pulse-cardinality-watermark-v0/discuss/wave-decisions.md`
  (FLAG 1 cap value, FLAG 2 counter location, FLAG 3 batch semantics,
  FLAG 4 new ADR vs amendment, WAL-replay caveat in the Risks table).
- [x] `docs/feature/pulse-cardinality-watermark-v0/discuss/user-stories.md`
  (US-01 through US-05; the boundary semantics at exactly N and N+1;
  US-04's pin that replay never refuses; US-05's per-metric loop never
  aborts).
- [x] `docs/feature/pulse-cardinality-watermark-v0/discuss/story-map.md`
  (the walking-skeleton seam; the five-taste-test contract; the
  oversize check passing).
- [x] `docs/feature/pulse-cardinality-watermark-v0/discuss/outcome-kpis.md`
  (seven KPIs; the guardrail KPI 7 limiting public-api change to one
  additive receipt field; the handoff-to-DEVOPS section confirming
  no new crate, no new dependency).
- [x] `docs/feature/pulse-cardinality-watermark-v0/slices/slice-01-pulse-cardinality-watermark-walking-skeleton.md`
  (the five carpaccio taste-tests; the OUT list; the effort
  breakdown).
- [x] `docs/product/architecture/residuality-analysis.md` (M-4 in the
  "Resilience modifications (prioritised)" section; the S04 row of
  the incidence matrix, pulse cell "B OOM under enough labels"; the
  A-U1 attractor "Silent data loss"; the closing summary's
  "capacity residues" phrasing).
- [x] `docs/product/architecture/adr-0045-pulse-series-identity-is-the-full-label-set.md`
  (the precedent that opened this gap: Decision 1 made the full label
  set part of identity; Consequences accepted unbounded cardinality
  as "a v2 concern if series cardinality per name ever grew large").
- [x] `docs/product/architecture/adr-0049-earned-trust-honour-fsync.md`
  (the Earned-Trust precedent on the WRITE side: probe must honour
  fsync, composition root invariant "wire then probe then use", the
  three orthogonal enforcement layers).
- [x] `docs/product/architecture/adr-0050-earned-trust-read-side-caps.md`
  (the immediate sibling on the READ side: window and result caps;
  the REFUSE-not-TRUNCATE discipline; the compile-time-constant
  posture; the existing `gate-5-mutants-<crate>` covers the change
  via `--in-diff`; the redaction-symmetric named refusal; the style
  template this ADR follows).
- [x] `crates/pulse/src/store.rs` (`IngestReceipt` at line 30 with
  one field `count: usize`; `MetricsRecorder` trait imported from
  `crate::metrics`; `InMemoryMetricStore::ingest` at line 137 with
  the per-metric loop and the `entry().or_insert_with()` shape;
  `MetricStore` trait at line 66 unchanged in this slice).
- [x] `crates/pulse/src/file_backed.rs` (`Inner` at line 83 holding
  `series: HashMap<(TenantId, SeriesKey), SeriesEntry>` and the WAL
  buffer; WAL replay at line 158 calling `apply_ingest(&mut series,
  &tenant, metrics)`; live ingest at line 273 calling the same
  `apply_ingest` after `append_wal`; `apply_ingest` at line 349 with
  the per-metric loop the cap edits).
- [x] `crates/pulse/src/lib.rs` (the public surface; `pub use
  store::{InMemoryMetricStore, IngestReceipt, MetricStore,
  MetricStoreError}`; this is where `pub const
  MAX_SERIES_PER_TENANT` lands).
- [x] `crates/pulse/src/metric.rs` (`SeriesKey` is `pub(crate)` and
  stays that way; the cap does not need to expose it).
- [x] `crates/pulse/src/metrics.rs` (the `MetricsRecorder` trait with
  `record_ingest` and `record_query`; `NoopRecorder` and
  `CapturingRecorder`; this is where the new
  `record_series_refused` hook is added).
- [x] `crates/self-observe/src/lumen_bridge.rs` (the
  `LumenToPulseRecorder` shape: bridge holds `Arc<dyn MetricStore +
  Send + Sync>`, emits one-point `MetricBatch`es with metric name
  `lumen.<event>.count`, kind Sum, value=1 per event; pulse ingest
  errors are swallowed deliberately).
- [x] `crates/self-observe/src/cinder_bridge.rs` (the same shape for
  cinder events; `cinder.<event>.count` naming; ADR-0038 locked the
  public surface; the new pulse-to-pulse bridge follows the same
  template).

## Decisions (FLAG resolution)

### D1: Cap value is 10_000 distinct SeriesKey per tenant

`pub const MAX_SERIES_PER_TENANT: usize = 10_000;` in
`crates/pulse/src/lib.rs`.

Candidates considered (per DISCUSS FLAG 1):

- **1_000** rejected. A real production tenant with 50 services and
  modest per-service cardinality (env + service + region + pod, 4
  labels of 5-20 distinct values each) plausibly crosses 1k under
  normal traffic. The cap would bite legitimate workloads; the
  operator would respond by tightening the meaning of "the cap" until
  it no longer means refusal of pathology.
- **100_000** rejected. At rough v0/v1 entry-cost (a `Metric`
  metadata struct plus a sorted point vector; the metadata alone is a
  small `String` + `BTreeMap` pair), 100_000 entries cost 10MB+ of
  metadata before any points. The cap would stop OOM only at the
  absolute upper bound and let a slowly-bleeding cardinality leak
  accumulate for hours before refusal. The residuality analysis
  named "100k" as a starting default but explicitly flagged it as
  unmeasured; DISCUSS preferred to start tighter.
- **10_000** accepted. The sweet spot: well above a healthy tenant's
  natural per-tenant series count (50 services x 100 distinct series
  per service = 5_000), low enough that 10_000 entries cost only a
  few MB of metadata, low enough that a cardinality bomb is refused
  within minutes rather than hours. Per-tenant tuning is a successor
  slice (env-driven configurability, declared OUT here).

### D2: Counter surfaces in BOTH `IngestReceipt` AND the existing `MetricsRecorder` trait

`IngestReceipt` grows one additive field `series_refused: usize`.
`crates/pulse/src/metrics.rs::MetricsRecorder` grows one new method
`record_series_refused(&self, tenant: &TenantId, count: usize)` with
a no-op default body so existing impls (and any downstream impl) do
not need to be re-implemented.

Candidates considered (per DISCUSS FLAG 2):

- **(A) Receipt field only** rejected. Synchronous per-call signal is
  immediate but the longitudinal view is invisible; the operator
  cannot see a refusal rate across the last hour without scraping
  every single call's receipt out of process logs. KPI 3 specifically
  asks for a longitudinal view.
- **(B) Self-observe metric only** rejected. The longitudinal view is
  cheap but the immediate per-call view is invisible; the
  aperture-storage-sink caller that translates pulse's receipt into
  OTLP partial-success has no per-call number to translate (it
  would have to inspect the bridge target store, which is the wrong
  coupling).
- **(C) BOTH** accepted. The two surfaces answer different questions
  and both are cheap. The receipt is the synchronous per-call signal
  the aperture partial-success path translates; the recorder hook is
  the longitudinal signal the existing `LumenToPulseRecorder` /
  `CinderToPulseRecorder` pattern surfaces.

The "extend existing trait" path is preferred over "add a small new
trait" because the cohesion is right: `MetricsRecorder` already
records ingest and query events; series-refusal is another ingest-side
event in the same family. The default no-op body keeps the addition
non-breaking for downstream implementers semantically (the only
public-api diff is a new method with a default; existing impls
continue to compile and behave identically). KPI 7 carved out one
additive field on `IngestReceipt` as the contracted public-api change;
the trait method addition is acknowledged as a second additive change,
explicitly recorded here.

The bridge in `crates/self-observe/` follows the
`LumenToPulseRecorder` / `CinderToPulseRecorder` template. Metric
name `pulse.series.refused.count`, value = `count`, kind `Sum`, point
attribute `{tenant}`. Best-effort emission, pulse errors swallowed
(the v0 `MetricStoreError` is an empty enum; the `let _ =` is
forward-compatible). The bridge is one new file under
`crates/self-observe/src/` mirroring the existing shape.

### D3: Batch semantics are PARTIAL APPLY

For each `Metric` in the batch, the per-metric arm in
`apply_ingest`:

- If the metric's `SeriesKey` matches an existing entry: extend the
  points vector (existing path, unchanged).
- If the metric's `SeriesKey` is new AND the per-tenant count is
  below `MAX_SERIES_PER_TENANT`: insert and extend (existing path
  with the cap check added at the new-key arm).
- If the metric's `SeriesKey` is new AND the per-tenant count is at
  or above `MAX_SERIES_PER_TENANT`: REFUSE this metric, increment
  `series_refused` by 1, drop the metric's points, **CONTINUE THE
  LOOP**.

The per-metric loop never aborts early. The receipt's `count`
accumulates points stored (matching + new-below-cap); the receipt's
`series_refused` accumulates refused metrics in this call.

Candidates considered (per DISCUSS FLAG 3):

- **REJECT-WHOLE** rejected. Loses good data (the existing-series
  points in the same batch the cap has no quarrel with); violates
  A-D6 "honest three-way outcomes"; produces a fabricated-empty
  signal (A-U4) by reporting the whole batch as failed when only one
  metric was over-cap.
- **PARTIAL APPLY** accepted. The cap is per-metric; the loop never
  breaks early; the receipt reports honestly on both counts. This
  aligns with the OTLP partial-success contract aperture already
  implements
  (`opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsPartialSuccess`).
  Aperture's translation of pulse's receipt into the wire-side
  partial-success report is its own slice; this DESIGN pins the
  pulse-side semantics.

### D4: The ADR is ADR-0051 (new); ADR-0045 is NOT modified

`ls docs/product/architecture/adr-0051*` returns no hits; the latest
existing is `adr-0050-earned-trust-read-side-caps.md`; the next free
number is 0051. The new ADR is
`docs/product/architecture/adr-0051-pulse-per-tenant-cardinality-watermark.md`.

Candidates considered (per DISCUSS FLAG 4):

- **Amendment of ADR-0045** rejected. ADRs in this repository are
  immutable (the project memory and ADR-0001 set this rule; every
  preceding ADR including ADR-0049 and ADR-0050 honour it). An
  amendment would create a precedent that drifts.
- **No ADR, design brief only** rejected. The cap is a contract
  change on pulse's ingest behaviour, visible at the receipt and at
  the recorder seam; an operator reading ADR-0045 for the identity
  decision should find the cap policy cross-referenced from there.
  The ADR is the durable cross-reference.
- **New ADR-0051** accepted. It cites ADR-0045 (the precedent that
  opened this consequence; NOT modified), ADR-0049 (the Earned-Trust
  WRITE-side sibling; NOT modified), and ADR-0050 (the Earned-Trust
  READ-side sibling; NOT modified). The three together (ADR-0049 +
  ADR-0050 + ADR-0051) are the Earned-Trust trilogy: durability,
  refusal-on-overreach at read, refusal-on-overreach at write.

### D5: WAL replay semantics. The cap NEVER refuses during replay

Pinned explicitly: during `apply_ingest` from WAL replay (the
`open()` path at `crates/pulse/src/file_backed.rs:158`), existing
entries rebuild WITHOUT the cap check. After replay completes, the
per-tenant count IS the rebuilt cardinality; the cap applies only to
LIVE ingest post-replay (the `ingest()` path at line 273, which calls
`apply_ingest` after `append_wal`).

Concretely, if a WAL contains 50_000 series for a single tenant and
`MAX_SERIES_PER_TENANT` is 10_000, `open()` rebuilds all 50_000 (the
persisted data is honoured) but blocks any NEW series ingest at live
time until the per-tenant count drops below 10_000. Since this slice
declares eviction OUT of scope, the count does not drop without
operator intervention; the tenant is in a "no new series" state until
the operator triggers a snapshot-and-prune in a future slice or
restarts with a tighter cap that still preserves existing series.

The cap is a FORWARD GATE, never a retroactive eviction. This
preserves ADR-0040 Decision 2 (append-and-sort: replay reconstructs
what was accepted) and avoids the A-U1 "silent data loss" attractor
via the restart path (US-04).

**Implementation shape**: `apply_ingest` gains a boolean parameter
`enforce_cap: bool`. The WAL-replay call site passes `false`; the
live-ingest call site passes `true`. The parameter is internal (the
function is private to the file; the public seam stays the same).
Two variants or a separate function would also work; the boolean
parameter is the smallest diff and the easiest to read. The
acceptance test that snapshots, reopens, and asserts replay rebuilds
all entries (US-04 Scenario 1) kills the mutant that swaps the
boolean.

### D6: Public surface, signature

```rust
// crates/pulse/src/lib.rs
pub const MAX_SERIES_PER_TENANT: usize = 10_000;
```

The constant is `pub` so the acceptance suite can address it by
name (the boundary tests at `MAX_SERIES_PER_TENANT` and
`MAX_SERIES_PER_TENANT + 1` directly; the seed loop uses it). It
mirrors ADR-0050's `MAX_WINDOW_SECONDS` / `MAX_RESULT_ROWS` posture.

```rust
// crates/pulse/src/store.rs
pub struct IngestReceipt {
    pub count: usize,            // existing: points appended to the store
    pub series_refused: usize,   // new: NEW SeriesKeys refused over the cap in this call
}
```

The field is additive. `IngestReceipt` derives `Default` if useful
(it does not today); each construction site that returns
`IngestReceipt { count: ... }` becomes `IngestReceipt { count: ...,
series_refused: ... }`. There are exactly two such sites today
(`crates/pulse/src/store.rs:170` in the in-memory adapter,
`crates/pulse/src/file_backed.rs:264` and `:280` in the file-backed
adapter); the crafter updates both. No `#[non_exhaustive]` is added;
the receipt is two simple `usize` fields and the slice declares no
further additive fields.

```rust
// crates/pulse/src/metrics.rs
pub trait MetricsRecorder: Send + Sync {
    fn record_ingest(&self, tenant: &TenantId, point_count: usize);
    fn record_query(&self, tenant: &TenantId, matched_count: usize);

    /// New in this slice. Default no-op so existing impls do not need
    /// to be re-implemented.
    fn record_series_refused(&self, _tenant: &TenantId, _count: usize) {}
}
```

The default body is intentionally empty so `NoopRecorder` continues
to do nothing without an explicit method override.
`CapturingRecorder` gains a `RecordedEvent::SeriesRefused { tenant,
count }` variant and an override that pushes onto the events vector;
this is a small additive change on a test-helper type.

### D7: Enforcement point and per-tenant count tracking

The check goes inside `apply_ingest` in
`crates/pulse/src/file_backed.rs`, INSIDE the per-metric for-loop,
BEFORE `series.entry(key).or_insert_with(...)` is called for a key
that DOES NOT already exist in the map. The same edit applies in
`crates/pulse/src/store.rs::InMemoryMetricStore::ingest` (its
per-metric loop on `batch.metrics`).

The per-tenant count is tracked via a SHADOW counter, NOT computed
on the fly.

Candidates considered:

- **Derive on the fly** (`series.keys().filter(|(t, _)| t == tenant).count()`
  per-metric): O(N) per metric where N grows up to
  `MAX_SERIES_PER_TENANT`. For a batch of 100 metrics on a tenant at
  10_000 series, this is 1M comparisons per batch (cheap absolutely,
  but mutation-test runtime suffers and the cost is linear in the
  cap; a successor slice raising the cap to 100_000 makes it 10M per
  batch). Rejected.
- **Shadow counter** (`HashMap<TenantId, usize>` next to the series
  map, updated atomically with each insert): O(1) per metric;
  trivially correct under the same Mutex that protects the series
  map (so the count-and-insert is atomic). The shadow counter is
  initialised on `open()` by counting the rebuilt series per tenant
  (a single pass over `series.keys()` once, after replay completes);
  for in-memory it starts empty. Accepted.

For the file-backed adapter, the shadow counter lives inside `Inner`
next to `series` (already a `Mutex<Inner>`); for the in-memory
adapter, it lives inside `InnerState` next to `series` (already a
`Mutex<InnerState>`). The same Mutex serialises the cap-check, the
shadow-counter increment, and the series-map insert; the three are
atomic per metric. The refused counter (for `record_series_refused`
emission and for `IngestReceipt::series_refused`) is a local
accumulator inside the ingest call; it does not live in `Inner`
because per-call accumulation is per-batch state, not store state.

A separate per-tenant cumulative refused-since-start counter is NOT
maintained inside `Inner`; the longitudinal view lives in the
`MetricsRecorder::record_series_refused` emission seam (which the
bridge in self-observe turns into pulse points; queries over time
return the cumulative-by-window view). This avoids duplicating state
that is already a derived view of the recorder events.

The shadow counter shape:

```rust
// inside Inner in file_backed.rs and inside InnerState in store.rs
series_count_per_tenant: HashMap<TenantId, usize>,
```

Maintenance rules:

- On `open()` (file-backed): after WAL replay, populate by counting
  the rebuilt series per tenant. One pass.
- On a NEW key insert (live ingest, cap check passed): increment the
  counter for that tenant by 1.
- On an EXISTING key match (any path): leave the counter alone.
- On WAL replay (`enforce_cap: false`): increment the counter even
  though no cap check fires, so the counter reflects rebuilt
  cardinality.
- On a REFUSED new key (live ingest, cap check fired): leave the
  counter alone (the refusal does NOT insert).

## Reuse analysis (mandatory table)

| Concern | Existing artefact | Action | Justification |
|---|---|---|---|
| Per-tenant series index | `Inner::series: HashMap<(TenantId, SeriesKey), SeriesEntry>` in `file_backed.rs:84`; `InnerState::series` in `store.rs:111` | **Reuse** | The map already keys by `(tenant, SeriesKey)`; the cap is a projection of the existing key. No new map. |
| Per-tenant count | None | **Add (additive)** | Shadow `HashMap<TenantId, usize>` next to the series map inside `Inner` / `InnerState`. Internal; not exposed. |
| Cap constant | None | **Add (additive)** | `pub const MAX_SERIES_PER_TENANT: usize = 10_000;` in `lib.rs`. |
| Receipt field | `IngestReceipt { count: usize }` in `store.rs:30` | **Extend (additive)** | Add `series_refused: usize`. Two construction sites updated. |
| Recorder hook | `MetricsRecorder` trait in `metrics.rs:24` | **Extend (additive, default-method)** | Add `fn record_series_refused(&self, _tenant: &TenantId, _count: usize) {}` with no-op default. `CapturingRecorder` overrides; `NoopRecorder` inherits the default. |
| `RecordedEvent` (test helper) | `enum RecordedEvent { Ingest, Query }` in `metrics.rs:38` | **Extend (additive variant)** | Add `SeriesRefused { tenant: TenantId, count: usize }`. Test-only type. |
| Self-observe bridge | `LumenToPulseRecorder` in `crates/self-observe/src/lumen_bridge.rs`; `CinderToPulseRecorder` in `cinder_bridge.rs` | **Add (new file, same shape)** | One new file `crates/self-observe/src/pulse_cardinality_bridge.rs` mirroring the existing pattern. Holds `Arc<dyn MetricStore + Send + Sync>`. One method `record_series_refused` emits `pulse.series.refused.count` value=count, kind Sum, point attribute `{tenant}`. Re-exported from `self-observe/src/lib.rs`. |
| `MetricStore` trait signature | `trait MetricStore { fn ingest, fn query, fn query_with }` in `store.rs:66` | **NO CHANGE** | The cap rides in the implementation, not on the trait. `cargo public-api` confirms byte-identity beyond `IngestReceipt`'s additive field. |
| `apply_ingest` | `fn apply_ingest(series, tenant, metrics)` in `file_backed.rs:349` | **Extend (additive parameter)** | Gain `enforce_cap: bool` and a `&mut HashMap<TenantId, usize>` for the shadow counter. Returns the in-call refused count. Private; not on the trait. |
| WAL on-disk record | `WalRecord::Ingest { tenant, metrics }` in `file_backed.rs:48` | **NO CHANGE** | Replay rebuilds existing series regardless of count; the WAL is the durable record of accepted ingests; no cap-related field is needed. |
| `SeriesKey` visibility | `pub(crate) struct SeriesKey` in `metric.rs:51` | **NO CHANGE** | The cap does not need the key to be `pub`. |

No new crate. No new external dependency. The change is bounded to:

- `crates/pulse/src/lib.rs` (one `pub const`),
- `crates/pulse/src/store.rs` (the receipt field; the shadow counter
  in `InnerState`; the cap arm in the per-metric loop; the
  `record_series_refused` invocation),
- `crates/pulse/src/file_backed.rs` (the shadow counter in `Inner`;
  the post-replay initialisation; the `apply_ingest` signature with
  `enforce_cap`; the cap arm and the refused-accumulator; the
  `record_series_refused` invocation; the two `IngestReceipt`
  construction sites),
- `crates/pulse/src/metrics.rs` (the trait method addition with
  default; the `RecordedEvent::SeriesRefused` variant; the
  `CapturingRecorder` override),
- `crates/self-observe/src/pulse_cardinality_bridge.rs` (new file;
  one struct + one trait impl),
- `crates/self-observe/src/lib.rs` (one re-export).

## Handoff to DEVOPS (Apex)

Slim handoff; nothing new on the operational surface:

- **NO new crate**. All work lands inside existing `pulse` and
  `self-observe` crates.
- **NO new external dependency**. The cap is in-process arithmetic
  on a `usize`; the bridge follows the existing `LumenToPulseRecorder`
  template (same crate dependencies: `aegis`, `pulse`).
- **NO new CI workflow**. `gate-5-mutants-pulse` already runs on
  every push for the `pulse` crate; it scopes via `--in-diff` so the
  changed files (`store.rs`, `file_backed.rs`, `metrics.rs`,
  `lib.rs`) are covered automatically. `gate-2-public-api` on
  `pulse` already runs on every push; the additive
  `IngestReceipt::series_refused` field and the additive default
  trait method appear as new informational items, NOT breaking
  changes (the trait method has a default body so downstream impls
  continue to compile). `gate-5-mutants-self-observe` covers the new
  bridge file.
- **NO new git tag**. Slice 01 ships on a normal feature commit on
  `main` per the trunk-based posture.
- **Acceptance entry point** is in-process integration tests under
  `crates/pulse/tests/` (and `crates/self-observe/tests/` for the
  bridge); no new gateway-level test.

## Earned-Trust verification layers (reproduced from ADR-0049 / ADR-0050)

The cap is a contract: "above the per-tenant ceiling, refuse new
keys; never silently drop, never panic, never evict existing". Three
orthogonal layers prove the contract holds:

- **Subtype check (compile-time)**: removing the
  `MAX_SERIES_PER_TENANT` constant fails the build at every test-site
  reference. The `enforce_cap: bool` parameter on `apply_ingest` is
  not optional; removing it shifts every call site.
- **AST structural check (gate-2 public-api)**: the `pub const` on
  `lib.rs`, the `IngestReceipt::series_refused` field, the
  `record_series_refused` trait method, and the new bridge struct in
  `self-observe` appear in the public-api diff as additive items. A
  removal would show as a breaking change; the diff IS the structural
  proof.
- **Behavioural check (acceptance suite)**: the five US-* scenarios
  exercise the cap boundary (N-1, N, N+1), per-tenant isolation, WAL
  replay coherence, partial-apply, and the receipt-and-bridge
  observability surface. Mutation testing (`cargo mutants` at the
  100% kill rate on changed files) kills the boundary mutants
  (`>=` to `>`), the loop-abort mutant, the
  enforce-cap-true-on-replay mutant, the shadow-counter
  mis-initialisation mutant.

A single-layer bypass is caught by at least one of the other two.

## Changelog

- 2026-05-27: feature `pulse-cardinality-watermark-v0` DESIGN wave
  artefacts written by Morgan. Four DISCUSS flags resolved (cap value
  10_000; counter in both receipt and recorder hook; partial apply;
  new ADR-0051). WAL-replay semantics pinned (cap NEVER fires during
  replay; `enforce_cap: bool` parameter on `apply_ingest`).
  Enforcement point pinned (per-metric loop in `apply_ingest`; shadow
  per-tenant counter inside `Inner` / `InnerState`, under the same
  Mutex as the series map for atomicity). Recorder hook pinned
  (extend existing `MetricsRecorder` with a default no-op method).
  Reuse analysis: no `MetricStore` trait change; no WAL on-disk
  change; no new crate; no new external dependency. ADR-0051 written
  citing ADR-0045 / ADR-0049 / ADR-0050 as the precedents (none
  modified).
