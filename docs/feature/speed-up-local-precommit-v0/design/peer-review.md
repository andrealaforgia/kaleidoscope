# Peer Review — speed-up-local-precommit-v0 (DESIGN)

Reviewer: `nw-solution-architect-reviewer` not nested-invocable from the
DESIGN agent's toolset, so the **critique-dimensions skill
(`nw-sa-critique-dimensions`) is applied directly** as a structured
self-review against `adr-0072-fast-local-precommit-deep-tests-in-ci.md`, the
brief section `## Application Architecture — speed-up-local-precommit-v0`, and
`design/wave-decisions.md`.

```yaml
review_id: "arch_rev_20260607_precommit"
reviewer: "solution-architect-reviewer (critique-dimensions, direct)"
artifact: "adr-0072 + brief §speed-up-local-precommit-v0 + design/wave-decisions.md"
iteration: 1

strengths:
  - "D1 (--lib) rests on a DETERMINISTIC structural measurement taken this wave (165 integration bins, 26 fsync-bound), not a preference: --lib excludes 100% of the slow surface by construction (ADR-0072 Context + Decision 1)."
  - "Measurement honesty is disclosed, not faked: the DESIGN agent had no shell tool, so wall-clock seconds were NOT measured and NOT fabricated; the <=5min bar is a DELIVER-gated AC owned by Apex. Recorded in ADR-0072 Context, brief, and wave-decisions — three places."
  - "Four alternatives rejected with for/against (keep-slow / curated-deny-list / delete-tests / parallelism-only) + one deferred (faster-fsync), exceeding the 2-alternative minimum (ADR-0072 Alternatives)."
  - "Honesty trade (D5) named plainly and mirrored on ADR-0070's accepted precedent; conditioned explicitly on the cadence being real (D3 makes it concrete)."
  - "Routing is explicit and correct: both deliverables are shell scripts => Apex writes them in DELIVER, NOT the crafter (CLAUDE.md crate-source rule). Stated in brief DEVOPS handoff + wave-decisions Constraints."
  - "Earned Trust applied to the one external boundary: ci-watch.sh MUST degrade honestly when gh/network is unavailable (never a false green) — a probe responsibility flagged to DELIVER."

issues_identified:
  architectural_bias:
    technology_bias: NONE   # --lib and gh are existing primitives; --lib justified by measured structure, not 'best practice'
    resume_driven: NONE     # the feature REDUCES complexity (one cargo flag + one thin gh wrapper); no trendy tech
    latest_tech_bias: NONE  # cargo --lib and gh are mature
  decision_quality:
    - issue: "ADR-0072 carries Context (verified code refs), Decision (7), Alternatives (4 rejected + 1 deferred), Consequences (positive+negative), Reuse table — complete."
      severity: "none"
      location: "ADR-0072"
      recommendation: "No action."
  completeness_gaps:
    - issue: "Wall-clock seconds (fmt/clippy/deny/--lib + total) not measured in DESIGN."
      severity: "medium"
      location: "ADR-0072 Context (Measurement honesty); wave-decisions Measured numbers"
      recommendation: "ACCEPTED AS DESIGNED. The DESIGN harness has no shell-execution tool; fabricating seconds would violate test-don't-assume/Earned Trust. The decision rests on a DETERMINISTIC structural measurement (--lib excludes all 165 bins incl. 26 fsync), and the <=5min wall-clock is converted into a DELIVER-gated US-01 timing AC owned by Apex with a documented re-measure-and-trim fallback (D2). Disclosed in three artifacts. This is the honest treatment of a genuine tooling constraint, not a hidden gap."
    - issue: "Observability / safety-net for the moved-off coverage."
      severity: "none"
      location: "D3 / ci-watch.sh"
      recommendation: "Addressed: ci-watch.sh + cadence surface gate-1 and gate-5 reds; honest gh degradation specified."
  implementation_feasibility:
    - issue: "Feasibility: one cargo flag change + one thin gh wrapper + docs."
      severity: "none"
      location: "Reuse table"
      recommendation: "Trivially feasible; all reused primitives. Testability: structural assertions + behavioural negative controls specified for DISTILL."

priority_validation:
  q1_largest_bottleneck:
    evidence: "Step 4 cargo test --workspace --all-targets --locked = 10-20 min, dominated by 26 fsync-bound + subprocess bins (measured: 26 of 165 bins; hook :92-93). The wait is the verified pain (DISCUSS, wedged-for-hours incident)."
    assessment: "YES"
  q2_simple_alternatives:
    assessment: "ADEQUATE"   # 4 rejected + 1 deferred; --lib IS the simplest cut, chosen over a more complex curated deny-list
  q3_constraint_prioritization:
    assessment: "CORRECT"    # do-not-weaken-CI + do-not-delete-tests honoured as hard guardrails; the slim is a one-flag cut at the exact 10-20-min source
  q4_data_justified:
    assessment: "JUSTIFIED"  # justified on the DETERMINISTIC structural measurement (165/26) which is the load-bearing datum; wall-clock is a DELIVER-gated AC, honestly disclosed as not-DESIGN-measured rather than guessed

approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
medium_issues_count: 1   # the wall-clock gap — accepted-as-designed (DELIVER-gated AC + deterministic structural basis), non-blocking
low_issues_count: 0
```

## Verdict: APPROVED (iteration 1)

No critical or high issues. The one medium item (wall-clock not measured in
DESIGN) is **accepted as designed**: it is a genuine DESIGN-harness tooling
constraint, handled honestly (no fabricated seconds), the decision rests on a
deterministic structural measurement, and the <= 5 min claim is converted
into a DELIVER-gated US-01 timing AC owned by Apex with a documented
re-measure-and-trim fallback. nWave-order note honoured: no hook edit exists
at DESIGN (expected — the hook edit is Apex's DELIVER act), which is NOT a
rejection reason. D1-D6 resolved, MANDATORY Reuse table present, routing
explicit, honesty trade stated. Ready to hand to DISTILL (acceptance-designer)
and DEVOPS (platform-architect). Do NOT proceed into DEVOPS (per directive).
