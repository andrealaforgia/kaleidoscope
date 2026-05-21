# Wave Decisions: query-api-label-matchers-v0 (DESIGN)

British English. No em dashes. Author: `nw-solution-architect` (Morgan).
Scope: application. Interaction mode: propose. DISCUSS pinned at commit 5feadeb.

## Configuration

| Decision | Value | Note |
|----------|-------|------|
| Architecture style | Unchanged from ADR-0042 | Hexagonal, `MetricStore` the only driven port, parser + translation the only mutable logic. This slice thickens the parse + filter rib of an already-shipped read loop. |
| New crate | No | Extends `crates/query-api/src/` only. |
| New dependency | No | Hand-rolled matcher parsing; NO regex crate (regex matchers are out of scope). |
| New HTTP surface | No | Same `/api/v1/query_range` route, same response envelope. |
| Probe change | No | The new logic is pure and in-process; ADR-0042 Decision 8 probe is untouched. |
| ADR verdict | New ADR-0044, refining ADR-0042 Decision 3 | ADRs are immutable; a refining ADR with a back-reference preserves immutability. 0044 is the next free number (highest was 0043). |

## Design Decisions

### DD1: The selector grammar and parse types (`crates/query-api/src/selector.rs`)

`parse` changes from `Result<MetricName, String>` to `Result<Selector, String>`,
accepting `metric_name [ "{" matcher_list "}" ]`.

- Grammar: `matcher := label_name op string_literal`; `op` in `{=, !=}` ONLY;
  `string_literal` double-quoted with minimal escapes `\"`, `\\`, `\n`, `\t`;
  `label_name` is `[a-zA-Z_:][a-zA-Z0-9_:.]*` (dots ALLOWED, see below);
  `metric_name` is the unchanged `[a-zA-Z_:][a-zA-Z0-9_:]*`. Whitespace tolerated
  around braces, ops, and commas; a single trailing comma tolerated; an empty
  value `""` valid; the bare-name and `name{}` forms valid (empty matcher list).
- **Dotted-label-name decision**: label names in matchers MAY contain dots, a
  deliberate divergence from strict PromQL (`[a-zA-Z_][a-zA-Z0-9_]*`). The derived
  label set is OTel-shaped (`service.name`, `tenant.id`); the headline filter is
  `{service.name="checkout"}`; a dot-free grammar could not name the keys it must
  filter. Forward-compatible (strict PromQL names are a subset). Documented in
  ADR-0044 Decision 2.
- Types:
  `Selector { name: MetricName, matchers: Vec<LabelMatcher> }`,
  `LabelMatcher { name: String, op: MatchOp, value: String }`,
  `MatchOp { Equal, NotEqual }`.
- Reject arms (all HTTP 400 status:error, DD6 redaction, never echo the raw query):
  regex `=~`/`!~` ("regex matchers ... not supported at v0; use = or !="),
  unterminated `{`, unquoted value, empty label name, bad escape, trailing junk.
  A malformed brace is NEVER degraded to a bare-name query or partial filter.

### DD2: The series filter (`crates/query-api/src/matrix.rs`)

Two new pure helpers: `matches(labels, matcher) -> bool` and
`keep_row(metric, point, matchers) -> bool`. `keep_row` derives the label set
with the SAME logic `merge_labels` uses (lifted to a shared private helper so the
derivation is not duplicated), so the predicate sees exactly what `to_matrix`
groups on. The orchestration applies `rows.retain(|(m,p)| keep_row(m,p,&matchers))`
in `lib.rs` BEFORE `to_matrix`. Empty matchers retain everything. An empty
surviving set produces `result: []` (HTTP 200 calm success arm, never an error).

**Matcher semantics (restated, the correctness-critical part)**, treating an
absent label as the empty string:

- `label="value"` (non-empty): keep iff present and equal.
- `label=""`: keep iff absent OR present-and-empty.
- `label!="value"` (non-empty): keep iff absent OR present-and-different.
- `label!=""`: keep iff present and non-empty.
- Multiple matchers ANDed: keep iff all match.

`Equal` keeps iff `get(name).unwrap_or("") == value`; `NotEqual` keeps iff
`get(name).unwrap_or("") != value`. This single absent-as-empty rule yields all
four arms. The four arms are the primary mutation targets.

### DD3: Orchestration wiring (`crates/query-api/src/lib.rs`)

`handle_query_range` calls `selector::parse` (now yielding `Selector`), passes
`selector.name` to `store.query` (unchanged port call), applies the matcher
`retain` to the returned rows, then `to_matrix`. Tenancy, bounds parsing,
response/error serialisation, and the probe are untouched. Order:
resolve-tenant -> parse-bounds -> parse-selector -> query -> filter -> translate.

## Reuse Analysis (MANDATORY)

| Asset | Reused / Extended / New | Detail |
|-------|------------------------|--------|
| `crates/query-api/src/selector.rs` | Extended | Bare-name production kept; matcher grammar + types added; signature widens to `Result<Selector, String>`. |
| `crates/query-api/src/matrix.rs` | Extended | `merge_labels` derivation reused by the new pure filter helpers; `to_matrix` grouping unchanged. |
| `crates/query-api/src/lib.rs` | Extended | One `retain` filter step inserted between query and translate; everything else unchanged. |
| `pulse::MetricStore::query` | Reused unchanged | Name-only selection; matchers filter the result, not the query. |
| `crates/query-api/src/composition.rs` (probe, tenancy) | Reused unchanged | No probe or composition-root change. |
| Prism contract / response envelope | Reused unchanged | Only WHICH series the success arm carries changes. |
| Regex crate | NOT added | Regex matchers are out of scope; hand-rolled parsing; ZERO new dependency. |
| New crate | None | All change in `crates/query-api/src/`. |

No existing code provides label-matcher parsing or series filtering, so the
extension is justified; there is no alternative to reuse. The change is the
smallest honest delta: two files extended, one wired, zero new crates, zero new
dependencies.

## DEVOPS handoff (to `@nw-platform-architect`, Apex)

- **No new gate.** `gate-5-mutants-query-api` (ADR-0005 Gate 5; CLAUDE.md
  mutation strategy) already covers `crates/query-api/src/` via `--in-diff`. The
  new parse + filter logic lands in that scope and is covered without a new gate.
  The four matcher boundary arms (present/absent x `=`/`!=` and the empty-string
  cases) are the primary mutation targets.
- **Zero new dependencies.** No regex crate, no new web framework, no new crate.
  Nothing to add to the supply-chain or licence audit.
- **No new external integration.** The Prism consumer-driven contract boundary
  (ADR-0042 handoff) is unchanged; the envelope does not change, so the existing
  contract posture (Apex's choice of Pact-JS or container fixtures) covers it.
- **Instrumentation (carried from outcome-kpis.md)**: per-query duration (existing
  `record_query` seam) now spans parse + filter; add matcher-count and
  kept/total series ratio per query, and reject-form counts (regex vs malformed).
- **Alerting**: KPI 5 p95 > 500 ms on ubuntu-latest; any contract-shape
  regression (KPI 4) and any cross-tenant leak (inherited US-04 guardrail) remain
  hard alerts.
- **No new probe.** The ADR-0042 Decision 8 startup probe and its
  three-orthogonal-layer enforcement are unchanged; the new logic is pure and
  in-process with no external substrate to probe.

## Development paradigm (handoff to crafter)

Rust idiomatic per CLAUDE.md: data (`Selector`, `LabelMatcher`, `MatchOp`) plus
free functions (`parse`, `matches`, `keep_row`). No inheritance, no `dyn` where
generics suffice. The crafter owns the internal parser structure (hand-rolled
char scan vs a small state machine) and the GREEN/REFACTOR shape; this design
fixes only the grammar, the types, the semantics, and where the filter applies.

## Peer review

Reviewer: `@nw-solution-architect-reviewer` requested; not invocable in this
session, so a structured self-review against the five critique dimensions was
run as the recorded fallback. Verdict: APPROVED, iteration 1.

```yaml
review_id: "arch_rev_query-api-label-matchers-v0_design"
reviewer: "structured-self-review (solution-architect-reviewer unavailable)"
artifact: "design/wave-decisions.md, design/application-architecture.md, adr-0044"
iteration: 1
strengths:
  - "Smallest honest delta: two files extended, zero new crates, zero new deps (DD1-DD3, Reuse Analysis)."
  - "Absent-label semantics documented as an explicit four-arm oracle and collapsed to one absent-as-empty rule (ADR-0044 Decision 3, DD2)."
  - "Dotted-label-name divergence justified by the OTel data shape and shown forward-compatible (ADR-0044 Decision 2)."
  - "Honest-400 discipline preserved: malformed brace never degraded to bare name or partial filter (DD1, US-08)."
  - "ADR immutability respected via a refining ADR with back-reference, not an in-place edit of Accepted ADR-0042."
issues_identified:
  architectural_bias:
    - issue: "Resume-driven complexity (e.g. a parser-combinator dep or full PromQL engine)."
      severity: "n/a"
      assessment: "ABSENT. Hand-rolled parse, no regex crate, no new dependency; full engine explicitly rejected (ADR-0042 + ADR-0044 alternatives)."
  decision_quality:
    - issue: "ADR alternatives and consequences."
      severity: "n/a"
      assessment: "ADR-0044 carries four rejected alternatives (in-place edit, strict names, unquoted values, absent-fails-all) and positive/negative consequences. Adequate."
  completeness_gaps:
    - issue: "Quality attributes addressed."
      severity: "n/a"
      assessment: "Correctness, reliability (no silent wrong answer), security (no leak, fail-closed), performance (single retain pass vs the 500 ms budget), testability all covered."
  implementation_feasibility:
    - issue: "Testability."
      severity: "n/a"
      assessment: "Parser and filter are pure functions in the lib seam, exercised without a server; the established query-api shape. No capability or budget risk."
  priority_validation:
    q1_largest_bottleneck:
      evidence: "Slice 01 rejects any '{' with a 400 (baseline 0% filtering); this slice is the next anticipated rib (ADR-0042 Decision 3)."
      assessment: "YES"
    q2_simple_alternatives:
      assessment: "ADEQUATE (full engine, strict names, unquoted values, absent-fails-all all considered and rejected)."
    q3_constraint_prioritization:
      assessment: "CORRECT (smallest delta on the existing path; no over-engineering)."
    q4_data_justified:
      assessment: "JUSTIFIED (KPI 5 latency budget carried; semantics matrix is the correctness oracle)."
approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

No critical or high issues; no revision iteration required.
