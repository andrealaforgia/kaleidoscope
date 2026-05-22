# Outcome KPIs: query-api-regex-matchers-v0

British English. No em dashes. Author: `nw-product-owner` (Luna).

## Feature: query-api-regex-matchers-v0

### Objective

An on-call operator can filter a noisy metric to a PATTERN of series in one server-side
query_range call, mid-incident, and trust the result is exactly correct (full anchoring,
absent-label matrix) or honestly refused (invalid regex), never a plausible wrong answer.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | On-call operator (Priya) | filters a metric to the series whose label fully matches a pattern, in one `=~` query | a `=~` query returns EXACTLY the fully-anchored matching series (substring does not match); 100% of these shapes round-trip Prism's `isPromSuccess` | 0% (every `=~` is a 400 today) | acceptance tests asserting kept/excluded series for prefix, substring-anchor, AND-composition, empty cases; contract test through Prism | Leading |
| 2 | On-call operator (Priya) | applies a regex over a sometimes-absent label and gets the right series | each of the 5 absent-label/empty-pattern arms returns EXACTLY the expected series; 0 silently dropped or wrongly kept | 0% (no regex absent-label behaviour exists) | one acceptance scenario per matrix arm; unit tests on the pure `matches` predicate | Leading |
| 3 | On-call operator (Priya) and the team | receives an honest 400 for an invalid regex, and a calm 200 empty for a valid-but-never-matching one | 100% of invalid patterns are 400 status:error; 0 panics/500s/silent mis-answers; 100% of valid-but-never-matching patterns stay 200 empty | n/a (today every `=~`/`!~` is a blanket 400) | one acceptance test per invalid form; a valid-but-empty-stays-200 test; a redaction test | Leading |
| 4 | Prism's query client | keeps accepting every arm of the response | 100% of success, empty, and 400 arms satisfy `isPromSuccess`/`isPromError`; 0 envelope-shape regressions | 100% (envelope unchanged by the `=`/`!=` slice) | contract test through Prism's pinned validators | Guardrail |

### Metric Hierarchy

- **North Star**: a `=~`/`!~` query returns EXACTLY the series the Prometheus semantics
  prescribe (full anchoring AND the absent-label matrix), or an honest 400 for an invalid
  pattern. Correctness, not throughput, is the point.
- **Leading Indicators**: per-arm acceptance pass rate (full-anchor arms, the 5 absent-label
  arms, invalid-regex arms); the mutation kill rate on the new parse + filter logic.
- **Guardrail Metrics**: the response envelope still satisfies Prism's validators (KPI 4); no
  cross-tenant leak (inherited US-04 guardrail); per-query p95 latency does not regress past the
  inherited 500 ms budget now that the filter also compiles and runs a regex per matcher.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 (full-anchor correctness) | `crates/query-api/tests` acceptance suite | per-arm assertions on returned series | every CI run + mutation gate | crafter |
| 2 (absent-label matrix) | acceptance suite + `matrix.rs` unit tests | one scenario per arm; pure-predicate units | every CI run + mutation gate | crafter |
| 3 (invalid-regex honesty) | acceptance suite | per-invalid-form 400 tests; valid-but-empty 200 test; redaction test | every CI run | crafter |
| 4 (envelope guardrail) | Prism contract validators | `prism_accepts_success`/`prism_accepts_error` per arm | every CI run | crafter |
| latency guardrail | existing `record_query` seam | per-query duration spanning parse + regex compile + filter | continuous in DEVOPS | platform-architect |

### Hypothesis

We believe that turning the rejected `=~`/`!~` operators into real, fully-anchored regex
matchers behind the same parser and `keep_row` filter, with the absent-label matrix exactly
right and an honest 400 for invalid patterns, will let an on-call operator filter a noisy metric
by pattern in one query. We will know this is true when a `=~`/`!~` query returns exactly the
Prometheus-prescribed series across the full-anchor and absent-label matrices, an invalid pattern
is a 400, a valid-but-never-matching pattern is a calm 200 empty, and Prism's validators accept
every arm.

### Handoff to DEVOPS (platform-architect)

- **Data collection**: extend the existing per-query duration (the `record_query` seam carried
  from the `=`/`!=` slice) to span regex compilation and matching; add a regex-matcher count and
  an invalid-regex reject count per query (distinct from the existing malformed-matcher count).
- **Alerting**: per-query p95 > 500 ms on ubuntu-latest (the inherited budget, now including
  regex compile + match); any contract-shape regression (KPI 4) and any cross-tenant leak
  (inherited US-04 guardrail) remain hard alerts. A spike in invalid-regex rejects is a soft
  signal, not an alert (it is operator typos, not a service fault).
- **Baseline**: KPIs 1-3 baseline at 0% / n/a because regex matchers do not exist before this
  feature; no pre-release baseline collection is required. KPI 4 carries the existing 100%
  envelope baseline.
- **No new instrumentation substrate**: the new logic is pure and in-process; no new probe, no
  new external integration (the regex engine is a library dependency, flagged for DESIGN, not an
  external system).
