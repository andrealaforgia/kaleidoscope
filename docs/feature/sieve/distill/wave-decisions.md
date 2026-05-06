# Sieve v0 — DISTILL wave decisions

- **Date**: 2026-05-06
- **Author**: `@nw-acceptance-designer` (Quinn)
- **Wave**: DISTILL (single iteration; reviewer is dispatched
  separately by Bea — Sentinel runs as
  `@nw-acceptance-designer-reviewer`)
- **Inputs**: `docs/feature/sieve/discuss/` (six LeanUX stories,
  journey YAML, six slice briefs); `docs/feature/sieve/design/`
  (Atlas-approved DESIGN package at `a624567`); ADR-0018 through
  ADR-0021; sister-crate precedents at `crates/aperture/` and
  `crates/spark/`
- **Outputs**: `crates/sieve/` skeleton (Cargo.toml + src/ + tests/);
  this file plus `test-mapping.md`

## Inputs read

| File | Status |
|------|--------|
| `docs/feature/sieve/design/wave-decisions.md` | ✓ |
| `docs/feature/sieve/design/peer-review.md` | ✓ |
| `docs/feature/sieve/design/c4-component.md` | ✓ |
| `docs/feature/sieve/design/slice-mapping.md` | ✓ |
| `docs/feature/sieve/design/technology-choices.md` | ✓ |
| `docs/product/architecture/adr-0018-sieve-public-api-and-crate-layout.md` | ✓ |
| `docs/product/architecture/adr-0019-sieve-dependency-pinning.md` | ✓ |
| `docs/product/architecture/adr-0020-sieve-summary-aggregator.md` | ✓ |
| `docs/product/architecture/adr-0021-sieve-aperture-integration.md` | ✓ |
| `docs/feature/sieve/discuss/user-stories.md` (US-SI-01 … US-SI-06) | ✓ |
| `docs/feature/sieve/discuss/journey-sieve.yaml` | ✓ |
| `docs/feature/sieve/slices/slice-01-walking-skeleton.md` | ✓ |
| `docs/feature/sieve/slices/slice-02-error-bias.md` | ✓ |
| `docs/feature/sieve/slices/slice-03-non-error-rate.md` | ✓ |
| `docs/feature/sieve/slices/slice-04-trace-id-determinism.md` | ✓ |
| `docs/feature/sieve/slices/slice-05-logs-metrics-passthrough.md` | ✓ |
| `docs/feature/sieve/slices/slice-06-observability.md` | ✓ |
| `crates/aperture/Cargo.toml` | ✓ |
| `crates/aperture/src/ports/mod.rs` | ✓ |
| `crates/aperture/src/testing.rs` | ✓ |
| `crates/spark/Cargo.toml` | ✓ |
| `crates/spark/tests/common/mod.rs` | ✓ |
| `Cargo.toml` (workspace) | ✓ |
| `scripts/hooks/pre-commit` | ✓ |
| `.github/workflows/ci.yml` | ✓ |

## DISTILL decisions

### D1 — Test-strategy posture: Strategy C "real local"

Sieve's tests use **real Aperture infrastructure** (the
`aperture::testing::RecordingSink`) as the inner sink wrapped by
`SamplingSink<S, N>`. No mocks of production code; no synthetic
`OtlpSink` doubles. The decorator's `accept_traces` body, when
DELIVER lands it, will hand off the kept-traces-only envelope to
the `RecordingSink` exactly as it would to a production sink at
runtime.

Rationale:

- ADR-0021 §7 documents the integration pattern: Sieve's slice tests
  construct `SamplingSink::new(RecordingSink::default(),
  HeadSampler::new(rate)?)` and assert against the recorded records.
- Sieve has **no external integrations** (it is pure CPU) — the only
  "I/O" surface in scope is the inner-sink hand-off, which is in
  process and exercised end-to-end via `RecordingSink`.
- The `@real-io` mandate from `nw-test-design-mandates` ("at least
  one scenario per driven adapter exercises real I/O") is satisfied
  trivially: there is no driven adapter to exercise; the decorator's
  inner-sink edge IS exercised against a real Aperture impl in
  every slice test.

### D2 — Public-surface boundary

Acceptance tests import only Sieve's public surface, locked at
ADR-0018:

- `sieve::Sampler`, `sieve::Decision`, `sieve::HeadSampler`,
  `sieve::SamplingSink`, `sieve::KeepReason`, `sieve::TraceView`,
  `sieve::SieveConfigError`
- Plus the two doc-hidden test seams:
  `sieve::__test_trace_view`, `sieve::__test_summary_tick_now`
- Plus the doc-hidden helper `sieve::sampler_env_for_tests()` that
  returns the env-var name `"SIEVE_NON_ERROR_TRACE_RATE"` so tests
  do not duplicate the literal

The slice tests' import sections grep-confirm this: every `use
sieve::*` statement names items from this list. No
`sieve::sampler::*`, no `sieve::decorator::*`, no other
private-module reach-through. The hexagonal boundary holds at the
crate boundary.

### D3 — Per-binary test isolation

Sieve has process-global state across tests:

1. The `Counters` struct holds three `AtomicU64`s; while owned by a
   `SamplingSink`, the timer task that snapshots them is spawned on
   the ambient Tokio runtime.
2. The `tracing` global subscriber is install-once-per-process; the
   `target = "sieve"` capture layer plus the `CAPTURED_EVENTS` Vec
   live in `tests/common/mod.rs` as static globals.
3. `SIEVE_NON_ERROR_TRACE_RATE` and `SIEVE_SUMMARY_TICK_MS` are
   process-level env vars; tests that set/unset them must serialise.

Per the Aperture (ADR-0015) and Spark (ADR-0011 + tests/common/mod.rs
SPARK_INIT_SERIAL pattern) precedents, Sieve adopts:

- **Per-binary process isolation** via eight separate `[[test]]`
  declarations in `Cargo.toml`. Cargo compiles each as its own
  binary and runs each as its own process. State that is
  "process-global" is therefore "binary-global" — naturally
  isolated across slices.
- **Within-binary serialisation** via the `serial_test` crate
  (`#[serial_test::serial]`) AND the `SIEVE_TEST_SERIAL` mutex in
  `tests/common/mod.rs`. The mutex is async-safe (acquired before
  `.await` points) and the `#[serial]` attribute serialises across
  the test runner's parallel scheduling.

Slice 06 is the heaviest user of this isolation (it touches the
capture buffer, the env vars, and the timer-task seam). Slices 01–04
do not touch process-global state and run without serialisation.
Slice 05 touches the inner sink only and runs without serialisation.

### D4 — RED state: every acceptance test panics

Per the Aperture and Spark precedents, every acceptance test in
Sieve's `tests/` directory currently panics on `unimplemented!()`.
The panic comes from one of three production functions:

- `HeadSampler::sample` (slices 01 / 02 / 03 / 04) — the sampling
  decision body lands at DELIVER.
- `SamplingSink::new` (slices 05 / 06) — the constructor's
  timer-task spawn lands at DELIVER.
- `SamplingSink::accept` and `SamplingSink::probe` (slice 05 / 06) —
  the routing and the probe-delegation bodies land at DELIVER.
- `__test_summary_tick_now` (slice 06) — the synchronous summary
  emission lands at DELIVER.

A small subset of tests passes at DISTILL because they exercise
infrastructure that IS real at this wave:

- `slice_01::head_sampler_exposes_its_configured_rate` — `rate()` is
  real.
- `slice_02::head_sampler_new_rejects_*` — `new` is real.
- `slice_06::head_sampler_from_env_*` — `from_env` is real.
- `invariant_public_api_smoke::*` — type-level checks, no panicking
  body.
- `invariant_sampling_sink_is_otlp_sink_and_probe::*` — type-level
  checks, no panicking body.

These passing tests are NOT a Mandate-3 violation (incomplete user
journey): they validate that the **configuration error surface**
(US-SI-06's "out-of-range or unparseable value is rejected with a
clear error") and the **type-level integration contract** (ADR-0021
§7) are real at DISTILL. Both are valid acceptance properties; the
slice tests cover the behavioural body, the invariant tests cover
the structural contract.

### D5 — Pre-commit hook + Gate 1 update

`scripts/hooks/pre-commit` and `.github/workflows/ci.yml` both add
`--exclude sieve` to their `cargo test --workspace --all-targets
--locked` invocations during Sieve's DISTILL/DELIVER cycle. Mirrors
how Aperture and Spark were handled in their RED phases (Aperture's
DEVOPS wave-decisions §A2, Spark's DEVOPS commit at the close of its
DISTILL phase).

DELIVER's final commit (when all six slices are GREEN) removes the
`--exclude sieve` flag and Sieve graduates to the workspace-wide
Gate 1 invocation.

Gate 4 (`cargo deny check`) requires the `BSL-1.0` allow entry per
ADR-0019 §"Licence audit table". The platform-architect handles
the `deny.toml` edit at the DEVOPS-to-DELIVER handoff; this DISTILL
wave does not modify `deny.toml` itself (the DEVOPS wave landed
the licence-audit groundwork already).

### D6 — Determinism strategy for slice 03's 10 000-trace fixture

Slice 03's "kept count at rate 0.5 lies in `[4700, 5300]` on a
10 000-trace fixture" is **statistically deterministic**:

- The fixture builds 10 000 distinct trace_ids from a 64-bit counter
  (seed `0..10_000`). The `fixture_trace_id(seed)` helper splats the
  seed into the lower 8 bytes of a 16-byte trace_id and seeds the
  upper 8 bytes with a fixed marker.
- `xxh3_64` distributes these otherwise-sequential keys uniformly
  in `[0.0, 1.0]` (its avalanche property is the entire point of
  using it for sampling).
- The kept count at rate 0.5 lands in the ±3% band on every run
  because the same fixture produces the same hash-mapped values
  produces the same partition.

The test is therefore **not flaky**. The bands account for the
discrete distribution's natural variance, not for runtime
randomness. This consolidates slice 03's DISCUSS Technical Note
("the 10000-trace fixture uses a deterministic seed so the
assertion is non-flaky") with ADR-0018 §"`HeadSampler::sample`
mechanism" ("there is no separate `RandomSource` abstraction; the
hash IS the probability source").

### D7 — Walking-skeleton boundary: user-centric framing

Slice 01's two assertions describe user outcomes, not technical
flow:

- "An error-bearing trace is kept at rate 0.0" — what Riley sees in
  the downstream backend (the trace appears).
- "A non-error trace is dropped at rate 0.0" — what Riley sees as a
  volume reduction (the trace does not appear).

The Then steps assert observable outcomes (the typed `Decision`
return value the operator-facing code consumes); they do not assert
internal side effects (no "the recording sink contains 1 element",
no "the counter increments", no "the DEBUG event fires"). Slice 06
is the slice that asserts on the observable event vocabulary; slice
01 asserts on the decision return value alone. This split mirrors
Aperture's slice-01 pattern and satisfies Mandate 5 (Walking
Skeleton User-Centricity, per `nw-ad-critique-dimensions`
Dimension 5).

## Mandate compliance evidence

### CM-A — Hexagonal boundary

Every slice test imports only the Sieve public surface. The grep
trail:

```text
$ grep -hE '^use sieve' crates/sieve/tests/*.rs | sort -u
use sieve::SamplingSink;
use sieve::__test_summary_tick_now;
use sieve::{
use sieve::{Decision, HeadSampler, Sampler};
```

Every imported name is a public item locked at ADR-0018. Zero
internal-module reach-through (no `use sieve::sampler::*`, no `use
sieve::decorator::*`, etc.).

The Aperture-side import (driven by the decorator's hand-off
contract) names only `aperture::ports::*` (`OtlpSink`, `Probe`,
`SinkRecord`, `SinkError`, `ProbeError`) and
`aperture::testing::RecordingSink` (the test seam Aperture
explicitly publishes for integration tests). Both are public
boundary items.

### CM-B — Business language purity

Slice tests' `#[test]` function names use domain language:

- `an_error_bearing_trace_is_kept_at_rate_zero`
- `at_rate_half_kept_count_lies_in_three_percent_band_around_half`
- `same_trace_id_across_one_hundred_queries_never_flips_decision`
- `a_log_record_passes_through_unchanged_at_rate_zero`
- `a_kept_error_trace_emits_a_debug_kept_error_bearing_event`

Every name names a user-visible outcome (kept / dropped / passes
through / emits event); none names a technical operation
(`validates_request`, `parses_protobuf`, `returns_200`). The
docstrings at the top of each test re-state the Given / When / Then
in business language drawn from US-SI-01 through US-SI-06.

### CM-C — Walking skeleton vs focused scenarios

- **Walking-skeleton scenarios** (user goal E2E, demo-able to
  Riley): slice 01's two `Decision::Keep` / `Decision::Drop`
  assertions plus slice 05's "log passes through unchanged" trio.
  Total: 3 user-centric walking skeletons covering the three
  signals (traces sampled, logs unchanged, metrics unchanged).
- **Focused boundary scenarios**: 33 across slices 02-06 plus the
  two invariant binaries. Each tests a specific business rule
  variation (rate boundaries, multi-span traces, env-var error
  paths, observability vocabulary) at the public-surface boundary.

Total: 36 `#[test]` functions across 8 test binaries. Ratio is in
range of `nw-test-design-mandates` Walking Skeleton Strategy ("2-3
walking skeletons + 15-20 focused scenarios"). The slightly
higher focused-count reflects Sieve's narrow scope (one decision
function, one decorator, one timer task) being easier to cover
exhaustively than a feature with multiple driven adapters.

### CM-D — Pure function extraction

Sieve's business logic is already extracted into pure functions
behind the public API:

- `HeadSampler::sample(&TraceView<'_>) -> Decision` is pure (no
  I/O, no env-var read at call time, no mutation). Tests call it
  directly without fixtures.
- `is_error_bearing(spans)` (a `pub(crate)` free function in
  `sampler.rs`) is pure. The decorator's `accept_traces` body uses
  it in the per-trace decision branch.
- The summary-emission body (`emit_summary(kept, kept_err, dropped,
  rate)`) is pure (it consumes pre-snapshotted counter values; the
  snapshot itself is the impure part, lifted into the timer task or
  the test seam).

The impure code (Tokio timer task, env-var read, `tracing` event
emission, inner-sink `accept`) is isolated behind the
`SamplingSink::new` constructor and the `accept` / `probe` methods.
Fixture parametrization applies only to the inner-sink type
(`RecordingSink` in tests; production sinks at runtime). Slice
tests do not parametrize across environments — Sieve has no
environment to parametrize across.

## Error-path coverage

Per `nw-bdd-methodology` (40%+ error-path target):

| Category | Count |
|----------|-------|
| Happy path (Keep / Drop with valid rate) | 14 |
| Error path (rate out of range, NaN, env-var unparseable, env-var out of range; dropped traces; mid-fixture variance) | 14 |
| Boundary / edge case (rate 0.0 at-most-2 kept, rate 1.0 at-least-9998 kept, multi-span trace with one error, same-trace-id-100-times, cross-rate determinism, type-level invariants) | 8 |

Error + edge ratio: 22 / 36 = **61%**. Above the 40% target. Every
slice has at least one error path; slice 02 has three rate-rejection
paths; slice 06 has two env-var rejection paths plus the default
fallback.

## Story-to-scenario coverage

Every US-SI-01 through US-SI-06 has at least one scenario; see
`test-mapping.md` for the per-scenario mapping.

## Quality gates checklist

- [x] All acceptance scenarios written with passing step
      definitions (function bodies compile and exercise the public
      surface; the production methods panic on `unimplemented!()` —
      the canonical RED state)
- [x] Test pyramid complete: acceptance tests at the public
      boundary, two compile-time invariant tests for the
      type-system contract; unit-test locations identified inside
      `src/` for DELIVER (each module has its dedicated
      responsibility per ADR-0018 §"Internal layout")
- [x] Hexagonal boundary enforcement: every test imports only
      Sieve's public surface (CM-A above)
- [x] Business language purity (CM-B above)
- [x] User journey completeness: each slice's test maps to a
      US-SI-* scenario; the acceptance criteria of every story has
      at least one scenario (CM-C above)
- [x] Pure function extraction (CM-D above)
- [x] Error path coverage 40%+ (61%, above target)
- [x] Walking skeleton user-centricity (D7 above)
- [x] Pre-commit hook and CI Gate 1 updated to `--exclude sieve`
      during DISTILL/DELIVER (D5 above)
- [x] `cargo build --workspace --all-targets --locked` succeeds
      (Sieve's tests compile; they panic on `unimplemented!()` at
      runtime, which is the RED state DELIVER turns GREEN one slice
      at a time)
- [ ] Peer review approved (Bea dispatches Sentinel
      `@nw-acceptance-designer-reviewer` separately; this is not
      Quinn's gate)

## Files produced by this wave

- `crates/sieve/Cargo.toml` (workspace member, AGPL-3.0-or-later,
  eight `[[test]]` declarations)
- `crates/sieve/src/lib.rs` (public-surface re-exports + two
  doc-hidden test seams)
- `crates/sieve/src/sampler.rs` (`Sampler` trait, `HeadSampler`
  concrete with real `new` + `from_env` + `rate`, `sample` panics
  on `unimplemented!()`)
- `crates/sieve/src/decision.rs` (`Decision` and `KeepReason`
  enums, real)
- `crates/sieve/src/decorator.rs` (`SamplingSink<S, N>` struct stub
  with `OtlpSink + Probe` impls returning `unimplemented!()`,
  plus the `__test_summary_tick_now` doc-hidden seam)
- `crates/sieve/src/aggregator.rs` (`Counters` with three
  `AtomicU64`s, summary-tick env parser real, timer-task body
  panics)
- `crates/sieve/src/observability.rs` (event vocabulary helpers,
  panicking)
- `crates/sieve/src/trace_view.rs` (`TraceView<'a>` borrowed view
  real, `__test_trace_view` doc-hidden seam real)
- `crates/sieve/src/error.rs` (`SieveConfigError` variants real)
- `crates/sieve/tests/common/mod.rs` (fixture helpers, capture
  layer, `SIEVE_TEST_SERIAL` mutex, fixture builders)
- `crates/sieve/tests/slice_01_walking_skeleton.rs` (3 `#[test]`)
- `crates/sieve/tests/slice_02_error_bias.rs` (9 `#[test]`)
- `crates/sieve/tests/slice_03_non_error_rate.rs` (3 `#[test]`)
- `crates/sieve/tests/slice_04_trace_id_determinism.rs` (3 `#[test]`)
- `crates/sieve/tests/slice_05_logs_metrics_passthrough.rs` (4 `#[test]`)
- `crates/sieve/tests/slice_06_observability.rs` (6 `#[test]`)
- `crates/sieve/tests/invariant_public_api_smoke.rs` (6 `#[test]`)
- `crates/sieve/tests/invariant_sampling_sink_is_otlp_sink_and_probe.rs` (2 `#[test]`)
- `Cargo.toml` (workspace) — added `crates/sieve` member
- `scripts/hooks/pre-commit` — added `--exclude sieve` to Gate 1
- `.github/workflows/ci.yml` — added `--exclude sieve` to Gate 1
- `docs/feature/sieve/distill/wave-decisions.md` (this file)
- `docs/feature/sieve/distill/test-mapping.md` (per-slice mapping)

## Back-propagation flags

None. The DESIGN contract holds without amendment. ADR-0018 through
ADR-0021's locked decisions are sufficient for the eight RED test
binaries; no DESIGN-side question surfaced during DISTILL.

## Handoff notes for DELIVER

DELIVER receives:

1. **Eight RED acceptance test binaries**, all compiling, all
   panicking on `unimplemented!()` at the right call sites.
2. **One-slice-at-a-time implementation sequence**, locked at
   `slice-mapping.md`: slice 01 first (walking skeleton), slice 02
   (error bias), slice 03 (non-error rate), slice 04 (determinism),
   slice 05 (passthrough), slice 06 (observability).
3. **The pre-commit hook and CI Gate 1 are scoped to exclude
   Sieve** until DELIVER's final commit graduates the crate.
4. **Mandate compliance evidence** documented above (CM-A through
   CM-D).

DELIVER's job is to drive each `unimplemented!()` panic away in
slice order, with mutation-test 100% kill rate per ADR-0005 Gate 5
on each slice's diff. The test binaries do not change shape during
DELIVER — DELIVER only writes production code under
`crates/sieve/src/`.
