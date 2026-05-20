# Ray v1 — Application Architecture (C4 L1 + L2)

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-21.

`FileBackedTraceStore` adds durability to the `ray` crate behind the
unchanged `TraceStore` port, alongside `InMemoryTraceStore`. NDJSON WAL
+ JSON snapshot, dual index (`by_trace`, `by_service`) rebuilt on
recovery through one shared `apply_ingest` routine. Structural carry-
forward of `crates/pulse/src/file_backed.rs`.

## C4 Level 1 — System Context

```mermaid
C4Context
  title Ray v1 — System Context
  Person(operator, "Platform binary", "Embeds Ray; ingests and queries spans")
  System(ray, "Ray TraceStore", "Per-tenant span storage behind the TraceStore port")
  System_Ext(fs, "Local filesystem", "<base_path>.wal + <base_path>.snapshot")
  Rel(operator, ray, "Ingests SpanBatch into / queries spans from")
  Rel(ray, fs, "Appends WAL records to and writes/reads snapshot on")
```

The filesystem is the single driven dependency. Recovery is the
Earned-Trust probe: `open` parses every persisted line and fails loud
on the first corruption rather than trusting the substrate.

## C4 Level 2 — Container View

```mermaid
C4Container
  title Ray v1 — Container View (ray crate)
  Person(operator, "Platform binary")
  Container_Boundary(ray, "ray crate") {
    Component(port, "TraceStore trait", "Rust trait", "Unchanged port: ingest, get_trace, query, query_with")
    Component(inmem, "InMemoryTraceStore", "Rust", "v0 adapter, unchanged")
    Component(fb, "FileBackedTraceStore", "Rust", "v1 adapter: dual index + WAL + snapshot")
    Component(apply, "apply_ingest", "Rust free fn", "Shared split into BOTH maps — no-drift guarantee")
    Component(types, "Span types", "Rust + serde", "Span/SpanBatch/IDs; hex serde on TraceId/SpanId")
    Component(rec, "MetricsRecorder", "Rust trait", "Observability seam, verbatim from v0")
  }
  ContainerDb_Ext(wal, "WAL file", "NDJSON", "<base_path>.wal — one Ingest line per SpanBatch")
  ContainerDb_Ext(snap, "Snapshot file", "JSON", "<base_path>.snapshot — by_trace buckets only")

  Rel(operator, port, "Calls")
  Rel(port, inmem, "Implemented by")
  Rel(port, fb, "Implemented by")
  Rel(fb, apply, "Routes live ingest AND WAL replay through")
  Rel(fb, types, "Serialises/deserialises")
  Rel(fb, rec, "Records ingest/query via")
  Rel(fb, wal, "Appends Ingest records to")
  Rel(fb, snap, "Writes by_trace buckets to / reads on open")
```

### Key shape notes

- **Dual index, single writer.** `Inner` holds `by_trace:
  HashMap<(TenantId, TraceId), Vec<Span>>` and `by_service:
  HashMap<(TenantId, ServiceName), Vec<Span>>` behind one `Mutex`.
  Both are populated only by `apply_ingest`.
- **No-drift guarantee.** Live `ingest` and WAL `replay` call the same
  `apply_ingest`; there is exactly one place that decides which map a
  span enters. Empty-`service.name` spans enter `by_trace` only.
- **Derived service index.** The snapshot persists `by_trace` buckets
  only; `by_service` is rebuilt from those spans on recovery. The
  on-disk format never duplicates a span.
- **Hex IDs.** `TraceId`/`SpanId` serialise as lowercase hex strings
  via a hand-rolled `hex` module; all other span types use plain serde
  derives.

### L3 — not produced

Single-`Mutex<Inner>` adapter; the reification conditions
(columnar/trace_id-partitioned index, write/read-index split,
compaction scheduler) are all v2. Two maps behind one lock with one
shared writer do not warrant a component-level decomposition.
