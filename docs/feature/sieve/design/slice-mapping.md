# Sieve v0 — slice → story → ADR → module → CI invariant → KPI

The mapping below ties every slice in the elephant carpaccio to the
LeanUX story it implements, the ADRs that govern it, the Sieve module
where the work lands, the CI invariants that defend it, and the KPIs
it makes measurable.

| Slice | Story | ADRs | Modules touched | CI invariants | KPIs |
|-------|-------|------|-----------------|---------------|------|
| **01 — Walking skeleton** (`tests/slice_01_walking_skeleton.rs`) | US-SI-01: error trace kept, non-error trace dropped | ADR-0018 §"Public surface" (Sampler trait, Decision, HeadSampler, TraceView, KeepReason); ADR-0021 §1 (decorator existence) | `lib.rs` (re-exports), `decision.rs` (Decision, KeepReason, TraceView), `sampler.rs` (Sampler, HeadSampler, is_error_bearing), `decorator.rs` (SamplingSink scaffold) | Gate 1 (`cargo test`); Gate 2 (`cargo public-api`); Gate 5 (`cargo mutants`) | KPI 1 (error retention at rate 0.0); KPI 6 (test wall time) |
| **02 — Error-bias detection** (`tests/slice_02_error_bias.rs`) | US-SI-02: errors always survive sampling | ADR-0018 §"`HeadSampler::sample` mechanism" (error-bias short-circuit) | `sampler.rs` (`is_error_bearing` body); `decorator.rs` (KeepReason::ErrorBearing routing) | Gate 1; Gate 5 | KPI 1 (extends to all rates) |
| **03 — Non-error rate honoured** (`tests/slice_03_non_error_rate.rs`) | US-SI-03: non-error rate honoured statistically | ADR-0018 §"`HeadSampler::sample` mechanism" (xxh3_64 mapping); ADR-0019 §1 (xxhash-rust pin) | `sampler.rs` (xxh3_64 mapping, rate comparison); the test fixture seeds 10000 deterministic trace_ids | Gate 1; Gate 4 (BSL-1.0 licence allow); Gate 5 | KPI 2 (rate ±3% on 10000-trace fixture) |
| **04 — Trace-id determinism** (`tests/slice_04_trace_id_determinism.rs`) | US-SI-04: same trace_id always yields the same decision | ADR-0018 §"`HeadSampler::sample` mechanism" (deterministic by construction) | `sampler.rs` (no new code; the determinism is a property of the existing mechanism); the test asserts the property | Gate 1; Gate 5 | KPI 3 (variance zero across 100 calls) |
| **05 — Logs and metrics passthrough** (`tests/slice_05_logs_metrics_passthrough.rs`) | US-SI-05: logs and metrics pass through unfiltered | ADR-0021 §1 (SinkRecord routing); ADR-0021 §4 (no Sieve-local Signal enum) | `decorator.rs` (Logs and Metrics passthrough arms); `tests/invariant_public_api_smoke.rs` (where-clause smoke for `SamplingSink: OtlpSink + Probe`) | Gate 1; Gate 5 | KPI 4 (logs and metrics 100% passthrough) |
| **06 — Observability** (`tests/slice_06_observability.rs`) | US-SI-06: sampling decision observability | ADR-0020 §1–6 (counters, timer, cancellation, tick interval, INFO emission, test seam); ADR-0018 §"Public surface" (`__test_summary_tick_now`) | `aggregator.rs` (Counters, SummaryTask); `observability.rs` (emit_debug_*, emit_summary); `decorator.rs` (DEBUG event emission per decision); `lib.rs` (re-export of `__test_summary_tick_now`) | Gate 1; Gate 2 (test seam recorded; convention signals it is internal); Gate 5 | KPI 5 (one INFO event per summary window) |
| **Cross-slice invariant** (`tests/invariant_public_api_smoke.rs`) | All six (defends the public-surface contract) | ADR-0018 §"Public surface" (full list); ADR-0021 §7 (subtype-check layer) | `tests/invariant_public_api_smoke.rs` only (no production code) | Gate 1; Gate 2; Gate 3 | none directly — the gate IS the invariant |

## Module-level traceability

| Module | Slices contributing | What it owns at v0 end-state |
|--------|---------------------|------------------------------|
| `lib.rs` | 01, 06 | Public re-exports of Sampler, Decision, HeadSampler, KeepReason, SieveConfigError, SamplingSink, TraceView; the two `__` doc-hidden test seams; crate-root `#![forbid(unsafe_code)]` |
| `sampler.rs` | 01, 02, 03, 04 | `Sampler` trait; `HeadSampler` struct with `new`, `from_env`, `rate`, `sample` methods; `is_error_bearing` predicate; xxh3_64 mapping |
| `decision.rs` | 01 | `Decision` enum; `KeepReason` enum; `TraceView<'a>` borrowed view; `__test_trace_view` seam |
| `decorator.rs` | 01 (scaffold), 02 (KeepReason::ErrorBearing), 05 (Logs/Metrics passthrough), 06 (DEBUG events) | `SamplingSink<S, N>` struct; `OtlpSink` impl; `Probe` impl (delegate); trace-grouping pass; per-decision routing |
| `aggregator.rs` | 06 | `Counters` (three AtomicU64); `SummaryTask` (tokio task + CancellationToken); snapshot-and-reset; `__test_summary_tick_now` body |
| `observability.rs` | 06 | `target="sieve"` constant + free fns: `emit_debug_kept_error_bearing`, `emit_debug_kept_sampled`, `emit_debug_dropped`, `emit_summary` |
| `error.rs` | 01, 03, 06 | `SieveConfigError` enum + thiserror derive |

## CI invariant matrix

| Invariant | Gate | Mechanism |
|-----------|------|-----------|
| Public surface stable | Gate 2 | `cargo public-api -p sieve --diff-git-checkouts main HEAD` |
| Additive-only changes between releases | Gate 3 | `cargo semver-checks check-release -p sieve --baseline-rev main` |
| Licence policy honoured | Gate 4 | `cargo deny check` (BSL-1.0 added to allow list per ADR-0019) |
| 100% mutation kill rate per slice | Gate 5 | `cargo mutants --package sieve --in-diff` |
| Error-bearing traces always kept | Gate 1 | `slice_02_error_bias.rs` parameterised over rates |
| Configured rate honoured ±3% | Gate 1 | `slice_03_non_error_rate.rs` deterministic-seed fixture |
| Same trace_id always yields same decision | Gate 1 | `slice_04_trace_id_determinism.rs` 100 calls assertion |
| Logs and metrics pass through unchanged | Gate 1 | `slice_05_logs_metrics_passthrough.rs` count-in-equals-count-out |
| Per-decision DEBUG and periodic INFO summary present | Gate 1 | `slice_06_observability.rs` with `tracing_subscriber` capture |
| `SamplingSink: OtlpSink + Probe` (compile-time) | Gate 1 (compile) | `tests/invariant_public_api_smoke.rs` where-clause function |
| Every OtlpSink also implements Probe (workspace-wide) | xtask AST walk | Aperture's existing structural-layer enforcement; covers `SamplingSink` automatically |

## KPI traceability

| KPI | Mechanism | Slice that lights it up |
|-----|-----------|--------------------------|
| KPI 1 — Error-bearing traces never sampled away | `slice_02_error_bias.rs` parameterised over rates | 02 |
| KPI 2 — Configured rate statistically honoured | `slice_03_non_error_rate.rs` 10000-trace fixture | 03 |
| KPI 3 — Trace coherence across batches | `slice_04_trace_id_determinism.rs` 100 calls assertion | 04 |
| KPI 4 — Logs / metrics unaffected | `slice_05_logs_metrics_passthrough.rs` count assertions | 05 |
| KPI 5 — Operator visibility | `slice_06_observability.rs` DEBUG + INFO captures | 06 |
| KPI 6 — Walking-skeleton round-trip is fast | All slice tests; CI Gate 1 wall time + Gate 5 mutation kill | 01–06 |

## Hand-off boundary to acceptance-designer

Acceptance-designer consumes:

- The seven public-surface items from ADR-0018: `Sampler`,
  `HeadSampler`, `SamplingSink<S, N>`, `Decision`, `KeepReason`,
  `SieveConfigError`, `TraceView<'a>`.
- The two `__` doc-hidden test seams: `__test_trace_view` and
  `__test_summary_tick_now`.
- Aperture's existing public surface from `aperture::ports`:
  `OtlpSink`, `Probe`, `SinkRecord`, `SinkError`, `ProbeError`.
- Aperture's `RecordingSink` from `aperture::testing` for the inner
  sink in slice tests that assert on records.
- The two env vars `SIEVE_NON_ERROR_TRACE_RATE` and
  `SIEVE_SUMMARY_TICK_MS`.

Acceptance-designer does NOT touch:

- `sampler.rs`, `decorator.rs`, `aggregator.rs`, `observability.rs`,
  `error.rs` internals — these are software-crafter's territory.
- The exact `xxh3_64` mapping body — the test suite asserts on the
  observable behaviour (the kept count in the band, the determinism
  across calls), not on the bytes the hash function produces.

## Hand-off boundary to platform-architect (DEVOPS wave)

Platform-architect consumes:

- The five CI gates per ADR-0005, scoped to `crates/sieve/**`.
- The new `BSL-1.0` allow entry needed in `deny.toml`.
- The new `gate-5-mutants-sieve.yml` workflow file (mirrors
  `gate-5-mutants-aperture.yml` and `gate-5-mutants-spark.yml`).
- The Sieve crate's addition to `crates/aperture/Cargo.toml` at the
  DELIVER wave (Aperture grows a runtime dep on `sieve`).
- The composition-root edit in `crates/aperture/src/compose.rs`
  documented in ADR-0021 §3.

**No external integrations**. Sieve has no third-party APIs to
contract-test. The handoff annotation per agent principle 10 is
explicit: contract-test recommendation does not apply because the
boundary set is empty.
