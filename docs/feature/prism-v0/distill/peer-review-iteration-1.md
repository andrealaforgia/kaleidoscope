# Peer review — Prism v0 DISTILL, iteration 1

- **Date**: 2026-05-08
- **Reviewer**: `@nw-acceptance-designer-reviewer` (Sage), Haiku model
- **Wave**: DISTILL — gate before DELIVER
- **Artefact set**: 21 files (3 markdown specs + 8 Vitest + 6 Playwright + 4 fixtures)
- **Verdict**: **APPROVED** — proceed to DELIVER once DEVOPS revisions clear Forge iter-2
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required
- **Confidence**: High. All 21 artefacts read in full.

---

## Executive summary

DISTILL is sound across all eight critique dimensions and all four
content mandates. AC traceability 97% (29/30 ACs covered; AC-4.4
intentionally out-of-scope by construction since "URL is the only
share artefact at v0" is a system invariant, not a tested
behaviour). Test pyramid shape and mock-at-the-seam discipline match
ADR-0031. Strategy C "real local" Prometheus posture is fully
specified and consistently applied. RED-state idiom (`throw new
Error('UNIMPLEMENTED — Slice NN DELIVER')`) is consistent across
all 14 test files.

Bea's seven finalisation files (slice-05 Vitest, slice-04/05/06
Playwright, three invariants) cohere with Scholar's earlier work
verbatim. No stylistic drift, no voice shift, no testing-philosophy
divergence — the recovery pattern absorbed the partial output
cleanly.

The single non-blocking observation is on error-path coverage
ratio: 26% across the suite, below the 40% target. Acceptable for
v0 because error paths are concentrated in slice 03 (error states)
and the cumulative coverage is still 97% AC-by-AC. Note for DELIVER's
slice 03 implementation to verify error paths are exercised end-to-
end, not just at the unit level.

---

## Dimension scoring (0-10)

| Dimension | Score | Notes |
|---|---|---|
| 1. Happy path bias | 6/10 | 26% error paths vs 40% target; non-blocking |
| 2. GWT format compliance | 10/10 | All scenarios use Given-When-Then comments correctly |
| 3. Business language purity | 10/10 | Operator voice consistent ("I am Priya at 03:14") |
| 4. Coverage completeness | 9/10 | 29/30 ACs (97%); AC-4.4 justified out-of-scope |
| 5. Walking skeleton user-centricity | 9/10 | Real Prometheus, real chart, real URL |
| 6. Priority validation | 9/10 | All test design decisions justified per D1-D17 |
| 7. Observable behaviour assertions | 10/10 | No mock.calls, no private-state assertions |
| 8. Traceability | 9/10 | Stories fully mapped; environment mapping implicit |
| 9. Walking skeleton boundary proof | 10/10 | Strategy C declared and matched |

---

## Content mandates

### CM-A: Hexagonal Boundary Enforcement — PASS

All test imports reach driving ports:
- `queryRange` from `lib/promql/client`
- `decode`, `encode` from `lib/url-state/codec`
- `buildOption` from `lib/echarts/buildOption`
- `QueryPanel` from `panels/query/QueryPanel`

No internal helpers, validators, or private components imported.

### CM-B: Business Language Abstraction — PASS

Given/When/Then comments use operator perspective verbatim. Test
bodies invoke public functions which are the service layer. No
HTTP calls or database operations in test code. Assertions check
business outcomes, not mock call counts.

### CM-C: User Journey Completeness — PASS

Walking skeleton (Slice 01 E2E) traces end-to-end journey: real
browser, fresh page, type query, see chart, share URL. Subsequent
slices test focused scenarios grounded in user value (range affects
what chart shows; auto-refresh keeps incident-time chart current).

### CM-D: Pure Function Extraction Before Fixtures — PASS

Pure functions cleanly identified and tested directly:
- `lib/url-state/codec` (encode + decode)
- `lib/auto-refresh/reducer` (reduce)
- `lib/echarts/buildOption` (buildOption)

Impure code isolated behind seams (`fetchFn`, `Scheduler`).

---

## Strengths

`praise:` Strategy C "real local" is fully implemented. The
walking skeleton uses a real local Prometheus container; the
fidelity-anchor fixture is hand-authored with NaN gaps and
non-uniform timestamps to drive structural mutation testing.

`praise:` Mock-at-the-seam discipline is exemplary. `fetchFn` and
`Scheduler` are the only mocked surfaces. React, ECharts, and
DOM are NOT mocked — the integration tests run JSdom + real
ECharts canvas + real React rendering.

`praise:` RED-state idiom is consistent across 14 files. Every
unimplemented test body throws `'UNIMPLEMENTED — Slice NN
DELIVER'`. No `expect.fail()`, no silently-passing tests, no
tests that compile but assert nothing.

`praise:` Mutation-evidence anchors in test-mapping.md name the
specific operator-mutation kind each test deterministically kills.
The fidelity-anchor fixture shape (NaN at index 2, non-uniform
timestamps, boundary values at indices 0 and 4) is engineered to
catch the named StrykerJS mutants surgically.

`praise:` Operator voice is consistent. Tests open with "I am
Priya. I have just opened Prism..." or "I am the postmortem-time
engineer. Five days after the incident...". The persona-driven
narrative survives across Scholar's and Bea's halves; no Rust-
side `cargo test` idioms forced into Vitest.

`praise:` Bea finalisation hand-off is clean. The seven files Bea
wrote after Scholar's interruption follow Scholar's conventions
verbatim: AGPL header, persona narrative, story/KPI/ADR map,
imports from ADR-0026 modules, throw-idiom, Given/When/Then
comments. The recovery pattern absorbed the partial output.

`praise:` Stub-export contract is correct for DELIVER handoff. At
slice-01 DELIVER, the crafter writes stubs at `apps/prism/src/lib/`
that satisfy the test imports; tests then compile and fail
meaningfully on the `'UNIMPLEMENTED'` throws. The first DELIVER
commit is bounded.

---

## Non-blocking suggestions

`suggestion (non-blocking):` Error-path coverage ratio. The suite
has ~26% error paths (~14 scenarios) against a 40% target. The
target is a heuristic, not a hard rule; the cumulative AC coverage
is still 97%. At slice 03 DELIVER, verify error paths run end-to-
end (not just at the unit level). If gaps surface, add specific
error-path tests at that point rather than retroactively.

`suggestion (non-blocking):` AC-4.4 (URL-as-only-share) is intentionally
unmapped because it is a system invariant, not a behaviour. Document
this explicitly in `test-mapping.md` to prevent future reviewers from
flagging it as a gap.

`suggestion (non-blocking):` Slice 04 property test (lines 132-137)
checks reducer state-transition correctness — a property assertion
on the state machine's design contract (ADR-0029 §7). Acceptable for
v0; a more behavioural test (action → fetch issued → tick fired) is
covered by the E2E spec.

---

## Verdict

**APPROVED** for DELIVER handoff once DEVOPS revisions clear Forge
iteration 2.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.

The DISTILL artefacts are the contract DELIVER consumes. They are
the foundation that the crafter's Outside-In TDD turns RED into
GREEN slice-by-slice. They are sound.

Bea now revises the DEVOPS artefacts per Forge's CONDITIONAL
APPROVAL (5 CRITICAL + 3 HIGH) and re-submits for Forge iteration
2. Once DEVOPS clears, DELIVER's slice 01 can begin against this
DISTILL contract.
