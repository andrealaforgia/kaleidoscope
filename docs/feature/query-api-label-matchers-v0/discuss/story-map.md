# Story Map: query-api-label-matchers-v0

## User: Prism's query client (pinned contract) + Sara Okafor, the on-call SRE behind it
## Goal: Narrow a metric to the series matching one or more label matchers, server-side, so the operator can read the series that matter during an incident

British English. No em dashes.

## Backbone

| Receive labelled query | Parse selector | Filter rows | Render filtered matrix |
|------------------------|----------------|-------------|------------------------|
| Prism forwards `name{...}` raw | Split name from matcher list | Apply `=` / `!=` to derived label set | Group kept rows into matrix series |
| (unchanged from slice 01) | Parse `=` and `!=` matchers, ANDed | Keep rows satisfying every matcher | Calm empty arm when nothing matches |
|  | Reject regex `=~` `!~`, malformed -> 400 | (filter on resource U point U __name__) | status:error 400 unchanged for bad input |

This feature thickens the **Parse selector** and **Filter rows** ribs of the loop that
`query-range-api-v0` already shipped end-to-end. No new walking skeleton: the skeleton
(request -> tenant -> parse -> Pulse query -> matrix) already exists and is demonstrable.

---

### Walking Skeleton

Not applicable as a fresh skeleton (Decision 2: No). The end-to-end loop already works for
the bare-name selector. The minimum demonstrable slice of THIS feature is US-06 alone:
parse `name{label="value"}`, filter, and render the narrowed matrix. US-06 is independently
demonstrable in a single session (ingest two services, query one, see one line).

### Release 1 (this feature): the operator filters by label
Stories: US-06 (equality `=` matcher, single + multiple ANDed, filtered matrix and calm
empty arm), US-07 (inequality `!=` matcher, including the absent-label and empty-string
semantics), US-08 (reject regex and malformed matchers honestly, 400).
Target outcome: an operator narrows a noisy metric to the series matching her matchers,
server-side, with correct Prometheus absent-label semantics. KPI: North Star + Correctness
(see outcome-kpis.md).

### Deferred (NOT in this feature scope): slice 02a / 02b
- US-future-a: `{__name__="x"}`-form metric selection (matcher selects the metric, not just
  a bare name). Briefed in `slices/slice-02a-name-matcher-selection.md`.
- US-future-b: regex matchers `=~` `!~`. Briefed in `slices/slice-02b-regex-matchers.md`.
- Unchanged from ADR-0042: instant `/api/v1/query`, range vectors, functions,
  aggregations, operators. Still v1.

## Priority Rationale

Priority by outcome impact and dependency, not by technical layer. All three stories share
one parser change and one filter predicate; they are sliced by user outcome (the kind of
filter the operator expresses) so each is independently demonstrable.

1. **US-06 equality matcher** (P1, Must) - the common, real query
   `metric{service="checkout"}`. This is the headline value: narrow to the series you care
   about. It carries the parser extension and the filter predicate that US-07 reuses.
   Derisks the core assumption (does server-side filtering narrow correctly?).
2. **US-07 inequality matcher** (P1, Must) - `metric{tenant.id!="x"}`. Co-equal in value
   (exclude noisy series) and it is where the SUBTLE, correctness-critical absent-label and
   empty-string semantics live. Must ship with US-06 or the matcher feature is half-honest.
3. **US-08 reject regex and malformed** (P2, Should) - the honest-400 guard rail extended to
   the matcher grammar. Protects the operator from a plausible wrong answer when she pastes
   a regex matcher the slice cannot honour. Slightly lower urgency than the success paths
   but required before the slice is trustworthy (the ADR-0042 discipline).

Deferred (Won't-Have this feature): `__name__`-form selection, regex matchers, instant
endpoint, full PromQL.

## Scope Assessment: PASS - 3 stories, 1 module touched (query-api crate; pulse/aegis reused unchanged), estimated 2-3 days

Oversized signals checked (none tripped at the 2+ threshold):
- User stories: 3 (<= 10). PASS.
- Bounded contexts/modules: only `crates/query-api/src/` changes (`selector.rs` parse +
  a filter applied in `lib.rs`/`matrix.rs`); pulse and aegis are reused unchanged. 1 (<= 3).
  PASS.
- Walking skeleton integration points: none new; the existing Prism/Pulse/aegis seams are
  reused. 0 new (<= 5). PASS.
- Estimated effort: 2-3 days total, split into three right-sized stories. PASS.
- Independent shippable outcomes: one (label filtering). The deferred items (regex,
  `__name__`-form) are genuinely separate later slices, already excluded. PASS.

Right-sized. No split required. Each story is 3-7 scenarios and 1 day or less.
