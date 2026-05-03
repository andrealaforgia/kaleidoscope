# DISCUSS-wave peer review (acceptance-design lens) — `otlp-conformance-harness-v0` — iteration 1

**Reviewer**: `nw-acceptance-designer-reviewer` (Sentinel) | **Date**: 2026-05-04 | **Iteration**: 1 of 2 | **Parallel to**: `nw-product-owner-reviewer` iteration 1 (APPROVED)

## Verdict: **APPROVED with caveats**

Sentinel returned APPROVED in the closing verdict but flagged four substantive issues using the conventional-comments labels `issue (blocking)` (×2) and `issue (high)` (×2), plus five non-blocking suggestions. The closing line read "Proceed to DISTILL with the five non-blocking suggestions above applied before test implementation". Honestly, this reads as "approved conditional on the four substantive fixes being applied" rather than a clean approval. The four substantive findings should be addressed in iteration 2 before the DISTILL handoff.

## Summary of dimensions reviewed

| Dimension | Verdict | Notes |
|---|---|---|
| BDD scenario executability and real data | PASS | One non-blocking question on US-07's hash algorithm. |
| Acceptance-criteria precision and verifiability | PASS with two blocking issues | US-03 AC 2 non-observable; US-06 AC 5 vague. |
| Outside-In TDD readiness | STRONG PASS | Public API entry points clear; observable outcomes throughout; OtlpViolation shape well-specified. |
| Coverage of the rule set | STRONG PASS | Every rule defended by at least one scenario; corpus strategy in US-07 closes the loop. |
| Mutation testing readiness | PASS with two high issues | US-02 scenario 1 byte-locus too loose; US-04 scenario 2 type-system assertion not runtime-testable. |
| Side-effects testability | STRONG PASS | Absence-of-side-effects assertions are specific (stdout, stderr, logging facades) and testable. |
| Observable-behaviour mechanical checklist | PASS with one violation | US-04 scenario 2 contains a compile-time-only assertion; should reframe as an AC verified by CI. |
| Mandates (CM-A hexagonal, CM-B business language, CM-C journey completeness) | PASS | All three. |
| Traceability (Check A story-to-scenario, Check B environment) | PASS | Every story has scenarios; Check B is not applicable to a pure library. |
| Fixture theatre detection | PASS | None detected. |

## Substantive findings (to address in iteration 2)

### Blocking — US-03 AC 2 (user-stories.md, lines 290–291)

**Original**: "When a byte sequence decodes as the asserted signal, the harness does not enter the alternative-decode path (i.e. there is no observable behaviour difference for valid input)."

**Problem**: The "does not enter the alternative-decode path" wording is an internal-state assertion. The DISTILL acceptance-test author cannot write a Cargo test for this without mocking or reflection.

**Recommended replacement**: "When a byte sequence decodes as the asserted signal, the harness returns `Ok(record)` immediately without attempting to decode as alternative signal types. The returned record is the typed value, not an intermediate state or surrogate."

### Blocking — US-06 AC 5 (lines 575–576)

**Original**: "At end of slice-06 the public API exposes exactly three `validate_*` functions, one per signal type, all sharing the same return-shape pattern."

**Problem**: "sharing the same return-shape pattern" is too vague. A test cannot verify it without inventing the contract.

**Recommended replacement**: "The public API exposes exactly three functions: `validate_logs(bytes: &[u8], framing: Framing) -> Result<ExportLogsServiceRequest, OtlpViolation>`, `validate_traces(bytes: &[u8], framing: Framing) -> Result<ExportTraceServiceRequest, OtlpViolation>`, and `validate_metrics(bytes: &[u8], framing: Framing) -> Result<ExportMetricsServiceRequest, OtlpViolation>`. All three return the same `OtlpViolation` type on the error path."

### High — US-02 scenario 1 (lines 162–170)

**Problem**: The byte-locus assertion "any value within the input" lets a mutation that always returns `ByteOffset(0)` pass.

**Recommended split**: Two scenarios. The first asserts the byte offset is between 40 and 60 (near the truncation point at byte 50), forcing the harness to compute a meaningful locus. The second asserts that the violation's `observed` field contains one of a named set of decode-error descriptions ("unexpected EOF", "wire type error", "missing length-delimited data").

### High — US-04 scenario 2 (lines 363–371)

**Problem**: The assertion "the type is not re-exported under a harness-local name" is a compile-time type-system assertion that cannot be runtime-tested without reflection or internal-state inspection.

**Recommended reframe**: Move the substance from a Gherkin scenario into an acceptance criterion verified by a CI check (`cargo expand` or `cargo doc --no-deps` grep) that the return type's full path matches the upstream `opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest`.

## Non-blocking suggestions

1. US-07 technical notes should name the hash algorithm explicitly: SHA-256, hex-encoded in `.expected.json`.
2. US-02 scenario 2 should name the decode-error types the `observed` field is expected to mention.
3. The corpus runner's `.expected.json` schema (`asserted_signal`, `asserted_framing`, `expected_verdict`, `content_hash`, `source`) should be confirmed before DISTILL writes the runner.
4. (Implicit from Dimension 7) The compile-time type-path verification described in US-04 is a CI check whose runner choice can be deferred to DEVOPS, but the check itself belongs in the DESIGN-wave deliverables.
5. The "no telemetry from the harness itself" assertion is well-tested via stdout/stderr/logging-facade observation; the test pattern should be documented once and reused across all three signals.

## Praise

The Outside-In TDD readiness is strong. Every scenario invokes a public API entry point, every assertion checks an observable outcome at the boundary, and the set of scenarios is sufficient to drive the implementation without further requirements gathering. The flat emotional arc and narrow journey are correct for a pure library, not bugs. The corpus strategy (US-07) is the load-bearing defence of the rule set as the harness evolves; it is well-conceived and well-scoped.

## Handoff gate for iteration 2

The four substantive findings (two blocking, two high) should be applied to `user-stories.md` before the DISTILL handoff. The five suggestions can either be applied at the same time or left as informal improvements for DISTILL to absorb.
