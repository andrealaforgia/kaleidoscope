# Wave Decisions: query-api-regex-matchers-v0 (DISCUSS)

British English. No em dashes. Author: `nw-product-owner` (Luna).
Scope: backend. Feature type: backend. Research depth: lightweight.

## Decision 4 (recorded inputs)

| Decision | Value | Note |
|----------|-------|------|
| JTBD wave | No | Lightweight backend slice extending a shipped, well-understood feature. The job statement is inherited from `query-api-label-matchers-v0` and ADR-0044: an on-call operator filters a noisy metric to the series that matter, mid-incident, server-side. This slice adds the pattern-filter capability to that same job. |
| Feature type | Backend | A query-api parser + filter extension; no UI work. Prism is an unchanged downstream consumer of the unchanged envelope. |
| Research depth | Lightweight | Prometheus regex matcher semantics are well documented and already partly pinned by the slice-01 reject arm and ADR-0044. No new research artefact; the semantics matrix below is the spec. |
| Walking skeleton | `metric{service.name=~"check.*"}` returns only the matching series | Driven through the existing `query_api::router` via the `oneshot` pattern, against a real durable Pulse. See `story-map.md`. |

## DISCOVER / DIVERGE provenance

No DIVERGE artefacts exist for this feature (`docs/feature/query-api-regex-matchers-v0/diverge/`
is absent). This is acceptable for a lightweight backend slice: the validated job is inherited
from the predecessor feature's ADR-0044 and `slices/slice-02b-regex-matchers.md`, the deferred
brief that seeds this work. Risk noted: there is no fresh JTBD/ODI grounding; mitigated because
the job is a direct, narrow extension of an already-validated and shipped one, and the deferral
brief already states the trigger ("when operators report needing pattern filters, e.g. all
`route=~"/api/.*"`") and the size (~1-2 stories, 1-2 days).

## Semantics pinned in DISCUSS (the two correctness matrices)

### Matrix A: full anchoring (the Prometheus rule)

A regex matcher is anchored at BOTH ends. `label=~"re"` keeps a series iff the label value
FULLY matches the pattern (equivalent to `^(?:re)$`). So `service.name=~"check"` does NOT match
"checkout"; `check.*` does. `!~` is the exact negation of `=~`.

### Matrix B: regex absent-label / empty-pattern (the second matrix, on top of A)

Treating an absent label as the empty string (consistent with the `=`/`!=` slice):

| Matcher | Keeps a row iff |
|---------|-----------------|
| `label=~""` | label ABSENT or present-and-empty |
| `label=~".+"` | label PRESENT and non-empty |
| `label!~""` | label PRESENT and non-empty |
| `label!~".+"` | label ABSENT or present-and-empty |
| `label!~"re"` (non-empty `re`) | label ABSENT, or present with a value not fully matching `re` |

`!~` is the exact negation of `=~` evaluated on the same absent-as-empty value. All matchers are
ANDed; regex and `=`/`!=` mix freely. An all-excluded result is the calm `result: []` at HTTP 200.

### Honest 400 boundary

An invalid regex (a pattern that fails to compile) is HTTP 400 `{status:error, error:"invalid
regex matcher"}`, satisfying Prism's `isPromError`, never echoing the raw query, the offending
pattern, or a forwarded header value (DD6). A valid-but-never-matching pattern is the calm 200
empty arm, NOT a 400.

## Risks and flags for DESIGN

| Risk / flag | Probability | Impact | Disposition |
|-------------|-------------|--------|-------------|
| **Regex engine dependency choice** | High (certain a choice is needed) | Medium | FLAGGED for DESIGN, NOT decided here. The `=`/`!=` slice deliberately added ZERO dependencies (hand-rolled parsing; ADR-0044 reuse analysis: "Regex crate NOT added"). This feature necessarily introduces a regex capability. Whether that is a crate (e.g. the ecosystem-standard one), and how full anchoring is achieved (compiling `^(?:pattern)$`, or using an anchored-match API), is the solution-architect's call. DISCUSS pins ONLY the observable semantics (Matrix A, Matrix B, the invalid-regex 400). This is the single most significant DESIGN decision in the slice and reverses a prior zero-dependency posture, so it warrants an ADR refining ADR-0044. |
| **Where the pattern compiles** (parse time vs filter-build time) | Medium | Low | DESIGN decision. DISCUSS requires only that an invalid pattern is a 400 and a valid one filters correctly. |
| **Latency: regex compile + match per query** | Low | Low | The inherited p95 < 500 ms budget now includes regex work. Patterns are short and per-query; compile-once-per-matcher is expected to stay well within budget. Guardrail carried in `outcome-kpis.md`. |
| **Absent-label matrix regression** | Medium | High | The five-arm Matrix B is regression-prone (a naive engine mishandles absent labels). Mitigated: one dedicated acceptance scenario per arm (US-10) plus pure-predicate unit tests; `gate-5-mutants-query-api` covers the filter via `--in-diff`. |
| **No DIVERGE grounding** | Low | Low | Noted above; mitigated by inherited validated job. |

## Scope confirmations

- IN: the `=~` and `!~` operators in the matcher grammar; full-anchored regex matching in the
  filter; the regex absent-label matrix (Matrix B); mixing regex with `=`/`!=`; invalid-regex 400.
- OUT: PromQL functions/aggregations; range/instant vectors beyond `query_range`; the `slice-02a`
  name-matcher SELECTION idea; NO change to the `query_api::router` signature.
- `__name__=~`: IN only as a trivial fall-out (it is already a filterable label in the derived
  set), NOT as a metric-selection mechanism. Noted honestly in `story-map.md`; no dedicated story.

## Artefacts produced

- `discuss/user-stories.md` (US-09, US-10, US-11; System Constraints; per-story Elevator Pitch,
  domain examples, BDD UAT scenarios, AC, outcome KPIs).
- `discuss/story-map.md` (backbone, walking skeleton, single release, priority rationale, scope
  assessment PASS).
- `discuss/outcome-kpis.md` (4 KPIs, hierarchy, measurement plan, DEVOPS handoff).
- `discuss/wave-decisions.md` (this file).
- `slices/slice-01-regex-matchers.md` (the carpaccio slice with learning hypothesis).
