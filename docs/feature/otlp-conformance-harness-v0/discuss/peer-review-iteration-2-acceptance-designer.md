# DISCUSS-wave verification pass — `otlp-conformance-harness-v0` — iteration 2

**Reviewer**: `nw-acceptance-designer-reviewer` (Sentinel) | **Date**: 2026-05-04 | **Iteration**: 2 of 2 | **Scope**: targeted verification of the four substantive fixes raised at iteration 1

## Verdict: **APPROVED**

All four substantive fixes from iteration 1 are correctly applied. No regression detected in previously-approved stories. The DISCUSS wave for this feature is now closed and ready for the DESIGN-wave handoff.

## Per-fix verification

| Fix | Location | Verdict | Evidence |
|---|---|---|---|
| **Fix 1** US-03 AC 2 — observable assertion on decode matching | `user-stories.md:300` | APPROVED | Internal-state wording removed entirely. New AC asserts `Result::Ok(record)` with the typed upstream value at the public-API boundary. Pattern-match testable from a Cargo unit test. |
| **Fix 2** US-06 AC 5 — named function signatures and shared error type | `user-stories.md:583` | APPROVED | Three exact function signatures spelled out (`validate_logs`, `validate_traces`, `validate_metrics` with full parameter and return types). Explicit assertion that all three return `OtlpViolation` on the error path. |
| **Fix 3** US-02 scenarios — mutation-resistant locus and observed-field categories | `user-stories.md:162–188` | APPROVED | Original scenario correctly split. New locus assertion is "between 40 and 60 inclusive" near the truncation at byte 50, defeating ByteOffset(0) mutations. New `observed`-field assertion names a closed set: "unexpected EOF", "wire type error", "missing length-delimited data". |
| **Fix 4** US-04 scenario 2 + AC 2 — runtime-observable downstream usability + CI type-path check | `user-stories.md:372–379` (scenario), `:396` (AC 2) | APPROVED | Compile-time type-system step removed from the Gherkin. Scenario now describes runtime-observable downstream usability (record passed to a function expecting the upstream type, type-checks and runs without conversion). AC 2 carries the type-path identity contract (`opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest`) as a CI-verified invariant; mechanism choice correctly deferred to DESIGN. |

## Non-blocking suggestions applied

- **Suggestion 1**: Hash algorithm explicit (SHA-256, hex-encoded in `content_hash`, computed at vector creation, re-verified before validation) — US-07 technical notes, line 706.
- **Suggestion 2**: Decode-error categories enumerated in the `observed` field — US-02 scenarios, lines 177 and 187.
- **Suggestion 5**: Three observable channels (stdout, stderr, logging facade) enumerated in side-effect-absence ACs — US-01 AC 4 (line 100), US-04 AC 4 (line 398).

Suggestions 3 and 4 correctly skipped: 3 is already documented in `shared-artifacts-registry.md`; 4 (CI runner choice) is explicitly deferred to DEVOPS.

## Regression spot-checks

- **US-01** (lines 23–122): content unchanged except AC 4 wording tightening per suggestion 5. No substance loss.
- **US-05** (lines 420–507): content unchanged.

No regression detected.

## Definition of Ready re-verification

Aggregate verdict in `dor-validation.md` remains **PASS** for all seven stories. Per-story re-verifications recorded for US-01 (item 5), US-02 (items 4 and 6), US-03 (item 5), US-04 (items 4 and 5), US-06 (item 5), US-07 (item 7). US-05 unchanged.

## Wave-decisions record

`wave-decisions.md` carries an "Iteration 2 (2026-05-04)" section with the fix-by-fix table referencing post-edit line numbers, the suggestion table, the skip rationale, the DoR re-verification summary, and the iteration-budget acknowledgement. Record is complete and accurate.

## Closing

The DISCUSS wave for `otlp-conformance-harness-v0` is closed. Handoffs are ready:

- **DESIGN-wave** (`nw-solution-architect`): receives the locked seven user stories, the journey schema, the seven shared artefacts with sources and integration risks, the seven outcome KPIs, the wave-decisions record, and both peer-review records. DESIGN's job is the crate-level decisions: public surface, `OtlpViolation` shape, dependency pinning policy, conformance-test-vector layout, CI contract.
- **DISTILL-wave** (`nw-acceptance-designer`): receives the same artefacts; the BDD scenarios are now executable-ready and mutation-resistant; the corpus contract is named.
