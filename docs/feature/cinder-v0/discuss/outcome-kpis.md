# Cinder v0 — outcome KPIs

## KPI 1 — Tier-metadata lookup

- **What**: `TieringStore::get_tier(tenant, item)` p95 ≤
  50 µs over 10 000 placed items on the in-memory adapter.
- **Why**: Cinder sits on every read path: storage engines
  consult it before going to disk. If the lookup is slow,
  Cinder becomes the bottleneck.
- **Measured by**: `cinder::tests::slice_01_walking_skeleton::
  get_tier_p95_latency_under_fifty_microseconds`. Place
  10 000 items across tenants. Warm up 50 lookups. Time
  1 000 lookups. Read off p95.
- **Target**: 50 µs p95 over 1 000 trials.

## KPI 2 — Lifecycle evaluation

- **What**: `TieringStore::evaluate_at(now, &policy)` p95
  ≤ 5 ms over 10 000 placed items.
- **Why**: The evaluator runs periodically (every few
  minutes) inside the operator binary. It must not dominate
  CPU. v1's columnar substrate will keep this cheap via an
  age-index; v0's linear pass is the v0 ceiling.
- **Measured by**: `cinder::tests::slice_02_lifecycle::
  evaluate_p95_latency_under_five_milliseconds`. Place
  10 000 items with varied ages, time 200 calls to
  `evaluate_at`, read off p95. The first call moves a lot
  of items; subsequent calls (idempotent) cost only the
  scan.
- **Target**: 5 ms p95 over 200 trials.

## Out-of-scope (deliberate)

- **Physical substrate** (S3 / OpenDAL / Iceberg) — v1.
- **Retention deletion / TTL** — v1.
- **Cross-tier query coordination** — v1; the storage
  engines look up the tier from Cinder and then fetch from
  the appropriate substrate themselves.
- **Tier-aware compaction** — v1.
- **Manifest format** — v1.
- **Operator-binary timer wiring** — v1; v0 exposes the
  pure `evaluate_at(now, &policy)` method and the caller
  controls when to invoke it.
