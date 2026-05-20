# Pulse v1 — outcome KPIs

Fourth v0 to v1 carry-forward in the platform plane. The KPI
budgets below carry CI-realism margin from the very first commit.
This is the explicit lesson from the 2026-05-19 timing-bump batch:
the Lumen v1 and Cinder v1 budgets were originally calibrated
against a fast workstation and then failed for roughly two weeks
on GitHub Actions ubuntu-latest before being raised. Pulse v1 does
not repeat that mistake — both latency budgets are set against the
slower CI substrate from day one, matching the post-bump Lumen v1
numbers.

## KPI 1 — Ingest latency

- **Who**: the platform binary embedding Pulse.
- **Does what**: ingests a 100-point `MetricBatch` into the
  durable `FileBackedMetricStore`.
- **By how much**: p95 ≤ 2 ms over 1 000 trials in a debug build.
- **Baseline**: Pulse v0 `InMemoryMetricStore` ingest is well
  under this; v1 adds three durable costs not present in v0 —
  cloning the batch metrics for WAL serialisation, JSON-encoding
  the batch into one NDJSON line, and flushing the `BufWriter` —
  on top of the existing per-series sort-after-extend.
- **Measured by**: `pulse::tests::v1_slice_01_wal_durability::
  ingest_p95_latency_under_two_milliseconds`. Open a fresh WAL in
  a tempdir, warm up with 100 ingests, time 1 000 ingests of a
  100-point batch, read off p95.
- **Why 2 ms and not a sub-millisecond guess**: identical honesty
  move to Lumen v1 KPI 1 (settled at 1.5 ms once the three v1
  costs were measured), Cinder v1 KPI 2, Ray KPI 1 and Aegis
  catalogue-load. The 2 ms budget describes the system that ships
  on the substrate the CI gate actually measures from, not the
  system the architect imagines on a fast workstation. Setting it
  at 2 ms now avoids the two-week CI-failure window that the
  lumen/cinder budgets suffered before the 2026-05-19 bump.

## KPI 2 — Recovery time

- **Who**: the platform binary embedding Pulse.
- **Does what**: calls `FileBackedMetricStore::open(path)` to
  recover state at process startup.
- **By how much**: p95 ≤ 2.5 s when recovering 10 000 points from
  snapshot + WAL in a debug build, over 20 trials.
- **Baseline**: Pulse v0 has no recovery (in-memory only; restart
  loses everything). v1 introduces recovery on the operator-binary
  startup path, so the time must be bounded.
- **Measured by**: `pulse::tests::v1_slice_02_snapshot::
  recovery_p95_latency_under_two_and_a_half_seconds`. Ingest
  10 000 points, call `snapshot()`, ingest 100 more, drop the
  store, time 20 reopens, read off p95.
- **Why 2.5 s and not a sub-second guess**: NDJSON parsing of a
  10 000-point snapshot in debug mode runs ~550 ms on a fast
  workstation but consistently 1500-1700 ms on GitHub Actions
  ubuntu-latest, dominated by `serde_json` token cost. Release
  mode is several times faster; v2's columnar substrate
  (Arrow / Parquet) will obliterate this number. 2.5 s is the
  post-bump Cinder v1 / Lumen v1 figure and is set here from the
  first commit with the CI margin already baked in.
- **CI-realism note (2026-05-19 lesson)**: Cinder v1's recovery
  budget was set at 1 s on 2026-05-04 against a local baseline and
  raised to 2.5 s on 2026-05-19 after sustained CI failures. The
  KPI intent (recovery is bounded — not microseconds-fast, not
  minutes-slow) survives the budget. Pulse adopts 2.5 s up front.

## KPI 3 — Durability completeness

- **Who**: the platform binary embedding Pulse.
- **Does what**: recovers points ingested both before and after a
  `snapshot()` call across a restart.
- **By how much**: 100% of pre-snapshot and post-snapshot points
  survive a drop-and-reopen — zero loss, zero duplication.
- **Baseline**: Pulse v0 survives 0% across restart (in-memory).
- **Measured by**: `pulse::tests::v1_slice_02_snapshot` parallel-
  store comparison — a store that snapshotted mid-stream and a
  store that never did, fed identical points, must return
  identical query results after reopen.
- **Type**: guardrail. This is a correctness invariant, not a
  latency target; it must hold at 100% regardless of the timing
  budgets above.

## Metric hierarchy

- **North Star**: durability completeness (KPI 3) — the whole
  point of the v1 adapter is that metrics survive restart.
- **Leading indicators**: ingest latency (KPI 1) and recovery
  time (KPI 2) — they predict whether durability is usable in a
  long-lived process.
- **Guardrail metrics**: KPI 3 must stay at 100%; KPI 1 and KPI 2
  must not regress past their budgets on CI.

## Out-of-scope (deliberate)

- **Columnar storage** — Arrow / Parquet / DataFusion / Prometheus
  TSDB blocks. v2. v1 ships the same NDJSON-row WAL + JSON snapshot
  precedent as Cinder, Sluice and Lumen; the columnar layout that
  `lib.rs` anticipates is deferred with the same honesty move the
  other three pillars used.
- **Compression** — v1 writes plain NDJSON. v2.
- **Retention policy** — no time-based eviction or downsampling at
  v1. v2.
- **Distributed replication** — single-process, single-WAL-path at
  v1. v2.
- **fsync semantics** — v1 uses `BufWriter::flush`; recovery from
  `kill -9` between flush and fsync is v2.
- **Atomic snapshot rename** — v1 writes the snapshot in-place;
  write-temp-then-rename is v2.
- **File locking** — v1 assumes one process per WAL path; advisory
  locking is v2.
- **Histogram / exponential-histogram / summary points** — v0/v1
  ship gauge + sum number points only; richer point types land
  with the v2 columnar substrate.
