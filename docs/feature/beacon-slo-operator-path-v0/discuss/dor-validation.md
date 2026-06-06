# Definition of Ready Validation — beacon-slo-operator-path-v0

British English throughout, no em dashes.

The 9-item DoR hard gate, applied to each of the five stories. Every item
must PASS with evidence before handoff to DESIGN.

---

## Story: US-01 — An SLO declared in the file synthesises and loads

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | Priya wants burn-rate alerting; the engine exists but is unreachable; she would otherwise hand-write four PromQL rules and keep thresholds in sync by hand. |
| 2. User/persona with specific characteristics | PASS | Priya Nadkarni, SRE on payments, runs beacon-server with a `--rules` TOML dir under Git, hand-authors `[[rules]]`, knows the MWMBR methodology. |
| 3. 3+ domain examples with real data | PASS | service `checkout`, good `http_requests_total{job="checkout",code!~"5.."}`, target `0.999`; mixed dir (`checkout.toml` + `disk.toml`); boundary `0.9999` giving limit `0.00144`. |
| 4. UAT in Given/When/Then (3-7) | PASS | 3 scenarios: synthesise-and-load, fast-burn-pages-slow-burn-tickets, deterministic-across-restarts (`@property`). |
| 5. AC derived from UAT | PASS | 5 AC trace to the scenarios (four rules loaded; canonical thresholds/names; page/ticket; determinism; `rules_loaded` count). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | Pure wiring of an existing engine; 3 scenarios; ~1-2 days. |
| 7. Technical notes: constraints/dependencies | PASS | REUSE `synthesise_slo` + `MWMBR_TABLE`; `Slo` is the deser target; DESIGN owns TOML keys (FLAG-1); depends on loader + catalogue. |
| 8. Dependencies resolved or tracked | PASS | Engine and loader exist and are green; the only open items are DESIGN mechanism decisions (FLAG-1), tracked in wave-decisions. |
| 9. Outcome KPIs with measurable targets | PASS | KPI 1: reachability 0% to 100%; measured by an acceptance test asserting four named rules + the `rules_loaded` event. |

### DoR Status: PASSED

---

## Story: US-02 — A malformed target availability is refused with a clear message

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | A `target_availability` typo makes budget 0 and the predicate `error_rate > 0`, a self-inflicted always-fire pager rule; unguarded once the path exists. |
| 2. User/persona with specific characteristics | PASS | Priya, editing SLO targets by hand under time pressure; operator-critical. |
| 3. 3+ domain examples with real data | PASS | `0.999` (valid loads), `1.0` (refused, the degenerate boundary), `0.0` and `1.5` (refused, outside the open interval). |
| 4. UAT in Given/When/Then (3-7) | PASS | 3 scenarios: target=1.0 refused, target outside `(0,1)` refused, valid target loads. |
| 5. AC derived from UAT | PASS | 4 AC trace to the scenarios (refuse at/outside `(0,1)`; diagnostic names file+value+range; no degenerate rule; valid loads). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | One validation predicate + diagnostic; 3 scenarios; ~1 day. |
| 7. Technical notes: constraints/dependencies | PASS | Validation in the SLO conversion path (FLAG-3), not in `synthesise_slo`; closes four-quadrants Q2 [LOW]. |
| 8. Dependencies resolved or tracked | PASS | Depends on US-01; exact diagnostic wording is DESIGN's (FLAG-3), tracked. |
| 9. Outcome KPIs with measurable targets | PASS | KPI 2: always-fire rules reaching evaluation = 0; out-of-range caught = 100%; measured by a refusal acceptance test. |

### DoR Status: PASSED

---

## Story: US-03 — A non-30-day budget is refused, making the doc claim true

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | MWMBR thresholds are 30d-only (ADR-0036 Knowledge Gap); slo.rs:49-51 already claims a loader rejection that does not exist; a 7d budget would apply the wrong-window thresholds. |
| 2. User/persona with specific characteristics | PASS | Priya, who might reach for a 7d/90d budget out of habit from other SLO tooling. |
| 3. 3+ domain examples with real data | PASS | `30d` (loads), `7d` (refused), `90d` (refused, the quarterly-habit boundary). |
| 4. UAT in Given/When/Then (3-7) | PASS | 2 scenarios: non-30d refused with clear message, 30d loads. (At the 3-7 floor; bundled with US-02 in the safety slice, jointly 5 scenarios.) |
| 5. AC derived from UAT | PASS | 5 AC trace to the scenarios incl. the doc-claim-made-true AC (slo.rs:49-51 updated). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | One equality check + diagnostic + a doc-comment fix; ~0.5-1 day. |
| 7. Technical notes: constraints/dependencies | PASS | Validation alongside US-02 (FLAG-3); update slo.rs:49-51 doc; grounded in ADR-0036. |
| 8. Dependencies resolved or tracked | PASS | Depends on US-01; shares the validation path with US-02. |
| 9. Outcome KPIs with measurable targets | PASS | KPI 5 (guardrail): false SLO doc claims from 1 to 0; KPI 2: non-30d caught = 100%. |

### DoR Status: PASSED

> Note on item 4: US-03 has 2 scenarios, below the 3-7 guideline in
> isolation. It is deliberately bundled with US-02 in one safety slice
> (Slice 2), which jointly has 5 scenarios and is demonstrated together. The
> validation is a single deserialisation concern split into two stories only
> to separate the two distinct operator harms (always-fire vs wrong-window).
> The pair is right-sized; splitting the doc-claim honesty fix into its own
> story keeps the slo.rs:49-51 remediation explicit and traceable. Accepted
> as PASS at the slice level.

---

## Story: US-04 — Synthesised SLO rules coexist with hand-authored rules

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | Adding an SLO must not disturb existing hand-authored `[[rules]]`; a silent shadow/drop would lose coverage Priya thinks she has. |
| 2. User/persona with specific characteristics | PASS | Priya with an existing hand-authored rule catalogue, adopting SLOs incrementally. |
| 3. 3+ domain examples with real data | PASS | mixed dir (`checkout.toml` SLO + `disk.toml` two rules) giving 6; rules-only dir unchanged; a literal name collision `checkout_page_1h_5m`. |
| 4. UAT in Given/When/Then (3-7) | PASS | 3 scenarios: coexist (6 rules), rules-only unchanged, collision surfaced. |
| 5. AC derived from UAT | PASS | 4 AC trace to the scenarios (one catalogue; `(4 x SLOs)+rules` count; rules-only unchanged + suites green; collision diagnostic). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | Append synthesised rules to the catalogue + a collision policy; 3 scenarios; ~1-2 days. |
| 7. Technical notes: constraints/dependencies | PASS | Collision policy + merge ordering are DESIGN's (FLAG-2); guardrail: existing rule path must not regress. |
| 8. Dependencies resolved or tracked | PASS | Depends on US-01; collision policy tracked (FLAG-2). |
| 9. Outcome KPIs with measurable targets | PASS | KPI 3: hand-authored rules silently dropped = 0; existing suites stay 100% green. |

### DoR Status: PASSED

---

## Story: US-05 — An SLO edit hot-reloads under SIGHUP

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | Priya tunes on a live daemon via `kill -HUP`; an SLO edit must reload like a rule edit, with a bad edit refused not partially applied (else an always-fire rule slips in on reload). |
| 2. User/persona with specific characteristics | PASS | Priya, who applies edits to a running beacon-server with SIGHUP and will not restart. |
| 3. 3+ domain examples with real data | PASS | valid edit `0.999` to `0.9995` reloads; malformed `target=1.0` edit refused, previous kept; a firing `checkout_page_1h_5m` survives an unrelated `search` SLO add. |
| 4. UAT in Given/When/Then (3-7) | PASS | 3 scenarios: valid edit reloads, malformed edit refused + previous kept, surviving firing rule keeps `since`. |
| 5. AC derived from UAT | PASS | 4 AC trace to the scenarios (atomic apply + `beacon.reload.succeeded`; refusal + retained + no degenerate rule; survivor keeps `since`; consistent with ADR-0063). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | Reuses the ADR-0063 reload path verbatim; SLO support falls out of the shared `load_rules` re-read; 3 scenarios; ~1-2 days incl. tests. |
| 7. Technical notes: constraints/dependencies | PASS | REUSE main.rs:292-440; DESIGN decides expansion-aware `added` count (FLAG-4); depends on US-01..US-04. |
| 8. Dependencies resolved or tracked | PASS | Depends on US-01 (synthesis), US-02/US-03 (validation), US-04 (merge); reload mechanism exists (ADR-0063). |
| 9. Outcome KPIs with measurable targets | PASS | KPI 4: SLO edits needing a restart = 0; bad edits going partially live = 0; survivors re-paged = 0. |

### DoR Status: PASSED

---

## Overall DoR Status: PASSED (5 of 5 stories)

All five stories pass all nine DoR items. The only open items are the five
DESIGN mechanism decisions (FLAG-1..FLAG-5 in wave-decisions.md), which are
correctly deferred to the DESIGN wave and tracked, not DoR blockers: DISCUSS
pins the observable behaviour; DESIGN picks the mechanism.

Requirements completeness check (functional / NFR / business rules):

- **Functional**: the declare-synthesise-merge-evaluate-reload flow (US-01,
  US-04, US-05).
- **NFR / quality**: determinism of synthesis (`@property`, US-01); the
  all-or-nothing reload invariant (US-05); the no-regression guardrail on the
  existing rule path (US-04); per-feature 100% mutation on modified lines.
- **Business rules**: `target_availability` strictly in `(0,1)` (US-02);
  `error_budget_period == 30d` (US-03); name-collision policy (US-04, DESIGN);
  no degenerate always-fire rule ever reaches evaluation (US-02, US-05).
