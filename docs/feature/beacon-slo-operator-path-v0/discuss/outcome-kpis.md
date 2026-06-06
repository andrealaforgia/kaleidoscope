# Outcome KPIs — beacon-slo-operator-path-v0

British English throughout, no em dashes.

## Feature: beacon-slo-operator-path-v0

### Objective

Make beacon's correct-but-dead SLO multi-window multi-burn-rate engine
reachable from the operator surface: an SRE declares an SLO in a rule file
and gets four correct burn-rate rules, with malformed SLOs refused at load,
honoured the same way under SIGHUP reload, by the close of this feature.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | SRE operators with a beacon `--rules` dir | declare an SLO and get four loaded burn-rate rules without hand-writing them | from impossible (0%) to 100%: one `[[slo]]` yields four loaded rules | 0; engine is library-only, unreachable | acceptance test: load an `[[slo]]` file, assert four named rules in the catalogue; `rules_loaded` startup event | Leading (Outcome) |
| 2 | SRE operators declaring SLOs | have a target/budget typo caught at load instead of by a pager | degenerate always-fire rules reaching evaluation: 0; out-of-range targets and non-30d budgets caught at load: 100% | unguarded; nothing validates (slo.rs:114 has no `(0,1)` check) | acceptance test: load `target=1.0` and budget `7d`, assert refusal diagnostic + zero loaded rules from that SLO | Leading (Outcome) |
| 3 | SRE operators with existing hand-authored catalogues | adopt SLOs without losing or shadowing existing alerting | hand-authored rules silently dropped/shadowed by SLO adoption: 0 | SLOs and rules cannot coexist (an `[[slo]]` poisons its file) | acceptance test: load a mixed dir, assert combined count and a collision diagnostic; existing rule + slice_05 suites stay green | Leading (Secondary) |
| 4 | SRE operators tuning a live daemon | apply SLO edits with SIGHUP without restart, with bad edits refused not partially applied | SLO edits needing a restart: 0; bad SLO edits going partially live: 0; survivors re-paged on unrelated reload: 0 | no SLO path, so no SLO reload exists | acceptance test: SIGHUP a valid edit (assert `beacon.reload.succeeded`) and a malformed edit (assert `beacon.reload.refused` + previous catalogue retained) | Leading (Outcome) |
| 5 | the codebase / future maintainers | carry honest SLO docs | false doc claims in the SLO area: from 1 to 0 (slo.rs:49-51 made true) | slo.rs:49-51 claims a loader rejection that does not exist | doc check: slo.rs:49-51 wording matches the shipped 30d-rejection path | Guardrail (Honesty) |

### Metric Hierarchy

- **North Star**: SLO operator-reachability. A declared `[[slo]]` produces
  four correct, loaded, evaluating MWMBR rules through the product surface.
  This is the single behaviour that takes the headline feature from dead to
  alive.
- **Leading Indicators**: declared SLOs requiring zero hand-authored MWMBR
  rules (KPI 1); out-of-range/non-30d SLOs refused at load (KPI 2); SLO
  edits applied without restart (KPI 4).
- **Guardrail Metrics** (must NOT degrade):
  - degenerate always-fire rules reaching evaluation MUST stay 0 (KPI 2);
  - hand-authored rules silently dropped/shadowed MUST stay 0 (KPI 3);
  - existing rule-load and slice_05 acceptance tests MUST stay 100% green
    (the merge must not regress the hand-authored path);
  - false SLO doc claims MUST stay 0 once US-03 lands (KPI 5);
  - the ADR-0063 all-or-nothing reload contract MUST hold for SLO edits
    (no partial apply, KPI 4).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|-------------|-------------------|-----------|-------|
| 1 | beacon acceptance suite; `rules_loaded` startup log | assert four named rules loaded from one `[[slo]]` | per CI run | DISTILL / DELIVER |
| 2 | beacon acceptance suite | assert refusal diagnostic + zero loaded rules for out-of-range target and non-30d budget | per CI run | DISTILL / DELIVER |
| 3 | beacon acceptance suite + existing rule/slice_05 suites | assert mixed-dir count + collision diagnostic; existing suites green | per CI run | DISTILL / DELIVER |
| 4 | beacon-server SIGHUP acceptance suite; `beacon.reload.succeeded` / `beacon.reload.refused` events | assert success for valid edit, refusal + retained catalogue for malformed edit | per CI run | DISTILL / DELIVER |
| 5 | source-doc review | slo.rs:49-51 wording matches the shipped rejection | at feature close | DESIGN / DELIVER |

### Hypothesis

We believe that wiring the existing `synthesise_slo` engine to the rule-file
loader and the SIGHUP reload path, for SRE operators running beacon-server,
will achieve a reachable, validated, hot-reloadable SLO declaration surface.
We will know this is true when an SRE declares one `[[slo]]` table and four
correct burn-rate rules load and evaluate (KPI 1), a malformed SLO is refused
at load rather than producing an always-fire rule (KPI 2), and an SLO edit
hot-reloads under SIGHUP without restart and without partial apply (KPI 4).

### Handoff to DEVOPS (platform-architect)

- **Data collection requirements**: the `rules_loaded` startup event already
  exists (main.rs:93-98); the `beacon.reload.succeeded` / `beacon.reload.refused`
  events already exist (ADR-0063). No new instrumentation is required for
  these KPIs; they are asserted by the acceptance suite, not by a dashboard.
- **Dashboard/monitoring needs**: none new for this feature. Existing reload
  events suffice.
- **Alerting thresholds**: the guardrails (always-fire rules = 0, silent
  shadowing = 0, partial apply = 0) are enforced by acceptance tests and the
  per-feature 100% mutation gate, not by runtime alerts.
- **Baseline measurement**: the baselines above are categorical (0% / nothing
  validated / no SLO reload) and need no pre-release collection.

### Smell-test notes

These KPIs are mostly categorical (0 to 100% reachability, counts of
degenerate rules that must stay 0) rather than continuous rates, because this
is a wiring feature for a brownfield observability daemon with no end-user
analytics surface. They remain actionable and outcome-shaped: each is an
observable behaviour change the acceptance suite verifies, not an output
("ship the loader change"). The guardrails are framed as "must stay 0 / must
stay green" precisely so a regression is a hard, automatable failure.
