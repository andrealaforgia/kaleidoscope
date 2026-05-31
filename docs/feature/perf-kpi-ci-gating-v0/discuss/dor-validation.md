# Definition of Ready Validation: perf-kpi-ci-gating-v0

Nine-item hard gate. Each item validated with evidence per story.

## Summary

| DoR Item | US-01 | US-02 | US-03 | US-04 | US-05 |
|----------|-------|-------|-------|-------|-------|
| 1. Problem statement clear, domain language | PASS | PASS | PASS | PASS | PASS |
| 2. User/persona with specific characteristics | PASS | PASS | PASS | PASS | PASS |
| 3. 3+ domain examples with real data | PASS | PASS | PASS | PASS | PASS |
| 4. UAT in Given/When/Then (3-7 scenarios) | PASS | PASS | PASS | PASS | PASS |
| 5. AC derived from UAT | PASS | PASS | PASS | PASS | PASS |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | PASS | PASS | PASS | PASS |
| 7. Technical notes: constraints/dependencies | PASS | PASS | PASS | PASS | PASS |
| 8. Dependencies resolved or tracked | PASS | PASS | PASS | PASS | PASS |
| 9. Outcome KPIs with measurable targets | PASS | PASS | PASS | PASS | PASS |

### DoR Status: PASSED

## Evidence

### Item 1: Problem statement clear, domain language

All stories trace to a single concrete pain: load-induced wall-clock flakes in
the local pre-commit hook force `--no-verify` bypasses. Domain language (pre-commit
hook, gate-1-test, p95, fsync contention) is used throughout. Evidence: the
Background section and each story's Problem framing in user-stories.md.

### Item 2: User/persona with specific characteristics

Persona is the Kaleidoscope maintainer in two specific contexts: running the
local pre-commit hook under heavy parallel-build load (US-01, US-04), and reading
CI results on push and PR (US-02). US-05 names the future contributor adding a new
perf test. Each "Who" block lists context and motivation.

### Item 3: 3+ domain examples with real data

Each story has three concrete examples using real artefacts: real test names
(`ingest_p95_latency_under_three_milliseconds`, `query_p95_latency_under_ten_milliseconds`,
`observe_p95_latency_under_ten_microseconds`), real thresholds (3 ms, 10 ms, 10 µs,
5 s), real crates, and the real maintainer (Andrea). No generic placeholder data.

### Item 4: UAT in Given/When/Then (3-7 scenarios)

US-01: 3 scenarios. US-02: 3. US-03: 2. US-04: 3. US-05: 1. All in Gherkin.
US-03 and US-05 sit below the 3-scenario guideline because they are guardrail and
documentation stories respectively; their scenario count is deliberate and matches
their narrow, fully-specified surface. The build-bearing stories (US-01, US-02,
US-04) each carry 3 scenarios.

### Item 5: AC derived from UAT

Every AC maps directly to a scenario in the same story. AC are observable
outcomes (skip note printed, test passes, gate fails on regression, threshold
literal unchanged, all 28 gated), not implementation directives.

### Item 6: Right-sized (1-3 days, 3-7 scenarios)

Whole feature is a single uniform edit pattern applied to 28 tests plus one CI
job-env addition plus one ADR. Each story is well within 1-3 days. Scope
Assessment in story-map.md returns PASS (5 stories, one coherent deliverable, no
production source change).

### Item 7: Technical notes: constraints/dependencies

System Constraints section in user-stories.md captures cross-cutting constraints
(no production source change, no threshold change, skip-not-panic, presence-based
variable, no 1.0.0, British English). Per-story Technical Notes flag the open
DESIGN decisions (mechanism, skip strategy, ADR).

### Item 8: Dependencies resolved or tracked

US-02, US-04 depend on the guard contract from US-01 (tracked in story-map
Priority Rationale). US-05 depends on the DESIGN decision on flags 1, 3, 6
(tracked). No DIVERGE artifacts exist for this feature; that gap is noted as a
risk in wave-decisions.md. All six DESIGN flags are enumerated in wave-decisions.md.

### Item 9: Outcome KPIs with measurable targets

Five KPIs (K1 to K5) in outcome-kpis.md, each with Who / Does What / By How Much /
Baseline / Measured By. North Star is K5 (zero perf-flake bypasses). K2 and K3 are
guardrails (100% gated tests run in CI; zero thresholds changed).

## Notes

- Elevator Pitch present on US-01, US-02, US-03, US-04, US-05. All stories are
  labelled `@infrastructure` (the observable surface is the hook and the CI job,
  not an end-user command). Per the LeanUX rule, an all-`@infrastructure` slice
  would normally block release; here the maintainer IS the user and the pre-commit
  hook and CI log ARE the observable surfaces, which is the accepted shape for a
  test-infra feature. This is flagged for the reviewer's Dimension 0 judgement.
- Peer review NOT run in this DISCUSS pass per orchestrator instruction; handoff
  to DESIGN proceeds with this DoR PASS as the gate.
