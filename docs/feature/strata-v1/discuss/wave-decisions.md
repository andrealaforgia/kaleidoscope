# Strata v1 — DISCUSS wave decisions

## Pre-decided (Andrea's standing instruction)

- **[D1] Feature type: backend storage adapter.** No CLI, no UX
  research beyond lightweight. The "operator" is the platform
  binary embedding Strata.
- **[D2] No walking skeleton.** Strata v0 already ships; this is a
  v1 adapter added alongside `InMemoryProfileStore`, not a new
  end-to-end flow.
- **[D3a] UX research depth: lightweight.**
- **[D4a] No JTBD.** The job is obvious — durable profiles survive
  a process restart.

## Key decisions

- **[D2-arch] Same crate, new adapter.** `FileBackedProfileStore`
  joins `InMemoryProfileStore` behind the same `ProfileStore`
  trait. No trait changes. The sixth and final such v0 to v1
  carry-forward after Cinder v1, Sluice v1, Lumen v1, Pulse v1 and
  Ray v1. With Strata done, every storage pillar — lumen, pulse,
  ray, strata, sluice, cinder — has a durable v1.

- **[D3] On-disk format: NDJSON WAL with per-batch records, JSON
  snapshot.** One `Ingest` record per `ProfileBatch`, matching the
  natural unit used by the prior five pillars. Columnar storage
  (Arrow / Parquet / DataFusion / RocksDB / gimli-addr2line
  symbolisation) is explicitly deferred to v2 — the same honesty
  move the other five pillars used. `lib.rs` already anticipates
  "the v1 columnar adapter (Arrow + Parquet + DataFusion + RocksDB
  + gimli/addr2line symbolisation)"; v1 here delivers the durable
  half on the proven NDJSON precedent and leaves the columnar half
  for v2 rather than over-building now. (The lib.rs doc comment
  calls the columnar adapter "v1"; this DISCUSS reframes the
  durable WAL+snapshot adapter as the v1 deliverable and the
  columnar substrate as v2, consistent with the other five
  pillars. DESIGN should update the lib.rs wording.)

- **[D4] `ProfileStoreError` grows from empty to one variant.** The
  v0 `enum ProfileStoreError {}` has a `match *self {}` Display
  impl using the never-type idiom (store.rs lines 34-43). v1 adds
  `PersistenceFailed { reason: String }` and rewrites Display to
  match it. Any v0 caller that pattern-matched on the empty enum
  needs an explicit arm. Mirrors Pulse v1 D4 / Ray v1 D4 / Lumen v1
  D4 exactly.

- **[D5] Index model — SINGLE per-service index (confirmed from
  store.rs).** Strata v0 keeps ONE index, the simplest of the v1
  set:
  - `per_service: HashMap<(TenantId, ServiceName), Vec<Profile>>`
    sorted by `time_unix_nano` — serves `query` / `query_with`.

  This is confirmed at store.rs lines 87-90 (`InnerState` /
  `per_service`) and restated in lib.rs line 44 ("Single index").
  It is closest to Pulse v1's single keyed-series model and
  simpler than Ray's dual index — there is no second index to
  rebuild. Profiles whose `service.name` resource attribute is
  empty are **dropped** from the index on ingest (store.rs lines
  122-125); v1 preserves this exactly.

  Consequence for the v1 adapter: WAL replay rebuilds the one
  index through a shared split routine that mirrors
  `InMemoryProfileStore::ingest` and is the same code path the
  live `ingest` uses, so the two cannot drift.

- **[D5a] Touched-bucket sort from the first cut (Ray's lesson,
  inherited not relearned).** The v0 `InMemoryProfileStore::ingest`
  already tracks the set of touched service buckets and sorts only
  those exactly once at the end of the batch (store.rs lines
  119-137), keeping ingest off the quadratic re-sort-everything
  path. v1 carries this forward from the first commit: the shared
  split routine returns the touched buckets and the live ingest
  path sorts only those. Recovery sorts all buckets once via a
  sort-all pass (replay order across batches is not guaranteed
  sorted). Ray learned the touched-bucket discipline the hard way
  during DELIVER; Strata inherits it because the v0 adapter already
  embodies it.

- **[D6] v0 profile types must derive Serialize + Deserialize.** As
  of v0, `Profile`, `ProfileBatch`, `Sample`, `Location`,
  `Function`, `Mapping`, `SampleType`, `ValueType`, `ServiceName`
  and `TimeRange` derive only `Debug`, `Clone`, `PartialEq`, `Eq`
  (plus `Hash` / `PartialOrd` / `Ord` / `Copy` / `Default` on
  some — see profile.rs). **None derive serde today.** v1 adds
  `Serialize` + `Deserialize` derives across this type set for
  WAL / snapshot round-tripping. The `BTreeMap<String, String>`
  attribute maps (`resource_attributes`, `attributes`, per-sample
  `attributes`) serialise naturally as JSON objects. The numeric
  vectors (`location_ids: Vec<u64>`, `values: Vec<i64>`) serialise
  as JSON number arrays — verbose but byte-stable, which is the
  only hard requirement (AC-1.5).

- **[D6a] Profile payload byte field — there is NONE; the weight
  is structured, not a blob.** A natural concern for a profiles
  pillar is a single large `Vec<u8>` sample blob, where `serde`'s
  default derive would emit a JSON number array (one integer per
  byte) and a base64 / hex string representation would be wanted
  instead. **Strata v0 has no such field.** The pprof payload is
  fully structured: `samples: Vec<Sample>`, `locations`,
  `functions`, `mappings`, `string_table: Vec<String>`, and the
  attribute maps (profile.rs lines 114-146). There is no raw byte
  blob to base64-encode. Therefore plain serde derive is the
  correct and accepted v1 choice for every field — no custom
  `serialize_with` is needed. The weight (which drives KPI 1, see
  D7) comes from the *number* of structured entries, not from a
  byte array. A more compact wire encoding for these structured
  vectors is deferred to v2 (outcome-kpis.md § out-of-scope).

- **[D7] Profile is the heaviest payload of any pillar — KPI 1
  budget set with eyes open.** Read off the v0 field set: a
  `MetricPoint` (Pulse) is light (2 ms sufficed), a `Span` (Ray)
  is heavier (needed 5 ms), and a `Profile` is heavier still —
  hundreds-to-thousands of `Sample`s each with vectors and an
  attribute map, plus the supporting pprof tables and a sizeable
  `string_table`. Serialising 100 profiles per batch into one
  NDJSON line is materially more work than 100 spans. KPI 1 is
  therefore set at p95 ≤ 8 ms (vs Ray's 5 ms, Pulse's 2 ms) from
  the first commit, with the reasoning stated in outcome-kpis.md
  § KPI 1. Better to set it right from DISCUSS than bump at
  DELIVER — the explicit 2026-05-19 timing-bump lesson.

- **[D8] Snapshot serialises the single index directly.** Because
  there is only one index and no derived second index (unlike Ray's
  by_service), the snapshot writes the per-service buckets straight
  out and recovery reads them straight back. Simplest snapshot
  shape of the v1 set. See slice-02. DESIGN may revisit the exact
  bucket grouping, but there is no derived-index invariant to
  honour here.

- **[D9] Recovery re-sorts every bucket.** The snapshot may hold
  profiles in any order and WAL `Ingest` records carry batches that
  may be individually unordered. Recovery re-sorts each service
  bucket once on `time_unix_nano` before exposing state, preserving
  the v0 ascending-time query contract.

- **[D10] `BufWriter::flush` semantics**, same as the other five
  pillars. fsync is v2.

- **[D11] Explicit `snapshot()` call**, same as the other v1s. No
  auto-compaction at v1.

- **[D12] Recorder / metrics hook posture: carry forward
  verbatim.** The `MetricsRecorder` seam from v0 is reused
  unchanged; `FileBackedProfileStore` records ingest / query
  exactly as `InMemoryProfileStore` does. No new observability
  surface in v1.

- **[D13] KPI budgets carry CI-realism margin from commit one.**
  Ingest p95 ≤ 8 ms (raised from the lighter pillars for payload
  weight, D7), recovery p95 ≤ 2.5 s (matching post-bump Pulse v1 /
  Ray v1 / Lumen v1 / Cinder v1). See outcome-kpis.md.

- **[D14] AGPL-3.0-or-later.**

- **[D15] Two carpaccio slices.** Slice 01 WAL durability, Slice 02
  snapshot compaction. Each ~1 day, each demonstrable via a single
  `cargo test` invocation. DESIGN collapses into the implementation
  commit.

## Slicing

- **Slice 01 — WAL durability** (US-SV1-01)
- **Slice 02 — snapshot compaction** (US-SV1-02)

## Out of scope (v2)

Columnar storage (Arrow / Parquet / DataFusion / RocksDB /
gimli-addr2line symbolisation), compression, retention policy,
distributed replication, fsync, atomic snapshot rename, file
locking, auto-triggered compaction, sample-payload encoding
optimisation, sample / location / function predicates. See
outcome-kpis.md § Out-of-scope for the full list and rationale.

## Risks

- **DIVERGE artifacts absent.** No `docs/feature/strata-v1/diverge/`
  recommendation or job-analysis exists. Acceptable here: the job
  is obvious (D4a) and the design is a verbatim carry-forward of a
  pattern proven five times. Noted as a low risk, not a blocker.
- **Payload-weight KPI risk.** The 8 ms ingest budget is set from
  the field set rather than a measurement (DISCUSS designs no
  implementation). If DELIVER measures materially higher on a
  realistic profile corpus, the budget is the item to revisit — but
  it is deliberately set high precisely to avoid the 2026-05-19
  fast-workstation trap. Low-to-medium probability, low impact
  (a KPI-doc bump, not a design change). Flagged for DELIVER.

## DESIGN handoff

DESIGN collapses into the implementation commit, as with the prior
five v1 adapters. DISTILL writes
`crates/strata/tests/v1_slice_01_wal_durability.rs` and
`crates/strata/tests/v1_slice_02_snapshot.rs`; DELIVER writes
`crates/strata/src/file_backed.rs`. The two items DESIGN must carry
beyond the Pulse precedent: the heavy-payload KPI 1 budget
reasoning (D7) and the lib.rs v1/v2 reframing (D3). The index model
(D5, single per-service) is in fact simpler than Ray's, so the Ray
precedent over-covers it.
