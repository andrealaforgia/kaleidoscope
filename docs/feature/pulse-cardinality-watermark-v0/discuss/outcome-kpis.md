# Outcome KPIs: pulse-cardinality-watermark-v0

British English. No em dashes. No emoji.

## Feature objective

The `pulse` store refuses NEW `SeriesKey`s above a configured
per-tenant ceiling and counts each refusal, instead of growing the
per-tenant index without bound and OOM-killing the process when a
client (misconfigured or hostile) attaches growing-cardinality labels
(a timestamp, a UUID, a per-request ID) to its metrics. EXISTING
series keep receiving points normally; one tenant's bomb does not
contaminate another tenant; the refusal is observable. The S04 OOM
surface flagged by the residuality analysis is closed for pulse in
one slice, refining the open question ADR-0045 explicitly named in
its Consequences. The walking-skeleton entry point is the existing
OTLP gRPC and HTTP-protobuf gateway path that already reaches
`pulse::FileBackedMetricStore::ingest`.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|---|---|---|---|---|---|
| 1 | An operator (Maya Kowalski) running `kaleidoscope-gateway` for tenant "acme-prod" while a misconfigured client (or attacker) emits growing-cardinality labels | sees the `(N+1)`th NEW `SeriesKey` for "acme-prod" REFUSED above the per-tenant cap N (`MAX_SERIES_PER_TENANT`), with the refused counter incrementing by 1, while the N existing series keep receiving points normally | 100 percent of NEW-above-cap ingests in the acceptance suite (US-01 Scenario 2, US-05 Scenario 1) refuse with the counter incrementing; 100 percent of EXISTING-series ingests post-cap (US-01 Scenario 3) succeed | 0 percent today: `apply_ingest` (`crates/pulse/src/file_backed.rs:349`) calls `series.entry(key).or_insert_with(...)` for every metric, no per-tenant count, no cap; the process OOMs under enough labels (incidence-matrix S04 row, pulse cell "B OOM under enough labels") | Acceptance-test pass / fail in the slice-01 suite; the test asserts (a) the refused count, (b) the index width stays at `MAX_SERIES_PER_TENANT`, (c) the process does not panic | Leading (outcome) |
| 2 | Same persona, plus the operator of a second tenant ("Globex Steady" on "globex-staging") | sees tenant A's cap breach NOT affect tenant B's ingest: tenant B can still ingest new series, tenant B's refused counter stays at 0, the cap is genuinely per-tenant | 100 percent of cross-tenant scenarios (US-02 Scenario 1, US-02 Scenario 2) leave tenant B unaffected when tenant A is at or beyond its cap | 0 percent today: there is no cap at all, so a sufficiently severe acme-prod bomb OOMs the WHOLE process, taking globex-staging down with it (A-U1 attractor: "Silent data loss") | Acceptance-test pass / fail; the test seeds tenant A to the cap, attempts a new series on tenant B, asserts success and asserts globex-staging's refused counter is 0 | Leading (outcome) |
| 3 | Same operator persona | observes the refused-ingest count via FLAG 2's mechanism (LIKELY recommendation: BOTH a `refused_new_series: usize` field on `IngestReceipt` AND a `pulse.cardinality.refused.count` self-observe metric via the existing bridge pattern) | 100 percent of refused-ingest scenarios in the acceptance suite (US-03 Scenario 1, US-03 Scenario 2 if BOTH is picked) expose the refused count via the chosen surface(s); the count is monotonically non-decreasing per tenant | 0 percent today: there is no counter; an OOM is the only signal, and it kills the process | Acceptance-test pass / fail; for the receipt-field surface, the test inspects the receipt; for the self-observe metric surface, the test queries the bridge target (or pulse itself if the bridge points back to pulse) | Leading (outcome) |
| 4 | An operator running the platform across a restart (a rolling deploy, a crash recovery, or a routine restart) | sees WAL replay rebuild EXISTING series for any tenant past the cap (because the WAL captured legitimate pre-cap ingests; replay should not refuse them), and the cap applies to NEW series at post-replay live ingest only | 100 percent of restart scenarios in the acceptance suite (US-04 Scenario 1, US-04 Scenario 2) preserve existing series on replay and apply the cap to subsequent live ingests; the cap NEVER refuses a key that was successfully ingested before the cap existed (or before the cap value tightened) | 0 percent today: the cap does not exist, so the question is undefined; the residuality analysis identifies WAL replay as a place the cap MUST be coherent (S04, S21) | Acceptance-test pass / fail; the test populates the store to the cap, calls `snapshot()` and reopens, asserts replay rebuilds the existing N series, attempts a NEW series above the cap post-replay, asserts the refusal | Leading (outcome) |
| 5 | Same operator persona | sees an ingest batch containing both existing-series points and new-series points above the cap PARTIALLY applied: existing series get their points, new series above the cap are refused, the receipt reports honestly, the whole batch is NEVER rejected | 100 percent of mixed-batch scenarios in the acceptance suite (US-05 Scenario 1) honour partial-apply semantics: existing points land, new-above-cap series are refused and counted, and the receipt's ingested count matches the existing-series points actually stored | 0 percent today: there is no cap at all, so the question is undefined; the LIKELY recommendation in FLAG 3 (PARTIAL APPLY) aligns with the platform's A-D6 "honest three-way outcomes" attractor and forbids the A-U4 "fabricated empty" alternative | Acceptance-test pass / fail; the test builds a mixed batch, calls `ingest`, asserts both the receipt's `count` and (per FLAG 2) `refused_new_series`, then asserts the existing-series stored points reflect the partial apply | Leading (outcome) |
| 6 | A Kaleidoscope developer maintaining `pulse` | sees the per-crate mutation gate stay at 100 percent kill on the changed files for `pulse` after the cap lands | 100 percent (ADR-0005 Gate 5; CLAUDE.md) | 100 percent today on the unchanged code; the new cap-check code does not yet exist | `cargo mutants` per-crate output on the changed files in `pulse` | Leading (secondary) |
| 7 | A reviewer comparing the public-api diff against the prior `pulse` tag | sees the `MetricStore` trait signature byte-identical to the prior tag (methods, parameters, return types unchanged); `IngestReceipt` may grow a field (additive; FLAG 2 LIKELY recommendation) but no method is added, removed, or re-signed on the trait itself; the WAL on-disk record shape stays the same | 0 trait-method-signature changes; 0 WAL-record-shape changes; at most 1 additive field on `IngestReceipt` (guarded by FLAG 2 and DESIGN's `#[non_exhaustive]` decision) | 0 changes today; the trait and WAL shape are both stable | The `gate-2-public-api` diff (ADR-0005 Gate 2) on `pulse`; manual inspection of `WalRecord::Ingest` in `crates/pulse/src/file_backed.rs:48` | Guardrail |

## Metric hierarchy

- **North star**: KPI 1, "the `(N+1)`th NEW SeriesKey for a single
  tenant is refused above the cap; the existing N keep ingesting".
  The whole feature exists to move this from 0 to 100. KPI 2 is the
  per-tenant safety case on the same axis.
- **Leading indicators**: KPI 3 (observability of the refusal),
  KPI 4 (WAL-replay coherence), KPI 5 (partial-apply semantics),
  KPI 6 (mutation kill rate stays at 100 percent on changed files).
- **Guardrail metrics**: KPI 7 (no trait or WAL change beyond an
  additive `IngestReceipt` field).

## Measurement plan

| KPI | Data source | Collection method | Frequency | Owner |
|---|---|---|---|---|
| 1 | Slice-01 acceptance suite on `pulse` (both `FileBackedMetricStore` and `InMemoryMetricStore`) | `cargo test --package pulse` in CI (Gate 1 of ADR-0005); explicit US-01 Scenario 2 and Scenario 3 | Every push | Crafty (DELIVER) writes the tests; Bea (DOCUMENT) records closure in the per-feature narrative |
| 2 | Same suite | Explicit US-02 scenarios with a second tenant | Same | Same |
| 3 | Same suite | Receipt-field assertion AND self-observe metric assertion (per FLAG 2 LIKELY: BOTH) | Same | Same |
| 4 | Same suite | Explicit US-04 scenarios that `snapshot()` and reopen, asserting replay rebuilds existing series and the cap applies to post-replay new series | Same | Same |
| 5 | Same suite | Explicit US-05 scenarios building a mixed batch | Same | Same |
| 6 | `cargo mutants` per-crate workflow on `pulse` | The per-crate Gate 5 workflow already in CI | Same | Apex (DEVOPS) maintains the workflow; Crafty keeps the kill rate at 100 percent on changed files |
| 7 | `cargo public-api` diff against the prior `pulse` tag; manual inspection of `WalRecord` | Gate 2 in the per-crate CI; reviewer eyes on the PR | Same | Apex |

## Hypothesis

We believe that ONE compile-time per-tenant cap
(`MAX_SERIES_PER_TENANT`) applied inside the shared `apply_ingest`
path of `pulse::FileBackedMetricStore` AND its in-memory mirror,
refusing NEW `SeriesKey`s above the ceiling while leaving EXISTING
series untouched, and incrementing a refused counter visible to the
operator (FLAG 2 LIKELY: BOTH a receipt field and a self-observe
metric), will close the S04 OOM surface for pulse without:

- changing the `MetricStore` trait method signatures,
- changing the WAL on-disk record shape,
- evicting any existing series (preserving the append-and-sort
  discipline of ADR-0040 Decision 2),
- introducing a global cross-tenant cap (preserving the A-D4
  per-tenant isolation attractor),
- adding any structured event beyond the counter,
- renegotiating the OTLP partial-success contract the gateway
  already implements.

We will know this is true when:

- 100 percent of NEW-above-cap ingest scenarios refuse with the
  counter incrementing and the process intact.
- 100 percent of cross-tenant scenarios leave tenant B unaffected
  while tenant A is at or beyond its cap.
- 100 percent of WAL-replay scenarios rebuild existing series and
  apply the cap only to post-replay NEW series.
- 100 percent of mixed-batch scenarios partial-apply honestly.
- 100 percent of the refusal scenarios surface the count via FLAG 2's
  chosen mechanism.
- 100 percent of mutants in the changed files are killed by the
  per-crate mutation gate.
- 0 changes to the `MetricStore` trait method signatures and 0
  changes to the WAL on-disk record shape.

We will know this is false if:

- The chosen cap value (FLAG 1) is too tight for legitimate tenants
  with naturally high series counts, generating false positives a
  real operator notices. Escalation path: re-pick the value in a
  successor slice, or move to env-driven configurability (slice 02,
  declared OUT).
- DESIGN concludes the right semantics on mixed-batch breach is
  REJECT-WHOLE rather than PARTIAL APPLY (FLAG 3). Re-frame the
  relevant scenarios; the DISCUSS-time LIKELY recommendation was
  PARTIAL APPLY.
- DESIGN concludes the receipt-field surface alone (or the
  self-observe metric surface alone) is enough (FLAG 2). Re-frame
  the US-03 scenarios.

## Smell-test review

| Check | Verdict | Note |
|---|---|---|
| Measurable today? | Yes | Acceptance-test outcomes, `cargo mutants` kill rate, and `cargo public-api` diffs are all already collected by the platform's existing CI surface (ADR-0005). The receipt-field surface is measurable in the same test that calls `store.ingest`. The self-observe metric surface is measurable via the bridge's target store (a pulse instance the test seeds) once FLAG 2 picks. |
| Rate not total? | Yes | All KPIs are rates over scenarios ("100 percent of NEW-above-cap ingests refuse"), not gross counts. The internal refused counter IS a gross count (a `usize` ticking up per refusal), but the KPI on it is a rate of "100 percent of refused-ingest scenarios surface the count". |
| Outcome not output? | Yes | The KPIs describe the operator's observable behaviour change (the `(N+1)`th series is refused, the existing N keep ingesting, the counter ticks, the process stays alive, the second tenant is isolated, replay is coherent, mixed batches partial-apply), not the shipped artefact ("we shipped a cap"). |
| Has baseline? | Yes | 0 percent today, with file-and-line evidence cited in the baseline column of KPIs 1, 2, 3, 4, 5; the residuality analysis names the same gap as S04 / "B OOM" / A-U1. |
| Team can influence? | Yes | The team owns `apply_ingest`, `IngestReceipt`, and the self-observe bridge; nothing external. |
| Has guardrails? | Yes | KPI 6 (mutation kill rate), KPI 7 (no trait or WAL change beyond an additive receipt field), plus the existing `gate-2-public-api` regression test on `pulse`. |

## Handoff to DEVOPS

Per the residuality follow-up roadmap, DEVOPS (Apex) for this
feature is SLIM: there is no new crate (the work lands inside the
existing `pulse` crate, with a possible 1-file addition to
`self-observe` if FLAG 2 picks the self-observe surface) and no new
dependency expected. The DEVOPS surface this feature touches:

- **Data collection**: the refused counter is a new in-memory
  number per tenant per store instance. If FLAG 2 picks the
  self-observe metric surface (LIKELY: BOTH), the metric name is
  `pulse.cardinality.refused.count`, value = the refused-event
  count (1 per emission, matching the existing
  `cinder.place.count` convention in `cinder_bridge.rs`), emitted
  through whatever pulse store the self-observe bridge is wired
  into. The metric IS a pulse metric, so it is subject to its own
  cap; the operator picks the cap above the natural self-observe
  cardinality.
- **Dashboards / monitoring**: NONE new at v0/v1. The platform
  has no live observability stack of its own yet; the self-observe
  metric (if emitted) becomes queryable via the existing
  `query-api` once the slice lands.
- **Alerting thresholds**: NONE. The refusal IS the signal; the
  operator (or an automated downstream consumer) reads the counter
  and reacts. A future feature MAY add a beacon rule on the
  self-observe metric; M-4 does not.
- **Baseline measurement**: the file-and-line evidence cited in
  the baseline columns suffices; no separate baseline collection
  needed.
- **Per-crate mutation gates**: `gate-5-mutants-pulse` already
  exists; the slice-01 change must keep it at 100 percent kill on
  the changed files. No new gate, no new workflow.
- **Public-api gate**: `gate-2-public-api` on `pulse` runs on
  every push; the cap rides in the store implementation and (per
  FLAG 2 LIKELY) adds one field to `IngestReceipt`. DESIGN owns
  whether to add `#[non_exhaustive]` for forward-compatibility; if
  it does, the field addition does not break consumers. If it does
  not, the gate flags the addition as a Major-version-relevant
  change; DESIGN documents it.

## Connection to the residuality analysis

The KPIs above map directly onto the analysis's incidence-matrix
S04 row, with the pulse cell currently reading "**B OOM** under
enough labels":

- The S04-row cell under `P` reads `B OOM under enough labels`
  today; KPI 1 plus KPI 2 transitions that cell to
  `S per-tenant cardinality watermark refuses new series; existing
  series keep ingesting; per-tenant isolation preserved`.
- The A-U1 attractor "Silent data loss" (the OOM kill path) stays
  blocked because the process no longer OOMs under a cardinality
  bomb; the refusal is honest and counted.
- The A-D4 attractor "Fail-closed tenancy at every plane boundary"
  is preserved because the cap is per-tenant, not global; tenant A's
  bomb does not refuse tenant B's ingest. (US-02.)
- The A-D2 attractor "Append-and-sort steady state" is preserved
  because no series is evicted; the cap refuses new ones, it never
  displaces existing ones. (US-04.)
- The A-D6 attractor "Honest three-way outcomes" is preserved
  because the refusal is named (the counter), not silent; the
  partial-apply semantics keep the receipt honest. (US-03, US-05.)

KPI 1 plus KPI 2 is the rate at which this matrix transition
completes on the pulse cell; KPI 3 plus KPI 4 plus KPI 5 are the
secondary residues (observability, replay coherence, partial-apply
honesty); KPI 6 plus KPI 7 are the guardrails that the transition
does not cost anything elsewhere in the matrix (no trait change,
no WAL format change, no mutation-kill-rate regression).
