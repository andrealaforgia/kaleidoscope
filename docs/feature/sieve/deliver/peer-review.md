# Peer review — Sieve v0 DELIVER

- **Date**: 2026-05-07
- **Reviewer**: `@nw-software-crafter-reviewer` (Crafty in review mode)
- **Wave**: DELIVER (six slices, 2026-05-06)
- **Artefact set**: `crates/sieve/src/`, `crates/sieve/tests/`, `crates/sieve/Cargo.toml`, plus ADRs 0018-0021
- **Verdict**: **APPROVED** — merge without iteration; coordinate graduation
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

Crafty executed six elephant-carpaccio slices with rigorous
RED → GREEN → REFACTOR discipline. Sieve v0's 36 acceptance tests
across eight binaries (six slice plus two cross-cutting invariants)
exercise the public surface exclusively; the decorator pattern
integrates cleanly with Aperture without surface amendment. All four
ADRs (0018-0021) are implemented faithfully; the `Any` downcast
pattern for rate-reading from `HeadSampler` is pragmatic and honestly
documented as a v0 compromise.

The intermediate CI failures on slices 01-05 (Gate 5 baseline break)
are intrinsic to slice-by-slice DELIVER when DISTILL writes all tests
upfront; final state at `6a62b46` passes all gates. Mutation kill rate
verified at 100% on the diff for slice 06 (the final closed slice).
Test integrity intact: no weakened assertions, no skipped tests, no
testing theatre.

---

## Quantitative validation

| Slice / binary | Tests | Status |
|---|---|---|
| Slice 01 walking skeleton | 3 | GREEN |
| Slice 02 error-bias | 9 | GREEN |
| Slice 03 non-error rate | 3 | GREEN |
| Slice 04 trace-id determinism | 3 | GREEN |
| Slice 05 logs/metrics passthrough | 4 | GREEN |
| Slice 06 observability | 6 | GREEN |
| `invariant_public_api_smoke` | 6 | GREEN |
| `invariant_sampling_sink_is_otlp_sink_and_probe` | 2 | GREEN |
| **Total active tests** | **36** | **all GREEN** |

Per-slice mutation kill rate (from each slice's commit log + the
`cargo mutants --in-diff` runs):

| Slice | Caught | Unviable | Missed | Kill rate (viable) |
|---|---|---|---|---|
| 01 | 4 | 1 | 0 | 100% |
| 02 | 0 (covered by slice 01's error-bias short-circuit) | n/a | 0 | n/a |
| 03 | 8 | 1 | 0 | 100% |
| 04 | 0 (side-effect green from slice 03's hash determinism) | n/a | 0 | n/a |
| 05 | 16 | 9 | 0 | 100% |
| 06 | 29 | 1 | 0 | 100% |

**Cumulative**: 57 viable mutants across the six slices, 57 caught,
zero missed.

---

## ADR fidelity check

| ADR | Implementation | Verdict |
|---|---|---|
| ADR-0018 — public surface | Seven public items + two `#[doc(hidden)]` test seams (`__test_trace_view`, `__test_summary_tick_now`); ADR-0011 precedent honoured | PASS |
| ADR-0019 — dependency pinning | `xxhash-rust = "=0.8"` exact-minor; `aperture` as runtime dep (AGPL-symmetric); tokio features minimal `["macros", "rt", "sync", "time"]`; BSL-1.0 audit complete | PASS |
| ADR-0020 — summary aggregator | Three `AtomicU64` with `Relaxed` ordering; `SummaryTask` Sieve-owned with `tokio_util::sync::CancellationToken`; tick interval parameterisable; `__test_summary_tick_now` synchronous seam | PASS |
| ADR-0021 — Aperture integration | `SamplingSink<S, N>` decorator over `OtlpSink + Probe`; routes Traces through sampler, Logs and Metrics passthrough; `Probe::probe` delegates honestly; zero Aperture amendment | PASS |

---

## Test fidelity check

`praise:` All 36 tests import only the public surface plus the two
doc-hidden test seams. Grep over `tests/*.rs` for
`use sieve::(sampler|decorator|aggregator|observability)` returns
zero matches. Hexagonal boundary upheld throughout.

`praise:` Strategy C "real local" implemented end-to-end: real
Aperture `RecordingSink` is the inner sink for `SamplingSink<S, N>`
in every fixture. The `SharedRecording` newtype in
`tests/common/mod.rs` is the test-scoped adapter that lets the
fixture share state with the decorator's inner sink without growing
Aperture's surface.

`praise:` Slice 03's deterministic-seed 10000-trace fixture is
exemplary. The xxh3_64 mapping plus the fixed-seed `fixture_trace_id`
helper produces an identical partition on every run; the ±3% band at
rate 0.5 is enforced as a deterministic tolerance, not as a
probabilistic flake.

---

## RED → GREEN trail

Each slice's commit history shows the canonical pattern: RED tests
in place from DISTILL, production stub returning `unimplemented!()`,
implementation lands one piece at a time driven by the smallest
failing test. The pre-commit hook ran on every commit (every commit
message implicitly confirms via the `[pass] all pre-commit gates green`
output).

Two pinning commits (Slice 03 and Slice 06) added targeted unit
tests when initial mutation runs surfaced survivors. This is the
right discipline: kill survivors with new tests rather than weaken
the contract.

No `--no-verify` was used. Every commit went to `main` directly per
pure trunk-based discipline.

---

## Hexagonal boundary compliance

`praise:` Production code does not leak internals. The public surface
is nine items (seven consumer-facing plus two test seams). All tests
import only from `sieve::` public re-exports or `aperture::` public
ports. No direct imports of `sieve::sampler`, `sieve::decorator`, etc.

The only real infrastructure adapter in tests is Aperture's
`testing::RecordingSink` (a test double itself, not production code).
No database, filesystem, or external network I/O.

---

## Defensive coding

`praise:` `#![forbid(unsafe_code)]` at `lib.rs:50`. No `unwrap`/
`expect` on user-input paths: `from_env` uses
`f64::from_str().map_err(...)`; `mapped_to_unit_interval` is pure;
`accept_traces` skips invalid `trace_id`s defensively.

AtomicU64 ordering: `Relaxed` on hot path (record/increment) and on
snapshot path (swap). Cross-counter race documented as benign for
the "approximate aggregate" contract per ADR-0020.

Drop is panic-safe: `SummaryTask::drop` calls `cancel()` on the
`CancellationToken` (sync, non-blocking) and abandons the JoinHandle.
No `await` in Drop.

Tokio task ownership: `SummaryTask` owned by `SamplingSink`, spawned
at construction, cancelled on Drop via cooperative cancellation.

---

## Back-propagation: `Any` downcast for rate-reading — ACCEPTED

`SamplingSink::new` reads the rate from the sampler via `Any`
downcast to `HeadSampler` so the periodic INFO summary can carry the
configured rate. The `Sampler` trait stays at one method per ADR-0018
lock. Future samplers that don't implement `HeadSampler::rate()` get
`f64::NAN` in the summary's rate field.

**Assessment**: pragmatic v0 shape, honestly documented in code at
`decorator.rs:111-147`. The `Any` downcast is bounded by `N: 'static`
which the existing `Sampler: 'static` bound already provides.

**Forward path** (post-v0, not blocking): when v1 introduces a second
`Sampler` impl (e.g. tail-sampling), extend the `Sampler` trait
additively with `fn rate(&self) -> f64 { f64::NAN }`. Non-breaking
since the default is the same NaN fallback the downcast falls back
to today. The downcast can then be removed.

For v0 the choice holds. No blocker.

---

## Cross-cutting invariants

`invariant_public_api_smoke` (6 tests): type-level checks that
`Decision`, `KeepReason`, `HeadSampler`, `TraceView`,
`SieveConfigError`, `SamplingSink` compile and constructors return
`Ok` for nominal inputs. Compile-time public-API contract.

`invariant_sampling_sink_is_otlp_sink_and_probe` (2 tests):
generic-bounds assertion that `SamplingSink<RecordingSink, HeadSampler>`
implements `OtlpSink + Probe`. Compile-time trait-impl check. The
existing Aperture xtask AST walk that verifies "every OtlpSink type
also implements Probe" covers `SamplingSink` automatically per
ADR-0021.

---

## House style

British English in commits and comments verified ("honoured", not
"honored"). No FTE estimates. Library framing for the platform
component is consistent.

---

## Workspace integrity

`cargo build --workspace --all-targets --locked` succeeds clean.
`cargo clippy --workspace --all-targets --locked -- -D warnings` clean.
`cargo deny --all-features check` clean (BSL-1.0 entry validated for
xxhash-rust). The harness, Aperture, and Spark crates' tests pass
with no regression.

---

## Suggestions (post-v0, non-blocking)

`suggestion (non-blocking):` Consolidate Slice 02's nine tests into a
parametrised pattern. The current shape (one `#[test]` per rate × per
trace shape) gives independent failure reporting at the cost of
verbosity. Spark's slice 05 had a similar consolidation candidate
flagged at its DELIVER review. Worth revisiting in a hygiene pass
once Sieve has a v1 in flight.

`suggestion (non-blocking):` Add `Sampler::rate(&self) -> f64` to the
trait additively in v1. Replaces the `Any` downcast in
`SamplingSink::new`. Non-breaking due to the default
`fn rate(&self) -> f64 { f64::NAN }`. Defer to the slice that
introduces the second `Sampler` impl.

`suggestion (non-blocking):` Slice 03's 10000-trace fixture is large
enough to slow down `cargo test` invocations during dev. If
developers find it noisy, consider gating it behind a feature flag or
`#[ignore]` with an opt-in run command. Non-blocking; the deterministic
seed makes it non-flaky.

---

## Praise

`praise:` The decorator pattern is the canonical Rust shape for "wrap
a trait to add a cross-cutting concern". Zero Aperture surface change.
The generic `SamplingSink<S, N>` lets tests use concrete types while
production uses erased `Arc<dyn OtlpSink + Probe>` — exemplary
composition.

`praise:` The error-bias rule (`status.code == ERROR` on any span
keeps the trace) is operationally clear, language-agnostic,
framework-agnostic. Slice 02's error-bias short-circuit is exactly
where the rule belongs.

`praise:` Test seams (`__test_trace_view`, `__test_summary_tick_now`)
follow the Spark precedent. The `#[doc(hidden)]` + `__` prefix
convention is now well-established in the codebase and reads as
"stable internals not part of the consumer-facing contract".

`praise:` Per-binary process isolation via eight `[[test]]`
declarations plus `SIEVE_TEST_SERIAL` mutex plus
`#[serial_test::serial]` defends process-global state cleanly.
Mirrors Aperture's ADR-0015 and Spark's ADR-0011 precedents exactly.

`praise:` The summary-aggregator concurrency model is wait-free on
the hot path without being clever. Three `AtomicU64`s, `Relaxed`
ordering, `swap`-based snapshot. The cross-counter race is documented
as benign for the approximate-aggregate contract.

---

## Approval

**APPROVED**. Merge without iteration. Coordinate Sieve v0 graduation:

1. Remove `--exclude sieve` from `scripts/hooks/pre-commit` Gate 1.
2. Remove `--exclude sieve` from `.github/workflows/ci.yml` Gate 1.
3. Tag `sieve/v0.1.0` with the canonical commit at the close of this
   DELIVER + graduation pair.
4. Update the narrative + slides for the DELIVER closure (per the
   wave-by-wave cadence rule).
5. Forge peer review on the DEVOPS workflow extensions can run
   independently once the next CI run on Sieve-touching commits comes
   back green.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.
