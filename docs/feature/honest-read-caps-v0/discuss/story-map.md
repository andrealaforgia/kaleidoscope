# Story Map: honest-read-caps-v0

British English. No em dashes. No emoji.

## User

Maya Kowalski and Idris Mbeki - the platform operator and on-call
SRE for tenant "acme-prod" - sending requests to the three
Kaleidoscope read APIs (`query-api`, `log-query-api`,
`trace-query-api`) from a Grafana dashboard, a curl on the command
line, or a Prism panel. A misconfigured client (Hands-off Hannah's
hand-edited dashboard) and an attacker probing the surface share
the same shape: they all hit the same three endpoints.

## Goal

When a request asks for too wide a window or would yield too many
rows, the platform refuses with a named 400 BEFORE the store is
touched (window cap) or BEFORE serialisation (result cap), instead
of saturating the listener or driving the process to OOM. The 400
envelope is the SAME shape the existing matcher and inverted-bounds
400s already use:
`{status:"error", error:"<names the breached cap>"}`. Honest caps
close the S13 self-DoS surface for all three pillars in one slice.

## Backbone

The user activities run left-to-right across one request:

| Send request | Parse and validate request | Query store | Serialise and return |
|---|---|---|---|
| Maya / Idris sends `GET /api/v1/query_range`, `GET /api/v1/logs`, or `GET /api/v1/traces` with `start`, `end`, and (on traces) `service` | The handler validates `service` (traces only), parses `start` / `end`, AND CHECKS THE WINDOW CAP. Over the cap is a 400 BEFORE the store. | The store query runs only when the window cap passes; the store returns the in-window rows / records / spans. | The handler CHECKS THE RESULT CAP. Over the cap is a 400 BEFORE serialisation. Within the cap is a 200 with the existing envelope. |

Each backbone column is one user activity. The walking skeleton is
the minimum slice across all four columns that delivers an honest
refusal for over-window OR over-result requests across all three
read APIs.

## Walking skeleton (slice 01)

The thinnest end-to-end slice that connects all four backbone
activities, across all three crates:

- **Send request**: an acceptance-test fixture sends a request to
  each of the three existing endpoints via the tower `oneshot`
  pattern each crate already uses. No new HTTP path, no new query
  parameter.
- **Parse and validate request**: each handler gains a compile-time
  constant `MAX_WINDOW_SECONDS` and a window-cap check that lives
  between `parse_time_range` and `state.store.query(...)`. Over the
  cap returns 400 with the named envelope; the store is NOT called.
- **Query store**: unchanged. The store is called only when the
  window cap passes. No store trait change.
- **Serialise and return**: each handler gains a compile-time
  constant `MAX_RESULT_ROWS` and a result-cap check that lives
  between the store response and `success_response(...)`. Over the
  cap returns 400 with the named envelope; serialisation is NOT
  attempted on the rejected result.

This is the thinnest slice that connects all four backbone
activities for all three pillars in one wave. Every later slice
extends column 2 / column 4 (env-driven configurability, per-pillar
tuning) without changing the shape of column 1 (the existing
endpoints) or column 3 (the unchanged store traits).

## Slice plan

### Slice 01 (this feature wave): walking skeleton, ALL THREE crates, compile-time constants

Stories (all P1-P2 inside the same atomic slice; see Priority
Rationale below):

- **US-01** (P1): a year-long window to `/api/v1/query_range` is
  refused with a named 400 before pulse is touched.
- **US-02** (P1, atomic with US-01): a year-long window to
  `/api/v1/logs` is refused with a named 400 before lumen is touched.
- **US-03** (P1, atomic with US-01): a year-long window to
  `/api/v1/traces` is refused with a named 400 before ray is
  touched, even though `service` is already validated.
- **US-04** (P2, atomic with US-01 through US-03): a response that
  would exceed the result-size cap is refused with a named 400, not
  silently truncated, across all three endpoints.
- **US-05** (P2, atomic with the previous four): the cap 400 body
  never echoes the requested window, the raw query, the raw
  pattern, the raw `service`, or a forwarded Authorization header,
  on any of the three crates.

Outcome KPI: every over-window or over-result request in the
acceptance suite returns the named 400 with no store call (window
cap) or no truncation (result cap), and every within-cap request
succeeds, on all three crates. See `outcome-kpis.md`.

### Slice 02 (deferred, named OUT in `wave-decisions.md`)

Lift the cap values from compile-time constants to env-driven
configurability per crate (e.g.
`KALEIDOSCOPE_QUERY_MAX_WINDOW_SECONDS`,
`KALEIDOSCOPE_LOG_QUERY_MAX_WINDOW_SECONDS`,
`KALEIDOSCOPE_TRACE_QUERY_MAX_WINDOW_SECONDS`,
`KALEIDOSCOPE_QUERY_MAX_RESULTS`, etc.), mirroring the existing
env-driven `TENANT` / `ADDR` posture each `composition.rs` already
uses. Cost: small; one resolver per crate, exact env name TBD by
DESIGN.

### Slice 03 (deferred, named OUT)

Per-pillar tuning of the cap values once measurements support it
(e.g. `pulse` may tolerate a wider window than `lumen`;
`trace-query-api` already narrows by `service` so its window can
likely be wider). Awaiting real-world data the v0/v1 platform does
not yet have.

### Slice 04 (deferred, future feature)

`query-http-common` extraction (M-5 / ADR-0048 Decision 5) is a
SEPARATE future feature, not a slice of this one. Once that crate
exists, the cap pattern is the natural first thing to live there.

## Priority Rationale

Priority order:

1. **US-01, US-02, US-03 (all P1, atomic with each other)**. The
   roadmap explicitly bundles the three crates into one feature
   ("the three crates share the cap pattern even though they keep
   their own time-range types, which is why this is one feature,
   not three separate slices"). Splitting them across slices would
   leave the S13 surface partially closed; the residuality analysis
   is clear that the gap exists in ALL THREE columns of the
   incidence matrix (QM, QL, QT).
2. **US-04 (P2, atomic with US-01-US-03)**. The window cap closes
   the EASY half of S13 (the over-wide-window self-DoS). The result
   cap closes the residual half (the cardinality-bomb-at-read-side
   self-DoS, S04 / S14 amplification). Without US-04, a
   narrow-window-but-wide-fan-out query still saturates the read
   path. US-04 is P2 only because it depends on the cap-pattern
   plumbing US-01-US-03 establish; it lands in the same slice for
   the same reason.
3. **US-05 (P2, atomic with the previous four)**. The redaction
   tests are the residue against A-U3 ("Header echo in error
   bodies") at the cap 400. The new error reasons need explicit
   redaction tests or A-U3 is left to drift on the next reason
   refinement. US-05 is P2 because the test code is small but
   load-bearing; it lands in the same slice because the cap reasons
   it asserts on are the ones US-01-US-04 introduce.

Dependencies:

- All five stories land in the SAME slice (slice 01). The per-crate
  mutation gate evaluates the whole crate after the change, not
  story-by-story; splitting the stories across slices would force
  multiple mutation runs to converge on 100 percent kill.
- All five stories depend on DESIGN resolving FLAG 1 (window cap
  value), FLAG 2 (result cap value), FLAG 3 (REFUSE vs TRUNCATE on
  result cap), and FLAG 4 (new ADR-0050 vs amend ADR-0042 / 0047 /
  0048) before DISTILL writes acceptance tests.
- US-04 depends conceptually on US-01-US-03 having established the
  per-crate constant module and the cap-check pattern; in practice
  the two caps are implemented in the same PR.
- US-05 depends on US-01-US-04 because the redaction tests assert
  on the cap 400 reasons those four stories introduce.

## Scope Assessment

PASS - 5 stories, 3 crates, estimated 1 day total.

The residuality follow-up roadmap explicitly carves this as ONE of
three numbered features ("the three crates share the cap pattern
even though they keep their own time-range types, which is why this
is one feature, not three separate slices"). The carpaccio slicing
rule is honoured: the three crates ship the same cap pattern in one
slice, with later slices reserved for env-driven configurability
and per-pillar tuning.

Oversize-check (any 2+ of):

- >10 user stories: NO (5).
- >3 bounded contexts or modules: edge case; the work spans 3
  crates (`query-api`, `log-query-api`, `trace-query-api`) which IS
  the boundary the residuality analysis bundles into one feature on
  purpose. The roadmap pre-authorises this grouping. Not oversized
  by the spirit of the rule (one cap pattern, three identical
  applications).
- Walking skeleton requires >5 integration points: NO (each crate
  is its own listener; no integration between them).
- Estimated effort >2 weeks: NO (~1 day).
- Multiple independent user outcomes that could ship separately:
  NO (the user outcome is "honest cap refusal on the read APIs";
  shipping it on one crate but not the others leaves the surface
  open and the roadmap rejected that split explicitly).

Carpaccio taste-tests (see `wave-decisions.md`):

1. Window over the cap refuses BEFORE the store (one scenario per
   crate, three total).
2. Result over the cap refuses AT serialisation (one scenario per
   crate, three total).
3. Redaction on cap refusal (one scenario per crate, three total).

Nine independently demonstrable behaviours, three per crate; one
slice; right-sized.
