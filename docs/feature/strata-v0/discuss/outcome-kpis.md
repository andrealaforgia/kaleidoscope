# Strata v0 — outcome KPIs

## KPI 1 — Ingest latency

- **What**: `ProfileStore::ingest(tenant, batch_of_10)` p95 ≤
  5 ms on the in-memory adapter.
- **Why**: Profiles are large — kilobytes per sample, often
  hundreds of samples per profile (so KB to MB total). The
  realistic OTLP batch shape for profiles is ~10 profiles
  per batch, not the 100 records used for logs / metrics /
  spans.
- **Why 5 ms not 1 ms** (matching Lumen / Pulse): profile
  cloning is fundamentally more expensive than span cloning.
  Each profile carries a string table, a location index, a
  function index, a mapping index, and a vector of samples.
  Cloning all of this into the in-memory adapter is the
  dominant cost. The v1 columnar substrate stores the string
  table once per tier, deduplicates locations / functions
  across profiles, and pays this cost only at compaction
  time.
- **Measured by**: `strata::tests::slice_01_walking_skeleton::
  ingest_p95_latency_under_five_milliseconds`. Warm up with
  20 ingests, time 200 ingests of 10-profile batches.
- **Target**: 5 ms p95 over 200 trials.

## KPI 2 — Query latency under predicate

- **What**: `ProfileStore::query_with(tenant, service, range,
  predicate)` p95 ≤ 10 ms when scanning 1 000 ingested
  profiles.
- **Why**: Riley's "show me CPU profiles for checkout in the
  last 30 min" must answer in under a second end-to-end.
  v1's columnar substrate tightens dramatically; the v0
  adapter's linear scan is the v0 ceiling.
- **Measured by**: `strata::tests::slice_02_structured_query::
  query_p95_latency_under_ten_milliseconds`. Ingest 1 000
  profiles across mixed profile_types, time 200 queries
  with a `profile_type` predicate.
- **Target**: 10 ms p95 over 200 trials.

## Out-of-scope (deliberate)

- **Disk durability** (v1)
- **Symbolisation** (v1 with gimli + addr2line)
- **Flame graph rendering** (Prism v1)
- **Diff flame graphs** (Prism v1)
- **Cross-pillar exemplars** (v1)
- **OpenTelemetry Profiles signal spec** — actively evolving
  upstream; Strata tracks pprof at v0 (the de facto format)
  and aligns with the OTel signal at v1.
