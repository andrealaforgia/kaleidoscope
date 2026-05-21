# Slice 01: a metric series is identified by its full label set

British English. No em dashes.

## Goal

One sentence: ingesting two metrics that share a name but differ by
`resource_attributes` keeps them as two distinct series, each carrying
its own labels, on the live path and across a durable restart.

## Stories

- US-01 (walking skeleton): distinct series at ingest and query.
- US-02: those distinct series survive a snapshot + reopen and a
  WAL-only reopen.

Both stories are delivered by ONE change to series identity (the keying
in the shared `apply_ingest` plus the in-memory `ingest`, and the
snapshot bucketing). They are one thin slice because US-02 falls out of
the US-01 fix through the shared recovery path; it is verified
separately because reopen is a distinct code path.

## IN scope

- Series key incorporates `resource_attributes` (full label set: metric
  name + `resource_attributes`), within a tenant.
- Remove the `entry.metric.resource_attributes = ...` overwrite
  (`crates/pulse/src/store.rs` ~line 161, `crates/pulse/src/file_backed.rs`
  ~line 318): a series's resource attributes are identity, not
  refreshable metadata.
- `query(name)` fans out across all series sharing that name within the
  tenant; each returned `(Metric, MetricPoint)` carries its own series'
  `resource_attributes`.
- Snapshot stores one bucket per distinct series (by full label set);
  WAL replay through `apply_ingest` rebuilds the same distinct series.
- Re-ingesting an identical full label set merges into the existing
  series (no duplication), points ascending by `time_unix_nano`.

## OUT of scope

- Label matchers (`query-api-label-matchers-v0`, the dependent feature
  blocked on this).
- Any new query language or filtering.
- Changes to the public `MetricStore` trait signature (`query` already
  returns `Vec<(Metric, MetricPoint)>`, shaped for multiple series).
- Point attributes (already per-point and correct).
- Resource-attribute hoisting to batch level; new point types; any
  migration or compatibility shim (no production data, snapshot format
  may change freely).

## Learning hypothesis

Keying a series by its full label set, applied in the shared
`apply_ingest`, makes per-service provenance survive ingest and durable
recovery. Confirmed if one acceptance run ingests checkout then cart
under one name and gets two distinct, correctly-labelled series back,
and the same two survive a snapshot+reopen and a WAL-only reopen.
Disproved if any path returns a collapsed series or re-merges on
restart.

## Acceptance criteria

Drawn from US-01 and US-02 (see `discuss/user-stories.md`):

- Two metrics with the same name but different `resource_attributes`
  produce two distinct series.
- Querying a name returns every series under it, each pair carrying its
  own `resource_attributes`; no cross-service overwrite.
- Identical full label set across two ingests merges into one series,
  points ascending.
- Point attributes stay per-point and never split or merge a series.
- Distinct series survive snapshot + drop + reopen.
- Distinct series are rebuilt by WAL replay on a no-snapshot reopen.
- A re-ingest after reopen joins the matching recovered series by full
  label set.
- The `MetricStore` trait signature is unchanged.

## Dependencies

- Depends on nothing.
- UNBLOCKS `query-api-label-matchers-v0` (six stashed acceptance
  scenarios resume once this ships).

## Effort and reference class

1-2 days. Reference class: the existing v1 snapshot/recovery slices for
the storage pillars (lumen, ray, pulse `v1_slice_02_snapshot`), which
established the WAL + snapshot + recovery shape this slice reuses. The
change here is to the series KEY, not the recovery discipline, so it
sits well inside that reference class.

## Pre-slice SPIKE

None required. The fix point and the data shapes are verified against
the code (see `discuss/wave-decisions.md`, "Verified-against-code
facts"). The one DESIGN-level open question (the exact series-key type:
a derived key in `metric.rs` versus an inline tuple) is a small,
low-risk DESIGN call, not an uncertainty needing a spike.
