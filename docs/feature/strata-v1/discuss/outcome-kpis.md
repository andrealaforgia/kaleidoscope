# Strata v1 — outcome KPIs

Sixth and final v0 to v1 storage carry-forward in the platform
plane. The KPI budgets below carry CI-realism margin from the very
first commit — the explicit lesson of the 2026-05-19 timing-bump
batch, where Lumen v1 and Cinder v1 budgets were calibrated against
a fast workstation and failed on GitHub Actions ubuntu-latest for
roughly two weeks before being raised. Strata does not repeat that.
Strata's one distinguishing factor is payload weight: a `Profile`
is the heaviest payload of any pillar, so KPI 1 is set deliberately
higher than the lighter pillars, with the reasoning stated below.

## KPI 1 — Ingest latency

- **Who**: the platform binary embedding Strata.
- **Does what**: ingests a 100-profile `ProfileBatch` into the
  durable `FileBackedProfileStore`.
- **By how much**: p95 ≤ 8 ms over 1 000 trials in a debug build.
- **Baseline**: Strata v0 `InMemoryProfileStore` ingest is well
  under this; v1 adds three durable costs not present in v0 —
  cloning the batch profiles for WAL serialisation, JSON-encoding
  the batch into one NDJSON line, and flushing the `BufWriter` — on
  top of the existing per-service touched-bucket sort.
- **Measured by**: `strata::tests::v1_slice_01_wal_durability::
  ingest_p95_latency_under_eight_milliseconds`. Open a fresh WAL in
  a tempdir, warm up with 50 ingests, time 1 000 ingests of a
  100-profile batch, read off p95.
- **Why 8 ms and not Ray's 5 ms or Pulse's 2 ms** — the payload
  weight is the whole story, and it is read directly off the v0
  `Profile` field set in `profile.rs`:
  - A `MetricPoint` (Pulse) is a timestamp, a value, and a small
    attribute map. Light. 2 ms sufficed.
  - A `Span` (Ray) adds nested events, links, status and two
    attribute maps. Heavier. Needed 5 ms.
  - A `Profile` (Strata) is heavier still. Each profile carries
    `samples: Vec<Sample>` where every `Sample` is a stack
    (`location_ids: Vec<u64>`, `values: Vec<i64>`, an attribute
    `BTreeMap`), PLUS the supporting pprof tables — `locations`,
    `functions`, `mappings`, and a `string_table: Vec<String>`
    holding every name / unit / filename / build-id — PLUS two
    resource/profile attribute maps. A realistic cpu or heap
    profile has hundreds to thousands of samples and a sizeable
    string table. Serialising 100 such profiles into a single
    NDJSON line is materially more JSON-encoding work per batch
    than 100 spans, let alone 100 metric points.
  - The 8 ms ceiling reflects that payload weight plus the GitHub
    Actions ubuntu-latest IO + serialise variance (roughly twice
    the local-workstation figure). Setting it correctly now, from
    the field set rather than from a copied lighter-pillar number,
    is exactly the discipline the 2026-05-19 timing-bump batch
    taught. Better to set it right from DISCUSS than bump it at
    DELIVER.
  - v2's columnar adapter (Arrow / Parquet) changes the
    serialisation cost profile entirely and this ceiling is
    expected to drop; v1 is the row-oriented NDJSON precedent.

## KPI 2 — Recovery time

- **Who**: the platform binary embedding Strata.
- **Does what**: calls `FileBackedProfileStore::open(path)` to
  recover state at process startup.
- **By how much**: p95 ≤ 2.5 s when recovering 2 000 profiles from
  snapshot + WAL in a debug build, over 20 trials.
- **Baseline**: Strata v0 has no recovery (in-memory only; restart
  loses everything). v1 introduces recovery on the operator-binary
  startup path, so the time must be bounded. Recovery rebuilds the
  single per-service index; the dominant cost is `serde_json` token
  parsing of the heavy profile payloads, not index insertion.
- **Representative count**: 2 000 profiles, not Ray's 10 000 spans.
  Because each profile is a far heavier payload, 2 000 profiles is
  a representative recovery load that exercises the parse path
  honestly without inflating the count. The budget is on the parse
  cost of a realistic profile corpus, not on a span-count parity
  with Ray.
- **Measured by**: `strata::tests::v1_slice_02_snapshot::
  recovery_p95_latency_under_two_and_a_half_seconds`. Ingest 2 000
  profiles, call `snapshot()`, ingest 100 more, drop the store,
  time 20 reopens, read off p95.
- **Why 2.5 s and not a sub-second guess**: JSON parsing of a
  2 000-heavy-profile snapshot in debug mode is dominated by
  `serde_json` token cost and runs several times faster in release
  mode; v2's columnar substrate will obliterate this number. 2.5 s
  is the post-bump Pulse v1 / Ray v1 / Cinder v1 / Lumen v1 figure
  and is set here from the first commit with the CI margin already
  baked in.
- **CI-realism note (2026-05-19 lesson)**: Cinder v1's recovery
  budget was set at 1 s on 2026-05-04 against a local baseline and
  raised to 2.5 s on 2026-05-19 after sustained CI failures. The
  KPI intent (recovery is bounded — not microseconds-fast, not
  minutes-slow) survives the budget. Strata adopts 2.5 s up front.

## KPI 3 — Durability completeness

- **Who**: the platform binary embedding Strata.
- **Does what**: recovers profiles ingested both before and after a
  `snapshot()` call across a restart.
- **By how much**: 100% of pre-snapshot and post-snapshot profiles
  survive a drop-and-reopen — zero loss, zero duplication, with the
  full sample payload of every profile intact.
- **Baseline**: Strata v0 survives 0% across restart (in-memory).
- **Measured by**: `strata::tests::v1_slice_02_snapshot` parallel-
  store comparison — a store that snapshotted mid-stream and a
  store that never did, fed identical profiles, must return
  identical `query` results after reopen.
- **Type**: guardrail. This is a correctness invariant, not a
  latency target; it must hold at 100% regardless of the timing
  budgets above.

## Metric hierarchy

- **North Star**: durability completeness (KPI 3) — the whole point
  of the v1 adapter is that profiles survive restart with their
  full payload.
- **Leading indicators**: ingest latency (KPI 1) and recovery time
  (KPI 2) — they predict whether durability is usable in a
  long-lived process given the heavy payload.
- **Guardrail metrics**: KPI 3 must stay at 100%; KPI 1 and KPI 2
  must not regress past their budgets on CI.

## Out-of-scope (deliberate)

- **Columnar storage** — Arrow / Parquet / DataFusion / RocksDB /
  gimli-addr2line symbolisation, as anticipated in lib.rs. v2. v1
  ships the same NDJSON-row WAL + JSON snapshot precedent as the
  other five pillars.
- **Compression** — v1 writes plain NDJSON; given the heavy payload
  this is the most obvious v2 lever, but deferred. v2.
- **Retention policy** — no time-based eviction or profile
  downsampling at v1. v2.
- **Distributed replication** — single-process, single-WAL-path at
  v1. v2.
- **Sample-payload encoding optimisation** — v1 serialises the
  pprof tables as plain serde-derived JSON. A more compact on-wire
  encoding for the sample/location/function vectors is a v2
  optimisation. v1 prioritises correctness and the proven
  precedent over byte efficiency.
- **fsync semantics** — v1 uses `BufWriter::flush`; recovery from
  `kill -9` between flush and fsync is v2.
- **Atomic snapshot rename** — v1 writes the snapshot in-place;
  write-temp-then-rename is v2.
- **File locking** — v1 assumes one process per WAL path; advisory
  locking is v2.
- **Sample / location / function predicates** — predicate.rs notes
  these "land at v1 with the columnar substrate (they are
  expensive on a linear scan)". v1 here is the durable adapter
  only; the columnar substrate that makes those predicates cheap is
  still v2. v1 carries forward exactly the v0 `profile_type`
  predicate.
