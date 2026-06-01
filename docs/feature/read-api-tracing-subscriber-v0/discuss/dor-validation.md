# Definition of Ready — read-api-tracing-subscriber-v0

9-item DoR hard gate. Each item passes with evidence, or carries explicit
remediation. Scope: US-01..US-06 in `user-stories.md`.

## 1. Problem statement clear, in domain language

PASS. Each story opens with a domain-language problem from the operator's
viewpoint (Priya Nair tails container stderr and sees nothing; she cannot
confirm the service is up or learn why it refused). Origin is a verified
EDD-verifier issue (005, operability), documented in
`wave-decisions.md > Origin`. No solution language in the problem
statements.

## 2. User/persona with specific characteristics

PASS. Persona: Priya Nair, platform on-call operator at a tenant running
the read tier in containers; reads stderr via `kubectl logs`/`docker logs`
on deploy or crash-loop. Defined once at the top of `user-stories.md` and
referenced per story. The EDD-verifier is a named secondary consumer
(US-05, K5).

## 3. 3+ domain examples with real data

PASS. Every story carries 3 domain examples with concrete values: real
env vars (`KALEIDOSCOPE_LOG_QUERY_TENANT=acme`,
`KALEIDOSCOPE_LOG_QUERY_ADDR=0.0.0.0:19091`, `RUST_LOG=warn`/`error`), real
ports (9090/9091/9092), real event names
(`log_query_api_starting`, `listener_bound`, `health.startup.refused`),
and a real tenant (`acme`). No `user123`-style placeholders.

## 4. UAT in Given/When/Then (3-7 scenarios)

PASS. Each story has 2-3 Given/When/Then scenarios; the per-binary stories
(US-01..04) carry 3 each. Scenario titles state operator outcomes ("sees
the service announce itself at startup", "learns why the service refused"),
not implementation ("subscriber writes to fd 2"). Across the slice the
scenarios cover startup, refusal, filter behaviour, and pre-init failure.

## 5. AC derived from UAT

PASS. Every story has an Acceptance Criteria checklist derived directly
from its scenarios (e.g. US-02's "fail-closed startup writes
`health.startup.refused` ... before non-zero exit" maps to the refusal
scenarios). AC are observable from outside the process (capture stderr,
grep the event, check exit status).

## 6. Right-sized (1-3 days, 3-7 scenarios)

PASS. See `story-map.md > Scope Assessment: PASS`. 6 stories (one
optional), 3 crates touched, 0 new integration points, no HTTP contract
change, estimated ~1 day. Each story is a few-line edit plus a black-box
acceptance test. Within the Elephant Carpaccio bound.

## 7. Technical notes: constraints/dependencies

PASS. `user-stories.md > System Constraints` and
`wave-decisions.md > Constraints Established` capture: no HTTP contract
change, no new crate, match aperture's subscriber config exactly, install
as first action in `main`, pre-init via `eprintln!`, events must reach
stderr greppably, keep `#[mutants::skip]`, no 1.0.0.

## 8. Dependencies resolved or tracked

PASS. The one hard dependency — `tracing-subscriber` is MISSING from all
three read crates — is identified by reading the three `Cargo.toml` files
and flagged to DESIGN (`wave-decisions.md > Flags 1`). Remediation is
specified: add aperture's exact line to each crate. The exact aperture
configuration and install location are flagged for Morgan to pin (Flags
2-3). No unresolved blockers; all open questions are DESIGN decisions, not
DISCUSS gaps.

## 9. Outcome KPIs defined with measurable targets

PASS. `outcome-kpis.md` defines K1-K5, each with who / does what / by how
much / measured by / baseline / numeric target. K3 (refusal visible before
non-zero exit, 0% -> 100%) and K5 (verifier unblocked, blocked ->
unblocked) directly map to the originating issue-005 resolution.

## Result

All 9 items PASS. DoR gate satisfied. Ready for DESIGN handoff to Morgan
(nw-solution-architect). Open items for DESIGN are decisions (exact
builder expression, inline vs shared `query-http-common` helper, whether
to convert the final bare `Err` into a structured event + non-zero exit),
not DISCUSS deficiencies.
