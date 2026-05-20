# Ray v1 — DISCUSS wave decisions

## Pre-decided (Andrea's standing instruction)

- **[D1] Feature type: backend storage adapter.** No CLI, no UX
  research beyond lightweight. The "operator" is the platform
  binary embedding Ray.
- **[D2] No walking skeleton.** Ray v0 already ships; this is a v1
  adapter added alongside `InMemoryTraceStore`, not a new
  end-to-end flow.
- **[D3a] UX research depth: lightweight.**
- **[D4a] No JTBD.** The job is obvious — durable spans survive a
  process restart.

## Key decisions

- **[D2-arch] Same crate, new adapter.** `FileBackedTraceStore`
  joins `InMemoryTraceStore` behind the same `TraceStore` trait.
  No trait changes. The fifth such v0 to v1 carry-forward after
  Cinder v1, Sluice v1, Lumen v1 and Pulse v1.

- **[D3] On-disk format: NDJSON WAL with per-batch records, JSON
  snapshot.** One `Ingest` record per `SpanBatch`, matching Lumen
  v1 and Pulse v1's natural unit. Columnar storage
  (Arrow / Parquet / DataFusion / trace_id-partitioned Iceberg) is
  explicitly deferred to v2 — the same honesty move the other four
  pillars used. `lib.rs` already anticipates "the v1 columnar
  (trace_id-partitioned Iceberg-on-Parquet) adapter"; v1 delivers
  the durable half on the proven NDJSON precedent and leaves the
  columnar half for v2 rather than over-building now.

- **[D4] `TraceStoreError` grows from empty to one variant.** The
  v0 `enum TraceStoreError {}` has a `match *self {}` Display impl
  using the never-type idiom. v1 adds
  `PersistenceFailed { reason: String }` and rewrites Display to
  match it. Any v0 caller that pattern-matched on the empty enum
  needs an explicit arm. Mirrors Pulse v1 D4 / Lumen v1 D4 exactly.

- **[D5] Index model — DUAL INDEX (the key finding for DESIGN).**
  Ray v0 differs from every prior v1 pillar. `store.rs` keeps two
  indices populated from the same spans on ingest:
  - `by_trace: HashMap<(TenantId, TraceId), Vec<Span>>` — serves
    `get_trace`.
  - `by_service: HashMap<(TenantId, ServiceName), Vec<Span>>` —
    serves `query` / `query_with`.

  Spans are **cloned into both** maps on ingest; a span with an
  empty `service.name` resource attribute lands only in
  `by_trace`. This is neither Lumen's flat per-tenant `extend` nor
  Pulse's single `(tenant, metric_name)` keyed-series split.

  Consequence for the v1 adapter: WAL replay must reconstruct
  **both** indices, so it routes through a shared split routine
  (the Pulse `apply_ingest` pattern, generalised to two maps)
  rather than a flat list append. That shared routine must be the
  same code path the live `ingest` uses, so the two cannot drift —
  this is the single most important shape constraint DESIGN
  carries forward. Both buckets are re-sorted on
  `start_time_unix_nano` after replay to preserve the v0 ordering
  contract.

- **[D6] Snapshot stores spans once, rebuilds the service index on
  recovery.** Rather than serialising both maps (which would
  duplicate every span on disk, paying the v0 in-memory 2× cost in
  the file), the snapshot stores spans once — naturally grouped by
  `(tenant, trace_id)` — and the `by_service` index is rebuilt
  from those spans on recovery via the same split routine, since
  each span already carries its own `service.name`. This keeps the
  snapshot file compact and makes the on-disk format index-shape-
  agnostic, which eases the v2 columnar migration. DESIGN may
  revisit the exact snapshot grouping, but the invariant is: the
  service index is derived, never independently persisted.

- **[D7] v0 span types must derive Serialize + Deserialize.** As
  of v0, `Span`, `SpanBatch`, `TraceId`, `SpanId`, `ServiceName`,
  `SpanKind`, `StatusCode`, `SpanStatus`, `SpanEvent`, `SpanLink`
  and `TimeRange` derive only `Debug`, `Clone`, `PartialEq`, `Eq`
  (plus `Hash` / `Ord` / `Copy` / `Default` on some). **None
  derive serde today.** v1 adds `Serialize` + `Deserialize`
  derives across this type set for WAL / snapshot round-tripping.
  `TraceId([u8; 16])` and `SpanId([u8; 8])` need attention: a raw
  `[u8; N]` serde derive produces a JSON array of integers, which
  round-trips byte-stable but is verbose; DESIGN may opt for a hex
  representation, but byte-stability (AC-1.5) is the only hard
  requirement. The `BTreeMap<String, String>` attribute maps
  serialise naturally.

- **[D8] Recovery re-sorts every bucket.** The snapshot may hold
  spans in any order and WAL `Ingest` records carry batches that
  may be individually unordered. Recovery re-sorts each trace
  bucket and each service bucket once on `start_time_unix_nano`
  before exposing state, preserving the v0 ascending-time query
  contract.

- **[D9] `BufWriter::flush` semantics**, same as Cinder v1, Sluice
  v1, Lumen v1 and Pulse v1. fsync is v2.

- **[D10] Explicit `snapshot()` call**, same as the other v1s. No
  auto-compaction at v1.

- **[D11] Recorder / metrics hook posture: carry forward
  verbatim.** The `MetricsRecorder` seam from v0 is reused
  unchanged; `FileBackedTraceStore` records ingest / query exactly
  as `InMemoryTraceStore` does. No new observability surface in v1.

- **[D12] KPI budgets carry CI-realism margin from commit one.**
  Ingest p95 ≤ 2 ms, recovery p95 ≤ 2.5 s, matching post-bump
  Pulse v1 / Lumen v1 / Cinder v1. This is the explicit lesson of
  the 2026-05-19 timing-bump batch: lumen/cinder were calibrated
  against a fast workstation and failed on GitHub Actions
  ubuntu-latest for ~two weeks before being raised. Ray does not
  repeat that. See outcome-kpis.md.

- **[D13] AGPL-3.0-or-later.**

- **[D14] Two carpaccio slices.** Slice 01 WAL durability, Slice 02
  snapshot compaction. Each ~1 day, each demonstrable via a single
  `cargo test` invocation. DESIGN collapses into the implementation
  commit.

## Slicing

- **Slice 01 — WAL durability** (US-RV1-01)
- **Slice 02 — snapshot compaction** (US-RV1-02)

## Out of scope (v2)

Columnar storage, compression, retention policy, distributed
replication, fsync, atomic snapshot rename, file locking,
auto-triggered compaction, trace-aware compaction (dedup, late-span
stitching). See outcome-kpis.md § Out-of-scope for the full list
and rationale.

## Risks

- **DIVERGE artifacts absent.** No `docs/feature/ray-v1/diverge/`
  recommendation or job-analysis exists. Acceptable here: the job
  is obvious (D4a) and the design is a verbatim carry-forward of a
  pattern proven four times. Noted as a low risk, not a blocker.
- **Dual-index drift risk.** Ray's two indices (D5) introduce the
  one novel hazard versus the prior v1s: the live ingest path and
  the WAL-replay path could populate the two maps inconsistently.
  Mitigation: a single shared split routine used by both paths.
  Medium probability if implemented twice, low impact if the
  shared-routine constraint is honoured. Flagged for DESIGN.

## DESIGN handoff

DESIGN collapses into the implementation commit, as with the prior
four v1 adapters. DISTILL writes
`crates/ray/tests/v1_slice_01_wal_durability.rs` and
`crates/ray/tests/v1_slice_02_snapshot.rs`; DELIVER writes
`crates/ray/src/file_backed.rs`. The dual-index finding (D5) and
the derived-service-index snapshot decision (D6) are the two items
DESIGN must carry that the Pulse precedent does not cover.
