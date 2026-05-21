# Peer Review: prism-backend-wiring-v0 (DESIGN)

The `@nw-solution-architect-reviewer` agent was not invokable in this
environment (no Agent dispatch tool available). Per the DESIGN process,
a structured self-review against `nw-sa-critique-dimensions` is recorded
as the fallback, with an explicit verdict.

```yaml
review_id: "arch_rev_prism_backend_wiring_v0_design"
reviewer: "solution-architect self-review (reviewer agent unavailable)"
artifact: "design/wave-decisions.md, design/application-architecture.md, adr-0043"
iteration: 1

strengths:
  - "DD1 picks the simplest honest mechanism (same-origin via ServeDir) and explicitly rejects shipping BOTH ServeDir and CorsLayer (no over-build)."
  - "ADR-0043 honours the immutable-ADR rule: refines ADR-0027 §5 prose rather than editing it in place; shipped buildUrl treated as authoritative."
  - "Reuse Analysis is concrete: tower-http already in Cargo.lock, oneshot test driver, existing Prism loader/queryRange, Vite default publicDir — every new element (ServeDir fallback, resolve_static_dir) is additive and default-off."
  - "Earned-Trust (principle 12) addressed: ServeDir is exercised by a real-filesystem RED test; the composition probe is preserved unchanged."
  - "DEVOPS handoff verified against actual CI line numbers (gate-5-mutants-query-api 1036-1123; Prism gates 6-11 1549-1732); no new gate invented."

issues_identified:
  architectural_bias:
    - issue: "Risk of resume-driven complexity (microservices, message bus, CORS framework)."
      severity: "n/a"
      assessment: "Not present. The design removes a mechanism (CORS) rather than adding one; one binary, one origin."
  decision_quality:
    - issue: "ADR-0043 alternatives count."
      severity: "low"
      assessment: "Four rejected alternatives recorded (CorsLayer cross-origin; both mechanisms; query-api /config.json route; build-time inject), each with for/against. Exceeds the 2-minimum."
  completeness_gaps:
    - issue: "Quality attributes coverage."
      severity: "n/a"
      assessment: "ISO 25010 table present in application-architecture.md; security (fail-closed tenancy, header redaction) and reliability (probe) explicitly preserved; no SLA gated at v0 by design (matches outcome-kpis guardrail framing)."
  implementation_feasibility:
    - issue: "tower-http fs feature availability."
      severity: "low"
      assessment: "tower-http 0.6.8 in Cargo.lock; ServeDir is behind the fs feature; adding the feature to query-api is a one-line Cargo.toml change. MIT licence, AGPL-compatible. Feasible."
    - issue: "Testability."
      severity: "n/a"
      assessment: "ServeDir route precedence testable via oneshot (no port bound); resolve_static_dir pure-unit testable; mutation-killable in the composition/router seam, keeping main.rs #[mutants::skip] honest."
  priority_validation:
    q1_largest_bottleneck:
      evidence: "DISCUSS pinned the topology fork as the single load-bearing decision; both blocking facts (no config.json, no CORS) addressed by the chosen mechanism."
      assessment: "YES"
    q2_simple_alternatives:
      assessment: "ADEQUATE — same-origin chosen as simpler than CORS; build-time inject and dedicated route rejected as more complex/duplicative."
    q3_constraint_prioritization:
      assessment: "CORRECT — slice 01 scope (no auth/TLS/multi-origin) respected; the additive change is default-off so the read-only path is unchanged."
    q4_data_justified:
      assessment: "JUSTIFIED — contract facts (buildUrl join, QUERY_RANGE_ROUTE, loader shape, tower-http in lock, Vite publicDir default) verified by reading the shipped source, not assumed."

approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

## Verdict

Approved (self-review fallback). No critical or high issues. The design is the
simplest honest option for slice 01, reconciles the ADR-0027 prose drift at
source, and reuses existing platform assets. Ready for DISTILL.
