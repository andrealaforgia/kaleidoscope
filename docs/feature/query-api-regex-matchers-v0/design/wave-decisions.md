# Wave Decisions: query-api-regex-matchers-v0 (DESIGN)

British English. No em dashes. Author: `nw-solution-architect` (Morgan).
Scope: application. Interaction mode: propose. DISCUSS pinned semantics:
`discuss/wave-decisions.md` (Matrix A full anchoring, Matrix B absent-label,
the invalid-regex 400).

## Key Decisions

| Decision | Value | Note |
|----------|-------|------|
| Architecture style | Unchanged from ADR-0042 / ADR-0044 | Hexagonal. `MetricStore` is the only driven port; `selector.rs` parse and `matrix.rs` filter carry the only mutable logic. This slice thickens the same parse + filter rib the `=`/`!=` slice established. |
| Regex engine | The `regex` crate, promoted to a DIRECT dependency of `crates/query-api` | RE2-derived, linear-time matching, no catastrophic backtracking. The pattern is exposed USER input, so a backtracking engine would be a ReDoS surface; `regex` removes that class of attack by construction. ALREADY present in `Cargo.lock` (v1.12.3) as a transitive dependency, so promoting it to direct likely adds NO new transitive crates. See DEVOPS handoff. |
| Anchoring mechanism | Compile the raw user pattern wrapped as `^(?:{pattern})$` | `regex::Regex::is_match` is UNANCHORED (it tests for a substring match). Prometheus anchors BOTH ends. Wrapping the raw RE2 pattern in a non-capturing group with `^`/`$` requires a FULL-string match, which is exactly Matrix A. The wrapping is applied to the raw pattern text before compilation, so a pattern's own alternation (`a|b`) is correctly bounded by the group. |
| Where compilation happens | ONCE per matcher, at filter-build time, before the row scan | The compiled `Regex` is built once per query when the filter is assembled, NOT per row. This keeps the linear-time guarantee across many rows (compile is the only super-linear-in-pattern step and it runs once). A compile failure here is the single origin of the invalid-regex 400. |
| Type shape | Extend `MatchOp` to `{Equal, NotEqual, Matches, NotMatches}`; keep `LabelMatcher` carrying the RAW pattern string | `LabelMatcher` stays a plain, comparable data struct (it still derives `Eq`/`Hash` for the parser tests and any future keying). A compiled `regex::Regex` is NOT `Eq` and NOT `Hash`, so it must NOT live in `MatchOp` or `LabelMatcher`. The compiled regex lives in a SEPARATE filter-side value built from the parsed matchers; the parsed types stay pure and comparable. |
| New crate | No | All change in `crates/query-api/src/`. |
| New HTTP surface | No | Same `/api/v1/query_range` route, same response envelope; one new 400 arm (invalid regex). |
| Router signature | UNCHANGED | `query_api::router(store, tenant, static_dir)` is byte-identical. Behaviour rides the existing `handle_query_range` handler. Verified against `lib.rs:105-118`. |
| Probe change | No | The new logic is pure and in-process. No new external substrate; the ADR-0042 Decision 8 startup probe and its three-orthogonal-layer enforcement are untouched. |
| ADR verdict | New ADR-0046, refining ADR-0044 | ADRs are immutable; a refining ADR with a back-reference preserves immutability. 0046 is the next free number (0045 is the latest, verified). ADR-0044 is CITED, NOT modified. |

## Architecture Summary

The feature flips the two `regex_reason()` reject arms in `selector.rs` into
real operators and adds one regex arm to the `matrix.rs` filter, with the
compiled regex held filter-side so the parsed types stay pure.

1. **Parse** (`selector.rs`). `read_operator` returns `MatchOp::Matches` for
   `=~` and `MatchOp::NotMatches` for `!~` instead of `Err(regex_reason())`.
   `LabelMatcher` carries the raw pattern string in its existing `value`
   field; the grammar, escapes, label-name class, and reject discipline are
   otherwise unchanged.
2. **Build the filter** (`matrix.rs` / `lib.rs`). Before the row scan, the
   parsed matchers are compiled into a filter value: each regex matcher's
   raw pattern is wrapped as `^(?:{pattern})$` and compiled ONCE. A compile
   error short-circuits to a single reason string ("invalid regex matcher").
   The compiled `Regex` lives only in this filter value, never in
   `LabelMatcher`/`MatchOp` (a compiled `Regex` is not `Eq`/`Hash`).
3. **Filter** (`matrix.rs`). `keep_row`/`matches` gain a regex arm over the
   SAME merged label set (`resource_attributes` then `point.attributes`
   winning, then authoritative `__name__`) the `=`/`!=` arms already use. The
   absent-as-empty rule is reused verbatim (`labels.get(name).unwrap_or("")`),
   so Matrix B falls out of one rule: `Matches` keeps iff the anchored regex
   matches the absent-as-empty value; `NotMatches` is its exact negation.
   Regex and `=`/`!=` matchers AND freely.
4. **Map errors** (`lib.rs`). The filter-build compile error maps to HTTP 400
   `{status:error, error:"invalid regex matcher"}` on the SAME `error_response`
   seam the parse 400s already use. The reason names the matcher as invalid
   and never echoes the raw query, the offending pattern, or a forwarded
   header (DD6). A valid-but-never-matching pattern is the calm 200 empty arm.

The orchestration order in `handle_query_range` is unchanged through query;
the only insertion is the compile-and-map step between `selector::parse` and
the existing `retain`: resolve-tenant -> parse-bounds -> parse-selector ->
build-filter (compile regexes, 400 on failure) -> query -> filter -> translate.

## Reuse Analysis (MANDATORY)

| Asset | Reused / Extended / New | Detail |
|-------|------------------------|--------|
| `crates/query-api/src/selector.rs` | Extended | `read_operator` flips the `=~`/`!~` arms from `Err(regex_reason())` to `MatchOp::Matches`/`MatchOp::NotMatches`; `MatchOp` enum gains two variants; `LabelMatcher` unchanged (raw pattern in its `value` field). Grammar, escapes, label-name class, all other reject arms unchanged. |
| `crates/query-api/src/matrix.rs` | Extended | `matches`/`keep_row` gain a regex arm over the same merged label set and the same absent-as-empty rule; a small filter-build helper compiles each regex once (wrapping `^(?:re)$`) and returns either the compiled filter or the invalid-regex reason. `to_matrix` and `merge_labels` unchanged. |
| `crates/query-api/src/lib.rs` | Extended | One compile-and-map step inserted between `selector::parse` and the existing `retain`; a compile error reuses the `error_response(BAD_REQUEST, ...)` seam. Tenancy, bounds, success/empty serialisation, the probe all unchanged. |
| `pulse::MetricStore::query` | Reused unchanged | Name-only selection; regex matchers filter the result, not the query. |
| `crates/query-api/src/composition.rs` (probe, tenancy) | Reused unchanged | No probe or composition-root change; the new logic is pure. |
| Prism contract / response envelope | Reused unchanged | Only WHICH series the success arm carries changes, plus one new 400 arm that already satisfies `isPromError`. |
| `regex` crate | New DIRECT dependency | Promoted from transitive to direct in `crates/query-api/Cargo.toml`. Already in `Cargo.lock` at 1.12.3; likely no new transitive crates. The ONLY new dependency and the only departure from the `=`/`!=` slice's zero-dependency posture. |
| New crate | None | All change in `crates/query-api/src/`. |

**Verdict: all EXTEND, one new direct dependency (`regex`), zero new
components, zero new crates, zero unjustified CREATE NEW.** No existing code
provides regex matching; the `=`/`!=` slice deliberately added no regex crate
because regex matchers were out of scope. This feature is the smallest honest
delta that adds the capability: three files extended, one dependency promoted
from the lock.

## Constraints

- **Router signature frozen.** `query_api::router` is unchanged; the behaviour
  rides the existing handler (DISCUSS System Constraints; verified `lib.rs`).
- **Envelope frozen.** Success/empty satisfy Prism's `isPromSuccess`; the new
  invalid-regex 400 satisfies `isPromError`. The envelope shape never changes.
- **Linear-time matching only.** The engine must be backtracking-free
  (`regex` is RE2-derived) because the pattern is exposed user input; a
  ReDoS-capable engine is rejected on security grounds (see ADR-0046).
- **Compile once per query, never per row.** The per-row scan stays linear in
  row count; the per-pattern compile runs once at filter-build.
- **DD6 redaction.** The invalid-regex reason names the matcher as invalid and
  never echoes the raw query, the offending pattern, or a forwarded header.
- **Latency budget carried.** The inherited p95 < 500 ms budget now spans the
  per-query regex compile + match; patterns are short and per-query, expected
  well within budget (DISCUSS risk table, low/low).

## Upstream Changes

None. This feature needs no change to `pulse`, `aegis`, or any other crate.
`pulse-series-identity-v0` (ADR-0045, shipped) already made `query` fan out
across series sharing a name, so the filter sees real per-series labels; this
feature consumes that without modifying it.

## DEVOPS Handoff Annotation (to `@nw-platform-architect`, Apex)

- **New direct dependency: `regex`.** Promoted from transitive to a direct
  dependency of `crates/query-api`. It is ALREADY in `Cargo.lock` (v1.12.3,
  pulled in transitively), so promoting it to direct most likely adds NO new
  transitive crates to the graph and therefore NO new licences. Its tail
  (`aho-corasick`, `memchr`, `regex-automata`, `regex-syntax`) is already in
  the lock under MIT/Apache-2.0/Unicode licences that the existing `deny.toml`
  allow-list already tolerates. **Apex MUST VERIFY this** against `cargo deny`
  Gate 4 (ADR-0005): confirm no new licence outside the allow-list and no new
  advisory or yanked crate appears once `regex` is a direct dependency. This
  DESIGN note is a flag, not the verification; the Gate-4 run is a DEVOPS task.
  Pin posture: add `regex` without a wildcard (Gate 4 `wildcards = "deny"`),
  matching the version already in the lock.
- **No new gate.** `gate-5-mutants-query-api` (ADR-0005 Gate 5; CLAUDE.md
  mutation strategy) already covers `crates/query-api/src/` via `--in-diff`.
  The new parse arm, the regex filter arm, and the compile-error mapping land
  in that scope at the project 100% kill-rate gate. Primary mutation targets:
  the full-anchor boundary (a substring must NOT match, so a mutant dropping
  the `^...$` wrapping must die), the `Matches`/`NotMatches` negation, and the
  invalid-vs-never-matching distinction (a 400 must not degrade to a 200 empty
  or vice versa).
- **No new external integration.** The Prism consumer-driven contract boundary
  (ADR-0042 / ADR-0044 handoff) is unchanged; the envelope does not change, so
  the existing contract posture (Apex's choice of Pact-JS or container
  fixtures) covers the new 400 arm without a new contract. No third-party API,
  webhook, OAuth provider, or vendor SDK is added; `regex` is an in-process
  library, not a network integration.
- **Instrumentation (carried).** The existing per-query duration seam now spans
  the regex compile + match. The matcher-count and kept/total-series ratio
  carried from the `=`/`!=` slice extend to regex matchers; add a regex-vs-exact
  matcher-form count and a reject-form count (invalid-regex vs malformed) if the
  existing reject-form counter does not already distinguish them.
- **No new probe.** The new logic is pure and in-process with no external
  substrate; the ADR-0042 Decision 8 startup probe and its three-orthogonal-layer
  enforcement are unchanged.

## Development paradigm (handoff to crafter)

Rust idiomatic per CLAUDE.md: data (`Selector`, `LabelMatcher`, the extended
`MatchOp`) plus free functions (`parse`, the filter-build helper, `matches`,
`keep_row`). No inheritance, no `dyn` where generics suffice. The crafter owns
the internal shape of the compiled-filter value (for example a parallel
`Vec` of compiled regexes alongside the matchers, or a small enum carrying the
compiled `Regex`) and the GREEN/REFACTOR structure; this design fixes only the
`MatchOp` extension, where the compiled regex lives (filter-side, not in the
parsed types), the anchoring mechanism (`^(?:re)$`), where the 400 originates
(compile failure at filter-build), and the absent-as-empty regex semantics.

## Peer review

Reviewer: `@nw-solution-architect-reviewer` requested; not invocable in this
session, so a structured self-review against the five critique dimensions was
run as the recorded fallback. Verdict: APPROVED, iteration 1.

```yaml
review_id: "arch_rev_query-api-regex-matchers-v0_design"
reviewer: "structured-self-review (solution-architect-reviewer unavailable)"
artifact: "design/wave-decisions.md, design/application-architecture.md, adr-0046"
iteration: 1
strengths:
  - "Smallest honest delta: three files extended, one dependency promoted from the lock, zero new crates/components."
  - "Security-first engine choice: RE2-derived regex crate eliminates ReDoS by construction for exposed user input (ADR-0046 Decision 1)."
  - "Anchoring mechanism pinned precisely: wrap the raw RE2 pattern as ^(?:re)$, giving Matrix A by full-string match (ADR-0046 Decision 2)."
  - "Eq/Hash hazard addressed head-on: compiled Regex is not Eq/Hash, so it lives filter-side and the parsed types stay pure and comparable (Key Decisions, ADR-0046 Decision 3)."
  - "Compile-once-per-query preserves the linear-time guarantee across rows; the 400 has a single, well-located origin."
  - "ADR immutability respected: ADR-0046 refines ADR-0044 by back-reference, not in-place edit."
issues_identified:
  architectural_bias:
    - issue: "Resume-driven complexity (a parser-combinator dep, a full PromQL engine, a custom regex VM)."
      severity: "n/a"
      assessment: "ABSENT. One well-known, already-in-lock crate; no full engine; hand-rolled matcher explicitly rejected for ReDoS/complexity (ADR-0046 alternatives)."
  decision_quality:
    - issue: "ADR alternatives and consequences."
      severity: "n/a"
      assessment: "ADR-0046 carries three rejected alternatives (no-regex status quo, hand-rolled matcher, compile-per-row) and positive/negative consequences. Adequate."
  completeness_gaps:
    - issue: "Quality attributes addressed."
      severity: "n/a"
      assessment: "Security (ReDoS-free engine, no leak), correctness (Matrix A + Matrix B oracle), reliability (no panic/500 on bad pattern), performance (compile-once vs the 500 ms budget), testability all covered."
  implementation_feasibility:
    - issue: "Testability."
      severity: "n/a"
      assessment: "Parser and filter remain pure functions exercised without a server; the filter-build compile is a pure function returning Result. No capability or budget risk."
  priority_validation:
    q1_largest_bottleneck:
      evidence: "selector.rs returns an honest 400 for any =~/!~ today (baseline 0% pattern filtering); this is the next anticipated rib (ADR-0044, slice-02b deferral)."
      assessment: "YES"
    q2_simple_alternatives:
      assessment: "ADEQUATE (no-regex status quo, hand-rolled matcher, compile-per-row all considered and rejected)."
    q3_constraint_prioritization:
      assessment: "CORRECT (smallest delta on the existing path; one dependency already in the lock; no over-engineering)."
    q4_data_justified:
      assessment: "JUSTIFIED (KPI latency budget carried; Matrix A + Matrix B are the correctness oracle; regex is verified present in Cargo.lock)."
approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

No critical or high issues; no revision iteration required.
