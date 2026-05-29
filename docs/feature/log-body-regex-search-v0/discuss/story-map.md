# Story Map: log-body-regex-search-v0

Brownfield carpaccio slice. The backbone is the existing log read
endpoint `GET /api/v1/logs`; this slice grows one optional
parameter and the predicate seam that supports it.

## Backbone

The user activity is "an SRE narrows the log read response by a
regex pattern matching the body field of `LogRecord`". Four
activities sequence from request arrival to response serialisation:

```text
Parse  ----->  Compile  ----->  Wire  ----->  Verify
 |              |               |              |
parse         compile         dispatch       acceptance
body_regex    Regex on        through        + mutation
helper        parse;          query_with     gate via
in handler    one             when filter    Predicate::matches
              compile         present
              per request
```

| Activity | What happens | Owner of the change |
|---|---|---|
| **Parse** | The handler extracts `body_regex: Option<String>` from `LogsParams`. Empty value / over-cap value / invalid syntax all return 400 with literal `invalid body_regex`. | `crates/log-query-api/src/lib.rs` — one new free fn `parse_body_regex`, one new field on `LogsParams`. |
| **Compile** | A non-empty in-cap value is compiled via `Regex::new`. Compile failure is the third 400 arm of `parse_body_regex`. | `crates/log-query-api/src/lib.rs` — same free fn. |
| **Wire** | A successful parse becomes `Predicate::body_regex(regex)`; the dispatch builds the composed predicate and calls `query_with`. The 6-arm dispatch is pruned by the mutual-exclusion check (steps 7) so only 6 of the 8 theoretical arms are reachable. | `crates/log-query-api/src/lib.rs` — extended dispatch; `crates/lumen/src/predicate.rs` — one new field, one new builder, one new `matches` arm, one new `is_empty` clause. |
| **Verify** | The acceptance suite drives all eight UAT scenarios; the `gate-5-mutants-lumen` workflow exercises the new `Predicate::matches` arm at the 100% kill-rate gate. | `crates/log-query-api/tests/slice_01_body_regex.rs` (DISTILL output), `gate-5-mutants-lumen` workflow (already shipped). |

## Walking Skeleton

**Decision 2 (Walking Skeleton): No.** This slice rides on the
walking skeleton that
`log-query-api-v0` / `log-query-api-caps-v0` / `log-query-severity-filter-v0` /
`log-body-text-search-v0` already shipped. The read endpoint,
durable store, tenant seam, caps, severity filter, and substring
filter are all live; no greenfield skeleton is rebuilt.

The closest analogue to a walking-skeleton story for this slice is
US-01 (a known pattern matches the failure family). It exercises
every activity in the backbone end-to-end: parse a pattern, compile
it, wire it through `query_with`, and verify the response carries
only matching records. Every other story in the slice is a
declared-corner / boundary case attached to the same backbone.

## Stories (7 thin carpaccio)

| # | Story | Activity covered | Scenario count |
|---|---|---|---|
| US-01 | A known regex pattern matches a family of variations (the "walking skeleton" of this slice) | Parse + Compile + Wire + Verify | 1 happy path |
| US-02 | An unknown pattern returns the calm empty array (never 404) | Wire + Verify (calm-empty arm) | 1 calm-empty |
| US-03 | Missing `body_regex` preserves today's behaviour (no-regression) | Wire (default arm) | 1 backward-compat |
| US-04a | Invalid regex syntax is a redacted 400 | Parse + Compile (compile failure) | 1 reject |
| US-04b | Empty `body_regex` is the same redacted 400 | Parse (empty rejection) | 1 reject |
| US-04c | Over-cap (>1024 bytes) `body_regex` is the same redacted 400 | Parse (length-cap rejection) | 1 reject |
| US-05 | Case-sensitive matching is pinned by acceptance test | Compile + Verify (case-sensitivity pin) | 1 acceptance pin |
| US-06 | `body_contains` and `body_regex` are mutually exclusive at slice 01 | Parse + Wire (mutual-exclusion 400) | 1 reject |
| US-07 | Cross-tenant isolation holds for `body_regex` | Wire + Verify (tenant scope) | 1 invariant pin |

Total: 7 stories (US-04 split into a, b, c for the three distinct
rejection arms — they share an envelope but pin different
boundaries), 9 UAT scenarios in US-01 with the per-story scenarios
attached. Story count is at the carpaccio sweet spot (3-7 per
slice; 7 here, each a distinct user-observable behaviour).

## Slice brief

```text
Slice name        log-body-regex-search-v0
Slice goal        Grow GET /api/v1/logs with an optional body_regex
                  parameter that filters records whose body field is
                  matched by a regular expression, with the same
                  envelope, redaction, cap, and tenant invariants as
                  body_contains.

Learning hypothesis
  Disproves if it fails:
    The `regex` crate's grammar (already proven in query-api's
    metric-label matchers) is NOT the right grammar for the body
    filter. Operators reject the syntax or hit ReDoS-like behaviour
    on real fixtures. Falsified iff the acceptance suite or a real
    operator complains about the grammar within one incident cycle.
  Confirms if it succeeds:
    Maria's mid-incident query "give me every shape of kafka
    timeout in this window" is served in one request, not three.
    The `gate-5-mutants-lumen` gate (shipped at d96a807) catches the
    boundary mutants on the new Predicate arm at 100%.

IN scope
  - body_regex=<pattern> on GET /api/v1/logs
  - Compile-once-per-request via Regex::new in parse helper
  - 1024-byte length cap (inclusive)
  - Empty / over-cap / invalid-syntax all -> 400 with
    "invalid body_regex" literal
  - Mutual exclusion vs body_contains -> 400 with
    "specify body_regex or body_contains, not both" literal
  - Composition with min_severity (conjunctive AND)
  - Cross-tenant isolation (preserved invariant)
  - Predicate::body_regex(regex) builder + body_regex field +
    matches arm + is_empty clause
  - ADR-0056 documenting the lumen surface growth and the new
    regex direct dependency

OUT of scope
  - Any alternative regex backend (PCRE, etc.)
  - Multi-field matching (body OR attributes)
  - Multiple regexes in one request
  - A per-regex result cap distinct from MAX_RESULT_ROWS
  - A regex-compile cache across requests
  - Case-folding default (operators use `(?i)` inline)
  - Combining body_contains AND body_regex (deferred to a
    future slice)

Production data
  Fixture records use realistic operational shapes
  ("kafka timeout connecting to broker-3",
  "kafka request timed out after 30s on topic orders",
  "kafka: connection timed out (broker-7)"). The Maria/Marcus/Priya
  personas mirror the operator vocabulary used in the
  body_contains slice.

Dogfood moment
  Once shipped, Andrea / any contributor running the
  kaleidoscope-cli with --observe-otlp <path> can query the
  workspace's own logs ingested via the OTLP-JSON sink with a
  body_regex pattern and see the same family-aware narrowing
  that an external SRE would see on acme-prod.

Reference class
  Prior carpaccio slices on /api/v1/logs:
    - log-query-severity-filter-v0 (ADR-0052) — first optional
      parameter on this route; closed.
    - log-body-text-search-v0 (ADR-0055, commit 1bfa609) — second
      optional parameter; closed.
  Effort shape is parallel to both. Predicate field count grows by
  one; handler dispatch grows by one arm (pruned to 6 reachable
  arms by the mutual-exclusion check).

Estimated effort
  - DESIGN wave: ~half a day (pin 6 flags, write ADR-0056, write
    application-architecture and parse-helper specs).
  - DISTILL wave: ~half a day (8 UAT scenarios in Gherkin; parser
    unit-test surface).
  - DELIVER wave: ~1 day (one Predicate field + builder + matches
    arm + is_empty clause; one parse helper; extended dispatch;
    Cargo.toml dep add on lumen; ADR-0056 cross-references).
  Total: 2 days end-to-end. Right-sized.

Pre-slice SPIKE
  None required. The regex grammar is proven in query-api; the
  predicate seam is proven in body_contains; the mutants gate is
  proven in gate-5-mutants-lumen-v0.
```

## Priority Rationale

This slice ships as a single coherent unit; the 7 stories are
boundaries on the same parameter, not independent deliverables.
Within the slice the implementation order is dictated by the
backbone:

1. **Parse** (US-04a + US-04b + US-04c first) — every other story
   depends on `parse_body_regex` returning a valid `Regex` on the
   happy path. Land the three rejection arms first because they
   are the cheapest, the most observable, and the easiest to
   mutation-test.
2. **Compile + Wire** (US-01 + US-03 next) — the happy path and
   the no-regression default. US-01 forces every backbone activity
   to fire end-to-end and is the first scenario where a wrong
   `Predicate::matches` arm becomes user-observable.
3. **Compose + isolate** (US-02 + US-05 + US-06 + US-07) — the
   composition and invariant pins. These ride on the parse +
   compile + wire path; failure here means a behavioural drift,
   not a structural break.

Outcome impact is concentrated in US-01: it is the only story that
delivers a new capability operators can observe. US-02 through
US-07 are the boundary pins that make the new capability honest
under failure / no-op / collision / multi-tenant load. None of the
boundary pins can be deferred without leaving a hole the
acceptance suite must catch; all 7 ship together as one slice.

Dependency order in the source:

```text
Predicate::body_regex field + builder (lumen)
   |
   v
Predicate::matches new arm + is_empty clause (lumen)
   |
   v
parse_body_regex helper (log-query-api)
   |
   v
LogsParams.body_regex field (log-query-api)
   |
   v
handle_logs dispatch + mutual-exclusion check (log-query-api)
   |
   v
acceptance suite + mutants gate (gate-5-mutants-lumen workflow)
```

## Scope Assessment: PASS — 7 stories, 2 crates, estimated 2 days

The slice ships 7 user stories across 2 crates (`log-query-api` and
`lumen`), with the only structural growth being one Predicate field
+ builder + matches arm + is_empty clause and one parse helper +
dispatch arm. The shape is identical to the immediate predecessor
(`log-body-text-search-v0`), which closed at commit 1bfa609 in the
same effort envelope. No oversized signals apply:

- 7 stories (within the 3-7 sweet spot).
- 2 bounded contexts touched (`log-query-api`, `lumen`); both
  already collaborate.
- Walking skeleton is implicit in the existing endpoint; 0 new
  integration points.
- Estimated effort 2 days; within the 1-3 day right-sized window.
- Single user outcome ("narrow log reads by regex"); no separable
  independent deliverables.

Right-sized as one slice. No split proposed.
