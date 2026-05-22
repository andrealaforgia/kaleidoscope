# Story Map: query-api-regex-matchers-v0

British English. No em dashes. Author: `nw-product-owner` (Luna).

## User: Priya Nandakumar, on-call SRE for tenant "acme-prod"

## Goal: filter a noisy metric to a PATTERN of series (a route family, a
service family, every service except a family) in one server-side
query_range call, mid-incident, instead of being limited to exact label values.

## Backbone

The operator's workflow is a single read loop, unchanged in shape from the `=`/`!=` slice;
this feature thickens the parse and filter ribs to understand `=~`/`!~`.

| Compose the pattern query | Parse the selector | Filter the series | Read the result |
|---------------------------|--------------------|--------------------|-----------------|
| Type `name{label=~"re"}` | Accept `=~`/`!~` ops | Full-anchor regex match | See only matching series |
| Mix regex with `=`/`!=` | Compile the pattern | Apply absent-label matrix | Calm empty arm on no match |
| Negate with `!~` | Reject invalid regex (400) | AND all matchers | Honest 400 on invalid regex |

---

### Walking Skeleton

The thinnest end-to-end slice that proves the whole loop works for regex:

- **Compose**: the operator types `http_requests_total{service.name=~"check.*"}`.
- **Parse**: the selector parser accepts `=~` (instead of returning the slice-01 regex 400)
  and yields a regex matcher.
- **Filter**: `keep_row` applies a full-anchored regex test to each row's derived label set.
- **Read**: only the series whose `service.name` fully matches `check.*` are returned, driven
  through the existing `query_api::router` via the `oneshot` pattern against a real durable
  Pulse, and Prism's success validator accepts the response.

This is exactly the scenario named in Decision 4: `metric{service.name=~"check.*"}` returns
only the matching series. One task from each backbone column, the minimum to make the regex
loop work end to end.

### Release 1 (the only release): regex matchers work, correctly and honestly

This feature is a single right-sized slice. All three stories ship together because the
correctness matrices (full-anchor, absent-label) and the honest-400 boundary are not
separable from the happy path without shipping a half-true filter.

- **US-09** (walking skeleton + happy path): the `=~` operator parses and filters with full
  anchoring; AND composition with `=`/`!=`; calm empty arm. Target outcome: an operator
  filters by pattern in one query.
- **US-10** (the absent-label/empty-pattern matrix): the five-arm regex absent-label matrix is
  exactly right. Target outcome: a pattern over a sometimes-absent label keeps exactly the
  right series, no silent mis-answer.
- **US-11** (invalid-regex 400): an invalid pattern is an honest 400; a valid-but-never-matching
  pattern stays a calm 200 empty. Target outcome: the operator is never misled by a malformed
  regex.

## Priority Rationale

1. **US-09 first (walking skeleton)**. It proves the riskiest structural assumption: that `=~`
   can flip from a 400 reject arm to a real operator behind the SAME parser and SAME `keep_row`
   filter, end to end through the unchanged router, with full anchoring. Until this works, the
   absent-label matrix and the invalid-regex arm have nothing to attach to. Tie-break:
   walking skeleton always first.
2. **US-10 second (riskiest correctness assumption)**. The absent-label/empty-pattern matrix is
   the regression-prone heart: a naive engine silently mishandles absent labels and `.+`/`""`
   patterns, the worst mid-incident lie. It is the highest-value correctness slice once the
   skeleton holds, and it derisks the assumption that absent-as-empty composes with full
   anchoring to give the Prometheus rule.
3. **US-11 third (honest boundary)**. The invalid-regex 400 is the smallest and least risky of
   the three (it extends the established honest-400 discipline), but it depends on `=~`/`!~`
   parsing as real matchers (US-09), so it lands last. It still ships in the same slice because
   a regex feature that 500s or silently passes an invalid pattern is not honestly shippable.

## Scope Assessment: PASS

3 stories, 1 bounded context (`crates/query-api`, two files extended: `selector.rs` parse arm,
`matrix.rs` filter arm), estimated under 1 day of focused work (the predecessor `=`/`!=` slice
of comparable shape shipped as one slice). No oversized signals: 1 module, the walking skeleton
needs the existing single integration point (Pulse via the unchanged port), one new 400 arm. The
three stories are one carpaccio slice, not three independent deliverables, because the
correctness matrices are inseparable from the happy path.

## OUT of scope (confirmed)

- PromQL functions, aggregations, operators (carried from ADR-0042 / ADR-0044).
- Range/instant vectors beyond what `query_range` already does; the `/api/v1/query` instant
  endpoint stays a 400.
- `slice-02a` name-matcher SELECTION (filtering the metric NAME by regex to choose which metrics
  to scan). See note below on `__name__=~`.
- NO change to the public `query_api::router` signature; the behaviour rides the existing HTTP
  handler.

## Note on `__name__=~`

Treated as IN scope only insofar as it falls out trivially, and noted honestly. `__name__` is
already part of the derived label set that `keep_row` filters on (`merge_labels` inserts it
authoritatively; the `=`/`!=` slice already has a passing `name_label_is_matchable` acceptance
test). Therefore `{__name__=~"http_.*"}` will filter the ALREADY-MERGED label set of the series
returned by the bare-name `pulse.query` for the metric named in the selector, exactly like any
other label. It does NOT, and is not intended to, SELECT which metrics to scan by name regex
(that is the deferred `slice-02a` selection idea, explicitly OUT). In practice the selector still
needs a concrete metric name to drive `pulse.query`, so `__name__=~` is a post-hoc filter on one
metric's series, not a metric chooser. We will note this boundary honestly and not over-scope a
dedicated story for it.
