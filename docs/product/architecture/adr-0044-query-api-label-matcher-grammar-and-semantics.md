# ADR-0044 — query-api label-matcher grammar and Prometheus semantics

- **Status**: Accepted
- **Date**: 2026-05-21
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `query-api-label-matchers-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Refines**: ADR-0042 Decision 3 (the minimal PromQL subset). ADR-0042
  Decision 3 anticipated this verbatim: "Slice 02 adds a single
  `{label="value"}` matcher behind the same parser." This ADR realises that
  anticipated extension for the `=` and `!=` operators.
- **Related**: ADR-0027 (Prism backend HTTP client; the consumer of the
  unchanged response envelope), ADR-0042 (the contract this extends).

## Context

ADR-0042 shipped a read API whose selector parser accepts only a bare metric
name and rejects any `{` with an honest 400. Decision 3 of that ADR explicitly
deferred a single `{label="value"}` matcher to a later slice. This feature
delivers the `=` and `!=` label matchers on the existing bare-name selector so
an on-call operator can filter a noisy metric to the series that matter,
server-side, during an incident.

ADRs in this repository are immutable (superseded, never edited). ADR-0042 is
Accepted and referenced. Rather than mutate it, this ADR is a separate,
referenceable record that REFINES its Decision 3, leaving the original intact.
A new ADR was chosen over an in-place edit for that immutability reason; the
back-reference keeps the subset contract discoverable as a two-document set
(0042 the frame, 0044 the matcher refinement). ADR-0044 is the next free number
(the highest existing was 0043).

Two things in this slice are non-obvious and correctness-critical, so this ADR
documents them explicitly: the Prometheus absent-label matcher semantics, and a
deliberate divergence from strict PromQL label-name grammar to accommodate
OTel-shaped dotted label keys.

## Decision

### 1. The matcher grammar extends the bare-name production

After trimming surrounding ASCII whitespace, the selector is
`metric_name [ "{" matcher_list "}" ]` where:

- `metric_name` is the unchanged bare-name production `[a-zA-Z_:][a-zA-Z0-9_:]*`
  and still selects the metric via `pulse.query(&tenant, &MetricName, range)`.
- `matcher_list` is zero or more comma-separated matchers (a single trailing
  comma tolerated), each `label_name op string_literal`.
- `op` is `=` or `!=` ONLY.
- `string_literal` is double-quoted with the minimal escape set `\"`, `\\`,
  `\n`, `\t`; an empty value `""` is valid and load-bearing; an unknown escape
  or an unquoted value is a malformed 400.

The parser returns `Selector { name: MetricName, matchers: Vec<LabelMatcher> }`
where `LabelMatcher { name: String, op: MatchOp, value: String }` and
`MatchOp` is `Equal | NotEqual`. The bare-name form (and the empty `name{}`
form) yields an empty matcher list and behaves exactly as before. The metric
name drives the store query; the matchers filter the result.

### 2. Label names in matchers MAY contain dots (deliberate PromQL divergence)

Strict Prometheus label names are `[a-zA-Z_][a-zA-Z0-9_]*` (no dot). Kaleidoscope
deliberately extends this to `[a-zA-Z_:][a-zA-Z0-9_:.]*` (the metric-name start
class plus a continuation class that adds `.`). Justification: the derived label
set is OTel-shaped. Resource attributes (`service.name`) and point attributes
(`http.route`, `tenant.id`) carry dotted keys (`crates/pulse/src/metric.rs`,
`BTreeMap<String,String>`). The headline use case is literally
`{service.name="checkout"}`. A label-name production that forbade dots could not
name the keys it must filter on. The divergence is forward-compatible: strict
PromQL names are a strict subset of the accepted set, so a future tightening
would only reject inputs currently accepted, never the reverse. A leading dot is
still rejected (the start class is unchanged).

### 3. Prometheus matcher semantics on the derived label set

Filtering is applied to each `(Metric, MetricPoint)` row's derived label set
(`resource_attributes` then `point.attributes` winning, then authoritative
`__name__`; the same derivation `to_matrix` groups on), BEFORE grouping.

Treating an absent label as the empty string gives exactly the Prometheus rules:

- `label="value"` (non-empty): keep iff label present and equal.
- `label=""`: keep iff label absent OR present-and-empty.
- `label!="value"` (non-empty): keep iff label absent OR present-and-different.
- `label!=""`: keep iff label present and non-empty.

Multiple matchers are ANDed: a row is kept iff it satisfies every matcher. The
filter predicate is a pure function (`matrix::keep_row` / `matrix::matches`),
unit- and mutation-tested in isolation. An all-excluded result is the calm
success arm `result: []` at HTTP 200, never an error.

### 4. Honest 400 for regex and malformed matchers (the ADR-0042 discipline)

Any operator other than `=`/`!=` (notably regex `=~`, `!~`), an unterminated
brace, an unquoted value, an empty label name, a bad escape, or trailing junk
returns HTTP 400 `{status:error, error:'<reason>'}`. A malformed brace section
is NEVER silently degraded to a bare-name query or a partial filter. The reason
names what is unsupported and what is accepted (`=`/`!=`), and never echoes the
raw query or a forwarded header value (DD6 redaction; ADR-0027 section 6). The
response envelope is unchanged: success/empty satisfy Prism's `isPromSuccess`,
the 400 satisfies `isPromError`.

## Alternatives considered

### Recording A (rejected): edit ADR-0042 in place

Add the matcher grammar directly into ADR-0042 Decision 3. For: one document for
the whole subset. Against: ADRs here are immutable once Accepted and ADR-0042 is
referenced by ADR-0027 and ADR-0043; mutating an Accepted, cited decision
breaks the immutability convention and the audit trail. Rejected; a refining ADR
with a back-reference preserves immutability while keeping the contract
discoverable.

### Label-name grammar A (rejected): strict PromQL names (no dots)

Keep `[a-zA-Z_][a-zA-Z0-9_]*` and forbid dotted label names. For: strict PromQL
fidelity. Against: the derived label set is OTel-shaped with dotted keys
(`service.name`); a strict grammar could not express the headline filter
`{service.name="checkout"}` at all. Rejected; the OTel shape of the data dictates
the grammar, and the divergence is documented and forward-compatible.

### Value-quoting A (rejected): accept unquoted values

Parse `{service.name=checkout}` by reading to the next `,` or `}`. For: fewer
keystrokes. Against: it diverges from PromQL (which requires quotes), is
ambiguous around whitespace and special characters, and a typo would parse as a
silent wrong filter rather than an honest rejection. Rejected; require quotes
and reject the unquoted form with a 400 (US-08).

### Semantics A (rejected): absent label fails every matcher

Treat an absent label as never matching (so `label=""` and `label!="x"` would
both exclude an absent-label series). For: a naive "missing means no match"
intuition. Against: it is wrong against Prometheus and would silently drop series
an operator expects to keep (the US-07 absent-label-keep case), the worst kind of
mid-incident lie. Rejected; the absent-as-empty rule above is the Prometheus
behaviour and is pinned by dedicated tests.

## Consequences

### Positive

- **Correct by an explicit oracle.** The four-arm semantics matrix is documented
  here and pinned by per-arm tests; the mutation gate protects the equality and
  inequality boundaries.
- **Honest scope boundary.** Regex and malformed matchers are tested 400s, never
  silent partial filters; the empty result is a calm success arm.
- **OTel-shaped filtering works.** Dotted label names let operators filter on
  `service.name`, the actual data shape, with a documented, forward-compatible
  divergence.
- **Zero contract drift.** The response envelope is byte-shape-unchanged; Prism's
  pinned validators still accept every arm.
- **Pure reuse.** No new crate, no new dependency (hand-rolled parsing, no regex
  crate since regex matchers are out of scope), no new HTTP surface; the change
  is two files in an existing crate.

### Negative

- **A divergence from strict PromQL to hold.** Dotted label names are accepted
  where Prometheus would reject them. Mitigated: documented here, forward-compatible
  (a superset), and justified by the OTel data shape.
- **A second document for the subset contract.** The subset is now ADR-0042 +
  ADR-0044. Mitigated: the explicit "Refines" back-reference makes the pair
  discoverable; this is the cost of ADR immutability.
- **Absent-label semantics are a convention to hold.** The absent-as-empty rule
  is subtle and regression-prone. Mitigated: each arm has a dedicated DISCUSS UAT
  scenario and the mutation gate covers the predicate.

### Trade-off summary

The slice trades full PromQL matcher fidelity (regex, the `{__name__="x"}`
selection form, single quotes) for an honest, correct, shippable `=`/`!=` slice
now, with every deferral recorded. It diverges from strict PromQL label-name
grammar where, and only where, the OTel-shaped data requires it.

## External-integration handoff

No new external integration. The Prism contract boundary
(`/api/v1/query_range`) recorded in ADR-0042's handoff is unchanged: the
response envelope (success, empty, 400 parse-error, 5xx) still satisfies Prism's
`isPromSuccess` / `isPromError`. The matcher feature changes only WHICH series
the success arm carries, never the envelope, so the existing consumer-driven
contract posture (Apex's choice of Pact-JS or container fixtures) covers it
without a new contract.

## Verification

- Per-arm acceptance tests for the four matcher semantics (present-equal,
  empty-matches-absent, absent-keeps-`!=`, `!=""`-keeps-present-non-empty) and
  the AND composition (US-06, US-07).
- One acceptance test per rejected form (regex, unterminated brace, unquoted
  value, empty label name) and a redaction test that the 400 never echoes the
  raw query or a forwarded header value (US-08, DD6).
- A bare-name regression test (no brace section) confirming slice-01 behaviour
  is unchanged, and an empty-filtered-result test confirming the calm `result:[]`
  success arm.
- A contract test that the success, empty, and 400 shapes still pass Prism's
  `isPromSuccess` / `isPromError` (KPI 4, envelope unchanged).
- Mutation testing: `cargo mutants` scoped to `crates/query-api/src/` via
  `--in-diff` at the project 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md).
  Covered by the existing `gate-5-mutants-query-api`; no new gate. The four
  matcher boundary arms are the primary mutation targets.
- No Earned-Trust probe change: the new logic is pure and in-process with no
  external substrate; the ADR-0042 Decision 8 startup probe and its
  three-orthogonal-layer enforcement are unchanged.
