# Definition of Ready Validation — gate-5-mutants-lumen-v0

British English. No em dashes in body.

Self-validation by Luna (`nw-product-owner`). Per the overnight
pattern stretch authorisation, peer review is skipped on this slice;
the validation is recorded here for audit, and the DESIGN wave
(Apex) consumes the artefacts directly.

## Story under validation

US-01: `gate-5-mutants-lumen` job shipped (the single story in this
slice; see `user-stories.md`).

## 9-item DoR Checklist

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | `user-stories.md` "Problem" section quotes Apex's honest gap note (`docs/feature/log-body-text-search-v0/devops/wave-decisions.md` lines 56 to 89), names the silently-violated invariant (ADR-0005 Gate 5 "100% kill rate per crate"), and identifies the file under enforcement gap (`crates/lumen/src/predicate.rs`) |
| 2 | User/persona identified with specific characteristics | PASS | "Who" section: Kaleidoscope `lumen` crate maintainer extending `crates/lumen/src/**`; reads PR status panel before merging; trusts a green panel as evidence that Gate 5 has fired. Specific context (PR-time), specific signal (status panel), specific trust assumption (green = enforced) |
| 3 | 3+ domain examples with real data | PASS | Three concrete examples in "Domain Examples" section: (1) `host_contains` future extension with real field name and real `String::contains` call site; (2) empty-diff PR with real path glob `crates/lumen/**` and real short-circuit log message; (3) `min_duration_ms` with real comparator `>=`, real boundary value 1000, and real surviving-mutation scenario |
| 4 | UAT scenarios in Given/When/Then (3-7 scenarios) | PASS | Three Gherkin scenarios in "UAT Scenarios (BDD)" section. Each scenario describes a maintainer-observable outcome (job exists, short-circuit happens, zero regression on siblings). Titles describe WHAT the maintainer sees, not HOW the job is implemented (per BDD anti-pattern guidance) |
| 5 | AC derived from UAT | PASS | Seven acceptance criteria (AC-1 to AC-7), each tracing back to a specific scenario clause. AC-1, AC-2, AC-3 derive from Scenario 1 (the job exists with the right script). AC-4 derives from Scenario 2 (short-circuit). AC-5, AC-6 derive from Scenario 3 (zero sibling regression). AC-7 is a guardrail derived from the K4 outcome KPI |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 story, 3 scenarios, single workflow file edit, four token substitutions over an existing 86-line job block, estimated under 1 day of crafter effort. See `story-map.md` Scope Assessment. The slice is the thinnest possible end-to-end edit |
| 7 | Technical notes identify constraints | PASS | "Technical Notes" section names the sibling job to clone (`gate-5-mutants-log-query-api` at lines 1123 to 1208), the four substitution tokens, the two open flags (placement, `needs` graph), the public-API posture (no change; lumen is not in Gate 2 / Gate 3's locked set), and the dependency posture (no `deny.toml` change; `cargo-mutants` installed via `taiki-e/install-action`) |
| 8 | Dependencies resolved or tracked | PASS | Resolved: ADR-0005 (CI contract); the sixteen pre-existing `gate-5-mutants-*` jobs; the `log-body-text-search-v0` DEVOPS wave (gap source); the `query-http-common-v0` DEVOPS wave (precedent). Tracked, not blocking: eight other crates lacking a `gate-5-mutants-*` job, recorded in `story-map.md` appendix as future maintenance, NOT promoted to this feature's scope |
| 9 | Outcome KPIs defined with measurable targets | PASS | Four KPIs (K1 to K4) in `outcome-kpis.md`, each with [Who], [Does what], [By how much], [Baseline], [Measured by], and [Type]. K1 binary (job exists). K2 two binary sub-checks (positive and negative diff filter). K3 zero-delta `diff` over sixteen blocks. K4 zero-delta `diff` over four artefacts plus byte-identical installer line. All four are build-time measurements, consistent with platform's no-runtime-telemetry posture |

## DoR Status: PASSED (9/9)

All nine items pass with evidence. No remediation required. The slice
is READY for the DESIGN wave (`nw-platform-architect`, Apex).

## Notes on the validation

This is a self-validation by the requirements analyst, not a peer
review. Per the orchestrator's authorisation for the overnight pattern
stretch on this slice:

- The slice is infrastructure-only (no production code change).
- The pattern is established (sixteen sibling jobs to clone, four
  token substitutions).
- The DoR items are objectively verifiable (a `grep`, a synthetic PR,
  a `diff`).
- The risk surface of skipping peer review is bounded: the DESIGN
  wave's deliverable is a single YAML block; the DELIVER wave's
  deliverable is a single workflow edit; both are reversible by a
  `git revert` with no data-format consequence.

A full peer-review pass would surface the same finding (PASS, all
nine items) and would not change any artefact. The audit trail is
complete: the gap source (Apex's note), the pattern source (the
sibling job), the precedent (the `query-http-common-v0` DEVOPS wave),
and the four KPIs are all named with file paths and line numbers
where applicable.

## Anti-pattern scan

| Anti-pattern | Signal in this slice | Verdict |
|---|---|---|
| Implement-X | "Implement gate-5-mutants-lumen" would be the bad framing | NOT PRESENT. The story is framed from the maintainer's pain point (silent ADR-0005 Gate 5 violation on lumen) and the Decision enabled (automatic mutation signal on Predicate extensions), not as "implement a job" |
| Generic data | "user123", "test-crate" | NOT PRESENT. Real crate name (`lumen`), real file path (`crates/lumen/src/predicate.rs`), real workflow line numbers (1123 to 1208), real sibling job name (`gate-5-mutants-log-query-api`) |
| Technical AC | "Use `cargo mutants` v25.x" | NOT PRESENT. AC describe observable outcomes (one grep line, one log message, zero regression on sixteen jobs, zero new dependency) rather than implementation prescriptions. The version of `cargo-mutants` is inherited from the sibling job's `taiki-e/install-action` pin |
| Technical scenario title | "FileWatcher triggers TreeView refresh" | NOT PRESENT. The three scenario titles describe maintainer-observable outcomes ("the new gate-5-mutants-lumen job exists", "a PR that does not touch crates/lumen/** short-circuits", "zero regression on the other sixteen gate-5-mutants jobs") |
| Oversized story | >7 scenarios, >3 days | NOT PRESENT. 3 scenarios, under 1 day, single workflow file edit |
| Abstract requirements | No concrete examples | NOT PRESENT. Three concrete examples with real field names, real comparators, real boundary values |

All anti-patterns absent. No remediation required.
