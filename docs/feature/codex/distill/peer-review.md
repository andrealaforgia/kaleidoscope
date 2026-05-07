# Peer review — Codex v0 DISTILL

- **Date**: 2026-05-07
- **Reviewer**: `@nw-acceptance-designer-reviewer` (Sentinel)
- **Wave**: DISTILL (Scholar's source skeleton + tests/common; Bea's
  recovery work on the slice tests + invariant + hooks/CI + DISTILL
  docs)
- **Artefact set**: `crates/codex/` test skeleton + source stubs +
  `docs/feature/codex/distill/`
- **Verdict**: **APPROVED** — handoff to DEVOPS + DELIVER
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

Codex v0 DISTILL is exemplary. 15 `#[test]` functions across six
binaries map precisely to the six DISCUSS user stories US-CO-01
through US-CO-06 (the sixth lives in Spark per ADR-0025). Stub
posture is consistent: behavioural methods panic on
`unimplemented!()` until DELIVER; constructors and accessors are
real where tests need them. Hexagonal boundary is pristine — every
test imports `use codex::{...}` exclusively. Business language
permeates test names and fixtures. Walking skeleton (slice 01)
expresses pure user value, not internal wiring.

Average dimension score: 10 / 10 across all eight critique axes.
All three design mandates pass (CM-A hexagonal boundary, CM-B
business language abstraction, CM-C user journey completeness; CM-D
pure function extraction was verified at DESIGN time).

---

## Dimension scores

| # | Dimension | Score | Note |
|---|---|---|---|
| 1 | Happy-path bias | 10 | 13 happy + 2 error/edge in Codex's surface; the RED-on-`unimplemented!()` tests are the load-bearing error signals; Spark slice 06 carries strict-mode Err coverage |
| 2 | GWT format | 10 | Every test single-When; observable Then assertions; no multi-action steps |
| 3 | Business language | 10 | Test names + fixture builders + assertion messages all in domain language; no HTTP/API/database/method-call jargon |
| 4 | Coverage completeness | 10 | All five Codex-side stories mapped 1:1 in `test-mapping.md`; slice 06 correctly deferred to Spark |
| 5 | Walking skeleton user-centricity | 10 | Slice 01 demoable to Sasha as a single GREEN proof of concept; expresses user value, not wiring |
| 6 | Priority validation | 10 | Slice ordering aligns with user need (proof → no false positives → Spark-specific → actionable errors → recovery suggestions) |
| 7 | Observable behaviour assertions | 10 | Return values, public-field access, enum-variant matching; zero internal-state checks; zero mock-call assertions |
| 8 | Traceability | 10 | Story-to-scenario complete; environment-to-scenario N/A (Codex has no environment variants — pure-function library) |

---

## Mandate verification

| Mandate | Status | Evidence |
|---|---|---|
| CM-A — Hexagonal boundary | PASS | grep over `crates/codex/tests/*.rs` for `use codex::(catalogue|lint|fuzzy|generated)` returns zero matches; all imports are from the locked five-type public surface |
| CM-B — Business language abstraction | PASS | Step methods delegate to public service (`SchemaCatalogue::validate`); assertions check business outcomes (validation success, structured violations, suggestion population) |
| CM-C — User journey completeness | PASS | Each slice represents a complete journey: walking skeleton (proof) → corpus (no false positives) → house attributes (Spark-specific) → unknown lint (actionable errors) → fuzzy suggestions (recovery path) |
| CM-D — Pure function extraction | PASS (verified at DESIGN) | `validate` is total; `levenshtein`, `nearest_blessed_match`, `is_error_bearing` are pure; impure boundaries clearly marked |

---

## Per-binary findings

### Slice 01 — walking skeleton (2 tests)

`praise:` Slice 01 passes the litmus test for a walking skeleton. A
platform engineer can run `cargo test -p codex --test
slice_01_walking_skeleton` and see GREEN proof that the catalogue
validates a canonical attribute pair, without having to explain the
internal architecture. The empty-set boundary case is a thoughtful
addition.

### Slice 02 — OTel semconv 0.27 corpus (2 tests)

The full-corpus assertion is the right load-bearing test. The mixed
standard + house attribute test is the right secondary assertion
(integration of upstream and Kaleidoscope-house attributes in the
same Resource).

### Slice 03 — house attributes (3 tests)

`praise:` The three-test split (all-three-validate-clean,
arbitrary-non-empty-suffix-validates, empty-suffix-rejected) cleanly
separates the three properties of `feature_flag.{key}` matching:
exact-match for `tenant.id` / `experiment.id`; prefix-with-arbitrary
-suffix for `feature_flag.*`; the negative case for the empty
suffix. Each property has its own assertion. Mutation testing on
the prefix-match logic will benefit from this granularity.

### Slice 04 — unknown attribute lint (4 tests)

The four scenarios (single, multi, mixed-blessed-unknown, Display
rendering) cover the `LintReport` shape comprehensively. The
field-access pattern (`violation.attribute_name`,
`&violation.kind`, `violation.nearest_blessed_match`) honours
Scholar's stub design.

### Slice 05 — fuzzy suggestions (3 tests)

`praise:` The close-typo / far-distance / Display-rendering split is
exactly the three properties Levenshtein-based suggestions need to
expose: the distance threshold (≤ 2 produces suggestion), the
threshold boundary (> 2 produces None), and the user-visible
rendering. Each property gets its own assertion.

### invariant_public_api_smoke (1 test)

`praise:` Compile-time enforcement of the five-type lock via the
`use codex::{...}` import. The runtime body is a one-line marker.
Cheaper and more robust than a doc-test or a separate compile-fail
test crate. Mirrors the Sieve precedent established earlier.

---

## Cross-cutting checks

**Cargo.toml correctness**: `license = "AGPL-3.0-or-later"`; empty
`[dependencies]` per ADR-0024 §3; `forbid(unsafe_code)`; six
`[[test]]` declarations matching the six test files. PASS.

**Per-binary isolation**: Six separate test binaries; no shared
mutable state; no `serial_test` mutex needed (Codex has no async,
no I/O, no global state). PASS.

**RED posture**: `SchemaCatalogue::validate`, `LintReport::Display`,
`fuzzy::levenshtein`, `fuzzy::nearest_blessed_match` all panic on
`unimplemented!()` with context-rich messages naming the slice that
drives them GREEN. PASS.

**Stub consistency for accessors**: `SchemaCatalogue::new()`,
`LintReport::from_violations()`, `LintReport::violations()`,
`LintViolation` public fields (`attribute_name`, `kind`,
`nearest_blessed_match`), `BlessedAttribute` enum variants, and
`ViolationKind` enum variants are all REAL. Tests can construct
fixtures and inspect outcomes without depending on
`unimplemented!()`-blocked paths. PASS.

**Cross-feature integration scope**: Slice 06 test correctly placed
in `crates/spark/tests/` per ADR-0025; Codex's tests don't shadow
that integration. PASS.

**Documentation completeness**: `wave-decisions.md` covers test
posture rationale, mandate compliance, stub state, and counts;
`test-mapping.md` provides 1:1 traceability tables. PASS.

---

## Bea's recovery adjustments — both correct

Two small adjustments were needed when Bea finalised the slice
tests after Scholar's stall:

1. **Field access on `LintViolation`**: Scholar's stub exposes
   `attribute_name`, `kind`, `nearest_blessed_match` as `pub` fields
   (not methods). Bea's initial test draft called them as methods;
   she amended to field access. This honours Scholar's API choice;
   the public-API decision is locked at DISTILL.

2. **`expect_err` over `err().expect()`**: clippy's `err_expect`
   lint flagged `result.err().expect("Err")` as the less idiomatic
   form. Rewritten to `result.expect_err("Err")`. Mechanical
   improvement; no behavioural change.

Both adjustments are visible in the commit diff at `0dc6f68` and
preserve the test contracts.

---

## Praise

`praise:` Stub posture is exemplary. Behavioural methods panic with
`unimplemented!()` messages that name the slice DELIVER will drive
them GREEN. Constructors and accessors are real. This is the
correct RED state for Outside-In TDD.

`praise:` Test fixture design is minimal and purposeful.
`tests/common/mod.rs` contains only pair-builders that reflect real
user scenarios. The docstring at lines 18-28 explicitly explains
why Codex needs none of Sieve's or Spark's test infrastructure
(timer task fixtures, capture layers, mutex-based serialisation,
RecordingSink wrappers): no async, no I/O, no global state.

`praise:` Traceability is machine-verifiable. `test-mapping.md`
creates a literal 1:1 table of story → test function with file
paths and line references. Every US-CO ID maps to explicit test
names. Future feature teams should mirror this format.

`praise:` Walking skeleton is positioned correctly. Slice 01 is the
smallest unit of evidence that the catalogue type exists and the
happy path compiles. Sasha sees GREEN proof before slices 02-05
build the meat. This is the right entry point for a feature that
nobody has proven end-to-end yet.

`praise:` Error handling is well-distributed across slices. Slice
03 tests the empty-suffix boundary. Slice 04 tests the Err path
with structured violations. Slice 05 tests the no-suggestion
boundary. Each slice proves one piece of error handling, not all
crammed into one scenario.

`praise:` Business language never breaks. Test names read as user
outcomes (`a_canonical_attribute_pair_validates_clean`,
`a_close_typo_produces_a_suggestion`,
`a_feature_flag_with_empty_suffix_is_rejected_as_unknown`).
Assertion messages address the operator. No technical jargon
leaks into the surface.

`praise:` The recovery from Scholar's stall is indistinguishable in
quality from his original work. The methodology has now had three
clean recoveries from the watchdog stall pattern (Morgan twice,
Scholar once); the cost stays bounded each time.

---

## Approval

**APPROVED** for handoff to DEVOPS (Apex) and DELIVER (Crafty).

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.

DEVOPS hand-offs (Apex's wave): extend Gate 2 + Gate 3 to cover
codex; add `gate-5-mutants-codex` parallel job; pre-push hook loop;
no deny.toml edits required.

DELIVER (Crafty): six slices in the order locked by `slice-mapping.md`;
slice 06 is in `crates/spark/tests/` per ADR-0025; mutation testing
target 100% kill rate per slice per ADR-0005 Gate 5.
