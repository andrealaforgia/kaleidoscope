# Shared Artifacts Registry: query-api-label-matchers-v0

British English. No em dashes. Every value that flows across the journey steps, its single
source of truth, and its consumers. Untracked artifacts are the primary cause of horizontal
integration failures.

## Registry

```yaml
shared_artifacts:

  raw_query_string:
    source_of_truth: "Prism buildUrl: URLSearchParams({ query: request.q, ... }) (apps/prism/src/lib/promql/queryRange.ts)"
    consumers:
      - "selector parser: splits into metric name + matcher list (crates/query-api/src/selector.rs)"
      - "pulse.query: uses the METRIC NAME only to select the metric"
      - "filter predicate: uses the MATCHER LIST to filter returned rows"
    owner: "Prism (producer of the raw query); query-api (parser/owner of all parsing)"
    integration_risk: "HIGH - the {...} section is forwarded verbatim and URL-encoded; the backend owns 100% of matcher parsing. A client-side assumption that Prism pre-parses would be wrong (confirmed it does not)."
    validation: "Confirmed in queryRange.ts buildUrl that query=request.q is passed raw into URLSearchParams. The backend must handle URL-decoded { } = ! \" , characters."

  derived_label_set:
    source_of_truth: "matrix.rs merge_labels: metric.resource_attributes U point.attributes U {__name__: metric.name}, point wins on clash, __name__ authoritative (crates/query-api/src/matrix.rs)"
    consumers:
      - "matcher filter predicate (must compute the IDENTICAL set merge_labels uses)"
      - "to_matrix grouping (groups kept rows by this set)"
      - "Prism legend (renders metric:{...} from the grouped set)"
    owner: "query-api matrix module"
    integration_risk: "HIGH - if the filter computes the label set differently from merge_labels (e.g. forgets __name__, or reverses the resource/point precedence), it filters on a set that does not match what Prism shows in the legend, producing a kept series whose displayed labels contradict the matcher."
    validation: "Filter and to_matrix must derive the label set by the same logic. A test asserts that a matcher on a point attribute, a resource attribute, and __name__ each filter on the precedence-correct value (point wins over resource)."

  matcher_semantics:
    source_of_truth: "wave-decisions.md 'Matcher semantics pinned' + the semantics matrix in journey-label-filter-visual.md"
    consumers:
      - "filter predicate (= present/equal; =\"\" matches absent; != absent-or-different; !=\"\" present-non-empty; ANDed)"
      - "UAT scenarios in US-06 and US-07"
      - "acceptance suite (DISTILL)"
    owner: "this feature (DISCUSS)"
    integration_risk: "HIGH - the absent-label and empty-string arms are the subtle, regression-prone heart of the feature. A wrong arm silently keeps or drops the wrong series."
    validation: "Each arm of the semantics matrix has a dedicated UAT scenario; mutation testing (gate-5-mutants-query-api) targets exactly these boundary decisions."

  response_envelope:
    source_of_truth: "PINNED contract: isPromSuccess / isPromError (apps/prism/src/lib/promql/queryRange.ts + ADR-0027)"
    consumers:
      - "Prism client validator (success/empty/error arms)"
      - "Prism QueryPanel renderer"
      - "query-api success_response / error_response (crates/query-api/src/lib.rs)"
    owner: "Prism (consumer-driven contract); query-api (provider)"
    integration_risk: "MEDIUM - the matcher feature must NOT change the envelope; it changes only WHICH series the success arm carries. A regression here breaks the pinned contract for ALL query forms, not just labelled ones."
    validation: "Contract test asserts success/empty/error shapes still pass isPromSuccess/isPromError after the matcher change."

  error_reason_text:
    source_of_truth: "selector.rs parse() Err(String) (crates/query-api/src/selector.rs)"
    consumers:
      - "error_response body {status:error, error:<reason>} (crates/query-api/src/lib.rs)"
      - "Prism isPromError validator + error banner"
    owner: "query-api selector module"
    integration_risk: "HIGH - the reason must NEVER echo the raw query or a forwarded header value (DD6 redaction symmetry). A regex matcher value pasted into the query, or a forwarded credential, must not be reflected back."
    validation: "The existing test the_reason_never_echoes_the_raw_query in selector.rs is extended to the matcher reject paths; a redaction test asserts a forwarded Bearer token never appears."
```

## Consistency check

1. `raw_query_string`: source verified in `queryRange.ts buildUrl`. Backend owns parsing.
   PASS.
2. `derived_label_set`: filter MUST reuse `merge_labels` logic. This is the single highest
   integration risk and is called out in the journey integration_validation block. PASS
   with a dedicated precedence test required (flagged to DISTILL).
3. `matcher_semantics`: single source in `wave-decisions.md`, mirrored in the visual
   semantics matrix; every arm has a UAT scenario. PASS.
4. `response_envelope`: unchanged by this feature; contract test guards regression. PASS.
5. `error_reason_text`: redaction discipline inherited and extended. PASS.

No untracked `${variable}` remains in the journey mockups: `${query}`, `${start_seconds}`,
`${end_seconds}`, the `metric:{...}` label set, and the `{status,error}` envelope all
appear above with a documented source.

## Quality gates

- Journey completeness: all steps have goals, contract actions, emotional annotations,
  tracked artifacts, integration checkpoints. PASS.
- Emotional coherence: Problem Relief arc (frustrated -> hopeful -> relieved); error and
  empty arms guide to resolution rather than adding frustration. PASS.
- Horizontal integration: every shared artifact has a single source of truth and documented
  consumers; the derived-label-set consistency is the explicit integration checkpoint. PASS.
- Contract UX compliance: response envelope unchanged; error answers what/why/what-next;
  redaction held. PASS.
