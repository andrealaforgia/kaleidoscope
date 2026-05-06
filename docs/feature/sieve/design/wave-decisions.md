# Sieve v0 — DESIGN wave decisions

- **Date**: 2026-05-06
- **Author**: `@nw-solution-architect` (Morgan)
- **Wave**: DESIGN (single iteration; reviewer is dispatched separately
  by Bea)
- **Inputs**: `docs/feature/sieve/discuss/` (eight scope decisions Q1–Q8;
  six LeanUX stories; six slice briefs; KPI table; shared-artefacts
  registry; Sentinel's APPROVED peer review with Findings 1 and 2 closed
  inline by Bea)
- **Outputs**: this file plus C4 diagrams, technology-choices table,
  slice-mapping; four ADRs (ADR-0018 through ADR-0021) at
  `docs/product/architecture/`

## Inputs read

| File | Status |
|------|--------|
| `docs/feature/sieve/discuss/wave-decisions.md` | ✓ |
| `docs/feature/sieve/discuss/peer-review.md` | ✓ |
| `docs/feature/sieve/discuss/user-stories.md` | ✓ |
| `docs/feature/sieve/discuss/journey-sieve.yaml` | ✓ |
| `docs/feature/sieve/discuss/shared-artifacts-registry.md` | ✓ |
| `docs/feature/sieve/discuss/outcome-kpis.md` | ✓ |
| `docs/feature/sieve/slices/slice-01-walking-skeleton.md` | ✓ |
| `docs/feature/sieve/slices/slice-02-error-bias.md` | ✓ |
| `docs/feature/sieve/slices/slice-03-non-error-rate.md` | ✓ |
| `docs/feature/sieve/slices/slice-04-trace-id-determinism.md` | ✓ |
| `docs/feature/sieve/slices/slice-05-logs-metrics-passthrough.md` | ✓ |
| `docs/feature/sieve/slices/slice-06-observability.md` | ✓ |
| `crates/aperture/src/lib.rs` | ✓ |
| `crates/aperture/src/ports/mod.rs` | ✓ |
| `LICENSING.md` | ✓ |
| `docs/product/architecture/adr-0007-otlpsink-trait-design.md` | ✓ |
| `docs/product/architecture/adr-0011-spark-public-api-and-crate-layout.md` | ✓ |
| `docs/product/architecture/adr-0013-spark-dependency-pinning.md` | ✓ |
| `docs/product/architecture/adr-0005-ci-contract.md` | ✓ |
| `Cargo.toml` (workspace) | ✓ |

## DESIGN questions resolved

DISCUSS Q1–Q8 are scope locks. DESIGN closes six implementation
questions Sentinel and the Technical Notes flagged:

| # | Question | Decision (one-line) | ADR |
|---|----------|---------------------|-----|
| D1 | `Sampler::sample` signature | `fn sample(&self, trace: &TraceView<'_>) -> Decision` where `TraceView` is a thin `&[Span]`-borrowing view exposing `trace_id()` and `spans()` | ADR-0018 |
| D2 | `Decision` enum shape | sealed two-variant `enum Decision { Keep, Drop }` plus a separate structured `KeepReason` carried by the observability event, not by the return value | ADR-0018 |
| D3 | Aperture integration point | Sieve crate exposes `sieve::SamplingSink<S, N>` — a generic `OtlpSink + Probe` decorator wrapping any inner sink. Aperture v0.1.0 needs **no** trait amendment because `OtlpSink` is already the integration seam. The DELIVER work composes Sieve's decorator into Aperture's `compose::wire_sink` | ADR-0021 |
| D4 | Summary aggregator synchronisation | three `AtomicU64` counters (`kept_total`, `kept_error_bearing`, `dropped`) updated on the hot path; the timer task snapshots and resets them with `swap` | ADR-0020 |
| D5 | Timer task ownership | owned by Sieve, spawned at `SamplingSink::new` on the ambient Tokio runtime, joined when the `SamplingSink` is dropped (cooperative cancellation via `tokio_util::sync::CancellationToken`) | ADR-0020 |
| D6 | Logs/metrics pipeline shape | Sieve consumes Aperture's existing three-variant `SinkRecord` enum (`Logs` / `Traces` / `Metrics`); the decorator dispatches `Traces` to the sampler and forwards `Logs` / `Metrics` to the inner sink unchanged. No Sieve-local signal enum | ADR-0021 |

D1 picks a borrowed view rather than `&[Span]` so the trace-id is
addressable without rescanning the span list, and rather than the
upstream OTLP batch type so the unit tests can construct fixtures
without building an `ExportTraceServiceRequest`. The view is built once
per trace by the decorator's grouping pass; the sampler does not
allocate.

## ADR table

| ADR | Title | Locks |
|-----|-------|-------|
| [ADR-0018](../../../product/architecture/adr-0018-sieve-public-api-and-crate-layout.md) | Sieve public API and crate layout | `Sampler` trait, `Decision` enum, `HeadSampler` concrete, `SamplingSink<S, N>` decorator, `KeepReason`, `TraceView`, internal module split |
| [ADR-0019](../../../product/architecture/adr-0019-sieve-dependency-pinning.md) | Sieve dependency pinning | `xxhash-rust` exact-pin (BSL-1.0 / MIT), `async-trait` from workspace, `tokio` features, `tokio-util` for cancellation, dev-dep `aperture` (AGPL containment), `tracing-subscriber` test-only |
| [ADR-0020](../../../product/architecture/adr-0020-sieve-summary-aggregator.md) | Sieve summary aggregator and timer task | `AtomicU64` counters, snapshot-and-reset via `swap`, `tokio::time::interval`, parameterisable tick (`SIEVE_SUMMARY_TICK_MS` env var defaulting to 60_000), cooperative cancellation, drop-time final flush |
| [ADR-0021](../../../product/architecture/adr-0021-sieve-aperture-integration.md) | Sieve–Aperture integration point | `SamplingSink<S, N>` as `OtlpSink + Probe` decorator over any inner `OtlpSink + Probe`; trace-grouping pass on `ExportTraceServiceRequest`; no Aperture-side trait amendment |

Four ADRs total. No padding ADRs.

## Architectural style

Modular monolith with **dependency-inversion / decorator** posture. The
Sieve crate is one library with three named modules
(`sampler`, `aggregator`, `decorator`); the public surface is a
`Sampler` trait, a `Decision` enum, a `HeadSampler` concrete, a
`SamplingSink<S, N>` decorator, a `KeepReason` enum, and a
`TraceView<'a>` borrowed view. Wiring happens in Aperture's
composition root: `compose::wire_sink` returns
`SamplingSink::new(inner_sink, HeadSampler::from_env()?)` instead of
the bare inner sink.

Default per agent principle 8 (modular monolith with ports-and-adapters)
applies here without override. The decorator lives in the same crate as
the sampler because the slice plan does not justify a sub-crate split.

## Earned-Trust posture (principle 12)

Every Sieve adapter implementing `OtlpSink` from Aperture must also
implement Aperture's `Probe`. `SamplingSink<S, N>` delegates `probe()`
to its inner sink and adds zero probe surface of its own — Sieve is a
pure-CPU stage with no external dependencies, and the sampling rate
parse runs at construction (not at probe time). The decorator's
`probe()` therefore returns `inner.probe().await`, preserving the
composition-root invariant `wire then probe then use`.

The composition root in Aperture remains the single point that calls
`probe()`; Sieve's wrapper surfaces probe failure unchanged. The
ArchUnit-style three-layer enforcement from ADR-0007 carries over: the
xtask AST walk that verifies "every type implementing `OtlpSink` also
implements `Probe`" applies to `SamplingSink<S, N>` automatically,
because `SamplingSink` is one such type. No new enforcement is added
in this DESIGN.

## ISO 25010 quality attributes

| Attribute | Strategy |
|-----------|----------|
| Functional Suitability — Appropriateness | One trait, one decorator, one concrete sampler. The four (five with `KeepReason`) public items map directly onto the six user stories |
| Performance Efficiency — Resource utilisation | `xxh3_64` is roughly an order of magnitude faster than `SipHasher`; `AtomicU64` counters take no lock on the hot path; the decorator borrows the inner sink via `Arc<S>` and clones cheaply |
| Reliability — Fault tolerance | The timer task uses `CancellationToken` so a `SamplingSink::drop` always joins it; the final summary flush on drop preserves accounting on early shutdown |
| Reliability — Recoverability | Sieve has no in-memory window across batches; no recovery posture is required at v0 |
| Maintainability — Modifiability | Three named modules behind a four-item public surface; `cargo public-api` Gate 2 catches drift; `cargo semver-checks` Gate 3 catches non-additive changes |
| Maintainability — Testability | The probability source is injected via a `RandomSource` trait; the timer interval is parameterisable via env var; the tracing target `"sieve"` makes capture-and-assert ergonomic |
| Compatibility — Interoperability | The decorator consumes Aperture's `OtlpSink` / `SinkRecord` / `Probe` / `SinkError` / `ProbeError` directly; no Sieve-local wrapper types shadow Aperture's surface |
| Security — Confidentiality | v0 has no PII handling; the v1 PII-scrubbing rules are out of scope per Q4 |
| Security — Integrity | `forbid(unsafe_code)`; no panic paths in the hot loop (rate parse fails fast at construction) |

## Architectural enforcement tooling

| Mechanism | Tool | What it enforces |
|-----------|------|------------------|
| Public-API lock | `cargo public-api -p sieve` (Gate 2) | Sieve's public surface stays at the items locked by ADR-0018 |
| SemVer correctness | `cargo semver-checks -p sieve` (Gate 3) | Additive-only changes between releases (variants on `Decision`, methods on `HeadSampler`, fields on the `SamplingSink` struct) |
| Licence policy | `cargo deny check` (Gate 4) | `xxhash-rust` (BSL-1.0 / MIT) accepted; AGPL refused in runtime closure (defends the dev-dep-only `aperture` posture; see ADR-0019) |
| Mutation kill rate | `cargo mutants --package sieve --in-diff` (Gate 5) | 100% kill rate per ADR-0005 Gate 5 |
| Earned-Trust structural layer | xtask AST walk (already in repo for Aperture) | Every type implementing `OtlpSink` also implements `Probe`. `SamplingSink<S, N>` is verified automatically because it is such a type |

No Rust analogue of `ArchUnit` is added; `cargo public-api` plus
`cargo semver-checks` plus the existing xtask AST walk provide the
three-layer Earned-Trust pattern at the public surface (compile-time
subtype, structural CI, behavioural CI via the slice-test suite).

## External integrations

Sieve has **no** external integrations. It is a pure-CPU pipeline
stage. The only "external" dependency is Aperture (an in-workspace
crate) and `xxhash-rust` (a small Rust library). No contract-test
recommendation is needed for the platform-architect handoff because
no third-party API is in scope.

## Downstream constraints (handed to acceptance-designer / DEVOPS)

1. **Public surface is closed**. Acceptance designer's tests import only
   `sieve::Sampler`, `sieve::Decision`, `sieve::HeadSampler`,
   `sieve::SamplingSink`, `sieve::KeepReason`, `sieve::TraceView`,
   plus the test-only `sieve::__test_summary_tick_now()` seam (analogous
   to Spark's `__reset_for_testing`; see ADR-0020).
2. **AGPL containment**. Sieve's `Cargo.toml` declares
   `license = "AGPL-3.0-or-later"`. Aperture's `Cargo.toml` adds Sieve
   as a runtime dependency at the DELIVER wave that wires Sieve into
   `compose::wire_sink`; Aperture is already AGPL so no licence
   refusal at Gate 4.
3. **Hash function is observable behaviour**. `xxh3_64` is locked at
   ADR-0019 and a code-comment in `sampler.rs` flags "operators
   notice if this changes; bumping `xxhash-rust` major version is a
   public-behaviour change covered by SemVer-major bump on the Sieve
   crate".
4. **Summary tick is parameterisable**. `SIEVE_SUMMARY_TICK_MS` env var
   defaults to `60_000` in production; the integration tests set it
   to `100` (or smaller) so the per-window assertion fires within a
   test wall-clock budget.
5. **Probability source is injectable**. `HeadSampler::sample` does
   not need a separate RNG: the `xxh3_64(trace_id)` mapping IS the
   probability source. Slice 03's "injectable RandomSource" Technical
   Note is resolved by collapsing slice 03 and slice 04 into a single
   deterministic mechanism — slice 03's tests use deterministic
   trace_ids (per the existing DISCUSS fixture) so the kept count
   lands in the band without a separate RNG abstraction. ADR-0018
   §"`HeadSampler::sample`" notes this consolidation; no DISCUSS
   amendment is needed because slice 03's brief already says "the
   probability source needs to be deterministic in tests" and the
   trace_id-keyed mechanism delivers that.

## Quality gates checklist

- [x] Requirements traced to components (slice-mapping.md)
- [x] Component boundaries with clear responsibilities (C4 Component)
- [x] Technology choices in ADRs with alternatives (ADR-0019)
- [x] Quality attributes addressed (ISO 25010 table above)
- [x] Dependency-inversion compliance (the decorator wraps the inner
      `OtlpSink`; the sampler depends on a `Sampler` trait; the
      probability source is the deterministic hash, no `dyn` indirection
      needed)
- [x] C4 diagrams (L1+L2 minimum, Mermaid; L3 for the decorator
      subsystem because it has 5 components)
- [x] Integration patterns specified (decorator over Aperture's
      `OtlpSink` + `Probe`; in-process Tokio task for the summary)
- [x] OSS preference validated (xxhash-rust BSL-1.0 / MIT; tokio MIT;
      async-trait MIT/Apache-2.0; tracing MIT)
- [x] AC behavioural, not implementation-coupled (the user stories'
      ACs are unchanged; DESIGN does not weaken them)
- [x] External integrations annotated with contract test recommendation
      (none — Sieve has no external integrations)
- [x] Architectural enforcement tooling recommended (public-api +
      semver-checks + xtask AST walk + cargo-deny + mutation testing)
- [ ] Peer review completed and approved (Bea dispatches Atlas
      separately; this is not Morgan's gate)

## Back-propagation flags

None. The DISCUSS contract holds without amendment. D5 / Slice 03's
"injectable probability source" technical note is satisfied by the
trace_id-keyed deterministic mechanism (DISCUSS Q7 already locks
xxh3_64); the resolution is internal to DESIGN and does not require
a DISCUSS edit.

## Files produced by this wave

- `docs/feature/sieve/design/wave-decisions.md` (this file)
- `docs/feature/sieve/design/c4-context.md`
- `docs/feature/sieve/design/c4-container.md`
- `docs/feature/sieve/design/c4-component.md`
- `docs/feature/sieve/design/technology-choices.md`
- `docs/feature/sieve/design/slice-mapping.md`
- `docs/product/architecture/adr-0018-sieve-public-api-and-crate-layout.md`
- `docs/product/architecture/adr-0019-sieve-dependency-pinning.md`
- `docs/product/architecture/adr-0020-sieve-summary-aggregator.md`
- `docs/product/architecture/adr-0021-sieve-aperture-integration.md`
