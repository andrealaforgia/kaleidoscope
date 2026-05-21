# Slice 01: equality and inequality label matchers (this feature)

British English. No em dashes.

## Elevator Pitch
An operator (Sara Okafor) in Prism queries `http_requests_total{service.name="checkout"}`
during an incident and sees only checkout's series, filtered server-side from Pulse,
instead of a dozen overlapping lines. She can also exclude a noisy series with
`{service.name!="batch"}`. A regex or malformed matcher is honestly rejected with a 400.

## Scope (IN)
- Parse `name{ matcher_list }` extending the existing bare-name production.
- Equality `=` matchers (US-06), including the empty-string `=""` matches-absent rule.
- Inequality `!=` matchers (US-07), including the absent-label keep and `!=""`
  present-non-empty rule.
- Multiple matchers ANDed.
- Filter the translated `Vec<(Metric, MetricPoint)>` by the derived label set
  (`merge_labels` logic) before `to_matrix`.
- Honest 400 for regex `=~` `!~` and malformed matchers (US-08), with DD6 redaction.
- Calm empty arm when a matcher matches nothing.

## Scope (OUT, held)
- Regex matchers `=~` `!~` (slice 02b).
- `{__name__="x"}`-form metric SELECTION rather than bare-name selection (slice 02a).
- Functions, aggregations, operators, instant `/api/v1/query`, range vectors (v1, unchanged
  from ADR-0042).

## Stories
US-06 (equality), US-07 (inequality), US-08 (reject regex/malformed). All in `user-stories.md`.

## Why one slice
All three share one parser extension and one filter predicate, touch only
`crates/query-api/src/`, and total 2-3 days. They are sliced by user outcome (the kind of
filter expressed), each independently demonstrable. No oversized signal tripped (see
`story-map.md` Scope Assessment).

## Demonstrable in a single session
Ingest `http_requests_total` for three services under tenant "acme-prod" via the gateway;
open Prism against the query backend; query `http_requests_total{service.name="checkout"}`
and see one line; query `{service.name!="batch"}` and see two; query
`{service.name=~"check.*"}` and see the honest 400.
