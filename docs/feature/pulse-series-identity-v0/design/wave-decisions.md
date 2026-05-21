# Wave Decisions: pulse-series-identity-v0 (DESIGN)

British English. No em dashes.

Author: `nw-solution-architect` (Morgan), DESIGN wave, application scope, propose
mode. Companion ADR: `docs/product/architecture/adr-0045-pulse-series-identity-is-the-full-label-set.md`.

## Key Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Series identity is the full label set (`MetricName` + `resource_attributes`) within a tenant. | The headline correctness fix. Two same-named metrics differing by `service.name` become two series; identical label sets merge. Point attributes stay per-point and never split a series. |
| D2 | Introduce a derived `SeriesKey { name: MetricName, resource_attributes: BTreeMap<String, String> }` in `metric.rs` with derived `Hash`/`Eq`/`Ord`/`Clone`/`Debug`. | A single home for series identity, self-documenting at each call site, derives the key traits once. `BTreeMap` is deterministically ordered so the derived traits are stable across ingests and processes. Preferred over an inline tuple repeated at every call site. Minimal: no builder, no extra methods. |
| D3 | The series index in `InMemoryMetricStore` and in `apply_ingest` becomes `HashMap<(TenantId, SeriesKey), SeriesEntry>`. | One keying correction in the shared `apply_ingest` is inherited by live ingest and WAL recovery, which cannot then drift. |
| D4 | Remove the `entry.metric.resource_attributes = metric.resource_attributes` overwrite (store.rs ~161, file_backed.rs ~318). | Resource attributes are now part of the key, so an ingest with different attributes lands in a different entry and cannot overwrite another series. `description`/`unit`/`kind` refresh behaviour is unchanged. |
| D5 | `query(name)` fans out across all series whose `SeriesKey.name` matches within the tenant, each row carrying its own series' `resource_attributes`. `query_with` fans out then applies its predicate per row. | The trait already returns `Vec<(Metric, MetricPoint)>`, shaped for multiple series. The fan-out is beneath the trait. |
| D6 | Snapshot buckets by full label set; recovery stays append-and-sort. | `SeriesBucket` already carries the canonical `Metric` (hence its `resource_attributes`); only the in-memory key the buckets rebuild into changes (`SeriesKey::from(&bucket.metric)`). Points are still re-sorted by `time_unix_nano` after replay. |
| D7 | The snapshot format MAY change freely; no migration, shim, or version negotiation. | Pulse is library-only at v0/v1 with no daemon and no production data (`lib.rs`). Stated so a future reader does not invent a migration story. |
| D8 | The public `MetricStore` trait signature is unchanged. | Verified against `lib.rs` and `store.rs` lines 77-82: `query` already returns `Vec<(Metric, MetricPoint)>`. Only the keying and fan-out change. |
| D9 | No new secondary index for the query fan-out. | At v0/v1 in-memory scale the linear pass over series sharing a name is fine. A known characteristic, not a problem to solve now; premature indexing is a v2 concern. |

## Architecture Summary

A small, contained data-model correction inside the existing `pulse` crate. No
new component, no new crate, no new module, no new external dependency, no trait
signature change. Architectural style is unchanged: Pulse is the `MetricStore`
port with two adapters (`InMemoryMetricStore`, `FileBackedMetricStore`) behind
it; this feature corrects the series-keying logic those adapters share, not the
port. The change touches three files:

- `metric.rs` gains the derived `SeriesKey` type (new data type, EXTEND).
- `store.rs` re-keys its index on `(TenantId, SeriesKey)`, removes the
  `resource_attributes` overwrite, and fans `query`/`query_with` out across
  series sharing a name (EXTEND).
- `file_backed.rs` re-keys its index and `apply_ingest` on `(TenantId,
  SeriesKey)`, removes the overwrite, rebuilds snapshot buckets by full label
  set on `open`, and fans `query`/`query_with` out (EXTEND).

Because live ingest and WAL recovery share `apply_ingest`, the identity
correction lands once and both paths inherit it; this is the property that lets a
single change cover the in-memory store and durable recovery without drift. The
recovery discipline stays append-and-sort (ADR-0040 Decision 2); only the
bucketing key changes. Development paradigm is Rust idiomatic per CLAUDE.md: a
plain data struct with derives plus free-function edits, no class hierarchy, no
new `dyn` boundary.

C4 L1 + L2 of the ingest/query/recovery path are in
`docs/feature/pulse-series-identity-v0/design/application-architecture.md`. L3 is
not produced: the change is to keying logic within existing adapters, not a new
multi-component subsystem.

## Reuse Analysis

Mandatory reuse table. Existing-system analysis (Glob/Grep + direct read of
`store.rs`, `file_backed.rs`, `metric.rs`, `lib.rs`) confirms every behaviour
this feature needs already exists and is EXTENDED in place. Zero CREATE NEW
components, zero new crates, zero unjustified creation.

| Element | Decision | Justification |
|---------|----------|---------------|
| `metric.rs` (`Metric`, `MetricPoint`, `MetricName`, `MetricBatch`) | EXTEND | Add the derived `SeriesKey` type beside the existing types. No type is replaced; `Metric.resource_attributes` is reused unchanged as the source of the key. |
| `store.rs` `InMemoryMetricStore` index + `ingest` + `query` + `query_with` | EXTEND | Re-key the existing index, drop one overwrite line, fan the existing query logic out across matching series. The `SeriesEntry` split, sort-on-ingest, `Predicate` composition, `MetricsRecorder` seam are all reused verbatim. |
| `file_backed.rs` `apply_ingest` + index + `open` recovery + snapshot bucketing + `query`/`query_with` | EXTEND | Re-key the shared `apply_ingest` and the index, drop one overwrite line, rebuild buckets by full label set on `open`, fan queries out. WAL format, snapshot file shape, append/flush, re-sort-after-replay, `PersistenceFailed` handling reused verbatim. |
| `MetricStore` trait (`lib.rs`, `store.rs`) | REUSE (unchanged) | Signature already returns `Vec<(Metric, MetricPoint)>`, shaped for multiple series. No change. |
| `WalRecord` / `Snapshot` / `SeriesBucket` on-disk shapes | REUSE (unchanged shapes) | The bucket already carries the canonical `Metric` with its `resource_attributes`; only the in-memory rebuild key changes. No new serde field. |
| `aegis::TenantId` | REUSE (unchanged) | Tenant scoping is unchanged; full-label-set identity applies within a tenant. |
| New crate / new module / new external dependency | NONE | No justification exists for any; the fix is three files in one existing crate. |

Verdict: all EXTEND (plus REUSE of unchanged elements). No CREATE NEW component.

## Constraints

- No `MetricStore` trait signature change (System Constraints; verified D8).
- No migration, shim, or version negotiation; snapshot format may change (D7).
- Point attributes are not touched (already per-point and correct).
- Tenant isolation is unchanged; identity applies within a tenant.
- Recovery discipline stays append-and-sort (ADR-0040 Decision 2 is cited as
  framing, NOT modified).
- Proportionate: a small focused feature; no gold-plating of `SeriesKey`, no new
  index, no batch-level resource hoisting.

## Upstream Changes

None to upstream artefacts. ADR-0040 is cited as framing and is NOT modified.
This feature UNBLOCKS the downstream `query-api-label-matchers-v0` (six stashed
acceptance scenarios resume once this ships); no change is made under that
feature's scope here.

## DEVOPS Handoff Annotation

For `@nw-platform-architect` (DEVOPS wave):

- **Library-only change.** No HTTP surface, no daemon, no network, no new binary.
  Pulse remains a library.
- **No new CI gate.** ADR-0005's five gates apply unchanged. No new or amended
  gate is warranted.
- **Per-feature mutation testing** scoped to the modified files
  `crates/pulse/src/{store.rs, file_backed.rs, metric.rs}` at the 100% kill-rate
  gate (ADR-0005 Gate 5; CLAUDE.md). Covered by the existing
  `gate-5-mutants-pulse`; no new gate.
- **No new `[dependencies]`.** `serde`/`serde_json`/`aegis` are already present;
  `SeriesKey` uses only `std::collections::BTreeMap` and derive macros.
- **External integrations: none.** No third-party API, webhook, OAuth provider,
  or vendor SDK. No contract tests apply (pure in-process data-model change over
  the local filesystem WAL + JSON snapshot, both pre-existing).
- **Earned Trust: no new probe.** The change is pure keying logic over existing
  in-process and local-filesystem substrate; the existing pulse-v1 recovery
  durability test is the empirical probe and is unchanged in shape (it now also
  exercises distinct-series survival). No new external dependency to probe.
- **DELIVER paradigm**: Rust idiomatic. One new data struct with derives, free
  helper edits, one map-key type change in three files. No class-style
  inheritance, no new `dyn` boundary.

## Handoff to DISTILL

For `@nw-acceptance-designer`: the eight acceptance criteria in
`slices/slice-01-series-identity-by-label-set.md` (US-01 distinct series at
ingest/query, identical-label-set merge, point attributes do not split, plus
US-02 survive snapshot+reopen, survive WAL-only reopen, re-ingest after reopen
joins the recovered series) translate into `#[test]` functions against a real
`FileBackedMetricStore` (the realistic invocable surface). The
`@walking_skeleton` scenario is the US-01 happy path. Required reading: this
file; `design/application-architecture.md`; ADR-0045; the verified-against-code
facts in `discuss/wave-decisions.md`.

## Peer review

Reviewer: `solution-architect-reviewer`. Max two iterations on rejection. To be
run at `*handoff-distill`.
