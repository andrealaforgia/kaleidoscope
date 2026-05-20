# Ray v1 — DESIGN wave decisions

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-21.
Mode: Propose. Paradigm: Rust idiomatic. Released under
AGPL-3.0-or-later.

> **Feature**: adds `FileBackedTraceStore` to the `ray` crate as a
> second adapter behind the unchanged `TraceStore` trait, alongside
> `InMemoryTraceStore`. Durability via NDJSON WAL (one `Ingest` record
> per `SpanBatch`) + JSON snapshot, a structural carry-forward of
> `crates/pulse/src/file_backed.rs`. The fifth v0 to v1 durable-adapter
> carry-forward after Cinder v1, Sluice v1, Lumen v1 and Pulse v1.
> Ray's one novelty versus the prior four: a **dual index**.

These decisions generalise the proven Pulse WAL+snapshot template to
Ray's two-map index. The two items the Pulse precedent does not cover
are the dual index (DD4) and the byte-array ID serde (DD5); everything
else is verbatim carry-forward.

## DD1 — `open` shape

`FileBackedTraceStore::open<P: AsRef<Path>>(base_path: P, recorder:
Box<dyn MetricsRecorder + Send + Sync>) -> Result<Self,
TraceStoreError>` mirrors `FileBackedMetricStore::open`
(`crates/pulse/src/file_backed.rs:97`). The struct holds `base_path`,
`recorder`, `state: Mutex<Inner>`; `Inner` holds **both** maps
(`by_trace`, `by_service`) plus the append `BufWriter<File>`.
Implements `TraceStore` identically to `InMemoryTraceStore` — a
drop-in. Rejected: a new `DurableTraceStore` trait (the port already
abstracts durability); returning `io::Error` (breaks the typed-error
port contract).

## DD2 — WAL format

NDJSON, one `WalRecord::Ingest { tenant: TenantId, spans: Vec<Span> }`
per `SpanBatch`, internally tagged `#[serde(tag = "op", rename_all =
"snake_case")]` — the `WalRecord` shape from `file_backed.rs:45-52`.
Each WAL `Span` carries its own `resource_attributes` (hence its own
`service.name`), so a replayed record is self-contained: the routine
that rebuilds the indices needs nothing beyond the record itself.
`append_wal` writes line + `\n` + `flush` per `file_backed.rs:347`.

## DD3 — Snapshot stores spans ONCE (by_trace only)

`Snapshot { traces: Vec<TraceBucket> }` where `TraceBucket { tenant:
TenantId, trace_id: TraceId, spans: Vec<Span> }`. The snapshot
serialises **only the `by_trace` buckets** — spans appear once, never
duplicated across both indices. The `by_service` index is **derived**
on recovery from those same spans (each span carries its own
`service.name`). This halves the on-disk footprint versus persisting
both maps, keeps the format index-shape-agnostic (eases the v2
columnar migration), and makes "service index is derived, never
independently persisted" an enforced on-disk invariant. `snapshot()`
flushes the WAL, writes the snapshot, then re-opens the WAL with
`truncate(true)` — mirror of `snapshot()` (`file_backed.rs:163`).
Explicit call only; idempotent; no auto-compaction at v1.

## DD4 — Shared `apply_ingest` over BOTH maps (the no-drift guarantee)

A single free function

```text
fn apply_ingest(
    by_trace:   &mut HashMap<(TenantId, TraceId), Vec<Span>>,
    by_service: &mut HashMap<(TenantId, ServiceName), Vec<Span>>,
    tenant: &TenantId,
    spans: Vec<Span>,
)
```

generalises Pulse's `apply_ingest` (`file_backed.rs:297`) from one map
to two. For each span: push a clone into the `by_trace` bucket; iff
`service_name()` is non-empty, push into the `by_service` bucket
(empty-`service.name` spans land in `by_trace` only — the exact v0
`store.rs:137-150` rule). Both the live `ingest` path and WAL replay
call this **same** function, so the two indices cannot drift — the
single most important shape constraint carried from DISCUSS [D5]. The
caller re-sorts each touched bucket once on `start_time_unix_nano`
(both maps), preserving the v0 ascending-time contract [D8].
Enforcement: the architecture rule "ingest and recovery route through
one `apply_ingest`" is verified behaviourally by the KPI durability
test (parallel-store equality) and structurally by there being exactly
one such function (mutation testing at 100% kill rate kills any
divergent second copy).

## DD5 — Hex-string serde for byte-array IDs (hand-rolled, no new crate)

`TraceId([u8; 16])` and `SpanId([u8; 8])` are byte arrays. A raw
derive emits a JSON array of 16/8 integers — byte-stable but verbose
and opaque. **Chosen**: a hand-rolled `hex` module in the `span`
module exposing `serialize`/`deserialize` free functions, applied via
**custom `Serialize`/`Deserialize` impls** on `TraceId` and `SpanId`
(not `#[serde(with)]` on a field, because the IDs are also used as
`HashMap` keys and appear in nested structs `SpanLink`, so the impl
must live on the type itself, not at each use site). IDs serialise as
lowercase hex strings (e.g. `"0102030405060708090a0b0c0d0e0f10"`),
deserialise by parsing exactly `2*N` hex chars back into `[u8; N]`,
rejecting wrong length or non-hex with a serde error.

**Why hand-rolled, not `serde_with`/`hex`**: the project posture is
hand-rolled-over-dependency (cf. the hand-rolled ISO 8601 in
`kaleidoscope-cli`). The hex codec is ~20 lines, has no edge cases
beyond length and the 0-9a-f alphabet, and adding a crate to the graph
to save 20 lines is the wrong trade. Every other v0 span type
(`Span`, `SpanBatch`, `SpanKind`, `StatusCode`, `SpanStatus`,
`SpanEvent`, `SpanLink`, `ServiceName`, `TimeRange`) gets plain
`#[derive(Serialize, Deserialize)]` — the `BTreeMap<String, String>`
attribute maps and `String`/`u64`/enum fields serialise naturally,
exactly as Pulse's metric types did (`crates/pulse/src/metric.rs:29`).
Byte-stability (AC-1.5) holds: hex is a total, injective encoding of
`[u8; N]`. Rejected: raw integer-array derive (verbose, unreadable
WALs); `serde_with` (new dependency for a 20-line job); base64
(non-canonical alphabet variants, less greppable than hex).

## DD6 — Reuse Analysis

**REUSE (read path + index semantics, copied verbatim):** both index
shapes (`store.rs:101-103`), the dual-index ingest rule including the
empty-`service.name` special case (`store.rs:137-150`), sort-once-
per-touched-bucket discipline (`store.rs:156-167`), `get_trace` /
`query` / `query_with` filter-and-clone logic, half-open
`TimeRange::contains`, `Predicate::matches(&Span)`, the
`MetricsRecorder` seam (D11 verbatim), `IngestReceipt`, empty-batch
no-op. The v1 adapter reimplements the read path against its own
`Inner` (it does NOT wrap an `InMemoryTraceStore` — Pulse v1 / Lumen
v1 did not; a wrapped inner would double the lock and obscure the
WAL/index coupling) but copies the *logic* verbatim. **EXTEND:**
`TraceStoreError` (+1 variant, DD7); the span type set (+serde
derives, +custom ID impls). **CREATE NEW (durability only):**
`WalRecord`, `Snapshot` / `TraceBucket`, `open`, `snapshot`,
`apply_ingest` (the two-map generalisation), `append_wal`,
`wal_path_of` / `snapshot_path_of`, the `io` / `parse` adapters, the
`hex` module — all structural mirrors of `file_backed.rs:289-353`. No
new public trait, no new module beyond `file_backed` (plus the small
`hex` helper inside `span`), no new external crate.

## DD7 — `TraceStoreError` grows from empty to one variant

The v0 `enum TraceStoreError {}` with its `match *self {}` never-type
Display impl (`store.rs:35-41`) grows to `PersistenceFailed { reason:
String }`, and Display is rewritten to match it. Any v0 caller that
pattern-matched the empty enum needs an explicit arm. Mirrors Pulse v1
/ Lumen v1 exactly. `io` and `parse` helpers map `std::io::Error` and
`serde_json::Error` into this variant; corrupt WAL lines produce a
`PersistenceFailed` naming the line number (fail-loud,
`file_backed.rs:130`).

## Earned Trust — the durable adapter is its own probe

`FileBackedTraceStore` depends on one external substrate: the local
filesystem. Per principle 12, recovery **is** the empirical probe.
`open` does not assume the WAL/snapshot on disk is honest — it parses
every line, fails loud (`PersistenceFailed` + line number) on the
first lie, and the KPI durability test exercises the real round-trip
(ingest, drop, reopen, assert zero loss / zero duplication across both
indices, including the empty-`service.name` edge case). Honest scope:
`BufWriter::flush` only; fsync, atomic snapshot rename and file
locking are explicitly v2 — so the probe's contract is "survives a
clean process restart", not "survives `kill -9` mid-fsync". That
boundary is stated, not assumed away.

## Handoff to DISTILL — `@nw-acceptance-designer`

Translates US-RV1-01 (AC-1.1..) and US-RV1-02 (AC-2.1..) into `#[test]`
functions under `crates/ray/tests/v1_slice_01_wal_durability.rs` and
`crates/ray/tests/v1_slice_02_snapshot.rs`, including KPI 1 (ingest p95
≤ 2 ms), KPI 2 (recovery p95 ≤ 2.5 s), and the KPI 3 North-Star
durability test. The durability test MUST cover **both** indices
(`get_trace` and service-`query` recover identically) **and** the
empty-`service.name` span (lands in `by_trace` only, must not appear in
any `by_service` recovery). AC-1.5 byte-stability asserts a round-trip
through hex serde. Flags the empty-`TraceStoreError` match-arm break to
v0 callers. Required reading: this file; `design/application-
architecture.md`; the `pulse-v1` brief subsection;
`crates/pulse/src/file_backed.rs` as the structural template;
`crates/ray/src/store.rs:137-167` as the dual-index logic to mirror.

## Handoff to DEVOPS — `@nw-platform-architect`

Receives KPI 1 (ingest, leading), KPI 2 (recovery, leading), KPI 3
(durability completeness, North-Star guardrail, 100%). ADR-0005's five
gates apply unchanged — **no new/amended gate**. Per-feature mutation
scope: `crates/ray/src/file_backed.rs` + touched `store.rs` / `span.rs`
lines at 100% kill rate (the mutation suite is the enforcement that the
single `apply_ingest` has no divergent twin). Cargo delta: two new
`[[test]]` blocks (`v1_slice_01_wal_durability`, `v1_slice_02_snapshot`)
— **no new `[dependencies]`** (`serde` / `serde_json` / `aegis` already
present; hex codec is hand-rolled). **External integrations: none** —
no HTTP, no webhook, no third-party API, no vendor SDK; pure local
filesystem WAL append + JSON snapshot. No contract tests apply. DELIVER
paradigm Rust idiomatic: one new struct + trait impl, free helper
functions (`apply_ingest` over two maps), two serde structs, two custom
ID serde impls, a tiny hex module, one additive `Error` variant; no
class-style inheritance; no new `dyn` boundary beyond the existing
`Box<dyn MetricsRecorder + Send + Sync>`. No new ADR — mirrors pulse-v1.
