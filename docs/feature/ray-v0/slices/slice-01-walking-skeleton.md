# Slice 01 — `TraceStore` trait + in-memory adapter (US-RA-01)

## Goal

Ship the trait the v1 disk-backed adapter will implement, plus
one in-memory adapter that proves the contract carries OTLP
spans end-to-end through both the `get_trace` and the
`service + range` query shapes.

## IN scope

- `TraceStore` trait with `ingest`, `get_trace`, `query`
- `InMemoryTraceStore` adapter with dual index — by
  `(tenant, trace_id)` and by `(tenant, service)`
- `Span`, `SpanKind`, `SpanStatus`, `SpanEvent`, `SpanLink`,
  `TraceId`, `SpanId`, `ServiceName`, `TimeRange` types
- Tenant isolation by construction
- Byte-stable field round-trip including events + links
- KPI 1

## OUT scope

- Predicates (slice 02)
- Sampling (Sieve v1)
- Exemplars (v1)
- Disk durability (v1)

## Learning hypothesis

Disproves "a dual `HashMap` index (by trace_id + by service) is
fast enough at v0 latencies despite the 2× memory cost". If KPI
1 fails, we either go to a single-index slower-lookup model or
push directly into the columnar substrate.

## Effort

≤1 day.
