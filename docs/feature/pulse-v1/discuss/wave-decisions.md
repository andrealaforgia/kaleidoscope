# Pulse v1 — DISCUSS wave decisions

## Pre-decided (Andrea's standing instruction)

- **[D1] Feature type: backend storage adapter.** No CLI, no UX
  research beyond lightweight. The "operator" is the platform
  binary embedding Pulse.
- **[D2] No walking skeleton.** Pulse v0 already ships; this is a
  v1 adapter added alongside `InMemoryMetricStore`, not a new
  end-to-end flow.
- **[D3a] UX research depth: lightweight.**
- **[D4a] No JTBD.** The job is obvious — durable metrics survive
  a process restart.

## Key decisions

- **[D2-arch] Same crate, new adapter.** `FileBackedMetricStore`
  joins `InMemoryMetricStore` behind the same `MetricStore` trait.
  No trait changes. The fourth such v0 to v1 carry-forward after
  Cinder v1, Sluice v1 and Lumen v1.

- **[D3] On-disk format: NDJSON WAL with per-batch records, JSON
  snapshot.** One `Ingest` record per `MetricBatch`, matching
  Lumen v1's natural unit (Sluice was per-op, Cinder per-state-
  change). Columnar storage (Arrow / Parquet / DataFusion /
  Prometheus TSDB blocks) is explicitly deferred to v2 — the same
  honesty move the other three pillars used. `lib.rs` already
  anticipates "the v1 columnar + durable adapter"; v1 delivers the
  durable half on the proven NDJSON precedent and leaves the
  columnar half for v2 rather than over-building now.

- **[D4] `MetricStoreError` grows from empty to one variant.** The
  v0 `enum MetricStoreError {}` has a `match *self {}` Display impl
  using the never-type idiom. v1 adds
  `PersistenceFailed { reason: String }` and rewrites Display to
  match it. Any v0 caller that pattern-matched on the empty enum
  needs an explicit arm. Mirrors Lumen v1 D4 exactly.

- **[D5] v0 metric types must derive Serialize + Deserialize.**
  `Metric`, `MetricPoint`, `MetricKind`, `MetricName`,
  `MetricBatch`, `TimeRange` and the `BTreeMap<String, String>`
  attribute maps need serde derives for WAL / snapshot
  round-tripping. `f64` values serialise as JSON numbers; the
  byte-stable round-trip AC (AC-1.5) covers exact value recovery.

- **[D6] Recovery re-sorts every series bucket.** The snapshot may
  hold points in any order and WAL `Ingest` records carry batches
  that may be individually unordered. Recovery re-sorts each
  `(tenant, metric_name)` bucket once on `time_unix_nano` before
  exposing state, preserving the v0 ascending-time query contract.

- **[D7] `BufWriter::flush` semantics**, same as Cinder v1, Sluice
  v1 and Lumen v1. fsync is v2.

- **[D8] Explicit `snapshot()` call**, same as the other v1s. No
  auto-compaction at v1.

- **[D9] Recorder / metrics hook posture: carry forward verbatim.**
  The `MetricsRecorder` seam from v0 is reused unchanged;
  `FileBackedMetricStore` records ingest / query exactly as
  `InMemoryMetricStore` does. No new observability surface in v1.

- **[D10] KPI budgets carry CI-realism margin from commit one.**
  Ingest p95 ≤ 2 ms, recovery p95 ≤ 2.5 s, matching post-bump
  Lumen v1 / Cinder v1. This is the explicit lesson of the
  2026-05-19 timing-bump batch: lumen/cinder were calibrated
  against a fast workstation and failed on GitHub Actions
  ubuntu-latest for ~two weeks before being raised. Pulse does not
  repeat that. See outcome-kpis.md.

- **[D11] AGPL-3.0-or-later.**

- **[D12] Two carpaccio slices.** Slice 01 WAL durability, Slice 02
  snapshot compaction. Each ~1 day, each demonstrable via a single
  `cargo test` invocation. DESIGN collapses into the implementation
  commit.

## Slicing

- **Slice 01 — WAL durability** (US-PV1-01)
- **Slice 02 — snapshot compaction** (US-PV1-02)

## Out of scope (v2)

Columnar storage, compression, retention policy, distributed
replication, fsync, atomic snapshot rename, file locking,
auto-triggered compaction, histogram / exponential-histogram /
summary point types. See outcome-kpis.md § Out-of-scope for the
full list and rationale.

## Risks

- **DIVERGE artifacts absent.** No `docs/feature/pulse-v1/diverge/`
  recommendation or job-analysis exists. Acceptable here: the job
  is obvious (D4a) and the design is a verbatim carry-forward of a
  pattern proven three times. Noted as a low risk, not a blocker.

## DESIGN handoff

DESIGN collapses into the implementation commit, as with the prior
three v1 adapters. DISTILL writes
`crates/pulse/tests/v1_slice_01_wal_durability.rs` and
`crates/pulse/tests/v1_slice_02_snapshot.rs`; DELIVER writes
`crates/pulse/src/file_backed.rs`.
