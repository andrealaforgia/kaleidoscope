# Strata v1 — DESIGN wave decisions

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-21.
Mode: propose. Paradigm: Rust idiomatic (data + free functions +
traits only where polymorphism is genuinely needed).

> **Feature**: adds `FileBackedProfileStore` to the `strata` crate as
> a second adapter behind the unchanged `ProfileStore` trait, alongside
> `InMemoryProfileStore`. Durability via NDJSON WAL (one `Ingest`
> record per `ProfileBatch`) + JSON snapshot, a structural
> carry-forward of `crates/pulse/src/file_backed.rs`. The **sixth and
> final** v0 to v1 durable-adapter carry-forward after Cinder v1,
> Sluice v1, Lumen v1, Pulse v1 and Ray v1. With Strata done every
> storage pillar has a durable v1. Released under AGPL-3.0-or-later.

Strata is the simplest of the six: a SINGLE per-service index, no
derived second index to rebuild (unlike Ray), so the Ray precedent
over-covers it and the Pulse single-index precedent maps almost
one-to-one. No new ADR — this is the sixth instance of a settled
pattern; Pulse and Ray added none, and Strata follows.

## Decisions

### DD1 — `open` shape

`FileBackedProfileStore::open<P: AsRef<Path>>(base_path: P, recorder:
Box<dyn MetricsRecorder + Send + Sync>) -> Result<Self,
ProfileStoreError>`, a mirror of `FileBackedMetricStore::open`
(`crates/pulse/src/file_backed.rs:97`). `Inner` holds the single
`per_service: HashMap<(TenantId, ServiceName), Vec<Profile>>` map + the
append `BufWriter<File>`. Implements `ProfileStore` identically to
`InMemoryProfileStore` — a drop-in.
**Rejected**: a new `DurableProfileStore` trait (the trait is already
the right seam — D2-arch); returning `io::Error` (leaks the substrate
across the port — `ProfileStoreError` is the port's error type).

### DD2 — WAL format

NDJSON, one `WalRecord::Ingest { tenant: TenantId, profiles:
Vec<Profile> }` per `ProfileBatch`, internally tagged
`#[serde(tag = "op", rename_all = "snake_case")]`. Each WAL `Profile`
carries its own `resource_attributes` (including `service.name`), so a
record is self-contained for replay. One line appended + flushed per
ingest. Mirrors Pulse/Ray verbatim.

### DD3 — Snapshot

`Snapshot { services: Vec<ServiceBucket> }`, one `ServiceBucket {
tenant, service, profiles }` per `(tenant, service)` key — the single
index serialised **directly**, the simplest snapshot shape of the v1
set (D8). There is no derived second index, so there is no
derived-index invariant to honour and nothing to rebuild beyond the
one map. `snapshot()` flushes the WAL, writes the snapshot, then
re-opens the WAL with `truncate(true)`. Explicit call only (D11);
idempotent; no auto-compaction at v1.

### DD4 — Shared `apply_ingest` over the single map

One free function mirroring Pulse's `apply_ingest`
(`file_backed.rs:297`) but over ONE map. For each profile: drop it if
`service_name()` is empty (the exact v0 `store.rs:122-125` rule —
preserved), else push it under `(tenant, ServiceName)` and record the
touched `ServiceName`. Returns the set of touched service keys
(`Touched` = `HashSet<ServiceName>`, a single set rather than Ray's
two). Live `ingest` and WAL replay call this SAME function, so the
on-disk and in-memory views cannot drift. The live path sorts only the
touched buckets once on `time_unix_nano` (D5a — inherited from the v0
adapter, which already embodies the touched-bucket discipline at
`store.rs:119-137`); recovery calls a `sort_all` pass once (D9 —
cross-batch replay order is not guaranteed sorted).

### DD5 — Plain serde derive across the profile type set

Add `Serialize + Deserialize` derives to `Profile`, `ProfileBatch`,
`Sample`, `Location`, `Function`, `Mapping`, `SampleType`,
`ValueType`, `ServiceName`, `TimeRange`. **No custom serde anywhere.**
Confirmed from `profile.rs:65-157`: there is **no `[u8; N]` and no
`Vec<u8>` field** on any profile type — the pprof payload is fully
structured. The heaviest fields are `Vec<u64>` (`location_ids`,
`function_ids`), `Vec<i64>` (`values`), `Vec<String>`
(`string_table`) and `BTreeMap<String, String>` (the three attribute
maps), all of which serialise as natural JSON arrays/objects. This is
the decisive contrast with Ray, whose `TraceId([u8;16])` /
`SpanId([u8;8])` needed hand-rolled hex; **Strata needs no `hex`
module**. Byte-stability (AC-1.5) holds trivially — the derives are
total and the JSON shape is deterministic. The number-array verbosity
for the numeric vectors is accepted for v1; a compact wire encoding for
the structured vectors is deferred to v2 (D6a, outcome-kpis.md
§ out-of-scope).

### DD6 — Reuse analysis

**REUSE (read path + index semantics, copied verbatim from
`store.rs`):** the single-index shape (`store.rs:87-90`), the
per-service ingest rule including the empty-`service.name` drop
(`store.rs:122-131`), the sort-only-touched-buckets discipline
(`store.rs:119-137`), `query` / `query_with` filter-and-clone logic,
half-open `TimeRange::contains`, `Predicate::matches(&Profile)`, the
`MetricsRecorder` seam (D12 verbatim), `IngestReceipt`. The v1 adapter
reimplements the read path against its own `Inner` (it does NOT wrap an
`InMemoryProfileStore` — Pulse/Ray did not; a wrapped inner would
double the lock and obscure the WAL/index coupling) but copies the
*logic* verbatim.
**EXTEND:** `ProfileStoreError` (+1 variant, DD7); the profile type set
(+serde derives only, DD5); `lib.rs` doc comment (D3 v1/v2 reframing).
**CREATE NEW (durability only):** `WalRecord`, `Snapshot` /
`ServiceBucket`, `open`, `snapshot`, the single-map `apply_ingest`,
`Touched`/`sort_touched`/`sort_all`, `append_wal`, `wal_path_of` /
`snapshot_path_of`, the `io` / `parse` adapters — all structural
mirrors of `pulse/src/file_backed.rs:289-353`. **No new public trait,
no new module beyond `file_backed`, no new external crate, no `hex`
helper.** A new `Error` variant **is** needed (the additive cost paid
by all five prior pillars).

### DD7 — `ProfileStoreError`

The empty never-type enum (`store.rs:35-43`, `match *self {}` Display
idiom) grows to one variant `PersistenceFailed { reason: String }`;
Display is rewritten to match it. Any v0 caller that pattern-matched on
the empty enum needs one explicit arm (flagged to DISTILL). Mirrors
Pulse v1 / Ray v1 / Lumen v1 D4 exactly.

## Quality attribute coverage (ISO 25010)

| Attribute | How addressed |
|---|---|
| Functional Suitability | KPI 3 (guardrail): 100% of pre- and post-snapshot profiles survive drop-and-reopen, zero loss/duplication, including the empty-`service.name` drop (those profiles are intentionally absent from the index pre- and post-recovery). v0 query semantics preserved (half-open range, predicate AND range, ascending `time_unix_nano`). |
| Performance Efficiency | KPI 1 ingest p95 ≤ 8 ms (raised from Ray's 5 ms / Pulse's 2 ms for the heaviest payload of any pillar — D7); KPI 2 recovery p95 ≤ 2.5 s. Both set against the CI substrate from commit one (D13), avoiding the 2026-05-19 fast-workstation trap. Touched-bucket sort keeps ingest off the quadratic re-sort path from the first cut (D5a). |
| Reliability | Recovery is the empirical Earned-Trust probe: reopen replays the WAL through the SAME `apply_ingest` the live path uses, so a divergent recovery cannot silently drift from live state. Corrupt WAL line → `PersistenceFailed` naming the line number (fail-loud). Honest scope: `BufWriter::flush` only (D10); fsync, atomic rename, file locking explicitly v2. |
| Maintainability | One new file mirroring a five-times-proven template; +serde derives only (no custom codec); +1 Error variant. Single index means LESS novelty than Ray (no second-map rebuild). Per-feature mutation testing scoped to the diff at 100% kill rate (ADR-0005 Gate 5) kills any divergent second copy of `apply_ingest`. |
| Compatibility | `ProfileStore` trait unchanged; `FileBackedProfileStore` is a drop-in; existing strata v0 tests untouched. One explicit match arm needed by any v0 caller of the empty `ProfileStoreError`. |
| Portability | No new external crate (`serde` / `serde_json` / `aegis` already present); no platform-specific syscall; std filesystem only. |

## Handoff to DISTILL (`@nw-acceptance-designer`)

Translate US-SV1-01 (AC-1.x) and US-SV1-02 (AC-2.x) into `#[test]`
functions under `crates/strata/tests/v1_slice_01_wal_durability.rs`
and `crates/strata/tests/v1_slice_02_snapshot.rs`, including KPI 1
(ingest p95 ≤ 8 ms) and KPI 2 (recovery p95 ≤ 2.5 s) latency tests and
the KPI 3 durability test. The durability test MUST cover: profiles
survive drop-and-reopen across WAL only AND across snapshot+WAL; the
empty-`service.name` profile is absent both before and after recovery
(the drop is intentional, not a loss). AC-1.5 byte-stability asserts a
serde round-trip over the full structured `Profile` (numeric vectors,
`string_table`, all three attribute maps) — no hex assertion is needed
because there is no byte field. Flag the empty-`ProfileStoreError`
match-arm break to v0 callers.
Required reading: this file; `design/application-architecture.md`;
`crates/pulse/src/file_backed.rs` as the structural template;
`crates/strata/src/store.rs:119-137` as the single-index ingest logic
to mirror.

## Handoff to DEVOPS (`@nw-platform-architect`)

KPIs: KPI 1 (ingest p95 ≤ 8 ms, leading — heaviest payload, D7), KPI 2
(recovery p95 ≤ 2.5 s, leading), KPI 3 (durability completeness,
guardrail — 100%). ADR-0005's five gates apply unchanged (**no
new/amended gate**). Per-feature mutation scope:
`crates/strata/src/file_backed.rs` + touched `store.rs` / `profile.rs`
lines at 100% kill rate (the enforcement that the single `apply_ingest`
has no divergent twin). Cargo delta: two new `[[test]]` blocks
(`v1_slice_01_wal_durability`, `v1_slice_02_snapshot`), **no new
`[dependencies]`** (no `hex`, no `serde_with`). **External
integrations: none** — no HTTP, no webhook, no third-party API, no
vendor SDK; pure local filesystem WAL append + JSON snapshot. No
contract tests apply. DELIVER paradigm: Rust idiomatic (one new struct
+ trait impl, free helper functions including the single-map
`apply_ingest`, serde derives on the profile types, +1 Error variant);
DESIGN collapses into the implementation commit, as with the prior
five v1 adapters.

Action for DELIVER: update the `lib.rs` doc comment (D3) — the v1
deliverable is the durable WAL+snapshot adapter; the columnar
substrate (Arrow / Parquet / DataFusion / RocksDB / gimli-addr2line)
is reframed as v2, consistent with the other five pillars.
