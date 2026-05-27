# ADR-0051 — Pulse per-tenant cardinality watermark on the shared ingest seam

- **Status**: Accepted
- **Date**: 2026-05-27
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `pulse-cardinality-watermark-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0045 (pulse series identity is the full label set;
  Decision 1 made `(tenant, SeriesKey)` the index key; Consequences
  Negative paragraph 2 accepted the cost characteristic as "a v2
  concern if series cardinality per name ever grew large"; this ADR
  is the closure of that named open question, cited and NOT
  modified). ADR-0049 (Earned-Trust at startup must honour fsync;
  the WRITE-side Earned-Trust precedent, the
  wire-then-probe-then-use invariant, the three orthogonal
  enforcement layers; cited as the immediate precedent on the WRITE
  side, NOT modified). ADR-0050 (Earned-Trust on the read side:
  per-request window cap and result-size cap; the READ-side
  Earned-Trust sibling, the REFUSE-not-TRUNCATE discipline, the
  compile-time-constant posture; cited as the immediate precedent on
  the READ side, NOT modified). ADR-0040 (the WAL + snapshot +
  replay recovery discipline whose append-and-sort posture the cap
  preserves; cited, NOT modified). ADR-0042 (the originating
  fail-closed tenancy at the read-API boundary that the cap
  reproduces at the write boundary on the resource axis; cited, NOT
  modified). ADR-0005 (the five CI gates including Gate 5 100%
  mutation kill on modified files and Gate 2 `cargo public-api`
  byte identity on store trait signatures; cited, NOT modified).

## Context

Pulse identifies a metric series by `(tenant, SeriesKey)`, where
`SeriesKey` is the metric name plus the full `resource_attributes`
label set (ADR-0045 Decision 1). The series index is
`HashMap<(TenantId, SeriesKey), SeriesEntry>` in both adapters
(`crates/pulse/src/file_backed.rs:84`,
`crates/pulse/src/store.rs:111`). The shared `apply_ingest` function
(`crates/pulse/src/file_backed.rs:349`) inserts a new entry for
every distinct `(tenant, SeriesKey)` and extends the points vector
for matching ones. There is no ceiling, no counter, no refusal.

ADR-0045 made this correctness call deliberately: making the full
label set part of identity preserved per-service provenance and
unblocked the matcher feature
(`query-api-label-matchers-v0`). It explicitly named the resulting
cost: "no secondary index is introduced; premature indexing is
explicitly out of scope and would be a v2 concern if series
cardinality per name ever grew large" (ADR-0045 Consequences
Negative, paragraph 2).

The residuality analysis
(`docs/product/architecture/residuality-analysis.md`, commit 50e20b5)
flagged this open consequence as the S04 row of the incidence
matrix. The pulse cell reads **B OOM under enough labels**. The
A-U1 attractor "Silent data loss" is realised on OOM kill: a client
(misconfigured or hostile) emitting metrics with growing-cardinality
labels (a timestamp, a UUID, a per-request ID) drives the per-tenant
index without bound until the resident set size of the process
exceeds the cgroup limit and the kernel kills it. The kill takes
every co-tenant down with it, violating the A-D4 "Fail-closed
tenancy at every plane boundary" attractor on the resource axis.

The follow-up roadmap (`docs/residuality-followups-roadmap.md`)
lists this as item 3 of 3 (M-4), the third and final residuality
follow-up. M-1 (`earned-trust-fsync-probe-v0`, ADR-0049) and M-2
(`honest-read-caps-v0`, ADR-0050) have shipped; M-4 lands one
feature wave after M-2.

ADR-0049 made the Earned-Trust claim CODE on the WRITE side
(durability). ADR-0050 made the same claim CODE on the READ side
(refusal-on-overreach). This ADR makes the same claim CODE on the
WRITE side at the cardinality boundary: the platform refuses **out
loud** when a tenant's distinct-series count would cross a
compile-time per-tenant ceiling, BEFORE the index grows past it,
with the same posture the other two precedents established (named
refusal, no silent drop, no panic, observable via two surfaces).

The cap is FORWARD-LOOKING. Replay rebuilds whatever series the WAL
holds, regardless of count; the cap applies only to NEW series at
post-replay LIVE ingest. This is a deliberate refinement: the WAL
is the durable record of accepted ingests, and a cap that fired
during replay would silently un-accept data, an A-U1 attractor by
another route.

ADRs in this repository are immutable (the convention set by
ADR-0001; every preceding ADR including ADR-0049 and ADR-0050
honour it). ADR-0045 is Accepted and referenced as the precedent
this ADR REFINES at the cost characteristic; it is NOT modified.
ADR-0049 and ADR-0050 are the two immediate Earned-Trust siblings;
both are cited as precedent and NOT modified. ADR-0051 is the next
free number (`ls docs/product/architecture/adr-0051*` returns no
hits; `adr-0050-earned-trust-read-side-caps.md` is the latest;
0051 is the next).

## Decision

### 1. Cap: 10_000 distinct SeriesKey per tenant, per store instance

`pub const MAX_SERIES_PER_TENANT: usize = 10_000;` in
`crates/pulse/src/lib.rs`. Each pulse store instance (file-backed or
in-memory) enforces this cap PER TENANT. The cap is on the index
WIDTH, NOT depth: an existing series can receive any number of
points; only NEW `SeriesKey` insertions are gated. The cap is
PER-TENANT, NOT global: one tenant's count does not include another
tenant's entries.

The boundary is `>=`, not `>`: a per-tenant count of EXACTLY
`MAX_SERIES_PER_TENANT` refuses the next new key. The first
`MAX_SERIES_PER_TENANT` distinct `SeriesKey`s for a tenant are
accepted; the `MAX_SERIES_PER_TENANT + 1`th is refused. This
boundary is asserted in the acceptance suite (US-01 Scenario 4) and
kills the `>=` to `>` mutant.

### 2. Refused-signal surfaces in BOTH the receipt and the recorder hook

`IngestReceipt` grows one additive field:

```rust
pub struct IngestReceipt {
    pub count: usize,            // existing: points appended in this call
    pub series_refused: usize,   // new: NEW SeriesKeys refused over the cap in this call
}
```

The field is the synchronous per-call signal. The
`aperture-storage-sink::ingest_metrics` helper that calls
`MetricStore::ingest` reads it directly to translate to the OTLP
`ExportMetricsPartialSuccess` wire-side report (aperture's slice,
not this one).

The `MetricsRecorder` trait
(`crates/pulse/src/metrics.rs`) grows one new method with a no-op
default body:

```rust
pub trait MetricsRecorder: Send + Sync {
    fn record_ingest(&self, tenant: &TenantId, point_count: usize);
    fn record_query(&self, tenant: &TenantId, matched_count: usize);
    fn record_series_refused(&self, _tenant: &TenantId, _count: usize) {}
}
```

The default no-op body keeps the addition non-breaking for any
downstream implementer: existing impls continue to compile and behave
identically (`NoopRecorder` does nothing extra; `CapturingRecorder`
overrides to push a `RecordedEvent::SeriesRefused` variant).

A new bridge `PulseCardinalityToPulseRecorder` lives in
`crates/self-observe/src/pulse_cardinality_bridge.rs`, mirroring
`LumenToPulseRecorder` and `CinderToPulseRecorder`:

- Holds `Arc<dyn pulse::MetricStore + Send + Sync>`.
- `record_series_refused(tenant, count)` emits a one-point
  `MetricBatch` with metric name `pulse.series.refused.count`,
  value=`count as f64`, kind `Sum`, point attribute `{tenant}`.
- `record_ingest` and `record_query` are no-ops in this bridge
  (pulse-to-pulse for ingest and query would loop; the lumen and
  cinder bridges cover the corresponding upstream pillars).

The two surfaces answer different questions: the receipt is the
synchronous per-call signal; the recorder hook (via the bridge) is
the longitudinal queryable signal. Both are cheap and both are
observable. The DISCUSS-time LIKELY recommendation was BOTH; this
ADR accepts it.

### 3. Batch semantics: PARTIAL APPLY, never reject-whole

For each `Metric` in the batch, inside `apply_ingest`'s per-metric
loop:

- If the metric's `SeriesKey` matches an existing entry: extend
  the points vector (existing path, unchanged).
- If the metric's `SeriesKey` is NEW AND the per-tenant count is
  strictly less than `MAX_SERIES_PER_TENANT`: insert and extend
  (existing path with the cap check at the new-key arm).
- If the metric's `SeriesKey` is NEW AND the per-tenant count is
  at or above `MAX_SERIES_PER_TENANT`: REFUSE the metric, increment
  the in-call refused accumulator by 1, drop the metric's points,
  CONTINUE THE LOOP.

The loop never aborts early. The receipt's `count` accumulates
points stored across the extend and new-insert arms; the receipt's
`series_refused` is the final value of the refused accumulator.

REJECT-WHOLE is rejected. It would lose good data (the existing-
series points in the same batch the cap has no quarrel with); it
would violate A-D6 "honest three-way outcomes"; it would produce a
fabricated-empty signal (A-U4) by reporting the whole batch as
failed when only one metric was over-cap.

### 4. WAL replay never refuses. The cap is a forward-looking gate

The shared `apply_ingest` function gains an internal boolean
parameter `enforce_cap: bool`:

- WAL replay call site (`crates/pulse/src/file_backed.rs:158`,
  inside `open()`) passes `enforce_cap=false`. Every WAL record is
  reconstructed; the cap arm is bypassed; the per-tenant count is
  incremented on each insert; the refused-in-call accumulator stays
  zero.
- Live-ingest call site (`crates/pulse/src/file_backed.rs:273`,
  inside `FileBackedMetricStore::ingest`) passes `enforce_cap=true`.
  The cap arm fires on new keys above the threshold.

After replay completes, the shadow per-tenant counter holds the
rebuilt cardinality. If the rebuilt count is already at or above
`MAX_SERIES_PER_TENANT` for some tenant (either because the cap was
tightened between deploys, or because slice 02's env-driven
configurability lifts the cap to a smaller value in a later
deploy), live ingest of any NEW series for that tenant is refused
until the count drops below the cap. Eviction is OUT of scope for
slice 01; the count does not drop without operator intervention
(snapshot-and-prune or restart with a different cap that still
preserves existing data).

The cap is FORWARD-LOOKING. The WAL is the durable record of
accepted ingests; replay rebuilds what was accepted, regardless of
how the cap value relates to the count today. A retroactive cap that
refused during replay would silently truncate the index for any
tenant near the cap; this is the A-U1 "silent data loss" attractor
by another route, and is forbidden by this decision.

### 5. Enforcement point: inside `apply_ingest`, with a shadow per-tenant counter under the same Mutex

The cap check goes inside `apply_ingest`'s per-metric for-loop,
BEFORE `series.entry(key).or_insert_with(...)` is called for a key
that DOES NOT already exist. The same edit applies in
`crates/pulse/src/store.rs::InMemoryMetricStore::ingest`. The two
adapters' semantics stay in lockstep.

The per-tenant count is tracked via a SHADOW counter, NOT computed
on the fly. The shadow lives inside `Inner` (file-backed) and
`InnerState` (in-memory), next to the series map, protected by the
SAME Mutex. The Mutex serialises the cap-check, the shadow-counter
increment, and the series-map insert; the three are atomic per
metric.

Computing the count on the fly (`series.keys().filter(|(t, _)|
t == tenant).count()` per metric) is O(N) where N grows up to
`MAX_SERIES_PER_TENANT`. For a batch of 100 metrics on a tenant at
10_000 series, this is 1M comparisons per batch (cheap absolutely,
but the cost is linear in the cap; a successor slice raising the
cap makes it worse). The shadow counter is O(1) per metric.
Rejected: derive-on-fly. Accepted: shadow counter under the same
Mutex.

The shadow counter is initialised:

- On `FileBackedMetricStore::open()`: after WAL replay completes
  (line 162 in the current code), one pass over `series.keys()`
  populates the counter.
- On `InMemoryMetricStore::new()`: empty.

The shadow counter is updated:

- On a NEW key insert (any path, both replay and live):
  `+= 1` for the tenant.
- On an EXISTING key match: unchanged.
- On a REFUSED new key (live only): unchanged (the refusal did not
  insert).

A separate per-tenant cumulative refused-since-start counter is NOT
maintained inside `Inner`. The longitudinal view lives in the
`record_series_refused` emission seam (the bridge in self-observe
turns each refusal into pulse metric points; query-api over a window
returns the cumulative-by-window view). This avoids duplicating
state that is already a derived view of the recorder events.

### 6. No `MetricStore` trait method-signature change. No WAL on-disk shape change

`pulse::MetricStore`'s three methods (`ingest`, `query`,
`query_with`) keep byte-identical signatures from the prior tag.
`cargo public-api` (Gate 2) confirms.

`WalRecord::Ingest { tenant, metrics }` (line 48 of `file_backed.rs`)
keeps byte-identical on-disk shape. The cap is a live-ingest policy;
the WAL records accepted ingests; replay rebuilds what was accepted.

The two additive items in the public-api diff:

- `IngestReceipt::series_refused: usize` (an additive field, not a
  new struct).
- `MetricsRecorder::record_series_refused(&self, &TenantId, usize)`
  with a default no-op body (an additive method, not a new trait).

A `pub const MAX_SERIES_PER_TENANT: usize = 10_000;` in `lib.rs`
appears in the public-api diff as a new informational item, NOT a
breaking change.

### 7. No new event, no new metric envelope, no new dashboard

The receipt field and the recorder hook ARE the signal. No
structured event log is emitted on a cap refusal beyond the
recorder's `record_series_refused` invocation (which the bridge in
self-observe turns into pulse metric points, themselves queryable
via the existing `query-api`). No Prism panel, no beacon rule, no
alerting threshold is wired by this slice; a future feature MAY add
those once a downstream consumer exists.

The metric name `pulse.series.refused.count` follows the existing
self-observe naming convention (`<source>.<event>.count`,
established by `lumen.<event>.count` and `cinder.<event>.count`).
Value=`count`, kind `Sum`. Point-level attribute `{tenant}` (NOT a
series-level attribute, so the bridge does not multiply self-observe
cardinality by the number of tenants under the cap; each emission
carries the tenant on the point).

## Uniform receipt shape

The two-field shape is the unique public-api shape `IngestReceipt`
takes after this slice:

```rust
pub struct IngestReceipt {
    pub count: usize,
    pub series_refused: usize,
}
```

The struct is NOT marked `#[non_exhaustive]`. Slice 01 declares no
further additive fields; if a future slice adds one (e.g.
`points_refused: usize` for a per-series-point cap), it lands with
its own ADR and the resulting public-api change is documented there.

## Alternatives considered

### Cap value A (rejected): 1_000

The most conservative bound. For: minimum memory footprint per
tenant. Against: a real production tenant with 50 services and
modest per-service cardinality (env + service + region + pod, 4
labels of 5-20 distinct values each) plausibly crosses 1k under
normal traffic. The cap would bite legitimate workloads; operator
response would be to raise the cap, defeating it rather than
honouring it. Rejected.

### Cap value B (rejected): 100_000

The most permissive bound; the residuality analysis's named
starting default. For: minimal false positives. Against: at rough
v0/v1 entry-cost (a `Metric` metadata struct plus a sorted point
vector; the metadata alone is a small `String` + `BTreeMap` pair),
100_000 entries cost 10MB+ of metadata before any points. The cap
would stop OOM only at the absolute upper bound and let a
slowly-bleeding cardinality leak accumulate for hours before
refusal. Rejected as too generous against an untested lifetime.

### Cap value C (accepted): 10_000

The sweet spot: well above a healthy tenant's natural per-tenant
series count (50 services x 100 distinct series per service =
5_000), low enough that 10_000 entries cost only a few MB of
metadata, low enough that a cardinality bomb is refused within
minutes rather than hours. Per-tenant tuning is a successor slice
(env-driven configurability, declared OUT here). Accepted.

### Counter location A (rejected): `IngestReceipt` field alone

For: simplest possible diff (one field added). Against: the
synchronous caller sees the per-call signal but the longitudinal
view is invisible; the operator cannot see a refusal rate across
the last hour without scraping every single call's receipt out of
process logs. KPI 3 specifically asks for a longitudinal view.
Rejected.

### Counter location B (rejected): self-observe metric alone

For: the longitudinal view is cheap (the existing bridge pattern
extends to one more recorder). Against: the immediate per-call view
is invisible; the aperture-storage-sink caller that translates
pulse's receipt into OTLP partial-success has no per-call number to
translate. Rejected.

### Counter location C (accepted): BOTH receipt field and recorder hook

For: the two surfaces answer different questions and both are
cheap. The receipt is the synchronous per-call signal; the recorder
hook is the longitudinal queryable signal. The recorder hook is
extended on the existing `MetricsRecorder` trait (cohesion: one
observability seam, one trait, one family of events) with a default
no-op body (non-breaking for any external impl). Accepted.

### Recorder shape A (rejected): new sibling trait `CardinalityRecorder`

Adding a new trait dedicated to cardinality refusals. For: surface
locality (the new event lives on its own trait). Against: two
recorder traits proliferate the observability seam; existing
implementations would need to opt in to two traits explicitly; the
cohesion is wrong (refusal-counting is in the same family as
ingest-counting). Rejected.

### Recorder shape B (accepted): extend `MetricsRecorder` with default-method

Adding `fn record_series_refused(&self, _tenant: &TenantId, _count:
usize) {}` to the existing trait with a no-op default body. For:
cohesion (one observability seam, one trait); non-breaking
(downstream impls continue to compile). Against: grows the
public-api diff by one method; KPI 7 originally carved out one
additive field on `IngestReceipt` as THE contracted public-api
change. Acknowledged: the trait-method addition is a second
additive change, recorded in this ADR. Accepted.

### Batch semantics A (rejected): REJECT-WHOLE

A batch containing any new-above-cap metric fails wholesale; nothing
is applied; the receipt reports zero. For: a single failure signal
is easier for the client to act on. Against: loses good data (the
existing-series points in the same batch the cap has no quarrel
with); violates A-D6 "honest three-way outcomes"; produces a
fabricated-empty signal (A-U4) by reporting the whole batch as
failed. Rejected.

### Batch semantics B (accepted): PARTIAL APPLY

Each metric is decided per-loop iteration; existing series receive
their points; new-below-cap series are inserted and receive their
points; new-above-cap series are refused and counted. The receipt
reports both `count` and `series_refused` honestly. Aligns with the
OTLP `ExportMetricsPartialSuccess` wire contract aperture already
implements. Accepted.

### WAL-replay semantics A (rejected): enforce the cap during replay

For: a uniform cap across live and replay paths. Against: a tenant
that legitimately accumulated N+5 series before restart (or before
the cap was tightened) would silently lose 5 series on every
restart; the platform would un-accept already-accepted data; this
is the A-U1 "silent data loss" attractor by another route. The WAL
IS the durable record of accepted ingests. Rejected.

### WAL-replay semantics B (rejected): refuse on replay and emit a startup event

For: the operator is alerted to a cap-tightening that would
truncate the index. Against: the platform still silently truncates
the index (the surplus series are not loaded), the startup event is
a parallel signal channel that does not actually preserve data, and
the operator has no remedy other than re-tightening to accommodate
the data already on disk. Rejected.

### WAL-replay semantics C (accepted): replay rebuilds existing series past the cap; cap applies only to live ingest post-replay

`apply_ingest` gains an internal boolean `enforce_cap`. WAL-replay
call site passes `false`; live-ingest call site passes `true`.
Replay reconstructs whatever the WAL holds; the cap fires from the
post-replay count forward. Accepted.

### Enforcement point A (rejected): outside `apply_ingest`, in `FileBackedMetricStore::ingest`

For: keeps `apply_ingest` simple. Against: would require the
in-memory adapter to mirror the cap check at a different seam
(`InMemoryMetricStore::ingest` does not call `apply_ingest`); the
two adapters' semantics would drift; ADR-0045 established
`apply_ingest` as the shared seam between live ingest and WAL
replay, and the cap-enforcement boolean parameter naturally
distinguishes the two. Rejected.

### Enforcement point B (rejected): on the `MetricStore` trait method itself

Push the cap into the trait, with a typed return such as
`Result<IngestReceipt, MetricStoreError::CapBreached>`. For: the
contract is visible at the trait. Against: requires a trait
signature change; would break `gate-2-public-api`; mixes a
boundary-error with a partial-apply policy decision (the cap is not
"the call failed", it is "some metrics in the call were refused").
Rejected.

### Enforcement point C (accepted): inside `apply_ingest`, with a shadow per-tenant counter under the same Mutex

The cap rides in the store implementation, not the trait. The
shadow counter is O(1) per check; the same Mutex serialises the
check, the increment, and the insert; the in-memory and file-backed
adapters mirror at the equivalent seam. Accepted.

### ADR shape A (rejected): amend ADR-0045

The cap closes an open question ADR-0045 explicitly named in its
Consequences. For: the cap belongs with the identity decision.
Against: ADRs in this repository are immutable; the amendment would
create a precedent that drifts (the convention is set by ADR-0001
and honoured by every preceding ADR including ADR-0049 and
ADR-0050). Rejected.

### ADR shape B (rejected): no ADR, document in design brief only

For: smaller documentation surface. Against: the cap is a contract
change on pulse's ingest behaviour visible at the receipt and at the
recorder seam; an operator reading ADR-0045 for the identity
decision should find the cap policy cross-referenced from there. The
ADR is the durable cross-reference. Rejected.

### ADR shape C (accepted): new ADR-0051, cites 0045 / 0049 / 0050, none modified

One ADR records the cap policy and the WAL-replay semantics; three
precedents are cited; immutability is preserved. The three together
(ADR-0049 + ADR-0050 + ADR-0051) form the Earned-Trust trilogy at
the ingest / read boundary: durability, refusal-on-overreach at
read, refusal-on-overreach at write. Accepted.

## Consequences

### Positive

- **The Earned-Trust trilogy is complete.** Where ADR-0049 made
  fsync-honesty real on the write side and ADR-0050 made
  refusal-on-overreach real on the read side, ADR-0051 makes
  refusal-on-overreach real on the write side at the cardinality
  boundary. The S04 row of the residuality incidence matrix
  transitions from `B OOM under enough labels` to `S per-tenant
  cardinality watermark refuses new series; existing series keep
  ingesting; per-tenant isolation preserved`.
- **No new envelope, no client-side change, no wire-side
  renegotiation.** The OTLP `ExportMetricsPartialSuccess` path
  aperture already implements is the natural translation of the
  pulse-side partial-apply (aperture's slice, NOT this one). Pulse
  itself exposes the refusal via the receipt and the recorder hook;
  both are pulse-internal types.
- **No `MetricStore` trait method-signature change.** `pulse::
  MetricStore`'s three methods stay byte-identical; the cap rides in
  the store implementation. `gate-2-public-api` on `pulse` confirms.
- **Operational legibility.** The operator sees a named class of
  refusal in two surfaces: the per-call receipt (visible to the
  aperture-storage-sink caller that translates to OTLP partial-
  success) and the longitudinal recorder hook (visible to query-api
  via the self-observe bridge). The refusal IS the signal; no
  dashboard wiring is required, and one is naturally addable in a
  successor slice once a downstream consumer exists.
- **Per-tenant isolation preserved (A-D4) at the resource axis.**
  One tenant's bomb does not affect another tenant's ingest. The
  per-tenant cap counts entries by tenant; the refused counter is
  per-tenant; the bridge emits per-tenant point attributes.
- **Recovery is lossless (A-D2 preserved).** WAL replay rebuilds
  existing series regardless of count. The cap is forward-looking;
  it never retroactively un-accepts already-accepted data.
- **Per-feature mutation 100% on the modified files.** Existing
  `gate-5-mutants-pulse` covers the changed lines in
  `crates/pulse/src/{lib.rs,store.rs,file_backed.rs,metrics.rs}` via
  `--in-diff`. Existing `gate-5-mutants-self-observe` covers the new
  bridge file. No new CI job is needed.

### Negative

- **The cap is compile-time at slice 01.** A misjudged value (too
  tight, too generous) requires a code change and a redeploy. The
  mitigation is a successor slice that lifts the constant to
  env-driven configurability via the existing
  `composition::resolve_tenant` posture
  (`KALEIDOSCOPE_PULSE_MAX_SERIES_PER_TENANT`); explicitly named in
  the slice OUT list.
- **The cap is uniform across tenants.** Per-tenant differentiation
  (e.g. tenant A gets 50_000 because it has many legitimate
  services; tenant B gets 1_000 because it has one) would be more
  honest about per-tenant cardinality expectations but requires
  per-tenant configurability the v0/v1 platform does not yet have. A
  successor slice tunes per tenant once env-driven configurability
  exists.
- **The cap does not evict existing series.** Once a `SeriesKey` is
  in the index, it stays. A tenant whose legitimately-accepted
  series count is already at or above a tightened cap (after a
  cap-value reduction between deploys) is in a "no new series"
  state until the operator triggers eviction (a future feature).
  This is intentional: ADR-0040 Decision 2 forbids invent-evict-
  inside-an-append-and-sort-pillar; the cap refuses, it does not
  displace.
- **Two additive items in the public-api diff.** The
  `IngestReceipt::series_refused` field and the
  `MetricsRecorder::record_series_refused` default method are both
  additive but visible in `cargo public-api`. Downstream impls of
  `MetricsRecorder` that wish to surface refusals override the
  default; impls that do not care inherit the no-op default and
  behave identically to today.
- **The shadow counter doubles the bookkeeping.** Per-tenant series
  count is tracked twice (once as a projection of the
  `(tenant, SeriesKey)` map, once as a `HashMap<TenantId, usize>`).
  The two are kept atomic under the same Mutex on every insert; a
  bug that desynchronises them would manifest as either a false
  refusal (count overstates) or a delayed refusal (count
  understates). Acceptance scenarios (US-01 Scenario 4 boundary
  test; US-04 Scenario 1 post-replay initialisation) and mutation
  testing pin the invariants.

### Trade-off summary

The refinement is intentionally narrow: it adds one compile-time
constant, one additive receipt field, one additive default-method on
the recorder trait, one additive `apply_ingest` parameter, one
additive shadow counter, and one new file under `self-observe` for
the bridge. The trade-off is "configurability and per-tenant
differentiation now" against "an honest, testable, deployable cap
policy now". v0/v1 takes the latter and records every deferral.

## Verification

- A workspace grep for `MAX_SERIES_PER_TENANT`, `series_refused`,
  `record_series_refused`, and `pulse.series.refused.count` returns
  the expected single declaration of each plus the cap-arm hits in
  `apply_ingest` and `InMemoryMetricStore::ingest` after slice 01
  lands. Today: zero hits.
- The slice-01 acceptance suite (`crates/pulse/tests/`, DISTILL-wave
  output) exercises:
  - Within-cap happy path (US-01 Scenario 1; tenant under the cap).
  - At-cap refusal (US-01 Scenario 2; the `(N+1)`th refuses; index
    width unchanged; refused counter increments).
  - Existing-series continues post-cap (US-01 Scenario 3).
  - Boundary at exactly N and N+1 (US-01 Scenario 4; kills the
    `>=` to `>` mutant).
  - Trait signature unchanged (US-01 Scenario 5; the `cargo
    public-api` diff is the proof).
  - Per-tenant isolation (US-02 Scenarios 1, 2, 3; kills the
    "global count" mutant that would use `series.len()` instead of
    the per-tenant projection).
  - Receipt-field observability (US-03 Scenarios 1, 3; receipt
    reports the refused count honestly).
  - Recorder-hook observability (US-03 Scenarios 2, 4; via a
    `CapturingRecorder` whose events vector accumulates the
    `RecordedEvent::SeriesRefused` variant; the bridge integration
    test in `crates/self-observe/tests/` mirrors with a second pulse
    instance as the bridge target).
  - WAL replay rebuilds existing series past the cap (US-04
    Scenarios 1, 2; populates the store to the cap, snapshots,
    reopens, asserts the index width).
  - Post-replay live ingest refuses (US-04 Scenario 2; kills the
    mutant that swaps `enforce_cap=true` for `enforce_cap=false` on
    the live-ingest path).
  - Batch partial-apply (US-05 Scenarios 1, 2, 3; the per-metric
    loop never aborts; kills the "break on first refuse" mutant).
- Gate 2 `cargo public-api` confirms `pulse::MetricStore`'s three
  method signatures are byte-identical to the prior tag. The two
  additive items (`IngestReceipt::series_refused`,
  `MetricsRecorder::record_series_refused` default method) and the
  new `pub const MAX_SERIES_PER_TENANT` appear in the public-API diff
  as new informational items.
- **Earned-Trust enforcement at the ingest cardinality boundary
  (three orthogonal layers reproduced from ADR-0049 / ADR-0050
  Verification)**: (a) subtype check at compile time (removing the
  `MAX_SERIES_PER_TENANT` constant fails the build at every test-
  site reference; removing `series_refused` from `IngestReceipt`
  fails the build at every construction site); (b) AST structural
  check via the `cargo public-api` diff (the additive items appear
  in the diff; removal would show as breaking); (c) behavioural
  gold-test via the acceptance suite (exercising the cap boundary,
  the partial-apply, the replay coherence, the per-tenant
  isolation, and the two observability surfaces). A single-layer
  bypass is caught by at least one of the other two.
- Mutation testing: `cargo mutants` scoped to the modified files
  via the existing `gate-5-mutants-pulse` workflow at the 100%
  kill-rate gate (ADR-0005 Gate 5; CLAUDE.md). Primary mutation
  targets:
  - The cap-arm `>=` boundary (a `>=` to `>` mutant must be killed
    by the boundary scenario at exactly N; a `>=` to `<` mutant must
    be killed by the over-by-one scenario at N+1).
  - The shadow-counter increment (a mutant that fails to increment
    on insert is killed by the at-cap refusal scenario; the next
    cap-arm fires earlier than expected).
  - The shadow-counter post-replay initialisation (a mutant that
    skips the initialisation is killed by the post-replay refusal
    scenario; the live ingest would not refuse).
  - The `enforce_cap=false` on the WAL-replay call site (a mutant
    that flips it to `true` is killed by the US-04 Scenario 3
    tightened-cap replay scenario; replay would refuse the surplus).
  - The per-metric loop's continue-vs-break (a mutant that aborts
    the loop on first refusal is killed by US-05 Scenario 1; the
    existing-series points in the same batch would not land).
  - The `MetricsRecorder::record_series_refused` invocation (a
    mutant that elides the call when refused > 0 is killed by US-03
    Scenario 2; the recorder events vector would be empty).

## External-integration handoff

None. The cap is in-process arithmetic on a `usize` shadow counter
keyed by `TenantId`. The bridge in `self-observe` emits into a
pulse store via `MetricStore::ingest`, which is in-process. No
third-party API is consumed by the cap path; no new external
dependency is introduced. The existing ingest contracts and the
ADR-0049 fsync probe at startup continue to run unchanged.

## Relationship to ADR-0045, ADR-0049, ADR-0050

- **ADR-0045** is the originating identity decision: a series is its
  metric name plus its full `resource_attributes` label set, scoped
  per tenant. Consequences Negative paragraph 2 explicitly named the
  open question "no secondary index is introduced; premature
  indexing is explicitly out of scope and would be a v2 concern if
  series cardinality per name ever grew large". ADR-0051 closes
  that open question with a per-tenant cardinality WATERMARK
  (refusal at a ceiling, not a secondary index). The identity
  decision is PRESERVED unchanged; the watermark refines the cost
  characteristic. Cited, NOT modified.
- **ADR-0049** is the immediate Earned-Trust sibling on the WRITE
  side at the durability boundary. The composition root must
  honour fsync; the wire-then-probe-then-use invariant; the three
  orthogonal enforcement layers (subtype + AST + behavioural).
  ADR-0051 applies the same discipline on the WRITE side at the
  cardinality boundary: the platform refuses out loud, in a named
  class, before the index grows past a per-tenant ceiling. The
  three-layer enforcement template is reproduced in this ADR's
  Verification section. Cited, NOT modified.
- **ADR-0050** is the immediate Earned-Trust sibling on the READ
  side at the request-shape boundary. The handler refuses out loud
  on a window or result that would self-DoS. ADR-0050 + ADR-0051
  together close the read and write surfaces of S04 / S13 in the
  residuality incidence matrix. The REFUSE-not-TRUNCATE posture
  and the compile-time-constant posture are reproduced. Cited, NOT
  modified.

## Forward-looking scope

Slice 01 ships the compile-time-constant cap. Successor slices
(separately roadmapped, NOT this ADR's scope) MAY:

- Lift the constant to env-driven configurability via the existing
  `composition::resolve_tenant` posture
  (`KALEIDOSCOPE_PULSE_MAX_SERIES_PER_TENANT`).
- Differentiate the cap per tenant once env-driven configurability
  exists.
- Introduce a structured event log on each refusal (beyond the
  recorder hook), once a downstream consumer (Prism panel, beacon
  rule) is ready to consume it.
- Introduce eviction policies (e.g. LRU-by-last-write,
  manually-triggered prune) for tenants that legitimately drift
  above a tightened cap; this would interact with the
  append-and-sort discipline of ADR-0040 Decision 2 and would
  warrant its own ADR.
- Extract the cap pattern into a `storage-pillar-common` crate once
  a second storage pillar adds a similar cardinality cap (lumen
  series, ray traces, etc.).

Each successor change is a separate slice with its own ADR; the
cross-references will name ADR-0051 as the originating per-tenant
cardinality watermark policy.
