# Outcome KPIs — beacon-sighup-reload-v0

British English throughout, no em dashes.

## Feature: beacon-sighup-reload-v0

### Objective

By the end of this slice, an on-call operator can apply rule-catalogue
edits to a running beacon-server with a single signal, trust that valid
edits take effect without a restart, and trust that a malformed edit
leaves the daemon running on the previous catalogue rather than crashing,
silently ignoring her, or re-paging on-call. The documented SIGHUP
promise becomes a kept promise (Earned-Trust theme).

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | On-call operators editing a running beacon-server's rules dir | Apply a valid rule edit live via SIGHUP, with the change taking effect | 100% of valid-catalogue reloads take effect within one evaluation interval (today: 0%) | 0% (SIGHUP unhandled; only a restart applies edits) | Black-box B03 harness (add rule, SIGHUP, assert it fires) + structured reload event | Leading |
| 2 | On-call operators | Recover from a malformed edit without crash, partial apply, or re-page | 100% of malformed reloads keep the daemon on the previous catalogue with a diagnostic; 0% crash, partial-apply, or re-page a surviving Firing rule | No defined safe path today (no SIGHUP reload exists) | Black-box malformed-reload-keeps-previous harness (co-equal negative) + structured refusal event | Leading |
| 3 | On-call operators | Stop restarting beacon-server solely to apply a rule edit | Restarts performed only to apply a rule edit fall to zero | Every edit today requires a restart | Operator runbook usage + absence of restart-to-apply in incident timelines; reload events present in logs | Leading (secondary) |
| 4 | A rule unchanged across a valid reload | Keep its in-flight Firing state and `since` across the swap | 100% of surviving Firing rules retain their original `since`; 0 spurious re-pages per swap | No swap exists today | Assertion that a surviving rule's `since` is unchanged across a valid swap; sink receives no duplicate Firing | Leading (guardrail-shaped) |

### Metric Hierarchy

- **North Star**: share of rule-catalogue edits applied to a running
  beacon-server via SIGHUP that take effect correctly and safely (valid
  edits apply, malformed edits are refused-with-diagnostic, surviving
  alerts never re-page). Target: 100%. This is the single number that
  says "the documented promise is kept."
- **Leading Indicators**: KPI 1 (valid reload take-effect rate) and KPI
  2 (malformed reload safe-refusal rate) jointly predict the north star;
  KPI 3 (restarts-to-apply falling to zero) is the downstream behaviour
  change that follows once operators trust the mechanism.
- **Guardrail Metrics** (must NOT degrade):
  - Spurious re-pages per swap == 0 (a surviving Firing rule must not
    emit a second incident; KPI 4).
  - Dropped active alerts per swap == 0 (no silently lost Firing across
    a swap or refusal).
  - beacon-server crash-on-reload rate == 0 (a bad edit never takes the
    daemon down).
  - Half-applied catalogues == 0 (the swap is all-or-nothing;
    ADR-0034 "the evaluator never sees a half-loaded catalogue").
  - No regression in existing SIGINT/SIGTERM shutdown behaviour.

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| 1 Valid reload take-effect | Black-box harness (B03) + beacon structured logs | Harness asserts added rule fires post-SIGHUP; parse `rules reloaded` event | Per CI run + on each reload in prod | DISTILL (acceptance) + platform-architect (log capture) |
| 2 Malformed reload safety | Black-box harness (malformed negative) + beacon structured logs | Harness asserts previous catalogue retained, no crash; parse `reload refused` event | Per CI run + on each refused reload | DISTILL + platform-architect |
| 3 Restarts-to-apply -> 0 | beacon-server process lifecycle logs + incident timelines | Count process restarts not attributable to deploy/upgrade; expect reload events instead | Weekly review | platform-architect |
| 4 In-flight state preserved | Black-box harness + sink emission log | Assert surviving Firing rule's `since` unchanged and no duplicate Firing incident | Per CI run | DISTILL |

### Hypothesis

We believe that delivering the documented SIGHUP hot-reload (atomic swap
on valid catalogue, keep-previous-with-diagnostic on malformed catalogue,
in-flight state preserved for surviving rules) for on-call platform
operators will achieve a live, trustworthy rule-edit workflow.

We will know this is true when operators apply 100% of valid rule edits
via SIGHUP within one evaluation interval, 100% of malformed reloads
leave the daemon running on the previous catalogue with a diagnostic and
zero re-pages, and restarts performed solely to apply a rule edit fall to
zero.

## Handoff to DEVOPS (platform-architect)

1. **Data collection requirements**: instrument the structured reload
   event (`rules_loaded`, `added`, `removed`) and the refusal event
   (`file`, parse error, "previous catalogue retained") as
   machine-parseable log fields so a harness and a dashboard can both
   consume them.
2. **Dashboard/monitoring needs**: a counter of successful vs refused
   reloads; an alert if a reload is refused (operator's edit needs
   attention) and an alert if spurious re-pages per swap is ever > 0
   (guardrail breach).
3. **Alerting thresholds**: crash-on-reload > 0, half-applied catalogue
   > 0, dropped active alerts per swap > 0, and spurious re-pages per
   swap > 0 are all immediate guardrail breaches.
4. **Baseline measurement**: none to pre-collect; the baseline is "no
   SIGHUP reload exists" (0% applied, no safe path), established by the
   verifier's B03 RED.
