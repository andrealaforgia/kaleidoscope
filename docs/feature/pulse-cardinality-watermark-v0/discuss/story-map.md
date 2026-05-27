# Story Map: pulse-cardinality-watermark-v0

British English. No em dashes. No emoji.

## User

Maya Kowalski - platform operator for tenant "acme-prod", running
the `kaleidoscope-gateway` binary that fronts pulse - and her
incident-response colleague Idris Mbeki, on-call for the same
tenant. The bomb-throwing client is "Hands-off Hannah", a tenant
operator who hand-edits an OTLP exporter to attach a per-request UUID
as a metric label and accidentally turns every HTTP request into a
new series. The cross-tenant safety case is "Globex Steady" - the
operator of a second tenant ("globex-staging") who must not be
affected by acme-prod's bomb.

## Goal

When a tenant's distinct `SeriesKey` count crosses the configured
ceiling, the platform refuses NEW `SeriesKey`s above the cap and
counts each refused ingest, instead of growing the per-tenant index
without bound and OOM-killing the process. EXISTING series keep
receiving points normally; one tenant's bomb does not contaminate
another tenant; the refusal is observable (FLAG 2: receipt field,
self-observe metric, or both). Honest cap closes the S04 OOM surface
the residuality analysis flagged on pulse, refining the open question
ADR-0045 explicitly named in its Consequences.

## Backbone

The user activities run left-to-right across one OTLP ingest:

| Send batch | Iterate batch metrics | Apply per-metric | Surface receipt |
|---|---|---|---|
| An OTLP client (or Hands-off Hannah's misconfigured exporter, or an attacker) sends an OTLP ExportMetricsServiceRequest via gRPC or HTTP-protobuf to the gateway under tenant "acme-prod". | The gateway resolves tenancy and routes through aperture-storage-sink to `pulse::FileBackedMetricStore::ingest`. The store iterates the batch via `apply_ingest`. | Per metric: compute the `SeriesKey`. If matching existing key, extend points (today and always). If new key, CHECK THE PER-TENANT CARDINALITY WATERMARK. Below cap: insert as today. At-or-above cap: REFUSE and increment the refused counter. | The receipt reports `count` (points ingested) and, per FLAG 2, `refused_new_series` (refused count); a self-observe metric records the same refusal longitudinally. The whole batch is NEVER rejected; the OTLP partial-success path reports the partial outcome. |

Each backbone column is one user activity. The walking skeleton is
the minimum slice across all four columns that delivers an honest
refusal of NEW `SeriesKey`s above the per-tenant cap without
touching the trait, the WAL format, the existing series, or the
other tenants.

## Walking skeleton (slice 01)

The thinnest end-to-end slice that connects all four backbone
activities on `pulse`:

- **Send batch**: an in-process integration-test fixture calls
  `store.ingest(&tenant, batch)` directly on a real
  `FileBackedMetricStore` opened on a `TempDir`, mirroring the
  existing pulse test pattern. The OTLP-wire end-to-end traversal
  is exercised by the aperture suite once the slice lands; this
  slice does not add a new gateway-level test.
- **Iterate batch metrics**: unchanged. `FileBackedMetricStore::
  ingest` calls `apply_ingest(&mut series, tenant, batch.metrics)`
  as today (`crates/pulse/src/file_backed.rs:273`).
- **Apply per-metric**: `apply_ingest` gains a per-tenant
  cardinality check. Before
  `series.entry(key).or_insert_with(...)` for a new key, count the
  current `(tenant, _)` entries; if the count is at or above
  `MAX_SERIES_PER_TENANT`, REFUSE and increment a refused counter.
  Matching keys take the existing extend-points path unchanged. The
  same edit applies in `InMemoryMetricStore::ingest` (the in-memory
  adapter does not call `apply_ingest` directly today; the cap is
  added at the equivalent seam to keep the two adapters' semantics
  in lockstep).
- **Surface receipt**: the receipt grows a `refused_new_series:
  usize` field (FLAG 2 LIKELY recommendation; DESIGN owns whether
  to add `#[non_exhaustive]`); a self-observe metric
  `pulse.cardinality.refused.count` is emitted via the existing
  self-observe bridge pattern (FLAG 2 LIKELY recommendation, BOTH).

This is the thinnest slice that connects all four backbone
activities on pulse in one wave. Every later slice extends column 3
(env-driven configurability, per-tenant tuning, structured event)
without changing the shape of column 1 (the existing OTLP entry
points) or column 4 (the receipt shape and the self-observe metric
name).

## Slice plan

### Slice 01 (this feature wave): walking skeleton, per-tenant cardinality watermark on pulse, compile-time constant

Stories (all P1-P2 inside the same atomic slice; see Priority
Rationale below):

- **US-01** (P1): the `(N+1)`th NEW SeriesKey for tenant
  "acme-prod" is refused when the cap is N, and the N existing
  series keep ingesting normally.
- **US-02** (P1, atomic with US-01): tenant A's bomb does not
  affect tenant B's ingest (per-tenant cap, not global).
- **US-03** (P2, atomic with US-01-US-02): the refused-ingest
  count is observable via FLAG 2's mechanism.
- **US-04** (P2, atomic with the previous three): WAL replay
  respects the same path: existing series rebuilt from the WAL all
  pass through; the cap applies only to NEW series at post-replay
  live ingest.
- **US-05** (P2, atomic with the previous four): an ingest batch
  containing both existing-series points and new-series points
  above the cap is PARTIALLY applied (existing series receive
  their points; new series above the cap are refused); the whole
  batch is NEVER rejected.

Outcome KPI: every NEW SeriesKey above the per-tenant cap in the
acceptance suite is refused with the refused counter incrementing,
no panic, no silent drop, no OOM; every EXISTING SeriesKey receives
its points; every cross-tenant ingest stays isolated. See
`outcome-kpis.md`.

### Slice 02 (deferred, named OUT in `wave-decisions.md`)

Lift the cap value from a compile-time constant to env-driven
configurability (e.g. `KALEIDOSCOPE_PULSE_MAX_SERIES_PER_TENANT`),
mirroring the existing env-driven `TENANT` / `ADDR` posture each
`composition.rs` already uses. Cost: small; one resolver, exact env
name TBD by DESIGN.

### Slice 03 (deferred, named OUT)

A structured event log on each refusal (beyond the counter), to
support an alerting workflow once a downstream consumer (Prism panel,
beacon rule) is ready to consume it. Awaiting a real consumer.

### Slice 04 (deferred, future feature)

Per-(tenant, metric-name) sub-caps, or per-tenant series weighting
(e.g. weight by resource-attribute count). These are richer policies
that the simple per-tenant count cannot express; they are
deliberately deferred until the simple cap has been exercised in the
field.

### Slice 05 (deferred, future feature)

A global (cross-tenant) hard-stop cap as a second line of defence
behind the per-tenant cap. M-4's per-tenant cap is necessary and
sufficient at v0/v1 (one tenant cannot OOM the process without
crossing its OWN per-tenant cap first); the global cap is a
follow-up if the platform ever runs at scale where the sum of
per-tenant ceilings approaches a meaningful global ceiling.

## Priority Rationale

Priority order:

1. **US-01 (P1)**. The headline value of the feature: the
   `(N+1)`th NEW SeriesKey for a single tenant is refused, the
   existing N keep ingesting. Without US-01 the feature does not
   exist. The walking skeleton IS US-01.
2. **US-02 (P1, atomic with US-01)**. The per-tenant scoping is
   the WHOLE POINT (one tenant's bomb does not contaminate
   another); a global cap would re-couple tenants and violate the
   A-D4 fail-closed-tenancy attractor. US-02 closes the
   cross-tenant safety case alongside US-01; splitting them
   across slices would leave a half-cap that could not honestly
   claim isolation.
3. **US-03 (P2, atomic with US-01-US-02)**. The counter is the
   OBSERVABILITY of the refusal. Without US-03 the operator sees
   a tenant that "just stopped ingesting new series" with no
   reason; the refusal becomes silent loss in the
   operator's experience even though it is not silent in the
   code (the refused points never made it in). US-03 is P2 only
   because it depends on US-01-US-02 establishing the refusal
   itself; it lands in the same slice because the counter is what
   makes the refusal HONEST.
4. **US-04 (P2, atomic with the previous three)**. WAL replay
   shares `apply_ingest` (the SAME shared path the live ingest
   uses); the cap is uniform across the two by construction. US-04
   pins the semantics: replay rebuilds existing series, the cap
   applies only to NEW series at post-replay live ingest. Without
   US-04 a reader cannot tell whether replay rebuilds past the cap
   or refuses; DISCUSS pins the answer (rebuilds; cap applies at
   live ingest post-replay) so DESIGN does not re-litigate.
5. **US-05 (P2, atomic with the previous four)**. Batch
   semantics: an ingest batch containing both existing-series
   points and new-series points above the cap is PARTIALLY
   applied. The whole-batch-reject alternative is an A-U4
   "fabricated empty" attractor (silently losing good data
   alongside the refusal); US-05 forbids that. US-05 is P2
   because the partial-apply behaviour falls out of US-01's
   per-metric loop naturally, but the test must assert the
   partial-apply explicitly to kill a "reject-whole" mutant.

Dependencies:

- All five stories land in the SAME slice (slice 01). The per-crate
  mutation gate evaluates the whole crate after the change, not
  story-by-story; splitting the stories across slices would force
  multiple mutation runs to converge on 100 percent kill.
- All five stories depend on DESIGN resolving FLAG 1 (cap value),
  FLAG 2 (counter location), FLAG 3 (partial-batch semantics), and
  FLAG 4 (new ADR-0051 vs amend ADR-0045) before DISTILL writes
  acceptance tests.
- US-04 depends conceptually on US-01 having established the cap
  pattern; in practice the WAL-replay assertion is one extra test
  on top of the same `apply_ingest` edit.
- US-05 depends conceptually on US-01 having established the
  per-metric loop's refusal arm; the partial-apply assertion is
  one extra test on top of the same `apply_ingest` edit.

## Scope Assessment

PASS - 5 stories, 1 crate, estimated under 1 day total.

The residuality follow-up roadmap explicitly carves this as ONE of
three numbered features ("A soft watermark in `apply_ingest` refuses
further new label sets above a per-tenant threshold and surfaces a
recordable event, while existing series keep ingesting normally").
The carpaccio slicing rule is honoured: ONE crate (`pulse`, both
adapters), ONE shared seam (`apply_ingest`), ONE compile-time
constant, ONE counter (in two surfaces per FLAG 2 LIKELY); later
slices reserved for env-driven configurability and structured
events.

Oversize-check (any 2+ of):

- >10 user stories: NO (5).
- >3 bounded contexts or modules: NO (the work spans 1 crate,
  `pulse`, and touches both adapters via the shared `apply_ingest`
  seam plus its in-memory mirror; the self-observe bridge is
  touched only if FLAG 2 picks option B or C, and the bridge is
  itself a 1-file addition mirroring the existing
  `LumenToPulseRecorder`).
- Walking skeleton requires >5 integration points: NO (one seam,
  `apply_ingest`).
- Estimated effort >2 weeks: NO (under 1 day; the residuality
  analysis estimated "~80 LOC" total).
- Multiple independent user outcomes that could ship separately:
  NO (the user outcome is "honest per-tenant cardinality refusal
  on pulse ingest"; shipping it without per-tenant scoping (US-02),
  without observability (US-03), or without WAL-replay coherence
  (US-04) is not a deliverable outcome - it is half a cap).

Carpaccio taste-tests (see `wave-decisions.md`):

1. NEW above cap refused; existing keeps ingesting (US-01).
2. Per-tenant isolation (US-02).
3. Counter visibility (US-03).
4. WAL replay respects the cap (US-04).
5. Batch partial-apply (US-05).

Five independently demonstrable behaviours; one slice; right-sized.
