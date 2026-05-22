# Upstream issue — DELIVER discovered a Pulse data-model gap

**Date:** 2026-05-21
**Wave:** DELIVER (slice 01, equality and inequality matchers)
**Status:** BLOCKED on a prerequisite; query-api work stashed, not lost.

## What happened

The query-api side of label matching was implemented correctly: the
parser extends the bare-name selector to `name{label="v", other!="x"}`,
and a pure filter in `matrix.rs` keeps each row whose derived label set
satisfies every matcher (ANDed, absent-as-empty Prometheus semantics).
Thirty-seven unit tests pass and clippy is clean. Thirteen of the
nineteen slice-01 acceptance scenarios go green.

The remaining six fail for a reason that lives entirely beneath
query-api, in the `pulse` crate.

## Root cause

Pulse keys each metric series by `(tenant, metric_name)` alone and, on
every ingest, overwrites the stored `resource_attributes` with the
latest batch (`crates/pulse/src/store.rs` around the `ingest` series
upsert; the same shape in `crates/pulse/src/file_backed.rs`). The inline
comment is explicit that v0 treats `resource_attributes` as refreshable
*metadata* rather than as part of the series identity.

The consequence: two metrics that share a name but differ by
`service.name` (checkout vs cart vs search) collapse into a single
series, and only the last-ingested `service.name` survives. Per-service
provenance is destroyed at ingest, before query-api ever sees a row.
No amount of correct filtering downstream can recover information that
was already discarded.

This is not specific to label matchers. Any query of a metric that
exists across multiple services is already wrong today; it returns one
arbitrary service's labels for all points. Label matching is simply the
first consumer that makes the gap visible.

## Decision (Andrea absent, decided autonomously)

The fix is a Pulse data-model correction: a series is identified by its
full label set, not by its name alone. That touches Pulse's in-memory
series map and its snapshot format, and it carries its own standalone
user value, so it is **not** folded into this feature's DELIVER as a
quiet patch. It is run as its own focused nWave feature,
`pulse-series-identity-v0`, with its own ADR, ahead of completing this
slice.

Once Pulse preserves per-label-set series, the query-api work resumes
from `git stash` (message references crafter agent
`ad50219470188a558`) and the six remaining scenarios are expected to go
green with no further query-api change.

## Not done

- query-api changes are stashed, not committed. The Iron Rule held: no
  acceptance test was weakened to force a pass.
- No cross-crate change was made under this feature's scope.

## Resolved (2026-05-22)

The prerequisite feature `pulse-series-identity-v0` shipped (commit
5ea579b): a Pulse series is now identified by its full label set and
`query` fans out, returning one row per distinct series with its own
`resource_attributes`. With that foundation, the stashed query-api work
was restored and the DELIVER completed (commit 0171388). All six
previously-failing scenarios passed as-is, with no production fix and no
weakened test, because the filter finally had distinct per-series labels
to work on. The block is closed.
