# ADR-0045 — Pulse series identity is the full label set

- **Status**: Accepted
- **Date**: 2026-05-22
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `pulse-series-identity-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0040 Decision 2 (append-and-sort versus keyed-latest-wins;
  cited as framing, NOT modified), ADR-0044 (the query-api label-matcher feature
  this unblocks).

## Context

Pulse identifies a metric series by `(tenant, MetricName)` alone. The series
index is `HashMap<(TenantId, MetricName), SeriesEntry>` in both adapters
(`crates/pulse/src/store.rs` line 110, `crates/pulse/src/file_backed.rs` line 82),
and on every ingest the stored metadata is refreshed, including the line
`entry.metric.resource_attributes = metric.resource_attributes;`
(`store.rs` line 161, `file_backed.rs` line 318). The inline comment is explicit
that v0 treats `resource_attributes` as refreshable metadata, not as identity.

The consequence is a correctness defect. Two metrics that share a name but
differ by `service.name` (checkout, cart, search under one tenant) collide on
the same key and collapse into one series. The last ingest's
`resource_attributes` overwrites the others, so a query of the name returns one
arbitrary service's labels applied to every point, regardless of which service
produced them. Per-service provenance is destroyed at ingest, before any query
runs. This was discovered downstream during DELIVER of
`query-api-label-matchers-v0`
(`docs/feature/query-api-label-matchers-v0/deliver/upstream-issues.md`): six
acceptance scenarios cannot go green because the information they filter on was
already discarded beneath query-api.

ADR-0040 Decision 2 records the platform's two recovery disciplines. The storage
pillars (cinder, sluice, lumen, pulse, ray, strata) are **append-and-sort**:
each WAL record is an event in a time series and recovery re-sorts by
`time_unix_nano`. Beacon is **keyed-latest-wins**: a rule has one current state
and the last `Put` per key wins. The present `resource_attributes` overwrite is
exactly the latent error ADR-0040 warns against: it applies a quiet,
accidental keyed-latest-wins to metadata *inside* an append-and-sort series.
Metadata that is part of a series's identity must not be latest-wins.

This ADR is the next free number; the highest existing was ADR-0044. ADR-0040 is
Accepted, immutable, and cited only as framing. It is NOT edited.

## Decision

### 1. A series is identified by its full label set, within a tenant

A series's identity is its metric name plus its `resource_attributes`, scoped
per tenant. Two metrics sharing a name but differing by any `resource_attributes`
entry are two distinct series. Two ingests carrying an identical full label set
target the same series. Point-level attributes (`MetricPoint.attributes`) are
per-point and never part of series identity; they are unchanged by this feature.

Introduce a derived key type in `crates/pulse/src/metric.rs`:

```text
SeriesKey { name: MetricName, resource_attributes: BTreeMap<String, String> }
```

with derived `Hash`, `Eq`, `PartialEq`, `Ord`, `PartialOrd`, `Clone`, `Debug`.
A `BTreeMap` is deterministically ordered, so the derived `Hash`/`Eq`/`Ord` are
stable across ingests and processes regardless of attribute insertion order,
which makes `SeriesKey` a sound hash-map key. The series index becomes
`HashMap<(TenantId, SeriesKey), SeriesEntry>` in both adapters. The named type is
preferred over an inline tuple at each call site because it gives series identity
a single home, is self-documenting at the point of use, and derives the key
traits once rather than relying on every call site agreeing on the tuple shape.
It is deliberately minimal: a plain data struct with derives, no builder, no
methods beyond what ingest and query already need.

### 2. Remove the resource-attributes overwrite

Delete `entry.metric.resource_attributes = metric.resource_attributes;` from both
the in-memory upsert (`store.rs` line 161) and the shared `apply_ingest`
(`file_backed.rs` line 318). Because `resource_attributes` is now part of the key,
an ingest with different attributes lands in a different `SeriesEntry` and cannot
overwrite another series; an ingest with identical attributes lands in the same
entry, where its attributes already equal the stored ones. The other metadata
refreshes (`description`, `unit`, `kind`) stay as they are: they are not part of
identity and the existing permissive-refresh behaviour is unaffected by this
feature.

### 3. Query fans out across all series under the name

`query(tenant, name, range)` now iterates every series whose `SeriesKey.name`
equals `name` within the tenant, and returns each matching point paired with its
own series' `Metric` (carrying that series's `resource_attributes`). The result
remains `Vec<(Metric, MetricPoint)>`; it now carries rows from more than one
series when a name is multi-service. `query_with` fans out identically and then
applies its predicate per row. This is a fan-out over series matching the name
within the tenant; see Consequences for the cost characteristic.

### 4. Snapshot buckets by full label set; recovery discipline unchanged

`SeriesBucket` already carries the canonical `Metric` (and thus its
`resource_attributes`), so the on-disk bucket shape needs no new field; what
changes is the in-memory key the buckets rebuild into. On `open`, each
`SeriesBucket` rebuilds into `(tenant, SeriesKey::from(&bucket.metric))` rather
than `(tenant, name)`. WAL replay already routes through the shared
`apply_ingest`, so the keying correction is inherited by recovery from a single
edit. Recovery stays append-and-sort: points are re-sorted by `time_unix_nano`
after replay exactly as today. Only the bucketing key changes; the discipline
(ADR-0040 Decision 2, append-and-sort) is untouched.

The snapshot format MAY change freely. Pulse is library-only at v0/v1 with no
daemon and no production data (`crates/pulse/src/lib.rs`: "Library only at v0. No
daemon, no network."), so a format change needs no migration, no compatibility
shim, and no version negotiation. This is stated explicitly so a future reader
does not invent a migration story this feature deliberately does not have.

### 5. The MetricStore trait signature is unchanged

`query` already returns `Vec<(Metric, MetricPoint)>` (`store.rs` lines 77-82),
where each point carries its owning `Metric` and that metric's
`resource_attributes`. The shape already supports many series under a name. The
fix is to the ingest keying and the query fan-out beneath the trait, not to the
trait. Verified against `crates/pulse/src/lib.rs` (the public `MetricStore`
re-export) and `store.rs`: no method signature changes.

## Alternatives considered

### A (rejected): keep the name-only key — the broken status quo

Continue keying by `(tenant, MetricName)` and overwriting `resource_attributes`
on each ingest. For: zero change. Against: this IS the defect. It collapses
every multi-service metric into one series wearing the last-ingested service's
labels and destroys per-service provenance at ingest, blocking
`query-api-label-matchers-v0`. Rejected; it is the bug being fixed.

### B (rejected): hoist resource attributes to the batch level

Move `resource_attributes` off each `Metric` and onto the `MetricBatch`, so a
batch carries one resource set shared by all its metrics (a future idea noted in
`metric.rs` and in the story map's deferred list). For: it matches the OTLP
`ResourceMetrics` envelope and would shrink per-metric duplication. Against: it
changes the `Metric` and `MetricBatch` shapes at the trait boundary, which is a
larger blast radius than this correctness fix needs; it does not by itself
establish series identity (two services in one batch still need distinct series
keys); and it is explicitly out of scope per DISCUSS (story map "Deferred"). It
is an orthogonal modelling change, not an identity fix. Rejected for this
feature; revisit when the v1 columnar substrate reshapes the batch envelope.

### C (rejected): inline `(name, resource_attributes)` tuple at each call site

Key the maps directly on `(TenantId, MetricName, BTreeMap<String, String>)`
without a named type. For: no new type. Against: series identity would have no
single home; every call site (the two ingests, the two queries, the two
`query_with`, the snapshot rebuild) would independently restate the tuple shape,
and a future point attribute or label addition would have to be threaded through
each by hand. A named `SeriesKey` derives the key traits once and documents
intent where it is used. Rejected; the named type costs one small struct and
removes a class of drift.

## Consequences

### Positive

- **Per-service provenance is preserved.** Distinct `resource_attributes` under a
  shared name become distinct series; a later ingest can no longer overwrite
  another service's labels. The headline correctness defect is fixed at its
  source.
- **Unblocks `query-api-label-matchers-v0`.** The six stashed acceptance
  scenarios filter on a label set that now survives ingest; they are expected to
  go green with no further query-api change once this ships.
- **One fix point covers both paths.** Live ingest and WAL recovery share
  `apply_ingest`, so the keying correction lands once and the in-memory store and
  durable recovery cannot drift.
- **Trait and downstream contract unchanged.** `MetricStore` is byte-identical;
  the `Vec<(Metric, MetricPoint)>` shape already anticipated multiple series, so
  no consumer signature changes.

### Negative

- **The snapshot format changes.** The in-memory bucket key changes from name to
  full label set. Accepted and safe: Pulse is library-only with no production
  data, so no migration, shim, or version negotiation is needed (stated in
  Decision 4).
- **Query fans out across series sharing a name.** `query(name)` now iterates the
  series matching the name within the tenant rather than a single keyed lookup.
  At v0/v1 in-memory scale this is a small linear pass and is fine; it is a known
  characteristic, not a problem to solve now. No secondary index is introduced;
  premature indexing is explicitly out of scope and would be a v2 concern if
  series cardinality per name ever grew large.

### Trade-off summary

The feature trades a single-key `O(1)` name lookup for a fan-out over the series
sharing a name, and trades snapshot-format stability for correctness. Both trades
are sound at the current library-only, in-memory scale, and both are recorded so
a future reader understands the cost was chosen, not overlooked.
