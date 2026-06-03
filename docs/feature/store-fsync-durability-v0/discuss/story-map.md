# Story Map: store-fsync-durability-v0

## User: Priya Nair, on-call SRE running a Kaleidoscope collector

## Goal: After a power loss or OS crash, restart the collector and still have every write it acknowledged as durable — and have the store open cleanly even if the crash hit mid-snapshot.

## Backbone

The operator's end-to-end durability journey. Each activity is a phase
of the crash-and-recover lifecycle the operator lives through.

| Acknowledge a write | Survive the crash | Reopen the store | Read back the data | Trust the result |
|---------------------|-------------------|------------------|--------------------|-----------------|
| Collector accepts a record and returns 2xx | Power loss / `kill -9` mid-write | Collector restarts, store `open()` runs | Operator queries the read API | Operator confirms acked data is present |
| WAL append `sync_all` puts bytes on disk | Crash may hit mid-snapshot | Torn-tail tolerated (ADR-0059) | Acked-before-crash record returned | A proving test demonstrates it, not the README |
| Snapshot written atomically (tmp+rename+fsync) | Crash may hit mid-WAL-record | Atomic snapshot is whole or absent, never torn | Store opens even after mid-snapshot crash | `event=health.startup.refused` if substrate lies |

---

## Walking Skeleton — lumen, end-to-end crash-durable AND proven

The thinnest slice that connects ALL five backbone activities for ONE
pillar (lumen, the logs pillar — its read path `GET /api/v1/logs` is the
easiest crash-survival outcome to observe). This slice proves the whole
pattern: the fsync discipline, the atomic snapshot, the kill-9 proving
test, and the operator-visible outcome.

The walking skeleton is **one task from each activity**:

- **Acknowledge a write** → lumen WAL append calls `sync_all` per record
  (reusing the proven `FsyncBackend` seam from pulse/ADR-0049).
- **Survive the crash** → lumen snapshot written atomically
  (write-tmp, fsync-tmp, rename, fsync-parent-dir).
- **Reopen the store** → lumen `open()` recovers the durable prefix
  (ADR-0059 torn-tail tolerance already lands lumen; this slice produces
  the genuine torn tail it recovers from).
- **Read back the data** → `GET /api/v1/logs` returns the
  acked-before-crash record.
- **Trust the result** → **the kill-9 proving test**: a real
  out-of-process crash mid-write (and a second variant mid-snapshot),
  reopen, assert the acked write is queryable and the store opens. This
  is the false-confidence fix and the single highest-value deliverable.

The walking skeleton is **US-01** (see `user-stories.md`).

---

## Carpaccio slices (one store per slice; each a thin vertical end-to-end repeat of the proven pattern)

Each subsequent slice applies the pattern proven in the walking skeleton
to one more store. Each is independently shippable and independently
verifiable (its own kill-9 proving test on that store's read or reopen
path). Ordering is by operator-visible blast radius and read-path
observability.

| Slice | Store | Pillar / role | WAL fsync | Atomic snapshot | Proving test |
|-------|-------|---------------|-----------|-----------------|--------------|
| **US-01 (WS)** | **lumen** | logs (read path `GET /api/v1/logs`) | add `sync_all` per record | add tmp+rename+fsync | **kill-9 mid-write + mid-snapshot, reopen, query** |
| US-02 | ray | traces (read path `GET /api/v1/traces`) | add `sync_all` per record | add tmp+rename+fsync | kill-9 mid-write + mid-snapshot, reopen, query |
| US-03 | strata | profiles | add `sync_all` per record | add tmp+rename+fsync | kill-9 mid-write + mid-snapshot, reopen |
| US-04 | cinder | tiering/migration | add `sync_all` per record | add tmp+rename+fsync | kill-9 mid-write + mid-snapshot, reopen |
| US-05 | sluice | queue (fallible `apply_record`) | add `sync_all` per record | add tmp+rename+fsync | kill-9 mid-write + mid-snapshot, reopen |
| US-06 | beacon state_store | rule-state | add `sync_all` per record | add tmp+rename+fsync | kill-9 mid-write + mid-snapshot, reopen |
| US-07 | pulse | metrics | **already done (ADR-0049)** | add tmp+rename+fsync **(snapshot-only — the gap ADR-0049 left)** | kill-9 mid-snapshot, reopen |

> US-07 (pulse) is **snapshot-only**: pulse's WAL is already
> `sync_all`-per-record under ADR-0049, but its snapshot still uses
> `File::create` straight onto the canonical path
> (`crates/pulse/src/file_backed.rs:257`), so a mid-snapshot crash tears
> the file pulse's own per-file `sync_all` cannot save. This slice closes
> the snapshot-atomicity gap ADR-0049 left open even in its own pillar.

---

## Priority Rationale

Priority order is driven by outcome impact (operator-visible blast
radius) and the riskiest-assumption-first discipline, not by feature
grouping or effort.

1. **US-01 (lumen) — Walking Skeleton — P1.** Validates the entire
   pattern end-to-end on the pillar with the most observable read path
   (`GET /api/v1/logs`). The riskiest assumption is "an out-of-process
   kill-9 proving test can deterministically demonstrate crash-survival
   in CI without `fork()`-in-tokio hazards" — this is the assumption that
   could kill the whole feature, so it is validated FIRST, in the
   skeleton. If the proving test cannot be made deterministic, every
   later slice is at risk; we learn that here, cheaply, on one store.
   Once US-01 lands, slices US-02..US-07 are mechanical repeats with the
   crash mechanism already proven.

2. **US-02 (ray) — P2.** Second-most-observable read path
   (`GET /api/v1/traces`). Traces are the second pillar an operator
   inspects after a crash. Confirms the pattern generalises across a
   second store with an HTTP read path.

3. **US-03 (strata), US-04 (cinder), US-05 (sluice), US-06 (beacon
   state) — P3.** Internal-state stores with no direct HTTP read path;
   their outcome is "the store opens cleanly after a mid-write/mid-snapshot
   crash and the acked prefix is present on reopen." Lower observability
   than the logs/traces read paths but equal correctness importance.
   sluice (US-05) carries the fallible-`apply_record` nuance (ADR-0059
   §5) and is sequenced after the infallible-apply stores so the simpler
   shape is proven first. Ordering within P3 is by data-loss blast radius
   (strata/cinder hold larger durable state than the beacon rule-state).

4. **US-07 (pulse snapshot atomicity) — P3.** Pulse's WAL is already
   crash-durable (ADR-0049); only the snapshot-atomicity gap remains.
   Smaller change (snapshot-only, no WAL work) and pulse already owns the
   `FsyncBackend` seam, so it is low-effort. Sequenced late because the
   WAL gap (the larger silent-loss surface) on the other six stores is
   the higher-impact correctness fix; pulse's residue is a mid-snapshot
   total-loss window that is real but narrower (only fires during the
   periodic snapshot, not on every acked write).

### Value × Urgency / Effort

| Slice | Value (outcome impact) | Urgency | Effort | Priority |
|-------|-----------------------|---------|--------|----------|
| US-01 lumen (WS) | 5 (proves pattern + most observable) | 5 (derisks the fatal assumption) | 4 (new crash mechanism) | **P1** |
| US-02 ray | 4 (second HTTP read path) | 3 | 2 (pattern proven) | P2 |
| US-03 strata | 4 (large durable state) | 3 | 2 | P3 |
| US-04 cinder | 4 (large durable state) | 3 | 2 | P3 |
| US-05 sluice | 3 (queue state, fallible apply) | 3 | 3 (fallible-apply seam) | P3 |
| US-06 beacon state | 3 (rule-state) | 3 | 2 | P3 |
| US-07 pulse snapshot | 3 (narrow mid-snapshot window) | 2 | 1 (snapshot-only, seam owned) | P3 |

Tie-break order applied: Walking Skeleton (US-01) > riskiest assumption
(US-01 again, the crash mechanism) > highest value (US-02 ray).

---

## Scope Assessment: Elephant Carpaccio Gate

Assessed against the oversized signals (any 2+ → oversized):

| Signal | Threshold | This feature | Over? |
|--------|-----------|--------------|-------|
| User stories | > 10 | 7 | No |
| Bounded contexts / crates touched | > 3 | 7 crates (lumen, ray, strata, cinder, sluice, beacon, pulse) | **Yes** |
| Walking skeleton integration points | > 5 | 1 (lumen end-to-end) | No |
| Estimated effort | > 2 weeks | ~7 thin slices | borderline |
| Independent shippable outcomes | multiple | 7 (each store ships alone) | **Yes** |

Two signals trip (7 crates; 7 independent outcomes). **The remediation
is already the structure of this story map**: the feature is sliced into
7 independent, thin, vertical, per-store deliverables, each shippable and
verifiable alone. This is the Elephant Carpaccio split itself — not one
big feature but seven thin slices that each deliver a working,
operator-verifiable behaviour (one store made genuinely crash-durable and
proven). The walking skeleton (US-01) is exactly one store end-to-end, so
no slice touches more than its own crate plus the shared `FsyncBackend`
seam already extracted in ADR-0049.

**Decision: PASS as a pre-split carpaccio.** The 7-crate breadth is
inherent to the defect (the same residue exists in 7 stores) and is
handled by slicing, not by reducing scope. Each slice is right-sized
(1 store, 3-7 scenarios, demonstrable in one session). No further split
needed; no user confirmation required (autonomous run, and the split is
the natural per-store shape the brief mandated).
