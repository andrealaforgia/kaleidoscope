# ADR-0021 — `sieve`–`aperture` integration point: `OtlpSink` decorator

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `@nw-solution-architect` (Morgan)
- **Feature**: `sieve` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

DISCUSS Q1 locks Sieve as a library: "`crates/sieve` exposes a
`Sampler` trait that Aperture's pipeline calls before the sink".
Aperture v0.1.0 ships with the `OtlpSink` and `Probe` traits as the
hand-off boundary (ADR-0007). DISCUSS Q6 locks the signal handling:
traces sampled, logs and metrics passthrough.

Sentinel's APPROVED peer review and slice-05's Technical Note flag two
DESIGN questions:

- **D3**: does Sieve plug in via a new trait Aperture exposes, or via
  the existing `OtlpSink`? If a new trait is needed, Sieve's DELIVER
  work includes a small Aperture amendment.
- **D6**: logs/metrics pipeline shape — a unified `Signal` enum
  Sieve-side or align with Aperture's three-variant `SinkRecord`?

Reading `crates/aperture/src/lib.rs` and `crates/aperture/src/ports/mod.rs`
shows Aperture's existing surface:

- `OtlpSink::accept(record: SinkRecord) -> Result<(), SinkError>` —
  one async method, takes the upstream OTLP envelope unwrapped.
- `Probe::probe() -> Result<(), ProbeError>` — separate trait every
  sink also implements.
- `SinkRecord { Logs(...), Traces(...), Metrics(...) }` —
  three-variant enum, one variant per OTLP-stable signal, each
  carrying the upstream `opentelemetry_proto` type.
- `Arc<dyn OtlpSink>` storage in the composition root (`compose::wire_sink`).

The composition-root invariant "wire then probe then use" is enforced
by the existing `wire_then_probe_then_use` mechanism. Adapter probe
contracts are mandatory per agent principle 12.

## Decision

### 1. Integration via the existing `OtlpSink` + `Probe` traits — **no Aperture amendment needed**

Sieve exposes `SamplingSink<S, N>`, a generic decorator over any
`S: OtlpSink + Probe + Send + Sync + 'static` and any `N: Sampler`.
The decorator implements `OtlpSink + Probe` itself, so Aperture's
composition root sees a `SamplingSink` everywhere it currently sees
a `dyn OtlpSink`.

```rust
// crates/sieve/src/decorator.rs (sketch; software-crafter writes the
// production implementation)

#[async_trait::async_trait]
impl<S, N> aperture::ports::OtlpSink for SamplingSink<S, N>
where
    S: aperture::ports::OtlpSink + aperture::ports::Probe + Send + Sync + 'static,
    N: Sampler,
{
    async fn accept(&self, record: aperture::ports::SinkRecord) -> Result<(), aperture::ports::SinkError> {
        match record {
            aperture::ports::SinkRecord::Logs(req) => {
                // Passthrough per Q6.
                self.inner.accept(aperture::ports::SinkRecord::Logs(req)).await
            }
            aperture::ports::SinkRecord::Metrics(req) => {
                // Passthrough per Q6.
                self.inner.accept(aperture::ports::SinkRecord::Metrics(req)).await
            }
            aperture::ports::SinkRecord::Traces(req) => {
                self.accept_traces(req).await
            }
        }
    }
}

#[async_trait::async_trait]
impl<S, N> aperture::ports::Probe for SamplingSink<S, N>
where
    S: aperture::ports::OtlpSink + aperture::ports::Probe + Send + Sync + 'static,
    N: Sampler,
{
    async fn probe(&self) -> Result<(), aperture::ports::ProbeError> {
        // Sieve has no external dependency to probe; the rate parse
        // happened at SamplingSink::new (returning SieveConfigError);
        // the sampler holds no I/O surface; the timer task has spawned
        // already. Delegate to the inner sink — that is the only
        // external dependency in scope.
        self.inner.probe().await
    }
}
```

The `accept_traces` private method runs the trace-grouping pass,
asks the sampler for a `Decision` per trace, emits the per-decision
DEBUG event with the right `KeepReason`, updates the aggregator's
`AtomicU64` counters, and forwards a kept-traces-only
`ExportTraceServiceRequest` to the inner sink:

```rust
async fn accept_traces(
    &self,
    request: ExportTraceServiceRequest,
) -> Result<(), aperture::ports::SinkError> {
    let mut groups: HashMap<[u8; 16], (Vec<&Span>, /* error-bearing */ bool)> = HashMap::new();
    for resource_spans in &request.resource_spans {
        for scope_spans in &resource_spans.scope_spans {
            for span in &scope_spans.spans {
                let trace_id = match <[u8; 16]>::try_from(span.trace_id.as_slice()) {
                    Ok(id) => id,
                    Err(_) => continue, // malformed; skip per defensive posture
                };
                let entry = groups.entry(trace_id).or_default();
                entry.0.push(span);
                let is_error = span.status.as_ref().map(is_error_status).unwrap_or(false);
                entry.1 = entry.1 || is_error;
            }
        }
    }

    // Decide per trace; track which trace_ids are kept.
    let mut kept_trace_ids: HashSet<[u8; 16]> = HashSet::new();
    for (trace_id, (spans, is_error_bearing)) in &groups {
        let view = TraceView::new(*trace_id, spans);
        let decision = self.sampler.sample(&view);
        match decision {
            Decision::Keep if *is_error_bearing => {
                self.counters.record_kept_error_bearing();
                emit_debug_kept_error_bearing(*trace_id);
                kept_trace_ids.insert(*trace_id);
            }
            Decision::Keep => {
                self.counters.record_kept_sampled();
                emit_debug_kept_sampled(*trace_id, self.sampler.rate());
                kept_trace_ids.insert(*trace_id);
            }
            Decision::Drop => {
                self.counters.record_dropped();
                emit_debug_dropped(*trace_id, self.sampler.rate());
            }
        }
    }

    // Rebuild a kept-traces-only envelope and forward to the inner
    // sink. The rebuild filters spans within each ResourceSpans /
    // ScopeSpans; an entirely-dropped ResourceSpans is omitted.
    let filtered = filter_resource_spans(request, &kept_trace_ids);
    self.inner.accept(aperture::ports::SinkRecord::Traces(filtered)).await
}
```

(Pseudocode; the production implementation is the crafter's
territory. The shape locks the contract: trace-grouping pass,
per-trace decision, per-decision DEBUG event with the right
`KeepReason`, counter update, kept-traces-only envelope forwarded.)

### 2. Why no Aperture amendment

Three reasons:

1. **`OtlpSink` is already the integration seam**. ADR-0007 §"Decision"
   names `OtlpSink` as "Aperture's hand-off boundary with the next
   pipeline stage". Sieve IS the next pipeline stage. The decorator
   pattern is the canonical Rust shape for adding a cross-cutting
   concern (sampling) to a trait-bound dependency.
2. **`SinkRecord`'s three variants exactly match Q6**. Logs and
   metrics pass through unchanged, traces are sampled. The variant
   discriminator is the routing decision; no new shape is needed.
3. **`Probe` is orthogonal** (ADR-0007 §"Decision"). Sieve has no
   external dependency to probe — its only external dependency is
   the inner sink, which it already holds. Delegating `probe()` to
   the inner sink is the cleanest possible shape.

ADR-0018 §"Public surface (final list)" Option D rejected an
Aperture-side `before_accept` hook. This ADR is the symmetric
decision on the Aperture side: no hook, no new trait, no surface
amendment.

### 3. DELIVER-wave wiring

Aperture's `crates/aperture/src/compose.rs` currently calls
`wire_sink(&config) -> Arc<dyn OtlpSink>`. The DELIVER-wave change
to wire Sieve in is:

```rust
// pseudocode, in Aperture's compose.rs at DELIVER time
pub(crate) async fn wire_sink(config: &Config) -> Result<Arc<dyn OtlpSink>> {
    let inner = build_inner_sink(config).await?;     // existing path
    let sampler = sieve::HeadSampler::from_env()?;   // new
    let decorated = sieve::SamplingSink::new(inner, sampler);  // new
    Ok(Arc::new(decorated) as Arc<dyn OtlpSink>)
}
```

The `wire_sink` signature is unchanged; the function returns
`Arc<dyn OtlpSink>` exactly as before. The `Arc<dyn OtlpSink>` is the
cell where the decorator's `OtlpSink + Probe` impl lives.

The composition root's `wire_then_probe_then_use` invariant calls
`probe()` on the returned `Arc<dyn OtlpSink>`. The decorator's
`probe()` delegates to the inner sink. The startup refusal semantics
are preserved: if the inner sink's probe fails, the
`SamplingSink::probe` returns the same error, and Aperture refuses
to start with a structured `health.startup.refused` event (per the
existing Aperture mechanism).

The wiring change is a **DELIVER-wave Aperture amendment**, not a
DESIGN-wave one. The amendment is small (three lines in
`compose.rs`), additive, and does not change Aperture's public
surface (`compose` is a `pub(crate)` module). Aperture's slice tests
and integration tests continue to use `RecordingSink` directly when
they want to bypass Sieve; the production binary path picks up Sieve
via the env-var-driven sampler construction.

If `SIEVE_NON_ERROR_TRACE_RATE` is unset, `HeadSampler::from_env()`
returns a sampler at the default rate of 0.1 (per Q5). To run
Aperture **without** sampling — useful for the harness's own
slice-01 walking-skeleton path — the operator sets
`SIEVE_NON_ERROR_TRACE_RATE=1.0`. There is **no** "disable Sieve"
toggle at v0; Sieve is always wired in.

A future v1 may add a `KALEIDOSCOPE_DISABLE_SIEVE=1` escape hatch for
operators who want to opt out entirely. v0's posture is "Sieve is
always on; rate=1.0 is the no-sampling configuration".

### 4. Why no Sieve-local `Signal` enum (D6 resolution)

Slice-05's brief and the user-stories' Technical Notes consider a
unified Sieve-local `Signal` enum versus three separate methods on
the Sieve surface. The third option — **"don't add a Sieve-local
type at all; consume Aperture's `SinkRecord` directly"** — is the
chosen path:

- `aperture::ports::SinkRecord` is exactly the three-variant enum
  (Logs / Traces / Metrics) Sieve needs. Inventing a parallel
  Sieve-local enum and converting between them adds two trait
  impls, two memcpys per record on the hot path, and zero
  behavioural value.
- The decorator's `accept` body matches on `SinkRecord` directly;
  the routing decision is a `match`, the variant carries the
  upstream OTLP envelope unwrapped, and the passthrough variants
  forward without unpacking.
- ADR-0011 §"no re-exports" precedent applies: Sieve does not shadow
  upstream type paths. `SinkRecord` is an Aperture type; Sieve
  consumes it through the `aperture::ports` import path.

### 5. Error mapping

The decorator's `accept` body must produce `Result<(), SinkError>`.
Two error sources are in scope:

1. **Inner sink errors** — bubble through unchanged. The `?` operator
   on `self.inner.accept(...)` returns the inner's `SinkError` to the
   caller verbatim.
2. **Sieve-internal errors** — none on the hot path. The rate parse
   happens at `HeadSampler::new` / `from_env` (returning
   `SieveConfigError`, which the composition root maps to a startup
   error). The sampler is wait-free. The aggregator's counter
   updates cannot fail. There is no Sieve-internal `SinkError`
   variant to add.

`SinkError` therefore needs **no** new variant for Sieve. The
existing `DownstreamUnavailable`, `DownstreamTimeout`, `Internal`
variants cover everything.

### 6. Probe contract for `SamplingSink`

```rust
async fn probe(&self) -> Result<(), ProbeError> {
    self.inner.probe().await
}
```

The decorator probe is **delegation, full stop**. Sieve has no
external dependency to probe:

- The hash function is in-process, deterministic, and tested.
- The aggregator's atomics are in-process.
- The timer task spawned at `SamplingSink::new`; if the spawn
  failed, `SamplingSink::new` would have returned an error before
  `probe()` was called. (The current contract has `SamplingSink::new`
  infallible; the timer-spawn path is fault-free in the absence of
  a missing Tokio runtime, and a missing Tokio runtime is a
  programming error not a runtime error.)
- The rate parse happened at construction.

The composition-root invariant "wire then probe then use" therefore
holds: Aperture wires the `SamplingSink`, calls `probe()`, the
decorator delegates to the inner sink, the startup-refusal
semantics fire if the inner probe fails. No probe surface from
Sieve itself; full transparency through the decorator.

This decision is **enforced** by the existing Aperture-side xtask
AST walk that verifies "every type implementing `OtlpSink` also
implements `Probe`". `SamplingSink<S, N>` is one such type; the
walker covers it automatically. No new enforcement is added.

### 7. Aperture's existing `RecordingSink` and the test seams

Aperture's `crates/aperture/src/testing/mod.rs` exposes
`RecordingSink` for integration tests. Sieve's slice tests construct
`SamplingSink::new(RecordingSink::default(), HeadSampler::new(rate)?)`
and assert against the recorded records. This pattern is already in
use in Aperture's own tests; Sieve inherits it.

The `tests/invariant_public_api_smoke.rs` test in Sieve's crate
(per ADR-0018 §"Internal layout") asserts the type-level integration:

```rust
fn assert_sampling_sink_is_otlpsink_and_probe<S, N>()
where
    S: aperture::ports::OtlpSink + aperture::ports::Probe + Send + Sync + 'static,
    N: sieve::Sampler,
    sieve::SamplingSink<S, N>: aperture::ports::OtlpSink + aperture::ports::Probe,
{}
```

The function's body never runs; the trait bounds in the where-clause
are checked at compile time. This is the **subtype-check layer** of
the Earned-Trust three-layer pattern for the
`SamplingSink: OtlpSink + Probe` invariant.

## Alternatives Considered

### Option A — `SamplingSink<S, N>` decorator over `OtlpSink + Probe`, no Aperture amendment, consume `SinkRecord` directly (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Zero Aperture surface change. The integration is a pure
  composition-root edit at DELIVER time.
- Decorator pattern is the canonical Rust shape for "wrap a trait
  with a cross-cutting concern".
- `SinkRecord`'s three-variant shape exactly matches Q6's
  trace-sample-logs-passthrough-metrics-passthrough requirement.
- `Probe` delegation keeps the Earned-Trust invariant intact.
- Generic over `S` so the test path uses concrete types and the
  production path uses `Arc<dyn OtlpSink>`.

**Cons**:
- The trace-grouping pass allocates a `HashMap<[u8; 16], Vec<&Span>>`
  per `accept(SinkRecord::Traces(...))` call. One allocation per
  call (not per trace) is acceptable; a future optimisation can use
  a thread-local arena if measurement shows this is a hot point.

### Option B — Aperture grows a `Sampler` hook on `OtlpSink`

```rust
pub trait OtlpSink: Send + Sync + 'static {
    async fn accept(&self, record: SinkRecord) -> Result<(), SinkError>;
    fn before_accept(&self, record: &SinkRecord) -> ShouldAccept { ShouldAccept::Yes }
}
```

**Pros**:
- One trait, no decorator.

**Cons**:
- Bloats `OtlpSink` for one consumer's needs. Every other sink
  (StubSink, RecordingSink, Sluice, Pulse) has to implement (or
  no-op) the hook.
- ADR-0007 deliberately keeps `OtlpSink` minimal. Adding a hook
  reverses that decision for no behavioural gain.
- Sieve's grouping-by-trace-id pass cannot be expressed as a
  per-record hook; the hook would need a bigger surface
  (`before_accept_traces(&self, request: &ExportTraceServiceRequest) -> FilteredRequest`).

**Rejected** (ADR-0018 Option D rejected the symmetric Sieve-side
question).

### Option C — Aperture grows a separate `pub trait Filter` and the composition root chains `Filter::filter` before `OtlpSink::accept`

```rust
pub trait Filter: Send + Sync + 'static {
    fn filter(&self, record: SinkRecord) -> Option<SinkRecord>;
}
```

**Pros**:
- Keeps `OtlpSink` unchanged.
- A linear chain of filters is a familiar pipeline shape.

**Cons**:
- Aperture's composition root grows new types and new wiring code
  for Sieve. The decorator pattern (Option A) keeps Aperture's
  composition root at one wiring change (the `wire_sink` body) and
  pushes the implementation entirely into Sieve.
- Filters that need observability (per-decision DEBUG events,
  periodic INFO summary) become awkward; the timer task either
  lives on the Filter (which has no natural lifecycle) or on
  Aperture (which is exactly Option E that ADR-0020 rejected).

**Rejected** for the lifecycle awkwardness and the unnecessary
Aperture-surface growth.

### Option D — Sieve as a separate process binary fed by an Aperture-side OTLP exporter

**Pros**:
- Hard isolation between Sieve and Aperture.
- Sieve could be deployed independently of Aperture in a future
  ops scenario.

**Cons**:
- Adds a network hop and a full OTLP-in / OTLP-out crate's worth of
  plumbing for no v0 user value. DISCUSS Q1 explicitly rejects this
  shape: "library at v0; process shape is deferred to v1+".

**Rejected** by DISCUSS Q1.

## Consequences

### Positive

- Aperture's public surface is unchanged. No ADR amendment to
  ADR-0007. No SemVer breaking change on `aperture`.
- The integration is a three-line edit in `crates/aperture/src/compose.rs`
  at DELIVER time (build inner sink, wrap with `SamplingSink`, return
  the wrapped Arc).
- The Earned-Trust composition-root invariant `wire then probe then
  use` is preserved verbatim. The decorator's `probe()` delegates to
  the inner sink; startup refusal fires if the inner probe fails.
- `SinkRecord`'s three-variant shape exactly carries Q6's
  trace-sample-logs-passthrough-metrics-passthrough contract.

### Negative

- The trace-grouping pass allocates a `HashMap` per
  `accept(Traces(...))` call. A future optimisation can pool the
  allocation; v0 keeps the code simple.
- The decorator's generic parameters mean the `Arc<dyn OtlpSink>` in
  Aperture's composition root holds an erased `SamplingSink<S, N>`
  rather than a concrete one. This is exactly the existing pattern
  (Aperture already stores `Arc<dyn OtlpSink>`), so no new
  consequence arises.

### Trade-off ATAM

This decision is a **sensitivity point** for **Compatibility —
Interoperability** (positive: Aperture's public surface is
unchanged; every existing `OtlpSink` impl continues to work) and for
**Maintainability — Modifiability** (positive: the decorator is the
extension point for future features — a v1 PII-scrubbing decorator
would chain inside the same composition root).

It is a **trade-off point** between **Performance Efficiency —
Resource Utilisation** (negative: one `HashMap` allocation per
batch) and **Maintainability — Readability** (positive: the
grouping pass is a clear, single-purpose function). The trade is
accepted; if measurement shows the allocation is a hot point, a
future optimisation can pool the map without changing the public
contract.

## Self-Application of Earned Trust (principle 12)

The integration contract is enforced by three mechanisms (the
ArchUnit-style three-layer pattern):

1. **Subtype check (compile-time)** — the
   `tests/invariant_public_api_smoke.rs` test contains a function
   with where-clause bounds that fail to compile if `SamplingSink<S,
   N>` does not implement `OtlpSink + Probe`. Sieve's compile gate
   rejects any mistake.
2. **Structural check (CI)** — Aperture's existing xtask AST walk
   covers "every type implementing `OtlpSink` also implements
   `Probe`". `SamplingSink<S, N>` is one such type; the walker
   verifies it automatically. Per ADR-0007 §"Self-Application", the
   walk is the structural layer; this ADR adds a new type that
   the walker covers.
3. **Behavioural check (CI)** — the slice-01 through slice-06 tests
   exercise the decorator's `accept` body via realistic fixture
   `ExportTraceServiceRequest`s, the `RecordingSink` test double, and
   captured `tracing` events. A regression in the decorator (e.g. a
   passthrough variant that accidentally drops the request, a Drop
   variant that forwards anyway) is caught by the slice tests.

A change that bypasses one layer is caught by another. The
integration contract — `SamplingSink<S, N>: OtlpSink + Probe`,
delegation of `probe()`, trace-grouping pass with per-decision
events, kept-traces-only forwarding to the inner sink — is defended
in depth.

The probe-delegation pattern is the **explicit application** of
agent principle 12 to the decorator: Sieve's design says "the only
external dependency I have is the inner sink; my `probe()` empirically
demonstrates that I can honour my contract iff the inner sink can
honour its contract; nothing else needs probing because nothing else
is external". This is not a convenience — it is the honest
application of the "wire then probe then use" invariant to a stage
that has no I/O of its own.
