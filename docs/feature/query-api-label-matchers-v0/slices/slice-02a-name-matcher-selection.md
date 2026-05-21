# Slice 02a (DEFERRED, briefed not scoped): __name__-form metric selection

British English. No em dashes.

## Idea
Allow a selector that names the metric via a `__name__` matcher rather than a bare name:
`{__name__="http_requests_total", service.name="checkout"}`. Today the metric is selected
by the bare name that drives `pulse.query(&tenant, &MetricName, range)`; `__name__` is only
MATCHABLE within the derived set (this feature, US-06), not yet usable to SELECT the metric.

## Why deferred
- Pulse's `query` is keyed by `MetricName`; selecting via `{__name__="x"}` means extracting
  the metric name from the matcher list and using it as the Pulse key, with the remaining
  matchers filtering. This is a meaningful change to the parse-to-query orchestration, not
  just the filter.
- The common, real query during an incident is `name{...}` (bare name + matchers), which
  this feature delivers. `{__name__="x"}`-only selection is rarer and can wait.

## Trigger to schedule
When an operator workflow or a Prism feature needs the all-matcher selector form, or when
a Grafana-style query builder emits `{__name__="x"}`.

## Estimated size
~1 story, 1 day. Reuses this feature's matcher parser and filter; adds name-extraction in
the orchestration.
