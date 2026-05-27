# Slice 01: pulse per-tenant cardinality watermark walking skeleton

British English. No em dashes. No emoji.

## Origin

This slice is the walking skeleton of M-4 in
`docs/product/architecture/residuality-analysis.md` (commit 50e20b5)
and item 3 in `docs/residuality-followups-roadmap.md` (commit
820176d). M-1 (`earned-trust-fsync-probe-v0`) and M-2
(`honest-read-caps-v0`) have shipped; M-4 is the third and final
residuality follow-up.

The current `apply_ingest`
(`crates/pulse/src/file_backed.rs:349`) and its in-memory mirror
in `InMemoryMetricStore::ingest`
(`crates/pulse/src/store.rs:147`) insert every distinct
`(tenant, SeriesKey)` into the
`HashMap<(TenantId, SeriesKey), SeriesEntry>` with no per-tenant
ceiling. A client (misconfigured or hostile) emitting metrics with
growing-cardinality labels (a timestamp, a UUID, a per-request
ID) grows the index without bound and OOM-kills the process. The
residuality analysis flagged this as the S04 row of the incidence
matrix; the pulse cell reads "B OOM under enough labels"; the
A-U1 "Silent data loss" attractor is realised on OOM kill. This
slice closes that gap on `pulse` in one wave.

## Slice goal

ONE compile-time per-tenant cap applied at the shared
`apply_ingest` seam in `pulse`:

- **WATERMARK** (`MAX_SERIES_PER_TENANT`): a NEW
  `(tenant, SeriesKey)` insertion is REFUSED when the tenant's
  current distinct-key count is at or above the cap.
  EXISTING `SeriesKey`s continue to receive points normally
  regardless of the cap state. The cap is per-tenant, NOT global.
- **REFUSED COUNTER**: each refused ingest of a NEW
  above-cap `SeriesKey` increments a per-tenant counter,
  surfaced via FLAG 2's mechanism (LIKELY recommendation: BOTH a
  `refused_new_series: usize` field on `IngestReceipt` AND a
  `pulse.cardinality.refused.count` self-observe metric via the
  existing bridge pattern).

The cap NEVER panics, NEVER silently drops, NEVER evicts an
existing series. The `MetricStore` trait method signatures stay
byte-identical to the prior tag (FLAG 2 may add a field to
`IngestReceipt`, but no trait method is added, removed, or
re-signed). The WAL on-disk record shape is unchanged.

## Walking-skeleton entry point

The LIVE entry point is the EXISTING OTLP gateway path: an OTLP
client (gRPC or HTTP-protobuf) hits `kaleidoscope-gateway` ->
`aperture::transport` (one of the three listeners in
`crates/aperture/src/transport.rs`) -> `aperture::app::
ingest_metrics` -> `aperture-storage-sink::ingest_metrics`
(`crates/aperture-storage-sink/src/lib.rs:463`) ->
`pulse::FileBackedMetricStore::ingest`
(`crates/pulse/src/file_backed.rs:257`) ->
`apply_ingest`
(`crates/pulse/src/file_backed.rs:349`). No new HTTP path, no new
gRPC method, no new query parameter, no new wire envelope, no
new event.

Acceptance is via in-process integration tests, mirroring the
existing pulse test pattern: a real `FileBackedMetricStore`
opened on a `tempfile::TempDir`; the test calls
`store.ingest(&tenant, batch)` directly; the test asserts the
receipt, the index width per tenant, the refused counter, and
(per FLAG 2) the self-observe metric on a second pulse
instance. The OTLP-wire end-to-end traversal is exercised by the
existing aperture suite once the slice lands; this slice does
not add a new gateway-level test.

The observable change is at pulse's ingest seam: a batch that
would have grown the per-tenant index past the cap is partially
applied (existing series get their points; new-above-cap series
are refused and counted) instead of growing the map.

## Stories in this slice

All five stories land in slice 01 atomically (see
`discuss/story-map.md` "Priority Rationale" for why):

- **US-01** (P1): the `(N+1)`th NEW `SeriesKey` for tenant
  "acme-prod" is refused when the cap is N; the N existing
  series keep ingesting; the refused counter increments.
- **US-02** (P1, atomic): tenant A's cap breach does not affect
  tenant B's ingest. The cap is per-tenant, not global.
- **US-03** (P2, atomic): the refused-ingest count is observable
  via FLAG 2's mechanism (receipt field, self-observe metric, or
  both).
- **US-04** (P2, atomic): WAL replay rebuilds existing series
  past the cap; the cap applies only to NEW series at
  post-replay live ingest.
- **US-05** (P2, atomic): a mixed batch (existing-series points
  plus new-above-cap series) is partially applied; the whole
  batch is never rejected.

All five stories live in `discuss/user-stories.md` with full
LeanUX shape, three domain examples each, and 3-5 BDD scenarios
each.

## Learning hypothesis

We believe that ONE compile-time per-tenant cap
(`MAX_SERIES_PER_TENANT`) applied at the existing per-metric
loop inside the shared `apply_ingest` seam, refusing NEW
`SeriesKey`s above the ceiling while leaving EXISTING series
untouched, and incrementing a refused counter visible to the
operator (FLAG 2), is enough to close the S04 OOM surface for
`pulse` without:

- changing the `MetricStore` trait method signatures,
- changing the WAL on-disk record shape,
- evicting any existing series (preserving the append-and-sort
  discipline of ADR-0040 Decision 2),
- introducing a global cross-tenant cap (preserving the A-D4
  per-tenant isolation attractor),
- adding any structured event beyond the counter,
- renegotiating the OTLP partial-success contract the gateway
  already implements.

We will know we are right when:

- 100 percent of NEW-above-cap ingest scenarios in the acceptance
  suite refuse with the refused counter incrementing, the index
  width staying at exactly `MAX_SERIES_PER_TENANT`, and the
  process intact (no panic, no OOM).
- 100 percent of cross-tenant scenarios leave tenant B's ingest,
  index width, and refused counter unaffected while tenant A is
  at or beyond its cap.
- 100 percent of refused-ingest scenarios surface the count via
  FLAG 2's chosen mechanism (receipt field, self-observe metric,
  or both).
- 100 percent of WAL-replay scenarios rebuild existing series
  regardless of count and apply the cap only to post-replay
  NEW series.
- 100 percent of mixed-batch scenarios partial-apply per metric;
  the whole batch is never rejected.
- 100 percent of mutants in the changed files are killed by
  `gate-5-mutants-pulse`.
- 0 changes to the `MetricStore` trait method signatures;
  0 changes to the WAL on-disk record shape; at most 1 additive
  field on `IngestReceipt` (per FLAG 2 LIKELY).

We will know we are wrong if:

- The chosen cap value (FLAG 1) is too tight for legitimate
  tenants with naturally high series counts. Escalation path:
  re-pick the value in a successor slice, or move to env-driven
  config (slice 02, declared OUT).
- DESIGN concludes the right semantics on mixed-batch breach is
  REJECT-WHOLE rather than PARTIAL APPLY (FLAG 3). Re-frame the
  relevant scenarios; the DISCUSS-time LIKELY recommendation was
  PARTIAL APPLY.
- DESIGN concludes one of (A) receipt field, (B) self-observe
  metric is enough alone (FLAG 2). Re-frame the US-03 scenarios.

## Carpaccio taste-tests (five independent demonstrations)

Five things slice 01 must prove. The slice is "done" when all
five pass AND the per-crate mutation gate is 100 percent kill on
the changed files in `pulse` (ADR-0005 Gate 5; CLAUDE.md).

1. **NEW above the cap is refused; existing keeps ingesting**
   (US-01). Seed tenant "acme-prod" to `MAX_SERIES_PER_TENANT`
   distinct series; attempt to ingest the `(N+1)`th NEW series;
   the cap fires; the refused counter increments by 1; the index
   width stays at exactly `MAX_SERIES_PER_TENANT`; an ingest into
   an existing series still succeeds.

2. **Per-tenant isolation** (US-02). Fill tenant "acme-prod" to
   the cap; attempt the `(N+1)`th NEW series on "acme-prod"
   (refused; counter on acme-prod ticks); attempt a new series
   on tenant "globex-staging" (accepted; counter on
   globex-staging stays at 0).

3. **Counter visibility** (US-03). The refused count is
   observable via FLAG 2's mechanism. With FLAG 2 LIKELY (BOTH),
   the receipt's `refused_new_series` matches the count of
   refused metrics in the call, and the self-observe metric
   `pulse.cardinality.refused.count` accumulates one point per
   refusal on a second pulse instance.

4. **WAL replay respects the cap by being NOT enforced on
   replay** (US-04). Populate the store to
   `MAX_SERIES_PER_TENANT` series; call `snapshot()`; close and
   reopen via `FileBackedMetricStore::open`; the replay rebuilds
   the existing N series (the cap is not enforced on replay); a
   post-replay attempt to ingest a NEW series above the cap is
   refused.

5. **Batch partial-apply** (US-05). A single batch containing
   points for an EXISTING series AND points for a NEW above-cap
   series is PARTIALLY applied: the existing-series points land;
   the new-series points are refused; the receipt reports the
   correct ingested count and refused count; the whole batch is
   NEVER rejected.

## Flagged to DESIGN

Four items are FLAGGED to DESIGN, NOT decided by DISCUSS (see
`discuss/wave-decisions.md` for the rationale on each):

1. **EXACT `MAX_SERIES_PER_TENANT` VALUE**. Candidates: 1_000
   (tight), 10_000 (LIKELY recommendation), 100_000 (the
   residuality analysis's named starting default). DISCUSS
   recommends 10_000 as the starting default.

2. **COUNTER LOCATION**. Three honest options:
   (A) `IngestReceipt` field `refused_new_series: usize`;
   (B) `pulse.cardinality.refused.count` self-observe metric
       via a new `PulseToPulseRecorder` mirroring
       `LumenToPulseRecorder` and `CinderToPulseRecorder`;
   (C) BOTH (LIKELY recommendation). The two surfaces answer
       different questions and both are cheap.

3. **BATCH SEMANTICS**. PARTIAL APPLY (LIKELY recommendation)
   vs REJECT-WHOLE on a batch containing both legitimate
   existing-series points and above-cap new-series points.
   DISCUSS's LIKELY recommendation is PARTIAL APPLY:
   existing-series points ingest, new-above-cap are refused
   and counted, the receipt reports honestly. REJECT-WHOLE
   would be an A-U4 "fabricated empty" attractor.

4. **NEW ADR-0051 vs amendment of ADR-0045**. DISCUSS's LIKELY
   recommendation is a NEW ADR-0051 "pulse per-tenant
   cardinality watermark" that cites ADR-0045 and refines its
   open question ("no secondary index is introduced; premature
   indexing is explicitly out of scope and would be a v2 concern
   if series cardinality per name ever grew large"). ADRs are
   immutable; ADR-0045 is not edited.

## Out of scope (deferred and DECLARED)

- **Runtime-tuned cap**. The cap is a compile-time constant for
  slice 01. Env-driven configurability (e.g.
  `KALEIDOSCOPE_PULSE_MAX_SERIES_PER_TENANT`) is deferred to
  slice 02 / a successor feature.
- **Structured event log beyond the counter**. No new event,
  no new wire envelope, no Prism panel. The counter IS the
  signal (per FLAG 2 LIKELY: both receipt and self-observe).
- **Any change to the `MetricStore` trait method signatures**.
  Methods, parameters, and return types are byte-identical to
  the prior tag. `IngestReceipt` MAY grow an additive field
  (FLAG 2 LIKELY); the trait stays the same shape.
- **Global (cross-tenant) cap**. The cap is per-tenant; the
  whole point is one tenant's bomb does not contaminate
  another. A global hard-stop is a separate future feature.
- **Eviction of existing series**. Once a `SeriesKey` exists in
  the index, it stays. The cap refuses; it does not displace.
  ADR-0040 Decision 2's append-and-sort discipline forbids
  evicting inside a series.
- **Per-(tenant, metric-name) sub-caps**. The simple per-tenant
  cap is the M-4 mandate; richer policies are a future feature.
- **Per-tenant weighting (e.g. by resource-attribute count)**.
  A `SeriesKey` with many attributes counts as one entry; the
  per-entry cost is a separate concern from the index width.
- **Caps on points per series, on attribute size per key, or
  on `MetricBatch` size**. The gateway already has backpressure
  for batch size; M-4 sits below that. Per-series-point caps
  and per-key-size caps are out of scope.

## Effort

Estimated under 1 day total for slice 01. The residuality
analysis estimated "~80 LOC"; with both adapters in lockstep,
the per-tenant counting, the refused counter, and (per FLAG 2
LIKELY) the new `PulseToPulseRecorder` bridge, the change is
small. The breakdown:

- US-01 (per-tenant cap on `apply_ingest` plus the in-memory
  mirror): roughly 0.3 days.
- US-02 (per-tenant isolation): roughly 0.15 days. The semantics
  fall out of US-01 (the cap is by definition per-tenant); the
  test asserts the cross-tenant isolation explicitly.
- US-03 (refused-count observability): roughly 0.25 days.
  Receipt field is small; the self-observe bridge mirrors the
  existing `LumenToPulseRecorder` shape.
- US-04 (WAL-replay coherence): roughly 0.15 days. The
  cap-vs-no-cap distinction is a small structural change to
  `apply_ingest` (boolean parameter, two variants, or a
  separate function; DESIGN owns the shape).
- US-05 (batch partial-apply): roughly 0.15 days. The
  per-metric loop already exists; the assertion is one extra
  acceptance scenario.

All five stories ship atomically because the per-crate mutation
gate evaluates the whole `pulse` crate after the change.
