# Wave Decisions: durable-stores-integration-v0

## DISCUSS wave configuration (decided, not asked)

| Decision | Choice | Consequence |
|----------|--------|-------------|
| Feature type | Backend / quality | No CLI surface, no GUI. Value exercised through the integration-suite crate. |
| Walking skeleton | No (brownfield) | `integration-suite` crate already exists with a working first-triad precedent. We extend it, we do not bootstrap it. |
| UX research depth | Lightweight | No emotional-arc theatre. One operator persona, one trust outcome, terse journey. |
| JTBD | Skipped | Straight to journey + story map + requirements per configuration. No DIVERGE artifacts present; noted as risk below. |

## Context grounding

The storage plane reached a milestone: all six storage pillars own durable v1
adapters (lumen, pulse, ray, strata, cinder, sluice), each WAL + snapshot +
replay behind its v0 trait. The first triad (cinder + sluice + lumen, all
FileBacked) is already proven to compose under one shared `aegis::TenantId` and
survive a drop-and-reopen, in
`crates/integration-suite/tests/v1_three_adapters_compose_under_restart.rs`.

This feature adds the SECOND triad: prove that `pulse::FileBackedMetricStore` +
`ray::FileBackedTraceStore` + `strata::FileBackedProfileStore` compose under one
tenant and recover identically across a restart, with the cross-crate
`TenantId` identity contract holding across all three.

This is the natural completion of a shipped milestone, not speculative work.
The three durable stores are real shipped code with their own crate-level v1
acceptance suites; nothing yet proves they COMPOSE under a shared tenant on the
durable path the way the first triad does.

## Risks surfaced

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| No DIVERGE artifacts (recommendation.md / job-analysis.md absent) | High (known) | Low | Feature is the deterministic completion of a shipped milestone with a direct in-repo precedent; the design space is fixed by the existing first-triad file. No divergence needed. |
| Timing budget set against fast workstation, not CI | Medium | High (project lesson, 2026-05-19 timing bump) | All KPIs in `outcome-kpis.md` are budgeted against GitHub Actions ubuntu-latest with generous headroom; recovery budget is a guardrail, not the north star. |
| Inventing a fake user surface to manufacture value | Medium | Medium | Honest framing: the operator-facing trust outcome is real (restart survival of metrics+traces+profiles under one tenant); the entry point is the `cargo test` command. No CLI / ingest path invented. |
| Cross-crate `TenantId` shape drift breaking the contract silently | Low | High | Dedicated identity-contract test compiles the same `&TenantId` through all three adapters; drift breaks the build, alerting the maintainer. |

## Reviewer gate

Peer review via `nw-product-owner-reviewer` is required before DISCUSS is
declared done. Verdict and any revisions are recorded at the foot of this file.

## Reviewer verdict

```yaml
review_id: "req_rev_20260521_durable_stores_integration_v0"
reviewer: "product-owner (review mode, nw-po-review-dimensions)"
artifact: "docs/feature/durable-stores-integration-v0/discuss/user-stories.md"
iteration: 1

dimension_0_elevator_pitch:
  us_01: "PASS — Before/After/Decision present; real runnable target; observable stdout; real decision."
  us_02: "PASS (N/A) — honestly @infrastructure, no pitch claimed; slice-level value held by US-01."
  blocking: false

strengths:
  - "US-01 entry point is an honest, runnable test target — no fabricated CLI surface."
  - "Sad-path coverage strong: cross-pillar leakage and partial-recovery are explicit scenarios."
  - "All API anchors verified against the actual crate source (open/ingest/query signatures)."
  - "CI-realistic guardrail honours the 2026-05-19 timing-bump lesson."

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

Verdict: APPROVED, iteration 1. No critical or high issues. DISCUSS wave
complete; ready for DESIGN handoff to solution-architect.
</content>
</invoke>
