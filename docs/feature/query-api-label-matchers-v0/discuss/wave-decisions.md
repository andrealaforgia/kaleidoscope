# Wave Decisions: query-api-label-matchers-v0 (DISCUSS)

British English. No em dashes.

## Configuration

| Decision | Value | Note |
|----------|-------|------|
| Feature type | Backend | No human-facing UI surface of its own; the operator-visible surface is Prism rendering the matrix. |
| Walking skeleton | No | This feature extends an already-shipped end-to-end loop (`query-range-api-v0`). The skeleton (request -> tenant -> parse -> query -> matrix) already exists; this slice thickens the parse + filter rib. |
| UX research depth | Lightweight | The contract and the operator persona were validated in `query-range-api-v0`; this slice reuses them. |
| JTBD | No | Job grounding is inherited from ADR-0042 and the predecessor feature; no fresh JTBD run. |

## Discovery grounding (DIVERGE absent)

No DIVERGE artifacts exist at `docs/feature/query-api-label-matchers-v0/diverge/`.
This is expected: the feature is the explicitly anticipated continuation of
`query-range-api-v0`. ADR-0042 Decision 3 states verbatim: "Slice 02 adds a single
`{label="value"}` matcher behind the same parser." Journey work is therefore grounded in:

- ADR-0042 (the parser/subset and the 400-not-lie discipline).
- The predecessor feature's persona (Sara Okafor, on-call SRE for "checkout"; tenant
  "acme-prod"), pinned contract, and redaction posture.

Risk noted: no independent JTBD validation of "label filtering during an incident" as a
job. Mitigation: the job is self-evident from the elevator pitch and the operator's
incident workflow, and is corroborated by ADR-0042 and Prometheus' own ubiquity. LOW.

## Verified-against-code facts

1. **Current selector grammar** (`crates/query-api/src/selector.rs`): after trimming
   surrounding ASCII whitespace the WHOLE query must match the Prometheus metric-name
   production `[a-zA-Z_:][a-zA-Z0-9_:]*`. Anything else (including any `{`) is rejected
   with a single honest 400 reason that NEVER echoes the raw query (DD6 redaction).
   `parse(raw) -> Result<MetricName, String>`.

2. **How matchers extend it**: the selector parser must now accept the form
   `name{ matcher_list }` where `name` is the existing bare-name production and
   `matcher_list` is zero or more comma-separated matchers `label_name OP "value"` with
   `OP` in `{=, !=}`. The bare-name-only form remains valid (empty/absent brace section).
   The metric name still selects the metric via Pulse's
   `query(&tenant, &MetricName, range)`; the OTHER matchers filter the translated result.

3. **Label-set derivation** (`crates/query-api/src/matrix.rs`, `merge_labels`): each
   point's label set is `metric.resource_attributes UNION point.attributes UNION
   {__name__: metric.name}`, point attributes winning over resource attributes on a
   clash, `__name__` always authoritative. Matchers filter on THIS derived set, applied
   to each `(Metric, MetricPoint)` row before `to_matrix` grouping.

4. **Prism sends `{...}` as-is** (`apps/prism/src/lib/promql/queryRange.ts`, `buildUrl`):
   `params = new URLSearchParams({ query: request.q, ... })`. The raw query string,
   including any `{...}` matcher section, is URL-encoded verbatim into the `query`
   parameter. No client-side PromQL parsing. So the backend receives the full selector
   text and owns all parsing. Confirmed.

5. **Labels come from** (`crates/pulse/src/metric.rs`): `Metric.resource_attributes`
   (e.g. `service.name`) and `MetricPoint.attributes` (e.g. `http.route`,
   `tenant.id`). Both are `BTreeMap<String, String>`. Label names with dots
   (`tenant.id`) are legal map keys, so a matcher like `tenant.id!="x"` filters on a
   point/resource attribute key literally named `tenant.id`.

## Matcher semantics pinned (the correctness-critical part)

Prometheus label-matching semantics, applied to the derived label set:

- **`label="value"`** (equality): matches a series iff the label is PRESENT and its value
  equals `value`. SPECIAL CASE: if `value` is the empty string `""`, then `label=""`
  matches a series where the label is ABSENT (or present and empty). So an absent label
  satisfies `=` ONLY when the matcher value is `""`.

- **`label!="value"`** (inequality): matches a series iff the label is ABSENT, OR present
  with a value DIFFERENT from `value`. SPECIAL CASE: `label!=""` matches a series where
  the label is PRESENT and non-empty (it excludes absent/empty). A present label equal to
  a non-empty `value` FAILS `!=`.

- **Multiple matchers are ANDed**: a series is kept iff it satisfies EVERY matcher.

- **`__name__` is matchable**: because `merge_labels` always inserts `__name__`, a matcher
  on `__name__` is honoured against the derived set. Slice 01 selects the metric by the
  bare name; `{__name__="..."}`-form selection is briefed as a possible slice 02 but kept
  OUT of slice 01 (see slices/).

These four lines are the subtle, regression-prone heart of the feature. Every one has a
dedicated UAT scenario and acceptance criterion.

## Scope boundary held

- IN: `=` and `!=` matchers, multiple ANDed, on the existing bare-name selector.
- OUT (return honest 400, never a plausible wrong answer): regex `=~` `!~`, functions,
  aggregations, operators, the instant `/api/v1/query` endpoint, range vectors. Unchanged
  unsupported forms keep the existing 400 behaviour.

## Deferrals recorded

- Regex matchers `=~` `!~` (briefed as slice 02b, OUT of this feature).
- `{__name__="x"}`-form metric selection (briefed as slice 02a, OUT of this feature).
- Instant endpoint, full PromQL, range vectors: unchanged from ADR-0042, still v1.

## DEVOPS flag

`gate-5-mutants-query-api` (ADR-0042 Verification; CLAUDE.md mutation strategy) already
covers `crates/query-api/src/` via `--in-diff`. The new matcher parse + filter logic
lands in the same scope and is covered without a new gate. Flagged to platform-architect.

## Peer review

Reviewer: `nw-product-owner-reviewer`. One revision permitted on rejection. See
`peer-review.md`.
