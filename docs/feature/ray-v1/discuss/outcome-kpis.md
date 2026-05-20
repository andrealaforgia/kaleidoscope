# Ray v1 — outcome KPIs

Fifth v0 to v1 carry-forward in the platform plane. The KPI
budgets below carry CI-realism margin from the very first commit.
This is the explicit lesson from the 2026-05-19 timing-bump batch:
the Lumen v1 and Cinder v1 budgets were originally calibrated
against a fast workstation and then failed for roughly two weeks
on GitHub Actions ubuntu-latest before being raised. Ray v1 does
not repeat that mistake — both latency budgets are set against the
slower CI substrate from day one, matching the post-bump Pulse v1
numbers.

## KPI 1 — Ingest latency

- **Who**: the platform binary embedding Ray.
- **Does what**: ingests a 100-span `SpanBatch` into the durable
  `FileBackedTraceStore`.
- **By how much**: p95 ≤ 2 ms over 1 000 trials in a debug build.
- **Baseline**: Ray v0 `InMemoryTraceStore` ingest is well under
  this; v1 adds three durable costs not present in v0 — cloning
  the batch spans for WAL serialisation, JSON-encoding the batch
  into one NDJSON line, and flushing the `BufWriter` — on top of
  the existing dual-index sort-after-push. Ray's dual-index ingest
  is slightly heavier than Pulse's single-series ingest because
  each span is split into two buckets, but the durable costs
  dominate and the 2 ms budget holds.
- **Measured by**: `ray::tests::v1_slice_01_wal_durability::
  ingest_p95_latency_under_two_milliseconds`. Open a fresh WAL in
  a tempdir, warm up with 100 ingests, time 1 000 ingests of a
  100-span batch, read off p95.
- **Why 2 ms and not a sub-millisecond guess**: identical honesty
  move to Pulse v1 KPI 1, Lumen v1 KPI 1, Cinder v1 KPI 2 and
  Aegis catalogue-load. The 2 ms budget describes the system that
  ships on the substrate the CI gate actually measures from, not
  the system the architect imagines on a fast workstation. Setting
  it at 2 ms now avoids the two-week CI-failure window that the
  lumen/cinder budgets suffered before the 2026-05-19 bump.

## KPI 2 — Recovery time

- **Who**: the platform binary embedding Ray.
- **Does what**: calls `FileBackedTraceStore::open(path)` to
  recover state at process startup.
- **By how much**: p95 ≤ 2.5 s when recovering 10 000 spans from
  snapshot + WAL in a debug build, over 20 trials.
- **Baseline**: Ray v0 has no recovery (in-memory only; restart
  loses everything). v1 introduces recovery on the operator-binary
  startup path, so the time must be bounded. Recovery rebuilds
  both indices, which is more work than a single flat list, but
  the dominant cost is `serde_json` token parsing, not index
  insertion.
- **Measured by**: `ray::tests::v1_slice_02_snapshot::
  recovery_p95_latency_under_two_and_a_half_seconds`. Ingest
  10 000 spans, call `snapshot()`, ingest 100 more, drop the
  store, time 20 reopens, read off p95.
- **Why 2.5 s and not a sub-second guess**: NDJSON / JSON parsing
  of a 10 000-span snapshot in debug mode is dominated by
  `serde_json` token cost and runs several times faster in release
  mode; v2's columnar substrate (Arrow / Parquet) will obliterate
  this number. 2.5 s is the post-bump Pulse v1 / Cinder v1 / Lumen
  v1 figure and is set here from the first commit with the CI
  margin already baked in.
- **CI-realism note (2026-05-19 lesson)**: Cinder v1's recovery
  budget was set at 1 s on 2026-05-04 against a local baseline and
  raised to 2.5 s on 2026-05-19 after sustained CI failures. The
  KPI intent (recovery is bounded — not microseconds-fast, not
  minutes-slow) survives the budget. Ray adopts 2.5 s up front.

## KPI 3 — Durability completeness

- **Who**: the platform binary embedding Ray.
- **Does what**: recovers spans ingested both before and after a
  `snapshot()` call across a restart.
- **By how much**: 100% of pre-snapshot and post-snapshot spans
  survive a drop-and-reopen — zero loss, zero duplication, across
  both the trace index and the service index.
- **Baseline**: Ray v0 survives 0% across restart (in-memory).
- **Measured by**: `ray::tests::v1_slice_02_snapshot` parallel-
  store comparison — a store that snapshotted mid-stream and a
  store that never did, fed identical spans, must return identical
  `get_trace` and `query` results after reopen.
- **Type**: guardrail. This is a correctness invariant, not a
  latency target; it must hold at 100% regardless of the timing
  budgets above.

## Metric hierarchy

- **North Star**: durability completeness (KPI 3) — the whole
  point of the v1 adapter is that spans survive restart.
- **Leading indicators**: ingest latency (KPI 1) and recovery
  time (KPI 2) — they predict whether durability is usable in a
  long-lived process.
- **Guardrail metrics**: KPI 3 must stay at 100%; KPI 1 and KPI 2
  must not regress past their budgets on CI.

## Out-of-scope (deliberate)

- **Columnar storage** — Arrow / Parquet / DataFusion /
  trace_id-partitioned Iceberg blocks. v2. v1 ships the same
  NDJSON-row WAL + JSON snapshot precedent as Cinder, Sluice,
  Lumen and Pulse; the columnar layout that `lib.rs` anticipates
  is deferred with the same honesty move the other four pillars
  used.
- **Compression** — v1 writes plain NDJSON. v2.
- **Retention policy** — no time-based eviction or trace
  downsampling at v1. v2.
- **Distributed replication** — single-process, single-WAL-path at
  v1. v2.
- **fsync semantics** — v1 uses `BufWriter::flush`; recovery from
  `kill -9` between flush and fsync is v2.
- **Atomic snapshot rename** — v1 writes the snapshot in-place;
  write-temp-then-rename is v2.
- **File locking** — v1 assumes one process per WAL path; advisory
  locking is v2.
- **Trace-aware compaction** — no span deduplication, no
  parent-child reconciliation, no late-span stitching at v1; the
  WAL replay reconstructs whatever was ingested. v2.
