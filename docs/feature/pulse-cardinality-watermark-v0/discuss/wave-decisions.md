# Wave Decisions: pulse-cardinality-watermark-v0 (DISCUSS)

British English. No em dashes. No emoji.

## Origin and frame

This is M-4 in the residuality analysis
(`docs/product/architecture/residuality-analysis.md`, commit 50e20b5)
and item 3 in the residuality follow-up roadmap
(`docs/residuality-followups-roadmap.md`, commit 820176d). It is the
THIRD and FINAL feature of the residuality follow-up sequence:
M-1 (`earned-trust-fsync-probe-v0`) and M-2 (`honest-read-caps-v0`)
have shipped; M-4 lands one feature wave after M-2.

The pulse series index has no per-tenant ceiling. ADR-0045 fixed
series identity (a series is its metric name plus its full
`resource_attributes`), but explicitly accepted that this turned
series collapse into a RAM bomb: every new label set is a new entry
in `HashMap<(TenantId, SeriesKey), SeriesEntry>` inside
`crates/pulse/src/file_backed.rs:84` (and its in-memory mirror at
`crates/pulse/src/store.rs:111`). A client (misconfigured or hostile)
emitting metrics with growing-cardinality labels (a timestamp as a
label, a UUID, a per-request ID) drives the process to OOM, because
`apply_ingest` inserts every distinct `(tenant, SeriesKey)` into the
map without consulting any ceiling. The residuality analysis flagged
this as S04, with the pulse cell marked B "OOM under enough labels"
in the incidence matrix; A-U1 "silent data loss via OOM kill" is the
attractor M-4 forbids.

A per-tenant soft watermark in `apply_ingest` converts the silent
OOM path into an observable, bounded refusal: NEW series above the
ceiling are refused; EXISTING series keep receiving points normally;
a counter records each refused ingest. The platform stays alive, one
tenant's bomb does not contaminate another tenant, and the operator
sees a named signal instead of a process death.

### Reads checklist

- [x] `docs/product/architecture/residuality-analysis.md` (M-4
  framing in "Resilience modifications (prioritised)"; the S04 row of
  the incidence matrix where the pulse cell reads "**B OOM** under
  enough labels"; the A-U1 undesired attractor "Silent data loss";
  the closing summary's "capacity residues (no caps on window or
  result size, no cardinality watermark, no fsync probing)" naming
  the gap this feature closes).
- [x] `docs/residuality-followups-roadmap.md` (this feature's
  position as item 3 of 3; ground rule "full nWave per feature";
  rationale "A soft watermark in `apply_ingest` refuses further new
  label sets above a per-tenant threshold and surfaces a recordable
  event, while existing series keep ingesting normally").
- [x] `docs/product/architecture/adr-0045-pulse-series-identity-is-the-full-label-set.md`
  (Consequences "Negative", "Query fans out across series sharing a
  name"; the explicit acceptance that "no secondary index is
  introduced; premature indexing is explicitly out of scope and would
  be a v2 concern if series cardinality per name ever grew large".
  The watermark addresses the CAPACITY consequence of that decision
  without contradicting the IDENTITY decision).
- [x] `crates/pulse/src/file_backed.rs` (the `apply_ingest` shared
  path at line 349; the `HashMap<(TenantId, SeriesKey), SeriesEntry>`
  at line 84; the WAL-replay branch that routes through
  `apply_ingest` at line 158; the live `ingest` arm that calls
  `apply_ingest` at line 273 after `append_wal`).
- [x] `crates/pulse/src/store.rs` (the `IngestReceipt` shape at
  line 30 with one field `count: usize`; the `MetricStore` trait at
  line 66 whose `ingest` returns `Result<IngestReceipt,
  MetricStoreError>`; the in-memory adapter's `ingest` at line 137
  which keys by `(tenant, SeriesKey::of(&metric))` line by line; no
  cap of any kind today).
- [x] `crates/pulse/src/metric.rs` (the `SeriesKey` type at line 51
  with `name` and `resource_attributes`; the comment confirming the
  derived `Hash`/`Eq`/`Ord` is stable across processes; the
  `MetricBatch` shape at line 113 whose `metrics: Vec<Metric>` is
  what `apply_ingest` iterates).
- [x] `crates/self-observe/src/lumen_bridge.rs` (the
  `LumenToPulseRecorder` pattern at line 42 emitting one-point
  `MetricBatch`es into pulse on every `record_*` event; the
  metric-naming convention `lumen.<event>.count`; the "best-effort
  observability" comment swallowing pulse ingest errors). This is
  the bridge a self-observe metric for refused ingests would
  follow if FLAG 2 picks the bridge route.
- [x] `crates/self-observe/src/cinder_bridge.rs` (the same shape
  for cinder events; same naming convention `cinder.<event>.count`;
  ADR-0038 mention; the `tier_lowercase` enforcement pattern). The
  precedent for a `pulse.cardinality.refused.count` self-observe
  metric.
- [x] `crates/aperture-storage-sink/src/lib.rs` (the
  `ingest_metrics` helper at line 463 that calls
  `store.ingest(tenant, MetricBatch::with_metrics(metrics))`; this
  is the LIVE entry point from the OTLP gateway via aperture into
  pulse, the path a real cardinality bomb travels).
- [x] `docs/feature/honest-read-caps-v0/discuss/` (M-2 precedent:
  structure, voice, the FLAG-not-DECIDE pattern, the compile-time
  constant approach for slice 01, the "no trait change" pin).
- [x] `docs/feature/earned-trust-fsync-probe-v0/discuss/` (M-1
  precedent: the walking-skeleton entry-point shape, the OUT-of-scope
  declaration pattern, the redaction symmetry approach).

The residuality analysis's framing of the gap is consistent with what
`apply_ingest` does today. The function (in both adapters) inserts an
entry for every new `(tenant, SeriesKey)` and extends the points
vector for matching ones; no ceiling, no counter. Live ingest goes
through `apply_ingest`. WAL replay goes through the same
`apply_ingest`. The two paths cannot drift, which is exactly why
M-4's cap lives there. No contradiction was discovered during
DISCUSS.

## DIVERGE status

No DIVERGE artefacts at
`docs/feature/pulse-cardinality-watermark-v0/diverge/`. The job
statement is taken from the residuality analysis and the roadmap:
"refuse new series above a per-tenant ceiling, before the process
OOMs; existing series keep ingesting normally; one tenant's bomb does
not contaminate another tenant". JTBD was explicitly NOT requested
by the invoking prompt; the journey is grounded in the residuality
analysis, ADR-0045 (the precedent that opened this gap), and the
existing self-observe bridge vocabulary.

Risk noted: without DIVERGE there is no separate ODI scoring; the
opportunity priority is taken from the roadmap rather than derived.
This is proportionate: this is item 3 in a numbered three-item
roadmap, not a competition between candidate features.

## Scope: SLICE 01 THIN (walking skeleton)

ONE feature wave puts ONE per-tenant soft watermark on the shared
`apply_ingest` path in `crates/pulse/src/file_backed.rs` AND its
in-memory mirror in `crates/pulse/src/store.rs`, with a counter that
records each refused ingest. Concretely:

- **WATERMARK**. A compile-time constant `MAX_SERIES_PER_TENANT`
  bounds the number of distinct `SeriesKey`s a single tenant may
  hold concurrently in the index. Above the ceiling, NEW
  `SeriesKey`s are refused; EXISTING `SeriesKey`s continue to
  receive points normally. The check is per-tenant
  (`count_distinct_series_keys_for(tenant) >= MAX_SERIES_PER_TENANT`),
  NOT global. The exact value is FLAGGED to DESIGN (FLAG 1).
- **REFUSED COUNTER**. Each refused ingest of a NEW
  `SeriesKey` increments a counter visible to the operator. The
  EXACT location (a `refused_new_series: usize` field on
  `IngestReceipt`, a self-observe metric via the existing pulse-into-
  pulse bridge, or both) is FLAGGED to DESIGN (FLAG 2). DISCUSS's
  LIKELY recommendation is BOTH.
- **PARTIAL BATCH SEMANTICS**. An ingest batch that contains BOTH
  existing-series points AND new-series points above the cap is
  PARTIALLY applied: existing series receive their points; new
  series above the cap are refused and counted; the receipt
  reports points-ingested honestly. The whole batch is NEVER
  rejected. Confirmed with DESIGN (FLAG 3).
- **NEVER PANIC, NEVER SILENTLY DROP**. The refusal is observable
  via the counter; the per-tenant index never grows past the cap;
  the process stays alive. No `unwrap_or_default`, no
  silently-discarded point, no panic on cap breach.

The shape of the change: `apply_ingest` (called by live ingest and
WAL replay, the shared path) consults the per-tenant series count
before inserting a new `(tenant, SeriesKey)` entry. If the count is
at or above the cap, the new key is REFUSED and the refused counter
increments. If the count is below the cap, the entry is inserted as
today. EXISTING entries are unaffected: a matching `(tenant,
SeriesKey)` extends the points vector as today, regardless of the
per-tenant count.

The `MetricStore` trait signature stays UNCHANGED. The receipt may
gain a field (FLAG 2 LIKELY recommendation), but the trait
methods, return types, and parameters are byte-identical to the
prior tag. The cap is a property of the store implementation, not
of the trait contract; the trait still says "ingest a batch and
return a receipt". The receipt's shape change (adding
`refused_new_series: usize`) is additive on a `#[non_exhaustive]`-
candidate struct; DESIGN owns whether to add `#[non_exhaustive]`
now.

### Walking-skeleton entry point

The LIVE entry point: an OTLP client (gRPC or HTTP-protobuf) hits
`kaleidoscope-gateway` -> `aperture::transport` (one of the three
listeners in `crates/aperture/src/transport.rs`) -> `aperture::app::
ingest_metrics` -> `aperture-storage-sink::ingest_metrics`
(`crates/aperture-storage-sink/src/lib.rs:463`) ->
`pulse::FileBackedMetricStore::ingest`
(`crates/pulse/src/file_backed.rs:257`) ->
`apply_ingest`
(`crates/pulse/src/file_backed.rs:349`). The watermark check fires
inside `apply_ingest`; the OTLP client receives a successful response
with a partial-success count when some of its metrics are refused.

Acceptance is via in-process integration tests, mirroring the other
pulse slices: a real `FileBackedMetricStore` is opened on a `tempfile::
TempDir`; the test calls `store.ingest(&tenant, batch)` directly,
asserts the receipt, asserts the series count, and asserts the
refused counter. The OTLP-wire end-to-end traversal is exercised by
the existing aperture suite once the slice lands; this slice does
not add a new gateway-level test (the contract is the
`MetricStore::ingest` shape, and the gateway already exercises that
shape).

### OUT of scope (deferred and DECLARED)

- **Runtime-tuned cap**. `MAX_SERIES_PER_TENANT` is a compile-time
  constant for slice 01 (per-crate `const MAX_SERIES_PER_TENANT:
  usize = ...;`). Env-driven configurability (e.g.
  `KALEIDOSCOPE_PULSE_MAX_SERIES_PER_TENANT`) that would mirror the
  existing env-driven `TENANT` / `ADDR` posture is DEFERRED to a
  later slice or successor feature. Slice 01 ships the cap; a
  future slice makes it tunable.
- **Telemetry / events beyond the counter via the existing
  self-observe bridge**. No structured event log, no dashboard
  widget, no Prism panel. The counter IS the signal; FLAG 2 picks
  where it surfaces (receipt field, self-observe metric, or both).
  A future slice MAY add a structured event; M-4 does not.
- **Any change to the `MetricStore` trait signature**. Methods,
  parameters, and return-type names are byte-identical to the prior
  tag (`gate-2-public-api` on `pulse`). `IngestReceipt` MAY grow a
  field (additive, behind FLAG 2); the trait stays the same shape.
- **Global (cross-tenant) cap**. The cap is per-tenant.
  Cross-tenant isolation is the WHOLE POINT (one tenant's bomb does
  not contaminate another); a global cap would re-couple them. If a
  future need surfaces for a global ceiling (e.g. a hard-stop OOM
  guard for the WHOLE process), it lives in a successor feature.
- **Eviction of existing series**. Once a `SeriesKey` exists in the
  index, it stays. The cap is ANTI-FRAGILE: new series above the
  ceiling are refused, but existing series are never evicted to make
  room for new ones. An eviction policy would invent a quiet
  "keyed-latest-wins" inside an append-and-sort pillar; ADR-0040
  Decision 2 forbids this kind of mixing. The cap refuses; it does
  not displace.
- **Special-casing the self-observe metric tenant**. The cap applies
  uniformly; the self-observe bridge ingests metrics under whichever
  tenant it was configured for and is subject to the same cap. If
  the self-observe tenant is also a real tenant, the operator picks
  a cap value above the natural self-observe cardinality; this is
  a DESIGN consideration tied to FLAG 1.
- **Caps on dimensions other than per-tenant SeriesKey count**. No
  cap on points per series (a hot series can still receive many
  points; the cap is on the series index width, not depth). No cap
  on resource-attribute size or count per key (a key with 100
  attributes still costs one entry; the entry's size is a separate
  concern). No cap on the size of any individual `MetricBatch` (the
  gateway already has backpressure for batch size; M-4 sits below
  that).

## Flagged to DESIGN (DISCUSS does NOT decide these)

1. **EXACT `MAX_SERIES_PER_TENANT` VALUE**. Candidates surfaced:
   - **1_000** - tight; fits a small dev tenant with a handful of
     services. Likely too tight for a real production tenant with
     dozens of services and modest per-service cardinality.
   - **10_000** (LIKELY recommendation): well above typical
     per-tenant series counts (a tenant with 50 services and 100
     distinct metric series per service = 5_000), low enough to
     stop a label-cardinality bomb well before OOM at typical
     `SeriesEntry` size in v0 (each entry is the `Metric` metadata
     plus its sorted point vector; the metadata alone is a small
     `String`+`BTreeMap` pair, so 10_000 entries cost a few MB of
     metadata before any points). The residuality analysis named
     "100k" as a starting default, but acknowledged this was a
     starting number, not a measured one; DISCUSS recommends
     starting tighter and loosening per measurement.
   - **100_000** - the residuality analysis's named starting
     default. Permissive; closes the OOM surface at the absolute
     upper bound but lets a slowly-bleeding cardinality leak
     accumulate longer. Likely too loose for slice 01's intent of
     fast, observable refusal.
   The right value depends on per-tenant cardinality expectations
   the v0/v1 platform does not yet have measurements for. DISCUSS
   recommends DESIGN pick **10_000** as a starting default and
   document the rationale; the env-driven configurability deferred
   to slice 02 lets operators tighten or loosen later.

2. **WHERE THE REFUSED COUNTER SURFACES**. Three honest options:
   - **(A) `IngestReceipt` field**: add
     `refused_new_series: usize` to `IngestReceipt`
     (`crates/pulse/src/store.rs:30`). The caller sees the
     refusal count alongside the points-ingested count, in the same
     synchronous response. Lowest-latency signal. Costs one
     additive field on the receipt (DESIGN owns whether to add
     `#[non_exhaustive]` for forward-compatibility).
   - **(B) Self-observe metric via the existing bridge**: emit
     `pulse.cardinality.refused.count` via the LumenToPulseRecorder
     pattern (`crates/self-observe/src/lumen_bridge.rs:42`) or a
     dedicated PulseToPulseRecorder. The metric is then queryable
     via `query-api` like any other pulse metric. Composes with the
     existing self-observe story.
   - **(C) BOTH** (LIKELY recommendation): the receipt field is the
     synchronous, per-call signal; the self-observe metric is the
     longitudinal, queryable signal. They cost different things and
     answer different questions; both are cheap and both are
     observable.
   DISCUSS recommends DESIGN pick BOTH; option (A) alone leaves the
   operator without a longitudinal view, option (B) alone leaves the
   per-call caller blind. Stated as a flag, not a decision; DESIGN
   owns the pick.

3. **EXACT SEMANTICS OF THE REFUSED ARM IN A BATCH**. The two
   honest options:
   - **PARTIAL APPLY** (LIKELY recommendation): an ingest batch that
     contains both existing-series points AND new-series points
     above the cap is PARTIALLY applied: existing series receive
     their points; new series above the cap are refused and
     counted; the receipt reports points-ingested honestly and (per
     FLAG 2) the refused count. The OTLP partial-success path the
     gateway already uses
     (`opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsPartialSuccess`)
     is the natural home for the wire-side report.
   - **REJECT WHOLE BATCH**: an ingest batch containing any
     above-cap new series is rejected wholesale; nothing is
     applied, no points are stored, the receipt reports zero. The
     operator sees a single signal but loses good data from the
     same batch.
   DISCUSS recommends DESIGN pick PARTIAL APPLY. The Earned-Trust
   discipline (ADR-0042 / 0047 / 0048 A-D6 "honest three-way
   outcomes") and the residuality analysis's A-U4 "fabricated empty"
   forbid silent loss; PARTIAL APPLY refuses what cannot be
   accepted, accepts what can, and counts what was refused. Stated
   as a flag, not a decision; DESIGN owns the pick and records the
   rationale in the ADR.

4. **NEW ADR-0051 vs REFINEMENT of ADR-0045**. ADR-0045 explicitly
   opened this gap ("no secondary index is introduced; premature
   indexing is explicitly out of scope and would be a v2 concern if
   series cardinality per name ever grew large") and accepted the
   cost characteristic. M-4 refines that open question by adding a
   per-tenant cardinality watermark. Three candidate paths:
   - **NEW ADR-0051** "pulse per-tenant cardinality watermark" that
     cites ADR-0045 and refines its open question (LIKELY
     recommendation): ADR-immutable, so ADR-0045 is NOT edited; the
     new ADR records the cap policy as a follow-up that closes the
     OOM consequence of the ADR-0045 identity decision.
   - **AMENDMENT of ADR-0045**: would contradict the ADR-immutable
     project convention (project memory and ADR-0001).
   - **NO NEW ADR, document in DESIGN brief only**: too thin; the
     cap policy is a contract change on pulse's ingest behaviour
     and operators reading ADR-0045 would not find the cap.
   DISCUSS recommends DESIGN pick NEW ADR-0051. Stated as a flag,
   not a decision; DESIGN confirms the ADR number (next free after
   ADR-0050 if ADR-0050 lands first; DESIGN counts) and writes the
   ADR.

These four items are FLAGGED, NOT DECIDED, by DISCUSS.

## Learning hypothesis

We believe that ONE compile-time per-tenant cap
(`MAX_SERIES_PER_TENANT`) applied inside the shared `apply_ingest`
path of `pulse::FileBackedMetricStore` AND
`pulse::InMemoryMetricStore`, refusing NEW `SeriesKey`s above the
ceiling while leaving EXISTING series untouched, and incrementing a
refused counter visible to the operator (FLAG 2), will close the
S04 OOM surface for pulse without:

- changing the `MetricStore` trait signature,
- changing the WAL format,
- evicting any existing series (no keyed-latest-wins inside an
  append-and-sort pillar; ADR-0040 Decision 2 preserved),
- introducing a global (cross-tenant) cap (one tenant's bomb does
  not contaminate another),
- adding any structured event beyond the counter.

We will know we are right when:

- A request that crosses the per-tenant ceiling refuses NEW
  `SeriesKey`s above the cap and DOES NOT panic, DOES NOT OOM, DOES
  NOT silently drop. Asserted by an integration test that seeds
  `MAX_SERIES_PER_TENANT` distinct series for tenant "acme-prod"
  then attempts to ingest one more new series; the cap fires; the
  refused counter increments; the index width stays at
  `MAX_SERIES_PER_TENANT`.
- An ingest batch with both existing-series points and new-series
  points above the cap is PARTIALLY applied (FLAG 3 LIKELY): existing
  series receive their points; new series above the cap are
  refused; the receipt is honest.
- Tenant A's bomb does not affect tenant B's ingest. Asserted by an
  integration test that fills tenant A to the cap, attempts to
  ingest one more new series on tenant A (refused), then ingests a
  new series on tenant B (succeeds).
- WAL replay respects the same path: existing series rebuilt from
  the WAL all pass (since they predate the cap check; they are
  matching keys, not new keys). The cap applies to NEW series at
  post-replay live ingest. Asserted by an integration test that
  populates a store to the cap, calls `snapshot()` and reopens,
  then attempts new ingests beyond the cap (refused).
- The `pulse::MetricStore` trait signature is byte-identical to the
  prior tag (FLAG 2 may add a field to `IngestReceipt`, but the
  trait methods themselves do not change).
- The per-crate mutation gate (`gate-5-mutants-pulse`) stays at 100
  percent kill on the changed files.

We will know we are wrong if:

- The chosen cap value (FLAG 1) is too tight for legitimate tenants
  with naturally high series counts (false positives Prism users or
  operators notice). Escalation path: re-pick the value in a
  successor slice, or move the cap to env-driven config (slice 02,
  declared OUT).
- DESIGN concludes the right semantics on partial-batch breach is
  REJECT-WHOLE rather than PARTIAL APPLY (FLAG 3). Re-frame the
  relevant scenarios; the DISCUSS-time LIKELY recommendation was
  PARTIAL APPLY.
- DESIGN concludes the receipt field is enough on its own and the
  self-observe metric is over-engineering, or vice versa (FLAG 2).
  Re-frame accordingly; the DISCUSS-time LIKELY recommendation was
  BOTH.

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Chosen cap value is too tight, blocking legitimate tenants. | Medium | High (legitimate ingests refused, real metrics lost from new series). | Slice 01 cap is a compile-time constant; tightening or loosening is a one-line PR. The env-driven configurability is explicitly deferred to a later slice (declared OUT). |
| Counter location wrong (a future operator wants to query the refusal rate but can only see it on the synchronous receipt, or vice versa). | Medium | Medium (operator workaround: turn on the other surface). | FLAG 2 LIKELY recommendation is BOTH; DESIGN owns the final pick. The two surfaces are additive: starting with one and adding the other later is a small follow-up. |
| Partial-apply semantics interact badly with OTLP partial-success expectations. | Low | Medium (a misreported partial-success count breaks a client's "did my batch succeed?" assertion). | The OTLP partial-success path already exists in `aperture` (`opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsPartialSuccess`); FLAG 3 LIKELY recommendation aligns with that existing path. DESIGN confirms the wire-side report mirrors the receipt. |
| `apply_ingest` is called from WAL replay; the cap firing during replay would refuse rebuilding legitimate pre-existing series. | Low | High (data loss on recovery). | The cap fires only for NEW `SeriesKey`s. WAL records contain ingested data that ALREADY landed before the cap existed; replaying them re-creates entries that are matching keys for any series the WAL had captured, NOT new ones from the cap's point of view. CAVEAT: on a fresh-open of a store whose snapshot+WAL together yield more than `MAX_SERIES_PER_TENANT` series (because the cap value was tightened post-deploy), replay would refuse the surplus. DESIGN must decide the replay semantics: either let replay rebuild past the cap (cap applies only at LIVE ingest post-replay), or refuse and emit a startup event analogous to `health.startup.refused`. DISCUSS recommends the former (replay rebuilds; cap applies at post-replay live ingest only). |
| Trait signature drifts. | Low | High (Gate 2 fail; downstream consumers break). | `gate-2-public-api` on `pulse` runs on every push (ADR-0005). The cap rides in the store implementation, not the trait. FLAG 2's receipt-field addition is additive on `IngestReceipt`; DESIGN owns whether to add `#[non_exhaustive]` for forward-compatibility. |
| Test seeding `MAX_SERIES_PER_TENANT` distinct series in an integration test is slow. | Medium | Low (test runtime). | The cap value (FLAG 1) directly affects test setup cost: 1_000 is cheap to seed, 100_000 is slow. The LIKELY recommendation of 10_000 is a deliberate compromise between operationally-useful headroom and test-suite manageability. If DESIGN picks 100_000 the test seeding may need a helper that bulk-inserts skipping the WAL append (test-only constructor on `FileBackedMetricStore`). |
| ADR-0051 (or refinement) drifts from ADR-0045's framing. | Low | Medium (ADR confusion). | ADR-0045 is immutable. The new ADR cites it as Related, names the open question it closes, and explicitly does not re-litigate the identity decision. DESIGN writes the ADR; reviewer flags any drift. |

## Carpaccio taste-tests (five independent demonstrations)

Five things slice 01 must prove on `pulse`:

1. **NEW series above the cap is refused; existing series keep
   ingesting** (US-01 walking skeleton). Seed tenant "acme-prod"
   to `MAX_SERIES_PER_TENANT` distinct series; attempt to ingest
   the `(N+1)`th NEW series; the cap fires; the refused counter
   increments by 1; the existing N series can still receive points.

2. **Per-tenant isolation** (US-02). Fill tenant "acme-prod" to the
   cap; attempt the `(N+1)`th NEW series on "acme-prod" (refused);
   attempt a new series on tenant "globex-staging" (accepted, the
   counter on globex-staging stays at 0).

3. **Counter visibility** (US-03). The refused count is observable
   via FLAG 2's mechanism (receipt field, self-observe metric, or
   both). The DISCUSS-time scenario tests both surfaces; DESIGN
   collapses to one if FLAG 2 picks one.

4. **WAL replay respects the cap** (US-04). Populate the store to
   `MAX_SERIES_PER_TENANT` series; call `snapshot()`; close and
   reopen via `FileBackedMetricStore::open`; the replay rebuilds
   the existing N series (they predate the cap check; they are
   matching keys, not new keys); a post-replay attempt to ingest
   a new series above the cap is refused.

5. **Batch partial-apply** (US-05). A single batch containing
   points for an EXISTING series AND points for a NEW above-cap
   series is PARTIALLY applied: the existing-series points land;
   the new-series points are refused; the receipt reports the
   correct ingested count and the refused count; the whole batch
   is NEVER rejected.

Each taste-test is one acceptance scenario in
`docs/feature/pulse-cardinality-watermark-v0/discuss/user-stories.md`;
the slice is "done" when all five pass AND the per-crate mutation
gate is 100 percent kill on the changed files in `pulse` (ADR-0005
Gate 5; CLAUDE.md).

## Honest contradiction check

The residuality analysis's framing of this gap was checked against
the pulse code. The framing is consistent:

- `crates/pulse/src/file_backed.rs:84` declares
  `series: HashMap<(TenantId, SeriesKey), SeriesEntry>` with no
  ceiling. `apply_ingest` at line 349 calls `series.entry(key).
  or_insert_with(...)` for every metric in the batch; no per-tenant
  count, no cap, no counter.
- The same applies in the in-memory adapter at
  `crates/pulse/src/store.rs:111` (same map shape) and
  `crates/pulse/src/store.rs:147` (same `entry().or_insert_with()`
  pattern, no cap).
- WAL replay at `crates/pulse/src/file_backed.rs:158` calls
  `apply_ingest(&mut series, &tenant, metrics)` directly, so the
  cap (or its absence) is uniform across live and replay paths.
- ADR-0045 explicitly accepted the cost characteristic of treating
  the full label set as identity ("no secondary index is
  introduced; premature indexing is explicitly out of scope and
  would be a v2 concern if series cardinality per name ever grew
  large"). M-4 refines that open question; it does NOT contradict
  the identity decision, it complements it.
- The cap applies per-tenant (`(TenantId, SeriesKey)` is the map
  key, so per-tenant counts are a natural projection); a global cap
  would require an extra counter and would re-couple tenants. The
  per-tenant choice respects A-D4 "Fail-closed tenancy" by keeping
  tenants isolated.

No contradiction surfaced. The cap is a refinement of an open
question ADR-0045 explicitly named.

## Changelog

- 2026-05-27: feature `pulse-cardinality-watermark-v0` DISCUSS wave
  artefacts written by Luna. Four items flagged to DESIGN; one
  walking-skeleton slice declared
  (`slice-01-pulse-cardinality-watermark-walking-skeleton.md`)
  covering five user stories on a single pulse-side seam
  (`apply_ingest`) entered via the OTLP gateway.
