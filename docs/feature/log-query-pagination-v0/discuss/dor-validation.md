# Definition of Ready Validation: log-query-pagination-v0

Luna self-validates the 9-item DoR hard gate. Each item passes with
evidence drawn from `user-stories.md`, `story-map.md`, and
`outcome-kpis.md`.

## Item 1 - Problem statement clear, domain language

PASS. Every story opens with a `## Problem` in operator language: Maria
Santos pulls a single block up to the 100_000-row cap and trims with
`jq '.[:50]'` client-side; she has no second-page primitive; she cannot
bound a first page. The problem is the operator's workflow pain (scroll a
large result set one page at a time), NOT a technical task ("add limit
param"). No "implement-X" framing.

## Item 2 - User/persona with specific characteristics

PASS. Three recurring personas with distinct motives and contexts:
Maria Santos (on-call SRE on `acme-prod`, terminal + curl + jq,
triage-urgency, wants a bounded first page then scroll), Marcus Webb
(platform engineer building a log-scroller UI / automation, throughput
motive, the page is the unit of work, owns scripts that must not break),
Priya Raman (support engineer, readability motive, a screenful at a
time). Each story names the personas it serves.

## Item 3 - 3+ domain examples with real data

PASS. Every story carries 2-3 concrete `### Domain Examples` with real
tenant ids (`acme-prod`, `globex-staging`), real log bodies (`kafka
timeout connecting to broker-3`, `redis: GET timeout on key user-42`,
`checkout: heartbeat`), real nanosecond timestamps
(`1_716_200_005_000_000_000`), and real parameter values (`limit=3`,
`offset=3`, `limit=100001`, `limit=0`, `offset=-1`). The eight-record
fixture is shared and concrete across US-01/US-04/US-06/US-07. No
`user123` or `test@test.com`.

## Item 4 - UAT in Given/When/Then (3-7 scenarios)

PASS. Each story carries 2-3 Gherkin UAT scenarios in Given/When/Then;
the slice as a whole carries well over the 3-7 envelope distributed
across the nine stories (walking-skeleton US-01 alone has 4). Scenario
titles describe WHAT the operator achieves ("A limit returns the first N
records in order", "Successive pages partition the result set with no
duplicate and no gap", "Tenant B's offset past its own end is a calm
empty page"), NOT how the system works. No technical scenario titles.

## Item 5 - AC derived from UAT

PASS. Every story has an `### Acceptance Criteria` checklist derived
directly from its scenarios: the limit-first-N AC from the limit
scenario, the partition AC from the honesty scenarios, the redacted-400
ACs from the invalid-parameter scenarios, the byte-equality AC from the
defaults scenario. Each AC is observable (status code, response body
shape, store-not-touched) without ambiguity.

## Item 6 - Right-sized (1-3 days, 3-7 scenarios)

PASS. `story-map.md` § Scope Assessment records PASS: 9 thin stories,
1 bounded context (`log-query-api`; the recommended cut does NOT touch
`lumen`), 0 new integration points, estimated under one day of crafter
dispatch. The nine stories are thin behavioural promises over ONE
parameter pair, several of them invariant-pins over unchanged seams. The
walking skeleton (US-01) is demonstrable in a single session. No story
exceeds 3-7 scenarios.

## Item 7 - Technical notes: constraints/dependencies

PASS. Each story carries `### Technical Notes` naming the existing seams
(`query`/`query_with` unchanged; the returned `Vec<LogRecord>` is the
slice target; `parse_limit`/`parse_offset` mirror `parse_body_contains`;
`query_http_common::MAX_RESULT_ROWS` for the over-cap check). The
`## System Constraints` block at the top pins the cross-cutting
constraints (caps preserved, envelope reused, tenant seam reused,
redaction, bare-array shape, stable order, filter-before-page, no
trait/predicate change). Six DESIGN flags are surfaced.

## Item 8 - Dependencies resolved or tracked

PASS. Each story's `### Dependencies` lists RESOLVED dependencies
(ADR-0047, ADR-0050, ADR-0052, ADR-0054, ADR-0055, ADR-0056;
`lumen::LogStore::query`/`query_with`; the `query_http_common` helpers;
the per-tenant isolation invariant) and TRACKED items (the six DESIGN
flags in `wave-decisions.md`, which are decisions-to-pin, NOT blockers).
No unresolved blocker. The source tree was read directly to confirm the
seams exist as described.

## Item 9 - Outcome KPIs defined with measurable targets

PASS. `outcome-kpis.md` defines five KPIs (K1 behaviour invariance, K2
pagination honesty, K3 `query-http-common` reuse, K4 invalid-params-400-
fast, K5 cap-interaction honest), each with Who / Does-what / By-how-much
/ Measured-by / Baseline and a numeric or 100%-class target. Each is
measured by the DISTILL acceptance suite and the existing regression +
CI gates, consistent with the contract-IS-the-signal posture.

## Result

9 / 9 DoR items PASS. The slice is READY for DESIGN handoff.

The six flags in `wave-decisions.md` are decisions Morgan pins in
DESIGN; they are NOT DoR gaps. DISCUSS records the recommendation for
each (handler-side slice; reject over-cap; skip-based offset; no default
limit; `limit=0` invalid / offset-past-end calm-empty; small ADR-0057)
and leaves the binding decision to DESIGN.
