# Peer Review: query-range-api-v0 (nw-product-owner-reviewer)

```yaml
review_id: "req_rev_20260521_001"
reviewer: "product-owner (review mode)"
artifact: "docs/feature/query-range-api-v0/discuss/user-stories.md (+ story-map, kpis, dor)"
iteration: 1

dimension_0_elevator_pitch:
  US-01: { presence: PASS, real_entry_point: "PASS - GET /api/v1/query_range", concrete_output: "PASS - matrix JSON body shown", job_connection: "PASS - read a trend, decide health" }
  US-02: { presence: PASS, real_entry_point: "PASS - GET /api/v1/query_range", concrete_output: "PASS - result:[] body shown", job_connection: "PASS - distinguish empty vs broken" }
  US-03: { presence: PASS, real_entry_point: "PASS - GET /api/v1/query_range", concrete_output: "PASS - HTTP 400 status:error body", job_connection: "PASS - correct the query" }
  US-04: { presence: PASS, real_entry_point: "PASS - GET /api/v1/query_range", concrete_output: "PASS - only acme-prod series / refusal", job_connection: "PASS - trust tenant isolation" }
  US-05: { presence: PASS, real_entry_point: "PASS - GET /api/v1/query_range", concrete_output: "PASS - HTTP 400 status:error body", job_connection: "WEAK - partly maintainer trust, but has operator-facing honest-refusal arm and visible 400 output; not infrastructure-only" }
  slice_level_check: "PASS - slice 01 contains user-visible value stories (US-01/02/03); not all @infrastructure"
  verdict: "NOT BLOCKED"

strengths:
  - "Response shape treated as a pinned external contract with the exact validator (isPromSuccess/isPromError) quoted; removes the largest ambiguity source."
  - "Scope honesty is explicit and executable: metrics-only, bare-name selector, no full PromQL, with US-05 turning the boundary into tests."
  - "Tenant fail-closed posture mirrors the existing gateway write path (KALEIDOSCOPE_DEFAULT_TENANT); behaviour pinned, mechanism correctly deferred as RED CARD 1."
  - "Sad paths are first-class: empty (US-02), parse-error (US-03), boundary half-open range, and header-redaction are all covered, not just the happy path."
  - "Unit hazard (seconds vs nanoseconds) surfaced with a boundary example and an AC, not left implicit."

issues_identified:
  confirmation_bias:
    technology_bias:
      - issue: "Stories name aegis/pulse/gateway env vars. Justified: these are existing platform components the feature must integrate with, not speculative tech choices. Service location explicitly left to DESIGN."
        severity: low
        location: "US-04 technical notes"
        recommendation: "Acceptable; the binding is to existing reality, and the one open tech choice (location) is deferred."
    happy_path_bias:
      - issue: "None material - empty, parse-error, tenant-refusal, and boundary scenarios are all present."
        severity: low
        location: "n/a"
        recommendation: "No action."

  completeness_gaps:
    - issue: "No scenario for malformed start/end (non-numeric or start > end), although the journey visual lists it as a failure mode."
      severity: high
      location: "US-01 / US-03"
      recommendation: "Add a scenario covering non-numeric or inverted start/end -> HTTP 400 status:error, so the time-parameter failure mode is testable, not just narrated."
    - issue: "No scenario for a Pulse persistence failure (MetricStoreError::PersistenceFailed -> HTTP 500), listed as a failure mode in the journey."
      severity: medium
      location: "US-01"
      recommendation: "Add a scenario: persistence failure maps to HTTP 5xx, which Prism renders as transport-error:http-status. Keeps the 5xx arm honest."

  clarity_issues:
    - issue: "US-05 Elevator Pitch decision leans maintainer-ward; could read as infrastructure."
      severity: low
      location: "US-05"
      recommendation: "Sharpen the operator-facing arm (honest refusal over partial answer). Not blocking."

  testability_concerns:
    - issue: "KPI 3 latency (500 ms p95) is measurable and CI-anchored; KPIs 1/2/4 are observable. No non-testable AC found."
      severity: low
      location: "outcome-kpis.md"
      recommendation: "No action."

  priority_validation:
    q1_largest_bottleneck: "YES - the read loop is the platform's single missing half; query_range is the pinned contract Prism waits on"
    q2_simple_alternatives: "ADEQUATE - bare-name-only selector and raw-points (no resample) are documented minimal choices; full PromQL explicitly rejected"
    q3_constraint_prioritization: "CORRECT - tenant isolation and contract shape (the two HIGH-risk artifacts) drive the skeleton"
    q4_data_justified: "JUSTIFIED - the contract is read directly from Prism's source; latency budget anchored to ubuntu-latest"
    verdict: PASS

approval_status: "conditionally_approved"
critical_issues_count: 0
high_issues_count: 1
```

## Required revisions (iteration 1)
1. (HIGH) Add a malformed/inverted start/end scenario returning HTTP 400 status:error.
2. (MEDIUM) Add a Pulse persistence-failure -> HTTP 5xx scenario.

Both are additive and do not change scope. Apply, then re-validate DoR for the touched
stories (US-01, US-03). No second full review needed for additive sad-path scenarios.

## Iteration 1 resolution

- HIGH (malformed/inverted start/end): RESOLVED. Added "Malformed or inverted time
  bounds are rejected" scenario + 1 AC to US-01.
- MEDIUM (persistence failure -> 5xx): RESOLVED. Added "A persistence failure surfaces
  as a server error" scenario + 1 AC to US-01.

US-01 now has 6 UAT scenarios (within 3-7); DoR for US-01 re-checked, still PASSED
(right-sized at 6 scenarios, ~1.5-2 days). No scope change.

### Final verdict: APPROVED
critical_issues_count: 0 | high_issues_count: 0 (resolved) | DoR: 5/5 PASSED.
Cleared for DESIGN handoff.
