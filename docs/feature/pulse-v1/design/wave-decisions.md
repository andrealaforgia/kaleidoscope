# Pulse v1 — DESIGN wave decisions

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-20.
Mode: Propose. Paradigm: Rust idiomatic per `CLAUDE.md`.

> **Feature**: add `FileBackedMetricStore` to the `pulse` crate as a
> second adapter behind the existing `MetricStore` trait, sitting
> alongside `InMemoryMetricStore`. Durability is a verbatim
> carry-forward of the NDJSON-WAL + JSON-snapshot pattern proven by
> Cinder v1, Sluice v1 and Lumen v1. No trait change. The fourth
> v0 to v1 carry-forward in the platform plane. AGPL-3.0-or-later.

The decision is to mirror `crates/lumen/src/file_backed.rs`
structurally, substituting the metric domain types and the
per-`(tenant, metric_name)` series index for Lumen's per-tenant
record buckets. DISCUSS already locked the substantive choices
(D2-arch, D3-D12); DESIGN reifies them into adapter-level decisions
DD1..DD6 and collapses into the implementation commit, exactly as
Lumen v1 did.

## Principal architectural decisions

### DD1 — `FileBackedMetricStore::open(path, recorder)` shape

```
pub fn open<P: AsRef<Path>>(
    base_path: P,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
) -> Result<Self, MetricStoreError>
```

Byte-for-byte mirror of `FileBackedLogStore::open`
(`file_backed.rs:86-141`). The struct holds `base_path: PathBuf`,
`recorder: Box<dyn MetricsRecorder + Send + Sync>`, and
`state: Mutex<Inner>` where `Inner` carries the in-memory series
index plus the append `BufWriter<File>`. It implements the
`MetricStore` trait identically to `InMemoryMetricStore`, so it is a
drop-in: the embedding binary swaps the constructor and nothing else.
Rejected: a new `DurableMetricStore` trait (no caller needs the
distinction; the port already abstracts durability); a builder (one
required path + one recorder do not earn one); returning `io::Error`
(breaks the typed-error port contract — see DD-Error).

### DD2 — WAL append format (NDJSON, one line per `MetricBatch`)

One `Ingest` WAL record per ingested `MetricBatch`, matching Lumen
v1's natural unit (Sluice was per-op, Cinder per-state-change).
serde shape, internally tagged on `op`, snake_case, mirroring
`WalRecord` at `file_backed.rs:43-50`:

```
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Ingest { tenant: TenantId, metrics: Vec<Metric> },
}
```

Each line is `serde_json::to_string(&record)` + `\n`, flushed via
`BufWriter::flush` (D7; fsync is v2). The carried `Vec<Metric>` is
the batch's metrics with their `points` populated (unlike the
in-memory index, the WAL line keeps points inside each `Metric` for
a self-contained replay record). This requires the v0 metric types
to derive `Serialize + Deserialize` (DISCUSS D5): `Metric`,
`MetricPoint`, `MetricKind`, `MetricName`, `MetricBatch`, `TimeRange`.
`f64` values serialise as JSON numbers; AC-1.5 byte-stable round-trip
covers exact recovery.

### DD3 — Snapshot (full state to JSON, truncate WAL)

`snapshot()` mirrors `FileBackedLogStore::snapshot`
(`file_backed.rs:145-179`): lock state, serialise the full in-memory
index to a `Snapshot { series: Vec<SeriesBucket> }` JSON file, flush
the WAL, write the snapshot file, then re-open the WAL with
`truncate(true)`. The snapshot bucket carries the canonical `Metric`
metadata (with empty `points`) plus the sorted `Vec<MetricPoint>`,
preserving the v0 metadata/data separation
(`store.rs:104-107`):

```
#[derive(Serialize, Deserialize)]
struct SeriesBucket {
    tenant: TenantId,
    metric_name: MetricName,
    metric: Metric,            // points empty; metadata only
    points: Vec<MetricPoint>,  // sorted on time_unix_nano
}
```

Explicit call only; no auto-compaction at v1 (DISCUSS D8). Idempotent:
a second `snapshot()` with no intervening ingest produces no
duplication (Slice 02 AC).

### DD4 — Recovery on open (snapshot then WAL replay then re-sort)

`open` mirrors `file_backed.rs:94-141`: if the snapshot file exists,
load it and seed the `(tenant, metric_name)` index; then if the WAL
file exists, replay each `Ingest` line on top, folding its metrics'
points into the matching series entry and refreshing the canonical
metadata exactly as `InMemoryMetricStore::ingest` does
(`store.rs:138-153`). After replay, re-sort every series'
`points` once on `time_unix_nano` (DISCUSS D6) to preserve the v0
ascending-time query contract. A corrupt WAL line surfaces as
`MetricStoreError::PersistenceFailed` naming the offending line
number (mirrors `file_backed.rs:112-115`). Snapshot-first + WAL-tail
recovery yields identical results to pure-WAL recovery (KPI 3,
parallel-store equality).

### DD5 — Reuse Verdict (RCA F-1)

**REUSE (read path + index semantics):** the per-`(tenant,
metric_name)` `SeriesEntry` index shape (`store.rs:104-107`), the
metadata/data separation, the sort-on-ingest discipline, the
`query` / `query_with` filter-and-clone logic
(`store.rs:160-211`), the half-open `TimeRange::contains` contract,
the `Predicate::matches(&Metric, &MetricPoint)` composition, the
`MetricsRecorder` seam (D9, carried forward verbatim), the
`IngestReceipt` return shape, the empty-batch no-op. The v1 adapter
reimplements the read path against its own `Inner` (it does not wrap
an `InMemoryMetricStore` instance — Lumen v1 did not, and a wrapped
inner would double the lock and obscure the WAL/index coupling), but
the *logic* is copied verbatim from the v0 adapter.

**EXTEND:** `MetricStoreError` grows from empty to one variant
(DD-Error). The six v0 metric types gain serde derives (DD2/D5).

**CREATE NEW (durability only):** the file I/O — `WalRecord`,
`Snapshot`/`SeriesBucket` serde structs, `open`, `snapshot`,
`append_wal`, `wal_path_of`/`snapshot_path_of` helpers, the
`io`/`parse` error adapters — all structural mirrors of
`file_backed.rs:253-287`. **No new public trait, no new module
beyond `file_backed`, no new external crate** (`serde`/`serde_json`
already in the workspace).

Verdict: the v1 adapter REUSES the v0 index + query logic + predicate
matching by faithful copy, and ADDS durability (file I/O + serde) on
the write and recovery paths. Strictly the Lumen v1 shape.

### DD-Error — `MetricStoreError` grows to one variant

The v0 `enum MetricStoreError {}` with its never-type `match *self {}`
Display impl (`store.rs:35-43`) gains exactly one variant, mirroring
Lumen v1 D4 (`file_backed.rs` uses `LogStoreError::PersistenceFailed`):

```
pub enum MetricStoreError {
    PersistenceFailed { reason: String },
}
```

Display is rewritten from the empty match to a single arm. **Yes, a
new Error variant is needed** — it is the additive cost the pattern
has paid three times before (Cinder, Sluice, Lumen). Any v0 caller
that pattern-matched the empty enum needs an explicit arm; the
DISTILL handoff flags this. WAL parse, snapshot parse, and all
`std::io::Error` paths funnel through `io`/`parse` adapters into this
single variant — no second variant is warranted at v1 (file-locking,
fsync, atomic-rename failures are all v2).

## ADR decision

**No new ADR.** Mirrors the lumen-v1 decision: lumen-v1 produced no
DESIGN-wave ADR (verified — no `docs/feature/lumen-v1/design/` exists
and no `adr-*` references the durable-adapter pattern as novel). The
durable file-backed adapter is a *settled property of the
methodology* after three identical applications; it warrants no new
ADR. The existing `serde`/NDJSON-WAL precedent (ADR-0039 on the CLI
side, the Cinder/Sluice/Lumen adapters on the storage side) already
governs the `OpenOptions::create(true).append(true)` +
`BufWriter::flush` posture. A pulse-specific ADR would duplicate
without adding a decision.

## DEVOPS handoff annotation

Recipient: `@nw-platform-architect`. Receives KPI 1 (ingest p95 ≤
2 ms, leading), KPI 2 (recovery p95 ≤ 2.5 s, leading), KPI 3
(durability completeness 100%, North Star guardrail) — all three
calibrated against the CI substrate from commit one (DISCUSS D10,
outcome-kpis.md). ADR-0005's five gates apply unchanged (**no new,
no amended gate**); per-feature mutation testing scoped to
`crates/pulse/src/file_backed.rs` + the touched `store.rs`/`metric.rs`
lines, 100% kill rate per ADR-0005 Gate 5 / `CLAUDE.md`. Cargo delta:
two new `[[test]]` blocks (`v1_slice_01_wal_durability`,
`v1_slice_02_snapshot`); **no new `[dependencies]`** (`serde`,
`serde_json`, `aegis` already present). **External integrations:
none** — no HTTP, no webhook, no third-party API, no vendor SDK;
pure local filesystem WAL append + JSON snapshot on an
operator-supplied path. No contract tests apply. DELIVER paradigm:
Rust idiomatic (one new struct + its trait impl, free helper
functions, two serde structs, one additive `Error` variant; no
class-style inheritance, no `dyn` boundary beyond the pre-existing
`Box<dyn MetricsRecorder + Send + Sync>`).

## Earned-Trust note (principle 12)

The single driven dependency is the local filesystem. The recovery
path IS the empirical probe: `open` reads back what `ingest` wrote
and `snapshot` compacted, and the KPI 3 parallel-store equality test
(snapshot + tail-WAL recovery must equal pure-WAL recovery) is the
behavioural gold-test that exercises the substrate's actual
append/flush/truncate semantics. v1's honest scope statement is
explicit (outcome-kpis.md): `BufWriter::flush` is NOT fsync, so
recovery from `kill -9` between flush and fsync is **out of scope and
documented as v2** — the design does not pretend the substrate is
more honest than it is. Overlayfs/tmpfs fsync-no-op hardening is the
v2 fsync work; v1 inherits the same flush-only posture the other
three pillars ship.

## DESIGN handoff

DESIGN collapses into the implementation commit, as with Cinder v1,
Sluice v1 and Lumen v1. DISTILL (`@nw-acceptance-designer`) writes
`crates/pulse/tests/v1_slice_01_wal_durability.rs` and
`crates/pulse/tests/v1_slice_02_snapshot.rs` from the AC in
`discuss/user-stories.md` (US-PV1-01, US-PV1-02). DELIVER
(`@nw-software-crafter`) writes `crates/pulse/src/file_backed.rs`,
adds the serde derives to `metric.rs`, and grows
`MetricStoreError` in `store.rs`. Required reading: this file;
`design/application-architecture.md`; `crates/lumen/src/file_backed.rs`
as the structural template.
