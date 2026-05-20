# Story Map: pulse-v1

## User: the platform binary embedding Pulse (the "operator")

## Goal: ingested metrics survive a process restart, recoverable in bounded time

Lightweight wave: Pulse v0 already ships, so there is no walking
skeleton to draw (D2). This is a v1 adapter added alongside the
existing `InMemoryMetricStore`. The map mirrors the two-slice
shape of Lumen v1.

## Backbone

| Ingest durably | Recover on startup | Compact the log | Stay bounded |
|----------------|--------------------|-----------------|--------------|
| Append batch to WAL | Replay WAL into per-series state | Write snapshot, truncate WAL | Recover snapshot + tail WAL |
| Update in-memory series | Re-sort each series on time | Snapshot is idempotent | Parallel-store equality check |
| Empty batch is a no-op | Corrupt WAL fails loudly | | |

---

## Slices

### Slice 01 — WAL durability (US-PV1-01)

The durability floor: every ingest appends an NDJSON `Ingest`
record to the WAL; `open` replays the WAL and re-sorts each
`(tenant, metric_name)` series. Drop, reopen, query, see the same
points. Covers the first three backbone activities at their top
rib (append, replay, isolation, ordering, corrupt-WAL handling).
Targets KPI 1 (ingest latency ≤ 2 ms p95) and the first half of
KPI 3 (pre-snapshot points survive).

### Slice 02 — snapshot compaction (US-PV1-02)

The bounded-recovery layer: `snapshot()` writes the full per-series
state and truncates the WAL; recovery loads the snapshot then
replays the tail WAL. Keeps recovery time bounded as the process
runs indefinitely. Targets KPI 2 (recovery ≤ 2.5 s p95) and the
second half of KPI 3 (post-snapshot points also survive; snapshot
+ WAL equals pure WAL).

## Priority Rationale

1. **Slice 01 first** — durability is the riskiest assumption and
   the prerequisite for everything else. Without WAL recovery there
   is nothing to compact. Highest outcome impact: it is the
   behaviour the whole feature exists to deliver (KPI 3 north star).
2. **Slice 02 second** — depends on Slice 01's recovery path
   existing. It does not add new durability, it bounds the cost of
   the durability Slice 01 already provides. Lower urgency: a
   long-lived process degrades gracefully (slower recovery) without
   it, so it is value-additive rather than fatal-assumption work.

Ordering follows Value x Urgency / Effort with the tie-break rule
Riskiest Assumption (Slice 01) > value-additive (Slice 02).

## Scope Assessment: PASS — 2 stories, 1 bounded context (pulse crate), estimated ~1 day each

No oversized signals: 2 user stories, one crate, no cross-context
integration, two thin end-to-end slices each demonstrable via a
single `cargo test` invocation. Mirrors the right-sized Lumen v1
shape exactly.
