# Augur v0 — outcome KPIs

## KPI 1 — Numeric observation latency

- **What**: `ZScoreObserver::observe(tenant, value,
  observed_at)` p95 ≤ 10 µs per call after warm-up.
- **Why**: Augur sits inline with every Pulse data point.
  The cost of `observe` must be tiny — Welford's algorithm
  is O(1) per sample, but allocations or unnecessary work
  on the hot path would still hurt.
- **Measured by**: `augur::tests::slice_01_zscore::
  observe_p95_latency_under_ten_microseconds`. Warm up the
  observer with 1 000 samples. Time 10 000 `observe` calls
  with mixed values. Read off p95.
- **Target**: 10 µs p95 over 10 000 trials.

## KPI 2 — Categorical observation latency

- **What**: `RareEventObserver::observe(tenant, event,
  observed_at)` p95 ≤ 20 µs per call on a vocabulary of
  up to 1 000 distinct events after warm-up.
- **Why**: Augur runs inline with Lumen log ingest. A
  1 000-distinct-bodies vocabulary is the realistic
  per-service load.
- **Measured by**: `augur::tests::slice_02_rare_event::
  observe_p95_latency_under_twenty_microseconds`. Seed
  1 000 distinct events into the observer, time 5 000
  cycled-event observations, read off p95.
- **Target**: 20 µs p95 over 5 000 trials.

## Out-of-scope (deliberate)

- **BOCPD / proper change-point detection** — v1.
- **Embedding-based log clustering** (sentence-
  transformers) — v1.
- **LLM summarisation** (Qwen / Mistral via vLLM /
  llama.cpp) — v1.
- **Beacon integration** — v1.
- **Rolling-window evaluation** — v1.
- **Multi-variate anomaly detection** — v1.
- **Persistence of observer state** — v1.
