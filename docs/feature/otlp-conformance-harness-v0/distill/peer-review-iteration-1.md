# DISTILL-wave peer review — `otlp-conformance-harness-v0` — iteration 1

**Reviewer**: Sentinel (`nw-acceptance-designer-reviewer`) | **Date**: 2026-05-04 | **Iteration**: 1 of 2

## Verdict: **APPROVED**

Zero blockers. Zero high-severity findings. All ten review dimensions pass. All four mandates (CM-A hexagonal boundary, CM-B business language, CM-C journey completeness, CM-D pure-function extraction) compliant. The DISTILL wave is closed and ready for the DELIVER handoff.

## Per-dimension verdicts

| # | Dimension | Verdict | Spot-check |
|---|---|---|---|
| 1 | Outside-in discipline (hexagonal boundary) | PASS | All seven test files import only `otlp_conformance_harness::{validate_logs, validate_traces, validate_metrics, ...}` plus the public types. Zero imports of `pub(crate)` symbols. |
| 2 | Single Then per fact (mutation resistance) | PASS | US-01 no-side-effects scenario split into three independent tests (stdout / stderr / log records). US-02 truncation split into locus-window test (40..=60) + observed-category test (closed set). US-06 AC 5 signature lock split into four structural tests. |
| 3 | Real-data discipline | PASS | Accept paths and signal-mismatch rejects use `common::encode_minimal_*()` (deterministic prost encoding from `opentelemetry-proto =0.27.0` types). Hand-crafted bytes only for synthesised malformed reject paths (`truncate`, `bad_varint`, `bad_tag`). Corpus capture program is deterministic and idempotent. |
| 4 | Iteration-2 fixes preserved | PASS | US-02 byte-locus assertion is `(40..=60).contains(&offset)`. US-02 observed-field assertion is membership in a closed set (`unexpected EOF`, `wire type error`, `missing length-delimited data`, plus `invalid varint` for the varint case). US-04 type identity exercised at runtime via downstream-consumer call sites. US-06 AC 5 signatures pinned via typed `fn` pointer assignments and a shared-`OtlpViolation` vec assembly. |
| 5 | Scenario coverage | PASS | All seven user stories mapped: US-01→12 tests, US-02→9, US-03→6, US-04→7, US-05→5, US-06→10, US-07→3 + corpus runner. 52 tests total. No unmapped scenario. |
| 6 | Side-effects assertions | PASS | `tests/common/mod.rs` `observe_silence(f)` redirects stdout/stderr via `gag::BufferRedirect::*` and captures the `log::Log` facade via a process-wide capturing logger; serialised through a global mutex. Side-effects tests in slices 01 and 04 actually exercise this helper. |
| 7 | Corpus integrity | PASS | The corpus runner reads each `.bin`, computes SHA-256, formats `sha256:{hex}`, and asserts equality with the descriptor's `content_hash` *before* invoking `validate_*`. Mutation-refusal probe verifies bit-flip detection without invoking the validator. Rule-coverage enumeration walks descriptors and panics on any uncovered rule variant. |
| 8 | Public-API stub honesty | PASS | `src/lib.rs` is type re-exports plus three one-line wrappers delegating to `validate::*`. `src/validate.rs` and `src/decode.rs` return `unimplemented!()` only. `src/violation.rs` `Display` impl returns `unimplemented!()`. Zero production logic in any source file. |
| 9 | Mutation-test readiness | PASS | Every assertion is narrow and named: rule equality, locus window, observed-field membership in closed set, typed-function-pointer assignment, runtime type identity via downstream-consumer call. Trivial mutations to the eventual production code will fail at least one test each. |
| 10 | Open-question quality for DELIVER | PASS | Five questions, all real and scoped. Q1 (`opentelemetry-proto` feature gates pulling SDK in transitively at build time) is the most consequential and is named with three concrete options. Q2 (`ByteOffset::Unknown` vs `Known(0)` for empty input) is well-scoped: the user story does not mandate one. Q3–Q5 are clearly bounded. |

## Mandates

| Mandate | Verdict |
|---|---|
| CM-A hexagonal boundary | PASS |
| CM-B business language | PASS |
| CM-C journey completeness | PASS |
| CM-D pure-function extraction | PASS (trivially — no driven adapters in v0) |

## Walking-skeleton strategy

Strategy A declared (pure-function leaf, no driven adapters): correctly declared and honestly implemented. No `@in-memory` claims, no Strategy B/C/D drift.

## Strengths called out

- Seven test files cover seven user stories with 52 tests total.
- Public API stubs (three `validate_*` functions + seven public types) are locked and documented byte-for-byte to US-06 AC 5.
- Shared test helpers are complete and mutation-resistant, including the realistic byte synthesis (`encode_minimal_*` reproduces what an OTel SDK would emit) and the silence-observation helper.
- Corpus capture program is deterministic and idempotent.
- 17 corpus vectors seeded under `tests/vectors/{signal}/{verdict}/` with `.expected.json` descriptors carrying `schema_version`, `asserted_signal`, `asserted_framing`, `expected_verdict`, `content_hash`, `spec_version`, `source`.
- Five open questions for DELIVER are real and actionable, not paperwork.

## Iteration budget

Iteration 1 of 2 maximum per the skill. Zero blocking items; no iteration 2 required. DISTILL wave is closed for `otlp-conformance-harness-v0`.

## Handoff readiness

DELIVER (`nw-software-crafter`) inherits 47 RED `unimplemented!()` panics across 47 tests, plus 5 structural-invariant tests already GREEN by design (signature locks, corpus rule-coverage, corpus mutation-refusal). Recommended slice order: 01 → 02 → 03 → 04 → 05 → 06 → 07. Exit criteria: all 52 tests green plus 100% mutation kill rate via `/nw-mutation-test`.

The most consequential open question for DELIVER is Q1 (`opentelemetry-proto` feature gates). The recommended path is: accept the substrate constraint and document it; or file an upstream issue requesting a `messages-only` feature gate; or investigate whether feature-gate isolation can decouple the messages from the SDK at build time.
