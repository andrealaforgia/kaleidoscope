# Slice 01: a metric series is identified by its full label set

British English. No em dashes.

This is the story map's Release 1: a single thin end-to-end slice covering US-01 and US-02,
because both are delivered by one change to series identity in the shared `apply_ingest`. US-02
falls out of the US-01 fix through the shared recovery path and is verified separately only
because reopen is a distinct code path.

## Goal

Ingesting two metrics that share a name but differ by `resource_attributes` keeps them as two
distinct series, each carrying its own labels, on the live path and across a durable restart.

## IN scope

- Series key incorporates `resource_attributes` (full label set: metric name +
  `resource_attributes`), scoped within a tenant.
- Remove the `entry.metric.resource_attributes = ...` overwrite (`crates/pulse/src/store.rs`
  ~line 161; `crates/pulse/src/file_backed.rs` ~line 318): a series's resource attributes are
  identity, not refreshable metadata.
- `query(name)` fans out across all series sharing that name within the tenant; each returned
  `(Metric, MetricPoint)` carries its own series' `resource_attributes`.
- Snapshot stores one bucket per distinct series (by full label set); WAL replay through
  `apply_ingest` rebuilds the same distinct series.
- Re-ingesting an identical full label set merges into the existing series (no duplication),
  points ascending by `time_unix_nano`.

## OUT of scope

- Label matchers (`query-api-label-matchers-v0`, the dependent feature blocked on this).
- Any new query language or filtering.
- Changes to the public `MetricStore` trait signature (`query` already returns
  `Vec<(Metric, MetricPoint)>`, shaped for multiple series).
- Point attributes (already per-point and correct).
- Resource-attribute hoisting to batch level; new point types; any migration or compatibility
  shim (no production data, snapshot format may change freely).

## Learning hypothesis

Keying a series by its full label set, applied in the shared `apply_ingest`, makes per-service
provenance survive ingest and durable recovery. Confirmed if one acceptance run ingests
checkout then cart under one name and gets two distinct, correctly-labelled series back, and the
same two survive a snapshot+reopen and a WAL-only reopen. Disproved if any path returns a
collapsed series or re-merges the two on restart.

## Acceptance criteria

Eight, drawn from US-01 and US-02 (see `discuss/user-stories.md`):

1. Two metrics with the same name but different `resource_attributes` produce two distinct
   series.
2. Querying a name returns every series under it, each pair carrying its own
   `resource_attributes`; no later ingest overwrites another service's labels.
3. Two ingests of an identical full label set merge into one series (no duplication), points
   ascending by `time_unix_nano`.
4. Point-level attributes stay per-point and never split or merge a series.
5. The `MetricStore` trait signature is unchanged.
6. Two distinct series survive `snapshot()` + drop + reopen, correctly labelled.
7. Two distinct series are rebuilt, correctly labelled, by WAL replay on a no-snapshot reopen.
8. A re-ingest after reopen joins the matching recovered series by full label set, not name
   alone.

## Dependencies

- Depends on nothing. The ingest, query, WAL, and snapshot seams already exist; their plumbing
  is corrected, not added.
- UNBLOCKS `query-api-label-matchers-v0` (six stashed acceptance scenarios resume once this
  ships).

## Effort estimate and reference class

1-2 days. Reference class: the existing v1 snapshot/recovery slices for the storage pillars
(lumen, ray, pulse `v1_slice_02_snapshot`), which established the WAL + snapshot + recovery
shape this slice reuses. The change here is to the series KEY, not the recovery discipline, so
it sits well inside that reference class.

## Pre-slice SPIKE

None required. The fix point and the data shapes are verified against the code
(`discuss/wave-decisions.md`, "Verified-against-code facts"). The one DESIGN-level open question
(the exact series-key type: a derived key in `metric.rs` versus an inline tuple) is a small,
low-risk DESIGN call, not an uncertainty needing a spike.

## Carpaccio taste tests

- **End-to-end?** Yes: ingest -> identity -> persist/recover -> query, exercised against a real
  `FileBackedMetricStore`. Not a horizontal layer.
- **Demonstrable in one session?** Yes: one walking-skeleton scenario (US-01 happy path)
  extended across a restart (US-02). Eight ACs, two right-sized stories of <= 1 day each.
- **Delivers a verifiable behaviour change?** Yes: a multi-service metric returns one correct
  series per service where today it returns one collapsed series.
- **Thinnest viable?** Yes: a live-only fix would be a half-fix on a durable store, so recovery
  is in; label matchers and new point types are out.
