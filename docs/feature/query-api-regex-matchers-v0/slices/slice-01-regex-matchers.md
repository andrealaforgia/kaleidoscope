# Slice 01: regex label matchers (=~, !~)

British English. No em dashes. Author: `nw-product-owner` (Luna).

Maps to `discuss/user-stories.md` US-09 (filter by pattern), US-10 (the regex absent-label
matrix), US-11 (reject invalid regex). Acceptance suite will mirror
`crates/query-api/tests/slice_03_label_matchers.rs` (oneshot, real durable Pulse, distinguishable
series), enabling one scenario at a time from a walking skeleton.

## Learning hypothesis

We believe that turning the rejected `=~`/`!~` operators into real, fully-anchored regex matchers
behind the SAME `selector::parse` and the SAME `matrix::keep_row` filter, treating an absent label
as the empty string, will let an on-call operator filter a noisy metric by pattern in one
`query_range` call and trust the result. We will know we were right when:

- `metric{service.name=~"check.*"}` returns only the fully-matching series end to end through the
  unchanged `query_api::router` (the walking skeleton), and
- the full-anchor matrix (Matrix A) and the five-arm absent-label matrix (Matrix B) each return
  exactly the Prometheus-prescribed series, and
- an invalid pattern is an honest 400 while a valid-but-never-matching pattern is a calm 200 empty,
  and
- Prism's `isPromSuccess`/`isPromError` validators accept every arm.

We will know we were wrong if the regex engine cannot express full anchoring cleanly, if the
absent-label arms need special-casing beyond the absent-as-empty rule, or if compiling a regex per
query breaches the inherited p95 < 500 ms budget.

## Walking skeleton (enabled first)

`http_requests_total{service.name=~"check.*"}` against a real durable Pulse seeded with
"checkout", "cart", "search" returns ONLY the "checkout" series, through `query_api::router` via
oneshot, accepted by Prism's success validator. Entry point exercised:
`GET /api/v1/query_range?query=http_requests_total{service.name=~"check.*"}&start=1716200000&end=1716200060&step=15s`.

## Scenario order (one at a time after the skeleton)

1. (skeleton) prefix `=~` narrows to the matching series [US-09]
2. full anchoring: `=~"check"` matches nothing, `=~"check.*"` matches both [US-09]
3. `=~` ANDs with `code="200"` [US-09]
4. `=~` matching nothing is the calm 200 empty arm [US-09]
5. `env=~""` keeps the absent-env series [US-10]
6. `env=~".+"` keeps only present non-empty env [US-10]
7. `env!~""` keeps only present non-empty env [US-10]
8. `env!~".+"` keeps the absent-or-empty series [US-10]
9. `env!~"prod"` keeps the absent-env series (absent satisfies `!~`) [US-10]
10. invalid regex (unclosed group) is a 400 status:error [US-11]
11. invalid negative regex (dangling quantifier) is a 400 [US-11]
12. valid-but-never-matching `=~"/admin/.*"` is a 200 empty, NOT a 400 [US-11]
13. an invalid-regex rejection never leaks a forwarded header or the pattern [US-11]

## Carpaccio taste tests

- **Thin (vertical, end to end)**: every scenario drives the FULL stack (HTTP handler -> parse ->
  Pulse name-select -> regex filter -> matrix -> envelope) through the unchanged public router. No
  scenario stops at a layer.
- **One day or less**: two files extended (`selector.rs` flips the `=~`/`!~` arm to real operators;
  `matrix.rs matches` gains a regex arm), the filter reusing the existing `keep_row` and
  derived-label-set machinery. Comparable to the `=`/`!=` slice that shipped as one slice. The only
  genuinely new element is the regex engine, a DESIGN concern flagged in `wave-decisions.md`.
- **Demonstrable in one session**: the walking-skeleton curl shows a pattern filter working; the
  matrix scenarios and the invalid-regex 400 each demo a single observable behaviour.
- **Delivers user value standalone**: an operator gains pattern filtering (`route=~"/api/.*"`,
  `service.name!~"batch-.*"`) that did not exist; useful the moment it ships, independent of any
  future PromQL work.
- **Right-sized**: 3 stories, 13 scenarios across them, 1 module, 1 inherited integration point
  (Pulse via the unchanged port), 1 new 400 arm. PASS (see `story-map.md` Scope Assessment).

## Out of this slice (honest deferrals)

- Metric-NAME selection by regex (the `slice-02a` idea): OUT. `__name__=~` filters the already
  merged label set of one named metric's series; it does not choose which metrics to scan.
- PromQL functions, aggregations, the instant `/api/v1/query` endpoint, range vectors beyond
  `query_range`: OUT, unchanged from ADR-0042 / ADR-0044.
- The regex engine choice and the anchoring mechanism: deferred to DESIGN (flagged, not decided).
