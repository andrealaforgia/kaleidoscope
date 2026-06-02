# Definition of Ready Validation — wal-torn-tail-recovery-v0

British English. No em dashes in body.

## Story: US-01 Crashed-then-restarted store recovers its intact acked prefix and warns about the dropped torn tail

| # | DoR Item | Status | Evidence / Issue |
|---|----------|--------|------------------|
| 1 | Problem statement clear, domain language | PASS | `user-stories.md` Problem section states the operator pain in storage-domain language: a torn final WAL line (the expected post-crash shape) blocks recovery of the intact acked prefix; cites the four exact replay-loop loci. No technology prescription. |
| 2 | User/persona with specific characteristics | PASS | `user-stories.md` Who section: the on-call SRE restarting `log-query-api` (lumen), `trace-query-api` (ray), or `kaleidoscope-gateway` (cinder/pulse) after an abrupt process death; reads `journalctl`/`docker logs`/`kubectl logs` and HTTP responses to judge recovery. Specific context and motivation, not "a user". |
| 3 | 3+ domain examples with real data | PASS | Three concrete examples with real tenant names and real torn-line bytes: (1) lumen recovers 10,000 acked records for `acme-corp` after a torn 10,001st; (2) ray recovers snapshot-only state for `globex` with a single torn WAL line; (3) cinder REFUSES a mid-file corruption for `initech`. Each names the pillar, the data, the action, and the outcome. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | Five Gherkin scenarios in `user-stories.md`: intact-prefix recovery; structured warning; snapshot-plus-torn-tail; mid-file stays fail-closed; newline-terminated-malformed stays fail-closed. Three positive, two negative. Titles describe operator-observable outcomes, not implementation. |
| 5 | AC derived from UAT | PASS | AC-1 through AC-10 trace to the scenarios and to the System Constraints: AC-1/AC-2/AC-4 from the positive scenarios, AC-5/AC-6 from the negative scenarios, AC-3 from the warning scenario, AC-7 (cinder doc), AC-8 (no trait change), AC-9 (scope), AC-10 (mutation kill). Each is observable and testable. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | One slice, five UAT scenarios, one operator outcome. Confined edit to one arm of a near-identical replay loop across three (conditionally four) pillars plus a doc fix and acceptance tests. Scope Assessment in `story-map.md` rates PASS on every Elephant Carpaccio dimension. Estimated one to three days. |
| 7 | Technical notes: constraints/dependencies | PASS | `user-stories.md` Technical Notes and System Constraints: confined to the parse-failure arm and cinder docs; no trait/write-path/snapshot change; detection mechanism is a DESIGN decision (FLAG 2); warning rides the existing `tracing` `event=` convention; ADR-0059 authored in DESIGN; Rust-idiomatic shared-vs-replicated decision (FLAG 4). |
| 8 | Dependencies resolved or tracked | PASS | `user-stories.md` Dependencies: resolved (ADR-0040, ADR-0049, the four replay loops, the existing `tracing` subscribers, each pillar's `gate-5-mutants-*` job). Tracked-not-blocking (pulse scope, FLAG 1; no external-integration dependency). |
| 9 | Outcome KPIs defined with measurable targets | PASS | `outcome-kpis.md`: K1 (intact-prefix recovery 0% to 100%, north star), K2 (torn-tail refusals to 0), K3 (warning on 100% of drops), K4 (mid-file/malformed stay fail-closed, guardrail), K5 (no false doc claim, guardrail). Each has Who/Does-what/By-how-much/Baseline/Measured-by. |

## DoR Status: PASSED

All nine items pass with evidence. No item blocked. Story is ready for the DESIGN wave subject to peer review approval (below).

---

## Peer Review Record

Reviewer: nw-product-owner-reviewer (review mode). Iteration 1.

### Dimension 0: Elevator Pitch Test (BLOCKING, checked first)

- Presence: PASS. `### Elevator Pitch` present with Before / After / Decision-enabled.
- Real entry point: PASS. The "After" line references the operator restart of the `log-query-api` binary (`FileBackedLogStore::open(pillar_root, ...)`) and the user-invocable `GET /api/v1/logs` read-API path, not an internal function or test runner.
- Concrete output: PASS. The "sees" clauses describe observable output: the store starts and binds its listener, the read API response body returns the acked records, and a structured WARN line appears on stderr. Not "tests pass" or internal state.
- Job connection: PASS. The "Decision enabled" line names a real operator decision: resume serving traffic vs escalate an incident / hand-edit the WAL / restore from backup, with the warning letting them confirm exactly one torn tail was dropped.
- Slice-level check: PASS. The single story is tagged `@user-visible`, not `@infrastructure`; the slice has clear release value.

Verdict for Dimension 0: no BLOCKING issue.

### Dimension 1: Confirmation Bias Detection

- Technology bias: none. Requirements are solution-neutral; the detection mechanism, warning field spellings, shared-vs-replicated factoring, and ADR-0059 are explicitly deferred to DESIGN as flags. No database/cloud/framework prescribed.
- Happy path bias: NOT PRESENT, and notably well-guarded. Two of five scenarios (AC-5, AC-6) are negative paths asserting fail-closed behaviour, and K4 elevates them to a co-equal guardrail. The story resists the most common requirements failure mode for this exact class of change.
- Availability bias: none. The Earned-Trust lineage (ADR-0040/0049/0050) is cited as a justified pattern fit, not a copied-from-last-project assumption; the four loci were verified by reading the code, not assumed.

### Dimension 2: Completeness Validation

- Stakeholder perspectives: operator (primary) covered; the platform-as-guardrail (K4) and the project-thesis-as-guardrail (K5) are represented; DEVOPS handoff present in `outcome-kpis.md`.
- Error scenarios: STRONG. The two negative scenarios plus the snapshot-only edge case plus the risk table (multiple torn lines, mis-classification, pulse interaction) cover the failure space well.
- NFRs: durability/recovery is the feature's whole subject and is quantified (K1 100%, K2 to 0). Mutation-kill NFR (AC-10) and no-trait-change NFR (AC-8) are present and measurable.

### Dimension 3: Clarity and Measurability

- No vague performance adjectives. "Intact acked prefix", "torn final line (last line, no trailing newline)", "byte-equivalent" are precise. Targets are numeric (100%, 0, exactly one WARN).
- Ambiguity: the "torn tail" definition is pinned to two checkable conditions (last line AND no trailing newline), removing interpretation latitude. Two architects would design the same observable behaviour.

### Dimension 4: Testability

- Every AC is observable and automatable: open-succeeds-and-query-returns-prefix (AC-1), absence of the torn record (AC-2), structured WARN fields (AC-3), `PersistenceFailed` with line number and no warning (AC-5/AC-6), doc-versus-behaviour read (AC-7), `cargo public-api` byte identity (AC-8), `cargo mutants` 100% kill (AC-10). No "easy to use" style criteria.

### Dimension 5: Priority Validation

- Q1 largest bottleneck: YES. This is the triaged verifier issue 006 (the durability-promise gap); the intact-prefix path is the verifier D04 expectation. The `story-map.md` Priority Rationale builds AC-1 first as the riskiest, highest-value assumption.
- Q2 simpler alternatives: ADEQUATE. The decision was triaged externally as "option 1" against the alternative of leaving fail-closed; the narrowness of the tolerance (negatives co-equal) is the considered guard against over-tolerating. `wave-decisions.md` records the decision lineage and five DESIGN flags.
- Q3 constraint prioritisation: CORRECT. The narrow-tolerance constraint (K4) is elevated to co-equal with recovery (K1), so the safety constraint does not get dominated by the convenience outcome.
- Q4 data-justified: JUSTIFIED. The four replay loci, the false cinder doc, and the `tracing` convention were verified by reading the codebase (recorded in `wave-decisions.md` Verified facts), not assumed.

Verdict: PASS.

### Review Verdict

```yaml
review_id: "req_rev_wal_torn_tail_recovery_v0_iter1"
reviewer: "nw-product-owner (review mode)"
artifact: "docs/feature/wal-torn-tail-recovery-v0/discuss/user-stories.md"
iteration: 1
strengths:
  - "Negative criteria (AC-5, AC-6) are co-equal with the positive path and elevated to a guardrail KPI (K4); the story resists happy-path bias on a change where over-tolerating corruption is the real danger."
  - "Elevator Pitch names a genuine operator-invocable entry point (binary restart + GET /api/v1/logs) and observable output, and is the verifier D04 path."
  - "Every claim about the code (four replay loci, false cinder doc, tracing convention) was verified by reading the source, recorded in wave-decisions.md."
  - "Solution-neutral: detection mechanism, field spellings, factoring, and ADR-0059 deferred to DESIGN as explicit flags."
issues_identified:
  confirmation_bias: []
  completeness_gaps: []
  clarity_issues: []
  testability_concerns: []
  priority_validation:
    q1_largest_bottleneck: "YES"
    q2_simple_alternatives: "ADEQUATE"
    q3_constraint_prioritization: "CORRECT"
    q4_data_justified: "JUSTIFIED"
    verdict: "PASS"
approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

### Review Outcome

APPROVED, iteration 1. Zero critical, zero high issues. No remediation required. DISCUSS may complete and hand off to DESIGN. Within the autonomous-run mandate, the peer-review gate is satisfied without a second iteration.
