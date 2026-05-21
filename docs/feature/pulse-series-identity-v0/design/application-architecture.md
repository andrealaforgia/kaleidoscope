# Application Architecture: pulse-series-identity-v0

British English. No em dashes.

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-22.

A data-model correction inside the existing `pulse` crate: the series index is
re-keyed from `(tenant, MetricName)` to `(tenant, SeriesKey)`, where `SeriesKey`
is the full label set (`MetricName` + `resource_attributes`). The
`resource_attributes` overwrite at ingest is removed; `query` fans out across all
series sharing a name. No new component, no new crate, no trait signature change.

## C4 Level 1 — System Context

```mermaid
C4Context
  title System Context — pulse-series-identity-v0
  Person(consumer, "query-api / operator", "Ingests a multi-service metric, queries it by name")
  System(pulse, "pulse crate", "MetricStore: identifies a series by its full label set")
  System_Ext(fs, "Local filesystem", "WAL + snapshot files (FileBackedMetricStore)")
  Rel(consumer, pulse, "Ingests batches and queries by name via")
  Rel(pulse, fs, "Appends WAL records and reads/writes snapshot in")
```

## C4 Level 2 — Container (ingest / query / recovery path)

The change point is the series-index KEY shared by the in-memory adapter and the
durable adapter's `apply_ingest`. Arrows are labelled with verbs; the relabelled
key is annotated where it changes.

```mermaid
C4Container
  title Container Diagram — pulse crate (series identity by full label set)
  Person(consumer, "query-api / operator")

  Container_Boundary(pulse, "pulse crate") {
    Container(trait, "MetricStore trait", "Rust trait (UNCHANGED)", "ingest, query, query_with -> Vec<(Metric, MetricPoint)>")
    Container(mem, "InMemoryMetricStore", "Rust adapter", "HashMap<(TenantId, SeriesKey), SeriesEntry>")
    Container(fb, "FileBackedMetricStore", "Rust adapter", "Same index; durable")
    Container(applyingest, "apply_ingest", "shared free fn", "Keys each metric by SeriesKey; live ingest AND WAL replay route through here")
    Container(metric, "metric.rs types", "OTLP-shaped data", "Metric, MetricPoint, MetricName, + new SeriesKey")
  }

  System_Ext(wal, "WAL file", "<base>.wal NDJSON")
  System_Ext(snap, "Snapshot file", "<base>.snapshot JSON")

  Rel(consumer, trait, "Ingests batch and queries name via")
  Rel(trait, mem, "Dispatches to")
  Rel(trait, fb, "Dispatches to")
  Rel(mem, applyingest, "Shares keying logic shape with")
  Rel(fb, applyingest, "Routes live ingest and WAL replay through")
  Rel(applyingest, metric, "Builds SeriesKey from Metric.name + resource_attributes using")
  Rel(fb, wal, "Appends Ingest record to")
  Rel(fb, snap, "Rebuilds buckets by full label set from")
  Rel(fb, snap, "Writes one bucket per distinct series to")
```

Note on query fan-out: `query(name)` now iterates the series whose
`SeriesKey.name` matches within the tenant and returns each row with its own
series' `resource_attributes`. At v0/v1 in-memory scale this linear pass is fine;
it is a known characteristic, not a problem solved here (no secondary index).

L3 is not produced: the change is to keying logic inside two existing adapters,
not a new multi-component subsystem.

## Changes Per File

| File | Change | Decision |
|------|--------|----------|
| `crates/pulse/src/metric.rs` | Add derived `SeriesKey { name: MetricName, resource_attributes: BTreeMap<String, String> }` with `Hash`/`Eq`/`PartialEq`/`Ord`/`PartialOrd`/`Clone`/`Debug`. No builder, no extra methods. | D2 |
| `crates/pulse/src/store.rs` | Re-key `InnerState.series` to `HashMap<(TenantId, SeriesKey), SeriesEntry>`; build the key in `ingest` (line ~144); remove `entry.metric.resource_attributes = ...` (line ~161); fan `query` (line ~176) and `query_with` (line ~203) out across series whose `SeriesKey.name` matches the queried name. | D3, D4, D5 |
| `crates/pulse/src/file_backed.rs` | Re-key `Inner.series` and `apply_ingest` to `(TenantId, SeriesKey)` (key build line ~303); remove the overwrite (line ~318); rebuild snapshot buckets into `(tenant, SeriesKey::from(&bucket.metric))` on `open` (line ~111); fan `query` (line ~242) and `query_with` (line ~269) out; snapshot iteration (line ~168) keyed by full label set. Re-sort after replay unchanged. | D3, D4, D5, D6 |

The `MetricStore` trait (`store.rs` lines 66-93, re-exported in `lib.rs`) is
unchanged. The on-disk `WalRecord` / `Snapshot` / `SeriesBucket` shapes need no
new serde field; only the in-memory rebuild key changes (D7: snapshot format may
change freely regardless, no migration).
