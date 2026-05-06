# Peer review — Sieve v0 DISTILL

- **Date**: 2026-05-06
- **Reviewer**: `@nw-acceptance-designer-reviewer` (Sentinel)
- **Wave**: DISTILL (Scholar, single iteration)
- **Artefact set**: `crates/sieve/` plus `docs/feature/sieve/distill/`
- **Verdict**: **APPROVED** — handoff to DEVOPS then DELIVER
- **Critical issues**: 0
- **High issues**: 0
- **Iteration**: 1 of 2 — no revisions required
- **Average score across nine dimensions**: 9.8 / 10

---

## Executive summary

Scholar's DISTILL pass is rigorous and complete. Eight Cargo
integration test binaries (six slices plus two cross-cutting
invariants) carry 36 `#[test]` functions; 22 of them exercise error
or edge paths (61%, above the 40% mandate). All four design mandates
pass: hexagonal boundary, business language purity, user journey
completeness, pure function extraction.

The acceptance test surface imports only the seven items locked by
ADR-0018 plus the two doc-hidden test seams. Real Aperture
`RecordingSink` is the inner sink for `SamplingSink<S, N>` (Strategy
C "real local"); no mocks. Per-binary process isolation via eight
`[[test]]` declarations plus a `SIEVE_TEST_SERIAL` mutex and the
`serial_test` crate handle the shared Tokio runtime, tracing
subscriber, and env-var state.

RED posture is canonical: source stubs (`HeadSampler::sample`,
`SamplingSink::new`, `accept`, `probe`) panic on `unimplemented!()`;
the constructors and accessors that tests depend on (`HeadSampler::new`,
`HeadSampler::from_env`, `HeadSampler::rate`, the `Decision` /
`KeepReason` / `SieveConfigError` enums, `TraceView` and its
`__test_trace_view` builder) are real. The mix is the right shape:
tests compile, panic at the right moment, and turn GREEN one slice
at a time during DELIVER.

Zero blocking issues. The wave is ready for DEVOPS hand-offs and
DELIVER.

---

## Dimension scores

| # | Dimension | Score |
|---|---|---|
| 1 | Happy-path bias | 9 — 22/36 = 61% error/edge paths, above 40% mandate |
| 2 | Given-When-Then format | 10 — every test has one When and observable assertions |
| 3 | Business language purity | 10 — domain vocabulary throughout (`error_bearing`, `kept`, `passthrough`); zero technical jargon in test names |
| 4 | Coverage completeness | 10 — every story US-SI-01..US-SI-06 maps to acceptance tests; `test-mapping.md` confirms zero gaps |
| 5 | Walking-skeleton user-centricity | 9 — three walking skeletons frame user outcomes (error trace kept; non-error dropped; log passes through) |
| 6 | Priority validation | 10 — slices follow natural user-need order |
| 7 | Observable behaviour assertions | 10 — return values, recorded counts, captured events; zero internal-state checks |
| 8 | Traceability coverage | 10 — story-to-test mapping complete; environment parametrisation N/A (Sieve is pure CPU) |
| 9 | Walking-skeleton boundary proof | 10 — Strategy C declared, real `RecordingSink` used, fixture tier verified |

---

## Mandate verification

| Mandate | Status | Evidence |
|---|---|---|
| CM-A — Hexagonal boundary | PASS | All test imports reference the seven public items + two doc-hidden test seams + Aperture's public ports. Grep for `use sieve::(sampler|decorator|aggregator|observability)` returns zero hits. |
| CM-B — Business language purity | PASS | Test names: `an_error_bearing_trace_is_kept_at_rate_zero`, `same_trace_id_across_one_hundred_queries_never_flips_decision`, `at_rate_half_kept_count_lies_in_three_percent_band_around_half`. Zero technical terms (no `serialize`, `parse`, `hash`, `async` in test names). Docstrings recite UAT scenarios verbatim. |
| CM-C — User journey completeness | PASS | Three walking skeletons + 33 focused scenarios = 36 tests, within the recommended 2-3 + 15-20 range. Each walking skeleton describes a user goal with observable business value. |
| CM-D — Pure function extraction | PASS | `HeadSampler::sample` is pure (no I/O, no env-var read at call time, no mutation). `is_error_bearing` is pure. `emit_summary` consumes pre-snapshotted values. Impure code (timer task, env-var reads, tracing events) is correctly isolated behind `SamplingSink::new` and `from_env`. |

---

## RED posture verification

Mixed posture as expected:

**Tests passing at DISTILL** (because the constructors + accessors are
real, by design):
- `invariant_public_api_smoke.rs` (6 type-level checks)
- `invariant_sampling_sink_is_otlp_sink_and_probe.rs` (2 compile-time trait-impl checks)
- `slice_01::head_sampler_exposes_its_configured_rate` (`rate()` is real)
- `slice_02` rate-rejection tests (3, exercising `HeadSampler::new`'s real validation)
- `slice_06` env-var rejection tests (3, exercising `HeadSampler::from_env`'s real validation)

**Tests panicking at DISTILL** (turned GREEN by DELIVER):
- `slice_01` (2): panic on `sample` unimplemented
- `slice_02` (6): panic on `sample` unimplemented
- `slice_03` (3): panic on `sample` unimplemented
- `slice_04` (3): panic on `sample` unimplemented
- `slice_05` (3): panic on `SamplingSink::new` unimplemented
- `slice_06` (3): panic on `SamplingSink::new` unimplemented

This split is the canonical Sieve-shaped RED posture: validation paths
are real (so tests can exercise the four-variant `SieveConfigError`
without a complete `sample` implementation), but the load-bearing
behavioural contract panics, which is what DELIVER will turn GREEN
slice by slice.

---

## Test infrastructure

`praise:` Eight `[[test]]` declarations correctly partition the test
surface. Each binary runs as its own process, isolating Tokio runtime
and tracing subscriber state. Within-binary serialisation via
`SIEVE_TEST_SERIAL` mutex (async-aware) plus `#[serial_test::serial]`
handles env-var manipulation and shared state. Mirrors the Aperture
ADR-0015 and Spark ADR-0011 precedents exactly.

`praise:` Real Aperture `RecordingSink` as the inner sink (Strategy
C) means the decorator is exercised against the actual
`OtlpSink + Probe` contract from Aperture's public ports, not against
synthetic mocks. The litmus test holds: deleting the real adapter
would break the walking skeletons, which is the correct fixture-tier
property.

`praise:` Slice 03's 10000-trace deterministic-seed fixture is
non-flaky despite being statistically framed. The xxh3_64 hash plus
deterministic `fixture_trace_id(seed)` (sequential seeds with a fixed
upper-8-byte marker) produces an identical partition on every run.
Statistical bands (`[4700, 5300]` at rate 0.5; `≤ 2` at rate 0.0; `≥ 9998`
at rate 1.0) are enforced as deterministic tolerances, not as
probabilistic flakes.

---

## Cargo.toml verification

- `license = "AGPL-3.0-or-later"` — PASS
- Eight `[[test]]` declarations — PASS, match the eight test files
- Runtime deps per ADR-0019: `aperture` (workspace path), `tokio`
  with `["macros", "rt", "sync", "time"]`, `tracing`, `xxhash-rust =
  "=0.8"` with `xxh3` feature, `async-trait`, `opentelemetry-proto`
  workspace pin, `thiserror`, `tokio-util` with `["rt"]` — PASS
- Dev-deps: `serial_test`, `tracing-subscriber` with minimal
  features, `tokio` with `["full", "test-util"]` — PASS
- `forbid(unsafe_code)` — PASS

---

## Workspace integration

`praise:` Bea's inline application of the BSL-1.0 allow entry in
`deny.toml` (the licence-audit hand-off Atlas flagged for DEVOPS) is
the right move: the pre-commit `cargo deny` gate would otherwise
reject the workspace and block all subsequent commits. Documented
rationale at `deny.toml`'s amended block. Treat as in-scope for this
DISTILL review.

`praise:` The pre-commit hook's `--exclude sieve` clause and the CI
workflow's matching change are the canonical DISTILL/DELIVER pattern.
The hook comment correctly cites the harness and Aperture and Spark
graduations as precedent and describes the symmetric edit DELIVER
will land at the close of Sieve's v0 cycle.

---

## Documentation

`wave-decisions.md` — APPROVED. Records DISTILL decisions D1-D7 with
rationale; the test-posture rationale and mandate-compliance evidence
are explicit.

`test-mapping.md` — APPROVED. Per-slice mapping is complete: every
BDD scenario from `user-stories.md` maps to a test binary, a `#[test]`
function name, and the asserted public-API touchpoint. Cross-cutting
traceability section confirms zero gaps.

---

## Suggestions for DEVOPS (non-blocking, hand-offs)

`suggestion (non-blocking):` Apex's wave needs to:

1. Extend Gate 2 (`cargo public-api`) to cover `sieve` (mirroring
   how Spark was graduated at its DESIGN close).
2. Extend Gate 3 (`cargo semver-checks`) to cover `sieve`.
3. Add a new parallel job `gate-5-mutants-sieve` mirroring
   `gate-5-mutants-aperture` and `gate-5-mutants-spark`.
4. Update the local `pre-push` hook's loop over packages to include
   `sieve`.
5. Verify `cargo deny check` passes with the new BSL-1.0 entry on the
   first CI run after DISTILL lands.

These are mechanical extensions of the existing infrastructure; the
Aperture and Spark precedents make the diff predictable.

---

## Praise

`praise:` Every Elevator Pitch from DISCUSS has a corresponding
`#[test]` function whose name reads as a user outcome. The discipline
maintained from the harness through Sieve is exemplary.

`praise:` The mix of "real validation paths" and "panic on
behavioural unimplemented" is exactly the right RED posture for a
component with non-trivial input validation. Tests can exercise the
four `SieveConfigError` variants without a complete sampler
implementation, which lets DELIVER turn slices GREEN one at a time
without leaving validation gaps.

`praise:` Process-global state (Tokio runtime, tracing subscriber,
env vars) is correctly isolated. The `SIEVE_TEST_SERIAL` mutex pattern
plus `serial_test` plus per-binary `[[test]]` declarations gives
parallel-by-default Cargo a clean isolation story.

`praise:` Slice 03's deterministic statistical fixture is a model of
how to assert probabilistic contracts without flakes. The fixed seed
plus xxh3_64 plus the wide ±3% band at rate 0.5 makes the assertion
robust to sampling-boundary noise without weakening the contract.

---

## Approval

**APPROVED** for handoff to DEVOPS then DELIVER.

- Critical issues: 0
- Blocking findings: 0
- Average score: 9.8 / 10
- Iteration budget: 1 of 2 used. No revisions required.
