# Definition of Ready Validation — `consolidated-runtime-v0`

Hard gate: each story passes all 9 DoR items with evidence before DESIGN handoff. Validated
2026-06-13 by Luna. Items: (1) problem in domain language, (2) persona with characteristics,
(3) 3+ domain examples with real data, (4) UAT 3-7 scenarios Given/When/Then, (5) AC derived
from UAT, (6) right-sized (1-3 days, 3-7 scenarios), (7) technical notes / constraints,
(8) dependencies resolved or tracked, (9) outcome KPIs with measurable target.

---

## US-01 — Send a metric and immediately query it back, no restart

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem in domain language | PASS | "Bring up the stack, send a metric, look" fails today; query froze its snapshot at startup and never re-reads (cites state assessment §4). No solution language in the problem. |
| 2 | Persona with characteristics | PASS | Andrea (local experimenter), contributor evaluating a change, integration suite/CI — each with context and motivation. |
| 3 | 3+ domain examples, real data | PASS | 3 examples: counter `request_count`=1 tenant `acme` read back; query-before-send empty; single-process integration proof. |
| 4 | UAT 3-7 scenarios G/W/T | PASS | 4 scenarios: immediate visibility; empty-store empty-success; post-startup append visible; one-process serves ingest+query. |
| 5 | AC derived from UAT | PASS | 5 AC, each tracing to a scenario (visibility, empty success, post-startup, one process, same instance). |
| 6 | Right-sized | PASS | 4 scenarios; composition reusing the existing `router` seam + already-Arc-shared metric store; ~1-2 days. |
| 7 | Technical notes / constraints | PASS | Names the reuse seam (`lib.rs:122`), the `&self`/Mutex evidence, the new-binary DESIGN flag, ephemeral-port note. |
| 8 | Dependencies resolved/tracked | PASS | None upstream; it is the walking skeleton. |
| 9 | Outcome KPIs measurable | PASS | KPI 1 (0%→100% no-restart) + freshness p95<1s; baseline 0%; measured by single-process test. |

### DoR Status: PASSED

---

## US-02 — Tenant isolation holds in the consolidated process

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem in domain language | PASS | Collapsing ingest+query into one process must not weaken the tenant boundary; data-leak would be the worst trade. Domain framed. |
| 2 | Persona with characteristics | PASS | Multi-tenant operator; security-minded reviewer deciding if consolidation is safe to run. |
| 3 | 3+ domain examples, real data | PASS | globex cannot see acme's `request_count`; acme sees its own; untenanted record → default tenant `acme` / fail-closed. |
| 4 | UAT 3-7 scenarios G/W/T | PASS | 3 scenarios: cross-tenant empty; owning-tenant returns; untenanted uses default. |
| 5 | AC derived from UAT | PASS | 4 AC covering isolation, owning-tenant, default/fail-closed, read-auth still available. |
| 6 | Right-sized | PASS | 3 scenarios; reuses aegis `TenantId` and the store tenant key unchanged; ~1 day. |
| 7 | Technical notes / constraints | PASS | aegis TenantId + `(tenant, metric)` key unchanged; guardrail for US-01. |
| 8 | Dependencies resolved/tracked | PASS | US-01 (shares metrics composition) — tracked. |
| 9 | Outcome KPIs measurable | PASS | KPI 4: 0 cross-tenant leaks; baseline 0 leaks (no-regression target); measured by ingest-A/read-B test. |

### DoR Status: PASSED

---

## US-03 — Send a log and immediately query it back, no restart

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem in domain language | PASS | Logs share the same frozen-snapshot gap; "send a log, see it" fails until restart. |
| 2 | Persona with characteristics | PASS | Andrea/contributor debugging via logs; integration suite/CI. |
| 3 | 3+ domain examples, real data | PASS | log `"checkout failed: card declined"` tenant `acme` read back; empty-before; cross-tenant. |
| 4 | UAT 3-7 scenarios G/W/T | PASS | 3 scenarios: immediate visibility; empty success; tenant isolation. |
| 5 | AC derived from UAT | PASS | 4 AC tracing to scenarios + one-process serving. |
| 6 | Right-sized | PASS | 3 scenarios; reuses `log_query_api::router` (`lib.rs:95`), same Arc pattern; ~1 day. |
| 7 | Technical notes / constraints | PASS | Names lumen reuse seam; same pattern as US-01. |
| 8 | Dependencies resolved/tracked | PASS | US-01 (composition pattern) — tracked. |
| 9 | Outcome KPIs measurable | PASS | KPI 1/2 for logs: 0%→100%, p95<1s; baseline 0%. |

### DoR Status: PASSED

---

## US-04 — Send a trace and immediately query it back, no restart

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem in domain language | PASS | Traces are the last frozen-snapshot signal, with TWO query routes (window + by-id) to prove. |
| 2 | Persona with characteristics | PASS | Andrea/contributor reading a trace by window and by id; integration suite/CI. |
| 3 | 3+ domain examples, real data | PASS | span `GET /api/v1/query_range` trace id `4bf92f3577b34da6a3ce929d0e0e4736` tenant `acme` by window; by id; empty/cross-tenant. |
| 4 | UAT 3-7 scenarios G/W/T | PASS | 4 scenarios: window visibility; by-id retrieval; empty by-id success; cross-tenant empty. |
| 5 | AC derived from UAT | PASS | 4 AC covering both routes, empty success, tenant, one process. |
| 6 | Right-sized | PASS | 4 scenarios; reuses `trace_query_api::router` (`lib.rs:100`); two routes make it slightly larger; ~1-2 days. |
| 7 | Technical notes / constraints | PASS | Names ray reuse seam; both `TRACES_ROUTE` + `TRACES_BY_ID_ROUTE` share state. |
| 8 | Dependencies resolved/tracked | PASS | US-01 (composition pattern) — tracked. |
| 9 | Outcome KPIs measurable | PASS | KPI 1/2 for traces (both routes): 0%→100%, p95<1s; baseline 0%. |

### DoR Status: PASSED

---

## US-05 — Bring the whole stack up with one command and exercise every signal live

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem in domain language | PASS | The roadmap bar: one process, all signals, one command; today is five hand-launched binaries with restart ordering. |
| 2 | Persona with characteristics | PASS | Andrea the experimenter; contributor/CI running the full loop. |
| 3 | 3+ domain examples, real data | PASS | one command three signals all live; all five ports answer on one process; fresh stack consistent across signals. |
| 4 | UAT 3-7 scenarios G/W/T | PASS | 3 scenarios: one command boots ingest+3 query; every signal queryable no restart; fresh stack empty-success across all. |
| 5 | AC derived from UAT | PASS | 4 AC: one command, three signals back, all five ports bound, fresh-then-populated. |
| 6 | Right-sized | PASS | 3 scenarios; capstone composing US-01/03/04; unique work is all-ports-bound + coexistence; ~1 day given the parts exist. |
| 7 | Technical notes / constraints | PASS | DESIGN flags (new-binary/extend, one tracing install, ephemeral ports); explicitly excludes C2 compose and C4 README. |
| 8 | Dependencies resolved/tracked | PASS | US-01, US-03, US-04 — tracked; capstone runs last. |
| 9 | Outcome KPIs measurable | PASS | KPI 3: 1 command, 3/3 signals, 100% attempts; baseline 5 binaries + restart. |

### DoR Status: PASSED

---

## Summary

| Story | DoR |
|-------|-----|
| US-01 | PASSED (9/9) |
| US-02 | PASSED (9/9) |
| US-03 | PASSED (9/9) |
| US-04 | PASSED (9/9) |
| US-05 | PASSED (9/9) |

All five stories pass the 9-item Definition of Ready hard gate. Cross-cutting NFRs (freshness
p95 < 1s, zero cross-tenant leaks, no durability/auth regression, no port conflicts) are
captured in `outcome-kpis.md` guardrails and the System Constraints in `user-stories.md`.
Happy-path bias is mitigated: across the five stories, 11 of 19 scenarios are edge/boundary/
guardrail (empty stores, cross-tenant, post-startup append, by-id miss, port co-binding,
untenanted/fail-closed) — ~58%, above the 40% target. Ready for peer review.
