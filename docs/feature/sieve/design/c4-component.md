# Sieve v0 — C4 Component (L3)

The Sieve library box from the container diagram contains five
components that collaborate to honour the contract:
`SamplingSink<S, N>` (the decorator), `HeadSampler` (the concrete
sampler), `Counters` (the aggregator state), `SummaryTask` (the timer
task), and `observability` (the tracing emission helpers). Plus
`TraceView` and `Decision` / `KeepReason` as boundary types.

L3 is justified per agent principle 9 (L3 only for complex
subsystems with 5+ components). Sieve has 5 internal components plus
2 boundary types — the threshold is met.

```mermaid
C4Component
  title Component Diagram — Sieve library internals

  Container_Boundary(sieve, "Sieve library (crates/sieve)") {
    Component(decorator, "SamplingSink<S, N>", "Rust struct in decorator.rs", "Implements aperture::ports::OtlpSink + aperture::ports::Probe. Owns the decorator's accept body: trace-grouping pass, per-decision routing, kept-traces-only envelope rebuild")
    Component(sampler, "HeadSampler + Sampler trait", "Rust struct + trait in sampler.rs", "Holds the configured rate and the xxh3_64 mapping. is_error_bearing predicate is also defined here as a free pub(crate) function")
    Component(counters, "Counters", "Rust struct in aggregator.rs", "Three AtomicU64s: kept_total, kept_error_bearing, dropped. record_kept_*, record_dropped, snapshot_and_reset")
    Component(timer, "SummaryTask", "Rust struct + tokio task in aggregator.rs", "Owns the tokio::time::interval and the CancellationToken; on tick, snapshots and emits the INFO summary; on cancel or drop, emits the final summary")
    Component(observability, "observability helpers", "Rust pub(crate) free fns in observability.rs", "emit_debug_kept_error_bearing, emit_debug_kept_sampled, emit_debug_dropped, emit_summary. Centralises the target=\"sieve\" vocabulary so renames are one-file edits")
    ComponentDb(decision, "Decision / KeepReason / TraceView (boundary types)", "Rust enums + struct in decision.rs", "The shapes the decorator and sampler exchange: Decision { Keep, Drop } as the routing decision; KeepReason { ErrorBearing, Sampled } as the observability metadata; TraceView<'a> as the borrowed-spans view")
  }

  Container_Ext(aperture_sink, "aperture::ports::OtlpSink + Probe (inner sink)", "Aperture trait", "The decorator's wrapped target")
  Container_Ext(global_subscriber, "tracing global subscriber", "tracing crate", "Receives DEBUG and INFO events with target=\"sieve\"")
  Container_Ext(env, "Process environment", "OS env vars", "SIEVE_NON_ERROR_TRACE_RATE and SIEVE_SUMMARY_TICK_MS")

  Rel(decorator, sampler, "Asks for a decision per trace via", "Sampler::sample(&TraceView)")
  Rel(decorator, decision, "Constructs and matches on", "Decision and KeepReason")
  Rel(decorator, counters, "Records each decision via", "record_kept_*, record_dropped")
  Rel(decorator, observability, "Emits per-decision DEBUG events via", "emit_debug_*")
  Rel(decorator, aperture_sink, "Forwards kept traces and all logs/metrics to", "OtlpSink::accept; delegates probe()")
  Rel(timer, counters, "Snapshots and resets every tick via", "snapshot_and_reset")
  Rel(timer, observability, "Emits the INFO summary via", "emit_summary")
  Rel(observability, global_subscriber, "Pushes events to", "tracing macros (target=\"sieve\")")
  Rel(sampler, env, "Reads rate at construction from", "SIEVE_NON_ERROR_TRACE_RATE")
  Rel(timer, env, "Reads tick interval at construction from", "SIEVE_SUMMARY_TICK_MS")
```

## Notes

- The decorator is the **only component** that talks to the
  Aperture-side inner sink; every other component is internal.
- The sampler depends on no other Sieve component. The hash
  mapping is a free function inside `sampler.rs`; the
  `is_error_bearing` predicate is a free `pub(crate)` function
  there too.
- The `Counters` and `SummaryTask` are split because the counters
  are touched by the hot path (the decorator) and read by the cold
  path (the timer task). Splitting the type from the task makes the
  ownership clear: the decorator holds `Arc<Counters>`; the timer
  task holds another `Arc<Counters>`; the cancellation token's
  cancel toggle is sync.
- `observability` is one module with seven free functions (per
  decision shape, per summary). One module so renaming the
  `target="sieve"` literal is one edit.
- `decision.rs` holds the boundary types (`Decision`, `KeepReason`,
  `TraceView`). One module so the public-API surface is in one
  place; `cargo public-api` diffs read cleanly.
- The arrows from `decorator` to `decision` (constructs and matches
  on) overlap with the trait-method arrow from `decorator` to
  `sampler` (Sampler::sample returns `Decision`). The diagram
  shows them separately for clarity.

## Component responsibility matrix

| Component | Owns | Does NOT own |
|-----------|------|--------------|
| `SamplingSink<S, N>` (decorator.rs) | OtlpSink + Probe impl, trace-grouping pass, per-decision routing, decorator-level error mapping | the rate parse (delegates to HeadSampler), counter updates' atomicity (delegates to Counters), event vocabulary (delegates to observability) |
| `HeadSampler` + `Sampler` trait (sampler.rs) | rate parse, xxh3_64 mapping, is_error_bearing predicate, `Decision` return value | event emission, counter updates, trace grouping |
| `Counters` (aggregator.rs) | three AtomicU64s, atomic updates, snapshot-and-reset | event emission, timing, lifecycle |
| `SummaryTask` (aggregator.rs) | tokio::time::interval, CancellationToken, tick → snapshot → emit cycle | counter mutation (only reads via snapshot), event vocabulary (delegates to observability) |
| `observability` (observability.rs) | the `target="sieve"` literal, the structured field set, the rendered messages | counter snapshots, sampling decisions |
| `decision.rs` boundary types | `Decision`, `KeepReason`, `TraceView<'a>`, the `__test_trace_view` seam | logic — these are pure data |
