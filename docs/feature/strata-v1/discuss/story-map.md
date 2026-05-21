# Story Map: strata-v1

## User: the platform binary embedding Strata (the "operator")

## Goal: ingested profiles survive a process restart, recoverable in bounded time

Lightweight wave: Strata v0 already ships, so there is no walking
skeleton to draw (D2). This is a v1 adapter added alongside the
existing `InMemoryProfileStore`. The map mirrors the two-slice
shape of Pulse v1 and Ray v1. Strata is the sixth and final
storage pillar to gain a durable v1.

## Backbone

| Ingest durably | Recover on startup | Compact the log | Stay bounded |
|----------------|--------------------|-----------------|--------------|
| Append batch to WAL | Replay WAL into per-service index | Write snapshot, truncate WAL | Recover snapshot + tail WAL |
| Update index (touched-bucket sort) | Re-sort buckets on time | Snapshot is idempotent | Parallel-store equality check |
| Empty batch is a no-op | Corrupt WAL fails loudly | | |

---

## Slices

### Slice 01 — WAL durability (US-SV1-01)

The durability floor: every ingest appends an NDJSON `Ingest`
record to the WAL; `open` replays the WAL and rebuilds the single
`per_service` index, re-sorting each bucket on `time_unix_nano`.
Drop, reopen, query by service and range, see the same profiles
with full payload. Covers the first three backbone activities at
their top rib (append, single-index replay, isolation, ordering,
empty-service-name drop, corrupt-WAL handling). Targets KPI 1
(ingest latency ≤ 8 ms p95) and the first half of KPI 3
(pre-snapshot profiles survive).

### Slice 02 — snapshot compaction (US-SV1-02)

The bounded-recovery layer: `snapshot()` writes the full
per-service index state and truncates the WAL; recovery loads the
snapshot then replays the tail WAL. Keeps recovery time bounded as
the process runs indefinitely — especially important here because
the heavy profile payload grows the WAL fast. Targets KPI 2
(recovery ≤ 2.5 s p95) and the second half of KPI 3 (post-snapshot
profiles also survive; snapshot + WAL equals pure WAL).

## Priority Rationale

1. **Slice 01 first** — durability is the riskiest assumption and
   the prerequisite for everything else. Without WAL recovery there
   is nothing to compact. Highest outcome impact: it is the
   behaviour the whole feature exists to deliver (KPI 3 north star).
2. **Slice 02 second** — depends on Slice 01's recovery path
   existing. It does not add new durability, it bounds the cost of
   the durability Slice 01 already provides. The heavy profile
   payload makes bounded recovery more valuable than for the lighter
   pillars, but it is still value-additive rather than fatal-
   assumption work: a long-lived process degrades gracefully
   (slower recovery) without it.

Ordering follows Value x Urgency / Effort with the tie-break rule
Riskiest Assumption (Slice 01) > value-additive (Slice 02).

## Scope Assessment: PASS — 2 stories, 1 bounded context (strata crate), estimated ~1 day each

No oversized signals: 2 user stories, one crate, no cross-context
integration, two thin end-to-end slices each demonstrable via a
single `cargo test` invocation. Strata's single per-service index
is in fact the SIMPLEST of the v1 set (simpler than Ray's dual
index) — the only distinguishing factor is payload weight, which
is a KPI-budget concern, not a scope expansion. Mirrors the
right-sized Pulse v1 shape.
