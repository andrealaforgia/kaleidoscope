# ADR-0018 — `sieve` public API surface and crate layout

- **Status**: Accepted
- **Date**: 2026-05-06
- **Author**: `@nw-solution-architect` (Morgan)
- **Feature**: `sieve` v0
- **Supersedes**: none
- **Superseded by**: none

## Context

Sieve v0 is the AGPL-3.0-or-later platform component that sits between
Aperture (the OTLP gateway) and the next pipeline stage. At v0 it
performs head-based probabilistic sampling on trace data with
error-bias retention; logs and metrics pass through unchanged. The
DISCUSS wave locks the **scope** (eight Q-decisions), the **stories**
(six LeanUX stories with embedded ACs), and the **slices** (six
elephant-carpaccio increments). DISCUSS deliberately leaves the
**exact public surface** to DESIGN.

Sentinel's APPROVED peer review and the Technical Notes at the foot of
each user story flag two interlocking DESIGN questions:

- **D1**: the exact `Sampler::sample` signature — does it take a slice
  of spans, an iterator, or borrow Aperture's batch type?
- **D2**: the `Decision` enum shape — sealed `Decision { Keep, Drop }`,
  or carry metadata such as `Decision::Keep { reason: KeepReason }`?

Per agent principle 9 (small public surfaces, decorator over wrappers),
agent principle 11 (architecture rules enforced via tooling, not
convention), and the Spark precedent in ADR-0011 (small public
surface, modules-from-day-one, `cargo public-api` lock), the public
surface for Sieve must be the smallest set of items the six slices
genuinely need.

The `shared-artifacts-registry.md > Sampler trait` and `> Decision
enum` entries classify public-API drift as **HIGH integration risk**.

## Decision

### Public surface (final list)

```rust
// from lib.rs, alphabetised:

/// Sampling decision: keep the trace or drop it. Sealed: the only two
/// possible outcomes a head sampler can produce. Observability metadata
/// (why a trace was kept) is NOT carried here; it travels via the
/// `KeepReason` enum on the DEBUG tracing event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decision {
    Keep,
    Drop,
}

/// Concrete head-based sampler. Constructed with a non-error rate in
/// `[0.0, 1.0]`. Decisions are deterministic functions of the trace's
/// `trace_id` (via xxh3_64 per ADR-0019).
pub struct HeadSampler { /* fields private */ }

impl HeadSampler {
    /// Construct a head sampler at the given non-error rate. Returns
    /// `Err(SieveConfigError::RateOutOfRange)` if `rate` is NaN or
    /// outside `[0.0, 1.0]`.
    pub fn new(rate: f64) -> Result<Self, SieveConfigError>;

    /// Construct a head sampler reading the rate from the
    /// `SIEVE_NON_ERROR_TRACE_RATE` environment variable. Defaults to
    /// 0.1 if unset. Returns `Err(SieveConfigError::*)` on parse
    /// failure or out-of-range value.
    pub fn from_env() -> Result<Self, SieveConfigError>;

    /// The configured non-error rate. Surfaced for the periodic INFO
    /// summary (it carries the rate so operators can confirm the
    /// configured value without reading config).
    pub fn rate(&self) -> f64;
}

/// Reason a trace was kept. Carried only by DEBUG tracing events; NOT
/// returned from `Sampler::sample`. Sealed at v0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeepReason {
    /// At least one span carried `status.code == ERROR`.
    ErrorBearing,
    /// The trace_id-keyed hash fell below the configured rate.
    Sampled,
}

/// Sieve's configuration error surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SieveConfigError {
    #[error("rate must be a finite value in [0.0, 1.0]; got {got}")]
    RateOutOfRange { got: f64 },
    #[error("rate value '{raw}' is not parseable as a float")]
    RateUnparseable { raw: String },
    #[error("summary tick value '{raw}' is not parseable as a positive integer (milliseconds)")]
    SummaryTickUnparseable { raw: String },
}

/// The sampling-decision contract Sieve exposes. Implemented by
/// `HeadSampler` at v0. Consumers programme against this trait; the
/// `SamplingSink` decorator does so internally (generic parameter `N`).
pub trait Sampler: Send + Sync + 'static {
    fn sample(&self, trace: &TraceView<'_>) -> Decision;
}

/// `OtlpSink + Probe` decorator that adds head-based sampling on the
/// `Traces` variant and forwards `Logs` / `Metrics` unchanged. Generic
/// over the inner sink type `S` and the sampler type `N` so the test
/// path uses concrete types and the production path uses
/// `Arc<dyn OtlpSink + Probe>` via Aperture's existing pattern.
pub struct SamplingSink<S, N>
where
    S: aperture::ports::OtlpSink + aperture::ports::Probe,
    N: Sampler,
{ /* fields private */ }

impl<S, N> SamplingSink<S, N>
where
    S: aperture::ports::OtlpSink + aperture::ports::Probe,
    N: Sampler,
{
    /// Wrap the inner sink with the given sampler. Spawns the periodic
    /// summary task on the ambient Tokio runtime per ADR-0020.
    pub fn new(inner: S, sampler: N) -> Self;
}

// `SamplingSink<S, N>` implements `aperture::ports::OtlpSink` and
// `aperture::ports::Probe` (impls below; implementation in `decorator.rs`).

/// Borrowed view over a logical trace's spans. The decorator's grouping
/// pass builds these views from `ExportTraceServiceRequest::resource_spans`
/// once per batch; the sampler reads `trace_id` and (for `is_error_bearing`)
/// iterates the spans without taking ownership.
pub struct TraceView<'a> { /* fields private; constructed inside `decorator.rs` */ }

impl<'a> TraceView<'a> {
    /// The 16-byte OTLP trace_id keyed across this view's spans.
    pub fn trace_id(&self) -> [u8; 16];

    /// Iterate the spans in order. The lifetime ties the iterator to
    /// the underlying `ExportTraceServiceRequest`.
    pub fn spans(&self) -> impl Iterator<Item = &'a opentelemetry_proto::tonic::trace::v1::Span> + '_;
}

// Test seams (NOT part of the consumer-facing API contract):
#[doc(hidden)]
pub fn __test_trace_view<'a>(
    trace_id: [u8; 16],
    spans: &'a [opentelemetry_proto::tonic::trace::v1::Span],
) -> TraceView<'a>;

#[doc(hidden)]
pub fn __test_summary_tick_now<S, N>(sink: &SamplingSink<S, N>)
where
    S: aperture::ports::OtlpSink + aperture::ports::Probe,
    N: Sampler;
```

That is the entire consumer-facing public surface. The Spark
ADR-0011 precedent (no re-exports of upstream OTel types) holds: Sieve
does **not** re-export `opentelemetry_proto`, `aperture::ports`, or
`tracing`. Consumers (Aperture's composition root, the integration
tests) depend on those crates directly.

The two `#[doc(hidden)]` test seams follow the same `__` prefix +
`#[doc(hidden)]` convention Spark adopted (ADR-0011 §"Test seam"). The
first seam constructs a `TraceView` from a fixture in tests without
running the decorator's grouping pass; the second seam fires the summary
tick synchronously so a slice-06 test does not depend on wall-clock
time.

### Why the borrowed `TraceView` (D1 resolution)

DISCUSS leaves the signature open: slice of spans, iterator, or borrow
the upstream batch type. The choice has three load-bearing constraints:

1. **The error-bias rule scans every span** (`status.code == ERROR`
   anywhere → keep). A slice-of-spans signature works for that.
2. **The rate-based decision needs the trace_id** as a 16-byte value to
   feed `xxh3_64`. A bare `&[Span]` signature would force the sampler
   to look up the trace_id from the first span, but every span in a
   trace carries the same trace_id, so any of them works. Still, an
   addressable `trace_id()` method on the view is friendlier than a
   `spans[0].trace_id` deref.
3. **The decorator builds traces from `ExportTraceServiceRequest`** —
   the upstream OTLP envelope groups spans by `ResourceSpans` and
   `ScopeSpans`, NOT by trace_id. The decorator runs a one-pass
   grouping (`HashMap<[u8; 16], Vec<&Span>>`) before invoking the
   sampler. Once the grouping is done, the natural shape passed to the
   sampler is `(trace_id, &[&Span])`, and a `TraceView<'a>` wraps that
   pair without reshape.

The borrowed view is **the smallest shape that the test harness can
construct** (see the `__test_trace_view` seam) and Aperture (via the
decorator) **can call without an allocation per batch** (the
`HashMap<[u8; 16], Vec<&Span>>` is one allocation per call into
`SamplingSink::accept`, not one per trace; the `TraceView`s borrow into
the map's `Vec`s).

### Why sealed `Decision` plus separate `KeepReason` (D2 resolution)

DISCUSS leaves the choice open: `Decision { Keep, Drop }` or
`Decision::Keep { reason: KeepReason }`.

The arguments for the metadata-carrying variant: observability is
easier; one return value, one piece of evidence. The arguments
against: every consumer that simply wants to filter (Aperture's
decorator does exactly this — keep means "forward to inner sink", drop
means "do nothing") has to either match-and-discard the metadata or
ignore it via `_`. The decorator does not need the reason for routing.

The metadata is needed in exactly one place: the DEBUG tracing event
emitted by `SamplingSink::accept`. The cleanest shape is therefore:

- `Decision { Keep, Drop }` is what the sampler returns and the
  decorator routes on (sealed two-variant enum, `Copy`, hashable).
- `KeepReason { ErrorBearing, Sampled }` is what the decorator passes
  to the tracing event. The decorator computes both: it asks the
  sampler for the decision AND, when the decision is Keep, it asks the
  sampler "was this kept because of error-bias?" via a small private
  helper, OR it computes `is_error_bearing(trace)` itself before
  asking the sampler. The latter is the chosen path (it keeps the
  `Sampler` trait minimal — one method, one return value).

The decorator's `accept` body therefore looks like (sketch only —
implementation is software-crafter's territory):

```text
for trace in group_by_trace_id(request) {
    let reason = if is_error_bearing(&trace) {
        KeepReason::ErrorBearing
    } else {
        // sampler.sample returns Keep iff non-error rate keeps it
        KeepReason::Sampled
    };
    match sampler.sample(&trace) {
        Decision::Keep => { emit_debug_kept(reason); forward(&trace); }
        Decision::Drop => { emit_debug_dropped(); }
    }
}
```

This shape gives the test the cleanest possible assertion (`assert_eq!(
sampler.sample(&trace), Decision::Keep)`), gives the decorator the
metadata it needs for observability, and keeps the `Sampler` trait at
one method.

### `HeadSampler::sample` mechanism

```text
HeadSampler::sample(trace):
    if any span in trace has status.code == ERROR:
        return Decision::Keep
    let h = xxh3_64(trace.trace_id())
    let mapped = (h as f64) / (u64::MAX as f64)        // in [0.0, 1.0]
    if mapped < self.rate:
        return Decision::Keep
    else:
        return Decision::Drop
```

The mechanism collapses slice 03 and slice 04 into one deterministic
path:

- Slice 03's "10000-trace fixture, ±3% band at rate 0.5" works because
  the `trace_id` distribution in the fixture is uniform; the
  xxh3_64-mapped values are uniform in `[0.0, 1.0]`; comparing against
  `rate = 0.5` yields ≈5000 kept.
- Slice 04's "same trace_id always yields the same decision" is a
  direct property of the deterministic mapping: same input, same
  output.

There is therefore no separate "RandomSource" abstraction (slice 03's
Technical Note flagged this as a possibility). The probability source
is the deterministic hash, and the test path uses deterministic
fixture trace_ids. This collapse is **internal to DESIGN** and does
not require a DISCUSS amendment because the slice-03 brief's stated
constraint ("the test path needs determinism") is satisfied verbatim.

### Boundary semantics for rate

- `rate == 0.0`: at least one of the two checks fails for every
  non-error trace. The natural reading of `mapped < 0.0` is "always
  false" because `mapped >= 0.0`. Therefore at `rate = 0.0`, no
  non-error trace is kept. The fixture in slice-03's brief tolerates
  ±2 traces; the implementation can hit exactly 0.
- `rate == 1.0`: `mapped < 1.0` is true except when `mapped == 1.0`,
  which can happen if `h == u64::MAX`. The probability of this is
  `1 / 2^64` — vanishingly small but not zero. The implementation
  uses `mapped <= rate` when `rate == 1.0` (a special case) so all
  non-error traces are kept exactly. This matches slice-03's brief
  ("rate 1.0 must be exactly all kept"). The crafter's exact
  implementation may differ; the public-behaviour contract is "at
  rate 1.0, every non-error trace is kept, regardless of trace_id".

### Internal layout

```
crates/sieve/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs            # public surface front door: re-exports of
│   │                     # Decision, HeadSampler, KeepReason, Sampler,
│   │                     # SamplingSink, SieveConfigError, TraceView
│   │                     # from the named internal modules. Crate-root
│   │                     # `#![forbid(unsafe_code)]` is here.
│   ├── sampler.rs        # `pub trait Sampler`, `pub struct HeadSampler`,
│   │                     # the `xxh3_64` mapping, the rate parse from
│   │                     # `SIEVE_NON_ERROR_TRACE_RATE`. Free function
│   │                     # `is_error_bearing(spans) -> bool` (kept
│   │                     # `pub(crate)` so the decorator can call it
│   │                     # without going through the trait).
│   ├── decision.rs       # `pub enum Decision`, `pub enum KeepReason`,
│   │                     # `pub struct TraceView`, the test seam
│   │                     # `__test_trace_view`. Small file; one place
│   │                     # for "what is observable on the wire".
│   ├── decorator.rs      # `pub struct SamplingSink<S, N>`, the OtlpSink
│   │                     # impl, the Probe impl (delegates to inner),
│   │                     # the trace-grouping pass over
│   │                     # `ExportTraceServiceRequest::resource_spans`,
│   │                     # the per-decision DEBUG event emission.
│   ├── aggregator.rs     # `pub(crate) struct Counters` with three
│   │                     # `AtomicU64`s; the timer-task spawn-and-join
│   │                     # logic; the snapshot-and-emit-INFO body. The
│   │                     # `__test_summary_tick_now` test seam fires
│   │                     # the snapshot path synchronously (per ADR-0020).
│   ├── error.rs          # `pub enum SieveConfigError` + thiserror.
│   └── observability.rs  # `pub(crate)` helpers for the tracing event
│                         # vocabulary: `target = "sieve"` always; one
│                         # function per decision shape so the structured
│                         # field set is consistent.
└── tests/
    ├── slice_01_walking_skeleton.rs
    ├── slice_02_error_bias.rs
    ├── slice_03_non_error_rate.rs
    ├── slice_04_trace_id_determinism.rs
    ├── slice_05_logs_metrics_passthrough.rs
    ├── slice_06_observability.rs
    └── invariant_public_api_smoke.rs   # asserts the public-surface
                                        # items compile and the expected
                                        # constructors return Ok for
                                        # nominal inputs.
```

The split-from-day-one decision mirrors Spark ADR-0011 §"Internal
layout". Each file is one concept; the tests/ layout is one binary per
slice plus one cross-cutting public-API smoke test. No `examples/`
directory at v0 (Sieve is not a developer-facing SDK; consumers are
Aperture's composition root and the slice tests).

### Cargo.toml skeleton

```toml
[package]
name = "sieve"
version = "0.1.0"
edition.workspace = true
license = "AGPL-3.0-or-later"   # platform component per LICENSING.md
rust-version.workspace = true
description = "Kaleidoscope's head-based trace sampler with error-bias retention. AGPL-3.0-or-later platform component; integrates with Aperture's `OtlpSink` via a generic decorator."
repository = "https://github.com/andrealaforgia/kaleidoscope"
publish = false

[lib]
path = "src/lib.rs"

[dependencies]
# Per ADR-0019.
aperture = { path = "../aperture", version = "0.1.0" }
async-trait = "0.1"
opentelemetry-proto.workspace = true
thiserror = "2"
tokio = { version = "1.40", features = ["macros", "rt", "sync", "time"] }
tokio-util = "0.7"
tracing = "0.1"
xxhash-rust = { version = "=0.8", features = ["xxh3"] }

[dev-dependencies]
tokio = { version = "1.40", features = ["full", "test-util"] }
tracing-subscriber = { version = "0.3", default-features = false, features = ["fmt", "env-filter", "registry"] }

[[test]]
name = "slice_01_walking_skeleton"
path = "tests/slice_01_walking_skeleton.rs"

[[test]]
name = "slice_02_error_bias"
path = "tests/slice_02_error_bias.rs"

[[test]]
name = "slice_03_non_error_rate"
path = "tests/slice_03_non_error_rate.rs"

[[test]]
name = "slice_04_trace_id_determinism"
path = "tests/slice_04_trace_id_determinism.rs"

[[test]]
name = "slice_05_logs_metrics_passthrough"
path = "tests/slice_05_logs_metrics_passthrough.rs"

[[test]]
name = "slice_06_observability"
path = "tests/slice_06_observability.rs"

[[test]]
name = "invariant_public_api_smoke"
path = "tests/invariant_public_api_smoke.rs"
```

The workspace `Cargo.toml` adds `crates/sieve` to `[workspace]
members`. The `aperture` runtime dependency is **acceptable** here
because Sieve is itself AGPL — Sieve consumes Aperture's public surface
(the `OtlpSink` and `Probe` traits, the `SinkRecord` enum, the
`SinkError` and `ProbeError` error types) the same way Aperture
consumes `opentelemetry_proto`. Spark cannot depend on Aperture at
runtime because Spark is Apache-2.0; Sieve's licence is symmetric
with Aperture's, so the dependency is licence-clean.

### CI gates (mirrored from ADR-0005, scoped to `crates/sieve/**`)

Five blocking gates, identical mechanism to Aperture and Spark:

1. `cargo test --workspace --all-targets --locked` — runs Sieve's
   slice tests, the public-API smoke test, the unit tests inside each
   module.
2. `cargo public-api --diff-git-checkouts main HEAD -p sieve` — locks
   the public surface above. Empty diff is the steady state.
3. `cargo semver-checks check-release -p sieve --baseline-rev main` —
   SemVer-aware compatibility. Variants on `Decision`, `KeepReason`,
   `SieveConfigError`; methods on `HeadSampler`; fields on
   `SamplingSink` and `TraceView` are the load-bearing surface.
4. `cargo deny check` — licence policy + advisories + pin policy. The
   workspace's `deny.toml` gets one new line for `xxhash-rust` (BSL-1.0
   / MIT both permitted; ADR-0019 documents the BSL-1.0 acceptance
   rationale).
5. `cargo mutants --package sieve --in-diff` (DEVOPS workflow:
   `gate-5-mutants-sieve`). 100% kill rate per ADR-0005 Gate 5.

The five gates execute in any order; they are independent. DEVOPS
chooses the runner specifics (`gate-5-mutants-sieve.yml` mirrors
`gate-5-mutants-aperture.yml`).

## Alternatives Considered

### Option A — Public `Sampler` trait + `HeadSampler` concrete + `SamplingSink<S, N>` decorator + sealed `Decision` + separate `KeepReason` (RECOMMENDED, accepted)

Detailed above.

**Pros**:
- Smallest possible public surface for the contract DISCUSS locks
  (one trait, one concrete, one decorator, three enums, one borrowed
  view).
- The decorator is the natural shape that integrates with Aperture's
  existing `OtlpSink + Probe` seam without an Aperture-side trait
  amendment (per ADR-0021).
- Sealed `Decision` keeps the routing branch clean in the decorator.
- The `KeepReason` lives on the side, so the trait stays at one
  method and the observability concern does not pollute the decision
  return value.
- `cargo public-api` keeps the surface stable.
- The internal-module split lets each concept (sampler, decision,
  decorator, aggregator, error, observability) live in its own file
  from day one, mirroring Spark ADR-0011.

**Cons**:
- A future `Sampler` impl that wants to surface a different keep
  reason (e.g. tail-sampling's "kept because of latency outlier")
  needs to extend the `KeepReason` enum. `KeepReason` is
  `#[non_exhaustive]` so this is additive and non-breaking. Acceptable.

### Option B — Metadata-carrying `Decision::Keep { reason: KeepReason }`

```rust
pub enum Decision {
    Keep { reason: KeepReason },
    Drop,
}
```

**Pros**:
- One return value carries everything observability needs.
- No risk of the decorator and the sampler disagreeing about the
  reason.

**Cons**:
- Every consumer that only wants to filter (the decorator's primary
  job) has to write `match d { Decision::Keep { .. } => ..., Decision::
  Drop => ... }`. Idiomatic Rust would let users write `if matches!(d,
  Decision::Keep { .. })` but the variant-with-fields shape is less
  ergonomic than the unit variant.
- Equality, `Copy`, and hashing become awkward (`KeepReason` would
  have to be `Copy + Eq` too).
- Couples the decision to the observability concern. If a future
  release wants to add a third keep reason (e.g. "kept because of an
  always-keep allow-list"), every match site widens.

**Rejected** for the ergonomic and coupling concerns.

### Option C — Raw `&[Span]` signature with no `TraceView`

```rust
pub trait Sampler: Send + Sync + 'static {
    fn sample(&self, spans: &[Span]) -> Decision;
}
```

**Pros**:
- Simpler signature.
- No new public type.

**Cons**:
- The decorator must group spans by trace_id before calling the
  sampler. If the signature is `&[Span]`, the grouping pass produces
  `Vec<&Span>` per trace, and the sampler then has to deref
  `spans[0].trace_id` (16 bytes copied out of the proto-generated
  `Span` struct) to compute the hash — every call. A `TraceView` with
  a precomputed `trace_id` field skips that work and makes the
  invariant "the trace_id is the same across all spans of a trace"
  explicit at the type level rather than implicit at the call site.
- The test harness has to construct a `Vec<Span>` with all the
  bytes set correctly, including the trace_id on every span. The
  `__test_trace_view` seam constructs a view from a separately-given
  trace_id and a `&[Span]`, decoupling the two.

**Rejected** for the trace_id ergonomics and the test-harness cost.

### Option D — Aperture amends `OtlpSink` with a `before_accept` hook

Aperture's `OtlpSink` could grow a method like `before_accept(&self,
record: &SinkRecord) -> SinkRecord` that Sieve overrides; the rest of
the system does not change.

**Pros**:
- No decorator; one trait, one impl per crate.

**Cons**:
- Bloats `OtlpSink`'s contract for one consumer. Every other
  `OtlpSink` impl (Aperture's `StubSink`, `RecordingSink` in the
  `testing` module, and any future sink — Sluice, Pulse, etc.) has to
  implement (or no-op) the hook.
- ADR-0007 deliberately keeps `OtlpSink` minimal (one async method
  plus the orthogonal `Probe` trait). Adding a hook just for Sieve
  reverses that decision.
- The decorator pattern is the canonical Rust shape for "wrap a trait
  to add a cross-cutting concern" and is exactly what Aperture's
  `Arc<dyn OtlpSink>` storage encourages.

**Rejected** for the violation of ADR-0007's minimal-contract posture
and the coupling to a single consumer's needs.

### Option E — Split `sieve-core` (the trait, the sampler, the enums) and `sieve-aperture` (the decorator)

**Pros**:
- The core crate would have no `aperture` dependency and could
  theoretically be re-used by a future non-Aperture pipeline.

**Cons**:
- Speculative generality. There is no second pipeline at v0; there
  may never be one. Splitting now adds two crates, two `Cargo.toml`s,
  two licence declarations, and two CI workflows for a benefit that
  exists only in imagination.
- The DISCUSS Q1 decision is "library at v0"; the simplest library
  shape is one crate.

**Rejected** for premature abstraction. If a v1 pipeline emerges that
genuinely needs the split, the crate can be split with a deprecation
cycle.

## Consequences

### Positive

- Consumer-facing public surface is exactly seven items: `Sampler`,
  `HeadSampler`, `SamplingSink<S, N>`, `Decision`, `KeepReason`,
  `SieveConfigError`, `TraceView<'a>`. `cargo public-api` keeps that
  locked.
- The decorator pattern lets Aperture's composition root wire Sieve
  without amending `OtlpSink`.
- The sealed `Decision` plus separate `KeepReason` keeps the trait at
  one method; the decorator owns the observability concern.
- The internal-module split commits the crafter to per-concept
  boundaries from day one (Spark ADR-0011 made the same trade-off and
  it landed cleanly).

### Negative

- The decorator owns more responsibility than the sampler (grouping,
  per-decision events, counter updates). The aggregator module
  contains the timer-task lifecycle. This is honest: the sampler is
  pure and small; the orchestration lives one layer up.
- The `__test_summary_tick_now` test seam adds a doc-hidden public
  function. Mirrors Spark's `__reset_for_testing` precedent; the
  convention is established.

### Trade-off ATAM

This decision is a **sensitivity point** for **Maintainability —
Modifiability** (positive: minimum surface, additive evolution via
non-exhaustive enums and the decorator's generic parameters) and for
**Functional Suitability — Appropriateness** (positive: matches the six
slice test invocations exactly; the borrowed `TraceView` is the shape
each test naturally constructs).

It is a **trade-off point** between **Maintainability — Modifiability**
and **Compatibility — Interoperability**: the decorator approach keeps
Aperture's `OtlpSink` contract closed (good for compatibility — no
existing sink implementations break) but pushes the integration work
into Aperture's composition root (a small modifiability cost paid once,
documented in ADR-0021). The trade is accepted because every other
`OtlpSink` impl currently in the workspace (StubSink, RecordingSink) is
unaffected by Sieve's existence.

## Self-Application of Earned Trust (principle 12)

The public-surface contract is enforced by three mechanisms (the
ArchUnit-style three-layer pattern):

1. **Subtype check (compile-time)** — `cargo public-api` (Gate 2)
   reads the type-checked surface and fails the build on any drift not
   accompanied by a version bump.
2. **Structural check (CI)** — `cargo semver-checks` (Gate 3) walks
   the SemVer rules: removed variants on `Decision` / `KeepReason` /
   `SieveConfigError`, signature changes on `Sampler::sample`,
   narrowed trait bounds on `SamplingSink`'s generics. A SemVer minor
   bump that should have been major fails Gate 3.
3. **Behavioural check (CI)** — the integration tests under `tests/`
   exercise the public surface via realistic fixture traces and the
   `tracing_subscriber` test layer. A surface change that compiles and
   is semver-clean but breaks the contract (e.g. `HeadSampler::new`
   stops accepting `0.5`) is caught by Gate 1.

A change that bypasses one layer is caught by another. There is no
scenario where the public-surface contract erodes silently.

The Aperture-side enforcement holds verbatim: the existing xtask AST
walk that verifies "every type implementing `OtlpSink` also implements
`Probe`" applies to `SamplingSink<S, N>` automatically because
`SamplingSink` is one such type. ADR-0021 documents the integration
mechanics; this ADR establishes the surface the integration uses.
