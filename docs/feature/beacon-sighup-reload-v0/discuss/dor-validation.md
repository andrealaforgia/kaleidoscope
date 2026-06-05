# Definition of Ready Validation — beacon-sighup-reload-v0

British English throughout, no em dashes.

Hard gate: every story passes all 9 items with evidence before handoff
to DESIGN. Two stories: US-01 (Apply edited rules with SIGHUP) and
US-02 (Refuse a malformed reload and keep the previous catalogue).

## Story: US-01 — Apply edited rules with SIGHUP, no restart

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | Sofia edits the rules dir, runs `kill -HUP <pid>`, nothing happens; SIGHUP unhandled (`main.rs:177,179`), catalogue loaded once (`main.rs:65`). Domain language: rules dir, catalogue, SIGHUP, evaluation interval, incident, sink. |
| 2. User/persona with specific characteristics | PASS | Sofia Okonkwo, on-call platform operator for a payments cluster, editing a running beacon-server's `--rules` dir in production. |
| 3. 3+ domain examples with real data | PASS | Three: added `checkout-error-rate.toml` fires; deleted `disk-pressure.toml` stops; no-op SIGHUP swaps cleanly. Real rule names, real PromQL queries, pid 8431. |
| 4. UAT in Given/When/Then (3-7) | PASS | Four scenarios: added rule fires, removed rule stops, no-change clean swap, structured success event. |
| 5. AC derived from UAT | PASS | Five AC, each traceable to a scenario (SIGHUP handler installed; valid swap without restart; removed rule silent; no spurious emissions; structured success event). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | 4 scenarios; brownfield change confined to beacon-server composition root reusing existing loader. 1-3 days. |
| 7. Technical notes: constraints/dependencies | PASS | Reuse `load_rules`; SIGHUP is composition-root (ADR-0037); install handler before spawning tasks; atomic-swap + in-flight-state flagged to DESIGN; depends on durable store. |
| 8. Dependencies resolved or tracked | PASS | Depends on `RuleStateStore` seam (beacon-durable-alert-state-v0, delivered) and the existing loader (delivered). US-02 depends on this story's swap path; tracked in story-map. |
| 9. Outcome KPIs with measurable targets | PASS | KPI 1 (100% valid reloads take effect within one interval, baseline 0%) and KPI 3 (restarts-to-apply -> 0). Measured by B03 harness + reload event. |

### DoR Status: PASSED

## Story: US-02 — Refuse a malformed reload, keep the previous catalogue

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | Sofia mistypes `for_duraton` under incident pressure and SIGHUPs; the danger is crash, partial apply, or silent ignore. Contract: "previous catalogue stays active" (slice-02), atomic swap (ADR-0034). |
| 2. User/persona with specific characteristics | PASS | Sofia Okonkwo, on-call operator, applying a malformed edit during an incident; needs daemon up, alerts kept, diagnostic naming the fault. |
| 3. 3+ domain examples with real data | PASS | Three: malformed `payments.toml` keeps previous (`service-down` Firing since 09:14:02); emptied dir refused; partly-broken catalogue applies good rules + names `inventory.toml`. |
| 4. UAT in Given/When/Then (3-7) | PASS | Five scenarios: keeps previous catalogue; does not re-page; diagnostic names problem; zero-rules refused; surviving rule keeps in-flight state. |
| 5. AC derived from UAT | PASS | Five AC traceable to scenarios, including the load-bearing safety AC and the FLAGGED-for-DESIGN in-flight-state AC. |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | 5 scenarios; same composition-root surface as US-01, reusing `LoaderDiagnostic::display` and the `has_any_rules` bar. 1-3 days. |
| 7. Technical notes: constraints/dependencies | PASS | Reuse `LoaderDiagnostic::display`; "valid == >=1 rule" mirrors startup `has_any_rules`; in-flight-state mechanism, resolver rebuild, task-race flagged to DESIGN; depends on US-01 + durable store. |
| 8. Dependencies resolved or tracked | PASS | Depends on US-01 (swap path) and the durable store. The in-flight-state-preservation MECHANISM is a DESIGN decision, explicitly flagged in wave-decisions.md, not an unresolved DISCUSS blocker: the OBSERVABLE AC is pinned. |
| 9. Outcome KPIs with measurable targets | PASS | KPI 2 (100% malformed reloads keep previous + diagnostic, 0% crash/partial/re-page) and KPI 4 (100% surviving Firing rules retain `since`, 0 spurious re-pages). |

### DoR Status: PASSED

## Cross-cutting notes

- **Happy-path bias check**: PASS. US-02 is dedicated to the sad path;
  the malformed-reload negative is co-equal with the B03 headline, and
  the guardrails (no crash, no partial apply, no re-page, no dropped
  alert) are explicit AC and KPIs.
- **Technical-AC bias check**: PASS. AC are observable operator outcomes
  (rule fires, daemon stays up, `since` unchanged, structured event
  emitted), not implementation prescriptions. Mechanism choices (how the
  swap is made atomic, how in-flight state is carried) are deferred to
  DESIGN by name.
- **Solution-neutrality**: the in-flight-state-preservation decision is
  flagged for DESIGN with named sub-decisions (matching key, inhibition
  resolver, task race, durable-store consistency); DISCUSS states the
  required observable behaviour only.

## Overall DoR: PASSED (both stories)
