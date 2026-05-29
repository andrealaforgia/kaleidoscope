# DoR Validation: log-body-regex-search-v0

The 9-item Definition of Ready hard gate. Each item passes with
explicit evidence; failure on any one blocks the handoff to
DESIGN.

## Item 1 — Problem statement clear, domain language

**Status**: PASSED.

**Evidence**: `user-stories.md` § US-01 "Problem" describes Maria
Santos's mid-incident task in domain language:

> A platform incident produces error messages in several
> closely-related but distinct shapes (the kafka client library
> and the application code each emit a different sentence for
> the same underlying network failure). Today's `body_contains`
> filter [...] restricts the response to records whose `body`
> field contains an exact byte substring. To isolate every
> variation of "kafka timeout" Maria runs three or four separate
> `body_contains` queries and reconciles the results [...]

The problem is operational ("isolate every shape of a failure
family in one query"), not technical ("add a regex matcher").
The domain vocabulary ("on-call SRE", "paging alert", "tenant",
"in-window record") is consistent with the sibling slices
(`log-query-severity-filter-v0`, `log-body-text-search-v0`).

## Item 2 — User / persona with specific characteristics

**Status**: PASSED.

**Evidence**: `user-stories.md` § US-01 "Who" names three
specific personas with concrete contexts and motives:

- **Maria Santos** — SRE on `acme-prod`, mid-incident, terminal
  + curl + jq, triage urgency motive.
- **Marcus Webb** — platform engineer building an automated
  incident classifier polling every 60 seconds, throughput motive.
- **Priya Raman** — support engineer triaging a customer ticket
  with a slightly different message shape, correctness motive.

Each persona has (a) a role, (b) a concrete tenant context, (c) a
toolchain, (d) a distinct motive that explains why the slice
matters to them.

## Item 3 — 3+ domain examples with real data

**Status**: PASSED.

**Evidence**: `user-stories.md` § US-01 "Domain Examples" lists
seven concrete examples (1, 2, 3, 4a, 4b, 4c, 5, 6, 7 — nine
counting splits), each with real-data fixtures:

1. Happy path with eight tenant records, three matching, real
   `observed_time_unix_nano` values, real `body` strings
   (`"kafka timeout connecting to broker-3"`,
   `"kafka request timed out after 30s on topic orders"`,
   `"kafka: connection timed out (broker-7)"`).
2. Calm-empty against a `cassandra.*timeout` pattern.
3. Default unchanged on Marcus's polling script.
4a. Invalid syntax: `body_regex=foo(bar`.
4b. Empty: `body_regex=`.
4c. Over-cap: 1025-byte payload.
5. Case-sensitive: `body_regex=kafka` vs `KAFKA timeout`.
6. Mutual exclusion: both `body_contains` AND `body_regex`.
7. Cross-tenant isolation: `globex-staging` queries
   `acme-prod`-shaped data.

The fixtures use real operator vocabulary (kafka, redis,
checkout, broker-3) and real tenant identifiers consistent with
the sibling slices.

## Item 4 — UAT in Given/When/Then (3-7 scenarios)

**Status**: PASSED.

**Evidence**: `user-stories.md` § US-01 "UAT Scenarios" carries
nine Gherkin scenarios, each in strict Given/When/Then form:

1. A known pattern matches all shapes of the failure family
   (happy path).
2. An unmatched pattern returns the calm empty array, never 404.
3. Parameter absent returns every record in the window.
4. An invalid regex pattern is a redacted 400.
5. An empty body_regex value is the same redacted 400.
6. An over-cap body_regex value is the same redacted 400.
7. The match is case-sensitive by default.
8. Body_contains and body_regex are mutually exclusive.
9. Cross-tenant isolation.

Count is 9, slightly above the 3-7 sweet spot. Split rationale:
the three redaction-400 scenarios (4, 5, 6) pin three distinct
boundary mutants in one envelope; merging them would reduce the
acceptance surface visibility (a reader could not tell which
mutant a green merged scenario kills). The split is deliberate
and earns the count.

## Item 5 — AC derived from UAT

**Status**: PASSED.

**Evidence**: `user-stories.md` § US-01 "Acceptance Criteria"
lists 17 checkbox items, each traceable to a Gherkin scenario or
to a constraint pin:

| AC item | Source scenario |
|---|---|
| Optional parameter accepted | Scenario 1 |
| Filter narrows response | Scenario 1 |
| Absent = no-op (slice-prior shape) | Scenario 3 |
| Empty = 400 with literal envelope | Scenario 5 |
| Over-cap = 400 with literal envelope | Scenario 6 |
| Invalid syntax = 400 with literal envelope | Scenario 4 |
| Anti-echo on all three 400 arms | Scenarios 4, 5, 6 |
| Case-sensitive default + `(?i)` opt-in | Scenario 7 |
| Unanchored default; explicit anchor available | Pinned via PIN 6 + regex grammar |
| Unmatched = 200 + `[]`, never 404/500 | Scenario 2 |
| Cross-tenant invariant | Scenario 9 |
| Mutual exclusion = 400 with explicit literal | Scenario 8 |
| Conjunctive AND with `min_severity` | Carried over from PIN 1 + sibling ADR-0055 |
| Caps preserved | Pinned via System Constraints; CI-enforced via K4 |
| Bare JSON array preserved | Pinned via System Constraints; covered by Scenarios 1, 2 |
| `LogStore` trait byte-identical | Pinned via System Constraints; Gate 2 `cargo public-api` enforces |
| Half-open `[start, end)` preserved | Pinned via System Constraints |
| `query-http-common` SOLE provider | Pinned via System Constraints + KPI K4 |

Every AC is observable in test or in CI; none is abstract.

## Item 6 — Right-sized (1-3 days, 3-7 scenarios)

**Status**: PASSED with one caveat.

**Evidence**: `story-map.md` § "Slice brief" estimates 2 days
end-to-end. Story count is 7; scenario count is 9 (split
rationale in Item 4 above). The scenario count is two over the
7-scenario soft cap; the split is deliberate and earns the
count.

The slice ships ONE field + ONE builder + ONE `matches` arm +
ONE `is_empty` clause on lumen and ONE parse helper + ONE
parameter field + ONE dispatch growth + ONE mutual-exclusion
check on log-query-api. The shape is identical to
`log-body-text-search-v0` (closed in similar effort at commit
1bfa609 plus the gate-5 follow-up at d96a807) plus one extra
arm in the parser (regex-compile) and one extra check
(mutual-exclusion). Estimated effort 2 days.

## Item 7 — Technical notes: constraints / dependencies

**Status**: PASSED.

**Evidence**: `user-stories.md` § US-01 "Technical Notes" lists:

- The existing `query_with` seam on `LogStore` (unchanged).
- The predicate-extension shape options (compiled vs raw) with
  recommendation and rationale (FLAG 2).
- The parse helper location (in `log-query-api`, not `lumen`).
- The `regex` crate dependency status (already in workspace via
  `query-api`; NEW direct dep on `lumen/Cargo.toml`; same pin
  via `Cargo.lock`).
- The order of handler checks (eight-step pipeline).
- The composition with `min_severity` (conjunctive AND).
- The `PartialEq`/`Eq` derive issue on `Predicate` once a
  `Regex` field lands (must be dropped; rationale provided).
- The mutation-test surface (eight categories of mutants, each
  with a named killer scenario or unit test).

`wave-decisions.md` § "Constraints Established" carries the
cross-cutting constraints (caps, envelope, redaction, tenant
seam, response shape, window semantics, sibling parameter
preservation).

## Item 8 — Dependencies resolved or tracked

**Status**: PASSED.

**Evidence**: `user-stories.md` § US-01 "Dependencies":

**Resolved** (every dependency live at HEAD):

- ADR-0047, ADR-0050, ADR-0052, ADR-0054, ADR-0055, ADR-0046.
- `lumen::LogStore::query_with`.
- `lumen::Predicate::matches` conjunctive composition (with the
  `body_contains` arm already in place per ADR-0055).
- The full `query_http_common::` public surface.
- `regex = "1"` (already direct in `query-api`'s `Cargo.toml`;
  new direct in `lumen`'s; same `Cargo.lock` pin at 1.12.3).
- `gate-5-mutants-lumen` CI workflow (shipped at d96a807; the
  new `Predicate::body_regex` arm benefits automatically).

**Tracked**: DESIGN flags 1-6 in `wave-decisions.md`. Each flag
has a recommendation and a fallback; none is a blocker on
DISCUSS.

## Item 9 — Outcome KPIs defined with measurable targets

**Status**: PASSED.

**Evidence**: `outcome-kpis.md` defines five KPIs (K1-K5), each
with a target, a measurement method, and a baseline:

- **K1 Honest matches** — false-pos = 0, false-neg = 0; measured
  by acceptance test; baseline 100% return today.
- **K2 Zero regression** — byte-equal slice-prior response on
  no-`body_regex` requests; measured by the four existing
  acceptance suites staying green; baseline HEAD at d96a807.
- **K3 Fast-fail invalid** — three 400 arms with no-store-call
  assertion; measured by acceptance scenarios with a
  `FailingLogStore` double; baseline the parameter does not
  exist today.
- **K4 Reuse confirmed** — zero new duplications, under-40-LOC
  budget; measured by static-grep CI step + code review;
  baseline existing shared-scaffold consumption.
- **K5 Mutants killed** — 100% kill rate on the lumen crate's
  mutants run; measured by `gate-5-mutants-lumen`; baseline
  100% pre-slice.

Each KPI has a numeric target (0, byte-equal, <40 LOC, 100%) and
a measurement method that fires in CI.

## Validation summary

| Item | Status |
|---|---|
| 1 Problem statement clear | PASSED |
| 2 Persona with specific characteristics | PASSED |
| 3 3+ domain examples with real data | PASSED |
| 4 UAT in Given/When/Then | PASSED |
| 5 AC derived from UAT | PASSED |
| 6 Right-sized | PASSED |
| 7 Technical notes | PASSED |
| 8 Dependencies resolved or tracked | PASSED |
| 9 Outcome KPIs with measurable targets | PASSED |

**Overall**: PASSED. The slice is ready for DESIGN.
