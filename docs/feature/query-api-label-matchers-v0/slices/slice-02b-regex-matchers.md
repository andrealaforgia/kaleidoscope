# Slice 02b (DEFERRED, briefed not scoped): regex label matchers

British English. No em dashes.

## Idea
Add regex matchers `=~` and `!~` to the matcher grammar: `{service.name=~"check.*"}` keeps
series whose label fully matches the regex; `!~` is the negation. Prometheus anchors regex
matchers (full-string match).

## Why deferred (lean OUT of slice 01 to keep it tight)
- Regex introduces a regex engine dependency and the full-anchor semantics (Prometheus
  anchors both ends), plus the absent-label rules for regex (`=~""` matches absent/empty;
  `!~""` matches present-non-empty), which is a second correctness matrix on top of the
  `=`/`!=` one.
- The common incident query is exact-match (`service.name="checkout"`). Regex is a power
  feature that can land behind the SAME parser once `=`/`!=` are solid and tested.
- This feature (US-08) explicitly REJECTS regex with an honest 400, so the boundary is
  executable and the deferral is visible to the operator.

## Trigger to schedule
When operators report needing pattern filters (e.g. all `route=~"/api/.*"`), or when the
`=`/`!=` slice is shipped and stable.

## Estimated size
~1-2 stories, 1-2 days. Adds the regex operators to the parser, a regex engine, the
full-anchor match, and the regex absent-label/empty matrix.
