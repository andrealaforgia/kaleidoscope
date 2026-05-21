# Peer Review: query-api-label-matchers-v0 (DISCUSS)

Reviewer: nw-product-owner-reviewer (review mode). Max 2 iterations.

```yaml
review_id: "req_rev_20260521_label_matchers"
reviewer: "product-owner (review mode)"
artifact: "docs/feature/query-api-label-matchers-v0/discuss/*"
iteration: 1

strengths:
  - "Dimension 0 PASS: every story (US-06/07/08) has a three-line Elevator Pitch with a real entry point (GET /api/v1/query_range?query=name{...}), a concrete output (the JSON matrix body or the 400 status:error body), and a named operator decision. The slice contains user-visible value stories (US-06, US-07), so it is not infrastructure-only."
  - "The correctness-critical absent-label and empty-string semantics for both = and != are pinned in wave-decisions.md, mirrored in a worked semantics matrix in the visual, and each arm has a dedicated UAT scenario. This is the regression-prone heart and it is covered."
  - "Verified against the actual code: selector.rs grammar, matrix.rs merge_labels precedence, queryRange.ts forwarding the raw {...}, and pulse metric.rs label sources. No assumption left unchecked."
  - "Scope boundary is executable: US-08 rejects regex and malformed matchers with an honest 400, and regex/__name__-form selection are briefed as deferred slices, not silently dropped."
  - "Shared-artifacts registry calls out the single highest integration risk (filter must compute the IDENTICAL derived label set as merge_labels) and makes it an integration checkpoint."

issues_identified:
  confirmation_bias:
    technology_bias:
      - issue: "Stories reference selector.rs / matrix.rs / merge_labels by name."
        severity: "low"
        location: "US-06/07 Technical Notes"
        recommendation: "Acceptable: these are existing, verified code seams named in Technical Notes (not AC), and the parser return type and filter placement are explicitly flagged as DESIGN decisions. AC remain observable-outcome. No change required."
    happy_path_bias:
      - issue: "Possible over-focus on the success (filter) path."
        severity: "low"
        location: "feature"
        recommendation: "Mitigated: US-08 is a full sad-path story (regex, unterminated brace, unquoted value, header-leak), and both US-06 and US-07 carry an explicit empty-arm scenario. Sad-path coverage is proportionate."

  completeness_gaps:
    - issue: "Whitespace inside the brace section (e.g. `{ service.name = \"checkout\" }`) is not given a dedicated scenario."
      severity: "low"
      location: "US-06"
      recommendation: "Minor. Slice-01 already trims surrounding whitespace; intra-brace whitespace tolerance is a parser detail DISTILL can pin. Noted as a red card for the acceptance designer, not a DISCUSS blocker."
    - issue: "A label name containing a dot (tenant.id) as a matcher key is mentioned in wave-decisions but not in a UAT scenario."
      severity: "low"
      location: "US-06/07"
      recommendation: "The prompt's `tenant.id!=\"x\"` example motivates this; the derived label set uses BTreeMap<String,String> keys that already include dotted keys (service.name). Covered implicitly by service.name examples. DISTILL may add a dotted-key edge scenario."

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

## Verdict

APPROVED, iteration 1. Zero critical, zero high. Three low-severity notes carried forward
as red cards for the acceptance designer (DISTILL), none blocking:

1. Intra-brace whitespace tolerance: a parser detail to pin in DISTILL.
2. Dotted label-name matcher key (`tenant.id`): add an explicit edge scenario in DISTILL.
3. (Already mitigated) sad-path balance is proportionate.

No revision required. The DISCUSS package is ready for handoff to DESIGN (solution-architect).
