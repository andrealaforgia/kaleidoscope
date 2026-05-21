# Definition of Ready Validation — beacon-durable-alert-state-v0

9-item hard gate per story. Evidence cited inline.

## Story: US-01 — Hold per-rule state through a store seam (@infrastructure)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | Cites the exact gap: `let mut state = RuleState::Inactive` at main.rs:146. |
| User/persona with characteristics | PASS | beacon-server orchestrator / per-rule loop (an infra story; the actor is the system). |
| 3+ domain examples with real data | PASS | Three examples with real rule names ("pay-latency", "disk-fill"). |
| UAT in Given/When/Then (3-7) | PASS | 3 scenarios. |
| AC derived from UAT | PASS | 4 AC trace to the scenarios. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | Refactor, 3 scenarios, ~1 day. |
| Technical notes: constraints/deps | PASS | ADR-0037, LogStore shape, depends on nothing; blocks US-02. |
| Dependencies resolved or tracked | PASS | None; must precede US-02 (tracked). |
| Outcome KPIs with measurable targets | PASS | 100% of state access via trait; baseline 0%. |

### DoR Status: PASSED

> Note: US-01 is `@infrastructure` and intentionally carries no
> Elevator Pitch. This is permitted because the slice it belongs to is
> not all-infrastructure at the feature level (slice 02 carries
> user-visible stories US-02, US-03).

## Story: US-02 — Firing alerts survive restart without re-paging

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | Priya restarts during a live incident; firing rules re-page today. |
| User/persona with characteristics | PASS | On-call operator Priya Nair, restarting during a live/near-live incident. |
| 3+ domain examples with real data | PASS | "pay-latency" Firing since 14:02; "disk-fill"; corrupt snapshot; "legacy-check" removed. |
| UAT in Given/When/Then (3-7) | PASS | 4 scenarios (happy, downtime-resolve, corrupt, removed-rule). |
| AC derived from UAT | PASS | 4 AC trace to the scenarios. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | One durable adapter + wiring, 4 scenarios, ~1.5 days. |
| Technical notes: constraints/deps | PASS | file_backed shape, Serialize derive flag, recovery budget, depends on US-01. |
| Dependencies resolved or tracked | PASS | Depends on US-01 (tracked, same feature). |
| Outcome KPIs with measurable targets | PASS | 0 spurious re-fires, down from 1 per firing rule per restart. |
| Elevator Pitch (Dimension 0) | PASS | Before/After/Decision present; real entry point (`beacon-server` invocation); concrete output (recovery log line + no re-page); names a real operator decision. |

### DoR Status: PASSED

## Story: US-03 — Pending dwell clocks preserved across restart

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | Pending-since lost on restart silently delays near-ready alerts. |
| User/persona with characteristics | PASS | On-call operator Priya, restarting while rules are mid-dwell. |
| 3+ domain examples with real data | PASS | "disk-fill" Pending since 14:00:00 / 120 s; cleared-during-downtime; future-dated since. |
| UAT in Given/When/Then (3-7) | PASS | 3 scenarios. |
| AC derived from UAT | PASS | 4 AC trace to the scenarios. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | Behaviour on the same adapter, 3 scenarios, ~0.5-1 day. |
| Technical notes: constraints/deps | PASS | SystemTime serialise precision, reuses US-02 adapter, depends on US-01+US-02. |
| Dependencies resolved or tracked | PASS | Depends on US-01, US-02 (tracked). |
| Outcome KPIs with measurable targets | PASS | 100% pending-since recovered, baseline 0%. |
| Elevator Pitch (Dimension 0) | PASS | Before/After/Decision present; real entry point; concrete output (recovered Pending state + fires-on-schedule); real operator decision. |

### DoR Status: PASSED

## Feature DoR Status: PASSED (3/3 stories)
