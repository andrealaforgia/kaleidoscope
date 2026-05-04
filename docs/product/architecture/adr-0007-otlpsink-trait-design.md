# ADR-0007 — `OtlpSink` trait design: async trait, three-variant SinkRecord, structured SinkError

- **Status**: Accepted
- **Date**: 2026-05-04
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

DISCUSS Q3 locks `OtlpSink` as the trait boundary between Aperture and the future Sieve component (Phase 1). DISCUSS D2 locks the contract at the requirements level:

- `Send + Sync`.
- An async method `accept(record) -> Result<(), SinkError>`.
- The `record` parameter is a `SinkRecord` enum with exactly three variants: `Logs(ExportLogsServiceRequest)`, `Traces(ExportTraceServiceRequest)`, `Metrics(ExportMetricsServiceRequest)` — each carrying the upstream `opentelemetry_proto` type **unwrapped**.
- The `SinkError` type names the failure shape (downstream unavailable, downstream timeout, etc.); DESIGN locks the variants.

DISCUSS D2 also records three rejected alternatives: synchronous trait, channel-based sink, callback-based sink. Those are NOT re-litigated here (they are rejected at the contract level by DISCUSS).

What DESIGN must lock:
1. The exact trait signature — async-trait flavour, lifetimes, `'static` bound, `Sized` posture.
2. The exact `SinkError` variant set with `#[non_exhaustive]` posture.
3. The exact `SinkRecord` variant set and posture.
4. Whether sinks are stored as `Arc<dyn OtlpSink>`, `Box<dyn OtlpSink>`, or generic-instantiated.
5. Whether the trait extends `Probe` (the Earned-Trust contract) or whether `Probe` is a separate trait.

## Decision

```rust
#[async_trait::async_trait]
pub trait OtlpSink: Send + Sync + 'static {
    async fn accept(&self, record: SinkRecord) -> Result<(), SinkError>;
}

#[async_trait::async_trait]
pub trait Probe: Send + Sync + 'static {
    async fn probe(&self) -> Result<(), ProbeError>;
}

#[derive(Debug)]
#[non_exhaustive]
pub enum SinkRecord {
    Logs(ExportLogsServiceRequest),
    Traces(ExportTraceServiceRequest),
    Metrics(ExportMetricsServiceRequest),
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SinkError {
    #[error("downstream unavailable: {reason}")]
    DownstreamUnavailable { reason: String },
    #[error("downstream timeout after {elapsed_ms} ms")]
    DownstreamTimeout { elapsed_ms: u64 },
    #[error("sink internal error: {message}")]
    Internal { message: String },
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProbeError {
    #[error("downstream unreachable at {endpoint}: {reason}")]
    Unreachable { endpoint: String, reason: String },
    #[error("downstream rejected probe at {endpoint}: status {status}")]
    Refused { endpoint: String, status: u16 },
    #[error("probe timed out after {elapsed_ms} ms against {endpoint}")]
    Timeout { endpoint: String, elapsed_ms: u64 },
}
```

The composition root stores `Arc<dyn OtlpSink>` and clones the Arc into each transport task. **`OtlpSink` and `Probe` are separate traits**, both required of every concrete sink. The structural-layer enforcement (xtask AST walk) verifies that every type implementing `OtlpSink` also implements `Probe`.

## Alternatives Considered

### Option A — `async-trait` crate, separate `OtlpSink` and `Probe`, `Arc<dyn OtlpSink>` storage (RECOMMENDED, accepted)

**Pros**:
- `async-trait` allows `dyn OtlpSink` storage in stable Rust; the application core can hold `Arc<dyn OtlpSink>` and clone the Arc into each transport adapter without generics threading through the entire codebase.
- Separate `Probe` trait keeps the Earned-Trust contract orthogonal to the runtime contract. A future test double or stub can opt out of probing trivially (`impl Probe { async fn probe(&self) -> Result<(), ProbeError> { Ok(()) } }`).
- `#[non_exhaustive]` on every public enum allows additive evolution. Adding `SinkError::DownstreamRateLimited { retry_after_ms: u64 }` in v0.2 is non-breaking.
- The `'static` bound is required by `async-trait`'s desugaring for `dyn` dispatch and by `Arc::new(...)` storage.
- `thiserror`'s derive gives `Display` + `std::error::Error` for free.

**Cons**:
- `async-trait` is a separate crate dependency (~600 LOC, MIT, mature, ~7M downloads/month). It desugars `async fn` to `Pin<Box<dyn Future>>` returning the same lifetime as the receiver.
- `Box<dyn Future>` allocation per call. At v0 throughput targets (single replica, ~100s of req/s typical), this is invisible. At Phase-1+ throughput targets, `cargo flamegraph` may suggest revisiting.

### Option B — Native `async fn in trait` (stable in Rust 1.75+)

**Pros**:
- Removes the `async-trait` dependency.
- Avoids per-call `Box<dyn Future>` allocation (the future is statically sized).

**Cons**:
- **Does NOT support `dyn` dispatch** in stable Rust 1.85. `Box<dyn OtlpSink>` and `Arc<dyn OtlpSink>` do not compile if `accept` is a native `async fn`. The workaround is `dyn OtlpSink<accept(): Send>` syntax (return-type-notation) which is unstable.
- Forces every consumer (transport adapter, composition root, tests) to be generic over `S: OtlpSink`. Generics threading through the codebase makes the call sites clumsier and increases compile times.
- **Trait incompatibility with Phase-1's Sieve plug-in shape** if the Sieve maintainer wants `dyn`-based hot-swapping.

**Rejected** for v0 because `dyn` storage is the integration shape Phase-1 will want. ADR Phase-1 revisit gate: when stable Rust supports `dyn` dispatch on `async fn in trait` cleanly (likely 2026/2027), revisit.

### Option C — Hand-rolled associated `Future` type

```rust
pub trait OtlpSink: Send + Sync {
    type AcceptFuture<'a>: Future<Output = Result<(), SinkError>> + Send + 'a where Self: 'a;
    fn accept<'a>(&'a self, record: SinkRecord) -> Self::AcceptFuture<'a>;
}
```

**Pros**:
- Zero allocation per call.
- Zero dependency.
- Static dispatch when called via generics.

**Cons**:
- GAT (Generic Associated Types) syntax; verbose at every call site and every impl site.
- Still does not support `dyn` dispatch; `dyn OtlpSink<AcceptFuture = ?>` is not expressible.
- Substantially harder to read than Option A.
- Documented as a niche optimisation in the Rust async book; nobody picks this for application code.

**Rejected** for being optimisation-of-an-optimisation in a path where allocations are not a bottleneck.

### Option D — Single combined `OtlpSink` trait with `probe` as a default method

```rust
#[async_trait]
pub trait OtlpSink: Send + Sync + 'static {
    async fn accept(&self, record: SinkRecord) -> Result<(), SinkError>;
    async fn probe(&self) -> Result<(), ProbeError> {
        Ok(()) // Default: no probe.
    }
}
```

**Pros**:
- Single trait; less type machinery.
- Trivial implementations for sinks with no external dependency get the default.

**Cons**:
- **Defeats the Earned-Trust enforcement.** Principle 12 explicitly states the probe contract is enforced by three semantically orthogonal layers; the structural-layer enforcement (xtask AST walk) needs to assert "every `impl OtlpSink` ALSO has a non-default `probe()`". With a default method, the structural check has to inspect the method body to know if it's the default — that is brittle.
- Conflates two concerns: runtime acceptance and startup health. They have different lifecycles (probe runs once, accept runs many times).
- A maintainer who silently accepts the default `probe()` for `ForwardingSink` would defeat the probe; the structural-layer check would have to be sensitive to this.

**Rejected** because the Earned-Trust contract MUST be a separate trait so its presence is structurally checkable.

### Option E — Generic-only, no `dyn`, monomorphise everywhere

**Pros**:
- Zero allocation, zero dependency.
- Maximum static dispatch.

**Cons**:
- The composition root can store ONE concrete sink type at compile time, not a `Box<dyn OtlpSink>`. To support `sink.kind = "stub"` vs `sink.kind = "forwarding"` at runtime, the entire transport-and-app graph must be generated twice (once per concrete sink). This either requires macro generation or duplicates the whole startup path.
- Phase 1's Sieve adds a third concrete type, multiplying again.
- This is the "build a static type-state machine for dynamic configuration" anti-pattern.

**Rejected** as the cure being far worse than the disease.

## Consequences

### Positive
- The trait shape Phase-1 Sieve will plug into is the standard Rust shape for an async boundary. A Sieve maintainer reading the trait sees exactly what they expect.
- `#[non_exhaustive]` on `SinkRecord` and `SinkError` allows additive evolution without major-version bumps. Adding a fourth signal (e.g. OTLP Profiles when the spec stabilises) is a `SinkRecord::Profiles(ExportProfileServiceRequest)` addition; adding a new failure shape (e.g. rate-limit) is `SinkError::DownstreamRateLimited { ... }` addition.
- `Arc<dyn OtlpSink>` storage at the composition root keeps generics out of the application core and the transport adapters. Reading the call sites is straightforward: "the sink is an Arc; clone it; call `accept`."
- The separate `Probe` trait makes the Earned-Trust contract structurally enforceable.
- The trait carries `'static` bound, satisfying `async-trait`'s and `Arc<dyn ...>`'s requirements.

### Negative
- One extra dependency (`async-trait` ≈ 600 LOC). Acceptable: it is the de-facto Rust ecosystem solution for this pattern.
- `Box<dyn Future>` allocation per `accept` call. At v0 throughput targets, invisible. Documented as a sensitivity point for Phase-1 profiling.
- Two traits to keep in sync (every concrete sink implements both). Acceptable: the structural-layer enforcement makes this mechanical.

### Trade-off ATAM

**Sensitivity point** for **Performance Efficiency — Time behaviour** (per-call allocation under `async-trait`) and for **Maintainability — Modifiability** (separate `Probe` trait makes the Earned-Trust contract additive).

**Trade-off point** for **Performance Efficiency vs Maintainability**: the `dyn`-based dispatch and per-call allocation pay maintainability dividends (less generics threading) at the cost of sub-microsecond per-call overhead. Bias toward maintainability is correct at v0 throughput; revisit when profiling demands.

### Phase-1 revisit gates

- When stable Rust supports `dyn`-dispatch on native `async fn in trait` (probably 2026/2027), revisit the `async-trait` dependency.
- If `cargo flamegraph` during Phase-1 load testing shows `Box::new` for the future allocation as a top-N profile entry, revisit.
- If Sieve's integration shape demands a different trait shape (e.g. batch acceptance, sink-side backpressure signal), revisit additively (a new trait method, with default impl, behind `#[non_exhaustive]`'s evolution rules).

## Earned-Trust enforcement (Principle 12 cross-reference)

The three semantically-orthogonal layers (per Principle 12c):

| Layer | Mechanism for probe-contract enforcement |
|---|---|
| Subtype | Composition root signature `wire_then_probe_then_use<T: OtlpSink + Probe>(...)` requires both traits at compile time. A non-`Probe` sink type fails to compile when wired. |
| Structural | `xtask` AST-walking pre-commit hook walks `crates/aperture/src/sinks/` and asserts every `impl OtlpSink for T` is matched by an `impl Probe for T`. Catches the "I added a sink and forgot to probe" case before commit. |
| Behavioural | `tests/probe_gold_runner.rs` (CI gold-test) starts Aperture against a fixture downstream that lies (200 OK to OPTIONS but 503 to POST); asserts Aperture refuses to start with `event=health.startup.refused`. Catches the "I implemented `probe()` as `Ok(())` without a network call" case. |

A single-layer bypass is caught by at least one of the other two. `import-linter` was investigated and rejected (Python-only; import-graph contracts have no API for method-presence enforcement on classes). For Rust, an `xtask` AST walk is the language-appropriate enforcement mechanism.
