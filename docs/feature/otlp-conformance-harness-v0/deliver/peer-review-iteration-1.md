# DELIVER-wave peer review — `otlp-conformance-harness-v0` — iteration 1

**Reviewer**: Crafty (Review Mode, `nw-software-crafter-reviewer`) | **Date**: 2026-05-04 | **Iteration**: 1 of 2 | **Commit range reviewed**: `c17e7bb..87b07d9`

## Verdict: **APPROVED**

All ten review dimensions pass. No defects in the critical categories (no test modification, no testing theatre, no internal-class testing, no mock-dominated SUT, no fixture theatre). The 73 of 73 green and 100% mutation kill rate verified by the toolchain reflect honest engineering; the survivors at intermediate `cargo mutants` passes were killed by production-code refactoring, not by test weakening. Ready to proceed to the DEVOPS wave.

## Per-dimension verdicts

| # | Dimension | Verdict | Notes |
|---|---|---|---|
| 1 | Code quality | PASS | Clean module split (`framing`, `signal`, `violation`, `decode`, `validate`). `#![forbid(unsafe_code)]` at crate head. No `unwrap()` on user input; all failures translated to `OtlpViolation`. No commented-out code, no `TODO`. `Cow<'static, str>` for `expected`/`observed` avoids needless allocations. Two helper formatters (`DisplayRule`, `DisplayLocus`) are pattern-match exhaustive — future variant additions will be compile errors, not silent Debug fallbacks. |
| 2 | Refactor discipline (red-green-refactor) | PASS | Slice 01 introduced shared `empty_input_violation` helper from the first cycle, anticipating reuse. Slice 02 grew the strict top-level-tag check (`first_tag_references_resource_field`) to honour US-02's rejection contract over prost's permissive default. Slice 03 extracted `decode_strict<M: Message + Default>` as the single chokepoint replacing an earlier closure-based wrapper. Gate 5 cycle simplified the prost-error classifier by collapsing redundant disjuncts after mutation pressure revealed them. Each refactor is a real design improvement, not just adding cases. |
| 3 | Inner-loop unit test quality | PASS | Spot-check of eight tests confirms observable assertions through the helper's contract, no inspection of private state, no tautologies. Per-disjunct tests for `classify_prost_decode_error` are precisely the shape mutation testing rewards: each `\|\|` disjunct gets an isolating test so a `\|\|→&&` flip breaks exactly one. |
| 4 | Public API contract preserved | PASS | Three function signatures from US-06 AC 5 unchanged. `OtlpViolation` field set from ADR-0002 unchanged: `rule`, `locus`, `expected`, `observed`, `signal_asserted`, `framing_asserted`. The crate-private `source: Option<Box<dyn Error...>>` carries `prost::DecodeError` without exposing it. `#[non_exhaustive]` preserved on every public enum and struct. Closed `Rule::WireType(WireTypeRule)` enum nesting honoured. `lib.rs` re-exports nothing from `opentelemetry-proto` per US-04 AC 2. |
| 5 | Acceptance tests untouched | PASS | All seven slice files import only the public harness surface and the upstream `opentelemetry_proto::tonic::collector::*` types directly. Wave-decisions records: "0 changes to acceptance tests, fixtures, or corpus vectors." |
| 6 | Mutation-survivors history | PASS | Three passes recorded. Pass 1: three `\|\|→&&` survivors in `classify_prost_decode_error`. Pass 2: one survivor in `matches_wire_type_category` after the per-disjunct extraction. Pass 3: zero survivors. **Each survivor was killed by production-code simplification or by writing a more discriminating test, never by test relaxation.** This is the right shape for mutation-driven development. |
| 7 | Q1 (`opentelemetry-proto` feature gates) | PASS | Constraint is real and confirmed via `cargo tree`: `features = ["logs", "trace", "metrics"]` pull `opentelemetry` and `opentelemetry_sdk` into the build graph. Crafty accepted with rationale: zero runtime impact (dead-code elimination), zero public-API impact, zero licence impact (transitives are Apache-2.0 / MIT). Trade-off documented in `deny.toml` (multiple-versions relaxation). Upstream `messages-only` feature-gate request listed as DEVOPS follow-up. Defensible. |
| 8 | DEVOPS open questions | PASS | Six questions: CI runner choice, toolchain provisioning for nightly Gates 2–3, `rust-toolchain.toml` policy, mutation-test budget for CI, KPI 4 verdict-counts artefact, upstream feature-gate issue. All real, scoped, actionable. None are paperwork. |
| 9 | Trunk-based discipline | PASS | Eight commits on `main`, no feature branches, no merge commits, Conventional Commits naming, each commit's diff atomic to a single slice or logical unit. Commit messages explain *why*, not *what*. |
| 10 | House style | PASS | British English throughout (`behaviour`, `honour`, `optimise`, `licences`). No human-effort estimates; cycle counts are design facts, not estimates. Library-not-service framing in lib.rs and README. Personas remain consumers (Aperture, Sluice, third-party engineers, Kaleidoscope CI) — no builder personas. |

## The test-budget question

The review formula gives a budget of 2 × 7 = 14 inner-loop unit tests for seven distinct acceptance behaviours. Crafty wrote 21. That looks like overspend.

It is not. ADR-0005 Gate 5 mandates 100% mutation kill rate. Three iterations of `cargo mutants` revealed that 14 tests do not catch every mutation in the prost-error classifier. The 21 tests are the *minimum* set that achieves Gate 5 compliance with the shape of the eventual production code, after Crafty had already simplified that production code to collapse redundant disjuncts.

The discipline call is: the tests reflect what Gate 5 requires, the production code reflects the simplest design that the tests defend. That is exactly the relationship the methodology asks for.

## Strengths called out

- Production code is clean, idiomatic Rust. Names that say what things do. Single responsibility per module. `?` for error propagation. Pattern-match exhaustiveness in the formatters means future enum additions are compile errors.
- Refactor discipline is visible across the diffs, not just claimed in the wave-decisions document. Chokepoint extraction (`decode_strict`), shared helpers (`empty_input_violation`), simplification under mutation pressure (collapsed wire-type disjuncts).
- Mutation kill rate hit 100% via design improvement, not test weakening. This is the single most reliable signal that the test suite is actually load-bearing rather than performative.
- The Q1 resolution is honest and the deny.toml documents the trade-off rather than hiding it.
- Six DEVOPS open questions are precise enough that the next agent does not have to re-discover the constraint set.

## Iteration budget

Iteration 1 of 2 maximum per the skill. Zero blocking items; no iteration 2 required. DELIVER wave is closed for `otlp-conformance-harness-v0`.

## Handoff readiness

DEVOPS (`nw-platform-architect`) inherits a working CC0-1.0 Rust crate at `crates/otlp-conformance-harness/` with all ADR-0005 gates either green (Gate 1 cargo test, Gate 4 cargo deny check, Gate 5 cargo mutants) or correctly deferred (Gate 2 cargo public-api and Gate 3 cargo semver-checks, both blocked locally only by the absence of `rustup`). The six open questions in the wave-decisions document are the DEVOPS-wave brief.
