# Outcome KPIs: earned-trust-fsync-probe-v0

British English. No em dashes. No emoji.

## Feature objective

The platform refuses to bind a listener on a substrate that does not
honour fsync, restoring the meaning of the Earned-Trust principle
recorded in ADR-0042 Decision 8, ADR-0047 Decision 6, and ADR-0048
Decision 8. The chosen pillar's binary (slice 01; pillar choice
FLAGGED to DESIGN) is the walking-skeleton entry point. Later
slices extend the same shape to the remaining pillars.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|---|---|---|---|---|---|
| 1 | An operator of the chosen pillar's binary | sees the binary refuse to bind on any substrate whose fsync is a no-op (or which silently loses bytes on fsync) | 100 percent of fsync-lying-substrate startups in the acceptance suite (Scenario 2, Scenario 3 of US-01) refuse to bind | 0 percent today (existing `composition::probe()` accepts every fsync-lying substrate; workspace grep confirms zero `fsync` / `sync_data` / `sync_all` / `FsyncProbe` calls in `crates/`) | Acceptance-test pass / fail in the slice-01 suite | Leading (outcome) |
| 2 | An operator of the chosen pillar's binary | sees the binary bind cleanly on every honest substrate (no false positives) | 100 percent of honest-substrate startups in the acceptance suite (Scenario 1 of US-01) bind | 100 percent today (the binary binds today on every substrate, honest or lying) | Acceptance-test pass / fail in the slice-01 suite; the guardrail KPI below | Leading (outcome) |
| 3 | A Kaleidoscope developer maintaining the changed crate | sees the per-crate mutation gate stay at 100 percent kill on the changed files | 100 percent (ADR-0005 Gate 5; CLAUDE.md) | 100 percent today on the unchanged code; the new code does not exist yet | `cargo mutants` per-crate output on changed files | Leading (secondary) |
| 4 | A reviewer comparing the public-api diff of the chosen pillar's storage trait against the prior tag | sees no change to `LogStore`, `MetricStore`, `TraceStore`, or beacon `RuleStateStore` | 0 trait signature changes | 0 today | The `gate-2-public-api` diff (ADR-0005 Gate 2) | Guardrail |

## Metric hierarchy

- **North star**: KPI 1, "the binary refuses to bind on a lying
  substrate, 100 percent in the acceptance suite". The whole feature
  exists to move this from 0 to 100.
- **Leading indicators**: KPI 2 (no false positives on honest
  substrates) and KPI 3 (mutation kill rate stays at 100 percent on
  the changed files).
- **Guardrail metrics**: KPI 4 (no storage trait change; the probe
  rides outside the trait); the existing `composition::probe()`
  regression test must continue to pass (Scenario 4 of US-01).

## Measurement plan

| KPI | Data source | Collection method | Frequency | Owner |
|---|---|---|---|---|
| 1 | Slice-01 acceptance suite for the chosen pillar's crate | `cargo test` in CI (Gate 1 of ADR-0005) plus the explicit fsync-lying-substrate scenarios | Every push (CI is feedback per the project's trunk-based stance, not a gate; the visible signal is the test outcome on PR / push) | Crafty (DELIVER) writes the tests; Bea (DOCUMENT) records the closure in the per-feature narrative |
| 2 | Same suite | Same | Same | Same |
| 3 | `cargo mutants` per-crate workflow on the chosen pillar's crate | The per-crate Gate 5 workflow already in CI | Every push | Apex (DEVOPS) maintains the workflow; Crafty (DELIVER) keeps the kill rate at 100 percent on changed files |
| 4 | `cargo public-api` diff against the prior tag | Gate 2 in the per-crate CI | Every push | Apex |

## Hypothesis

We believe that an fsync-honesty probe at startup of ONE pillar's
binary, written as the cheapest portable behavioural test (write +
fsync + drop handle + re-open + verify) rather than the syscall-
inspection or true-crash routes, will let the platform refuse to
serve over a lying substrate, restoring the meaning of the
Earned-Trust principle.

We will know this is true when an operator (or the acceptance
suite) observes the chosen pillar's binary refusing to bind on every
fsync-lying-substrate scenario AND binding cleanly on every
honest-substrate scenario, with 100 percent mutation kill on the
changed files.

We will know this is false if the behavioural probe leaves
documented false negatives in the field (cases where it passes on a
substrate that subsequently lost data on a real crash), in which
case the escalation path to a fork+SIGKILL true-crash probe is
documented and reserved for a successor slice.

## Smell-test review

| Check | Verdict | Note |
|---|---|---|
| Measurable today? | Yes | Acceptance-test outcomes and `cargo mutants` kill rate are both already collected by the platform's existing CI surface. |
| Rate not total? | Yes | "100 percent of fsync-lying-substrate startups refuse" is a rate over scenarios; not a gross count. |
| Outcome not output? | Yes | The KPI describes the operator's observable behaviour change (the binary refuses), not the shipped artefact ("we shipped a probe"). |
| Has baseline? | Yes | 0 percent today, with the workspace-grep evidence cited in the residuality analysis and re-verified during DISCUSS. |
| Team can influence? | Yes | The team owns the probe code and the seam; nothing external. |
| Has guardrails? | Yes | KPI 2 (no false positives on honest substrates), KPI 4 (no trait change), plus the existing `composition::probe()` regression test (Scenario 4 of US-01). |

## Handoff to DEVOPS

Per the residuality follow-up roadmap, DEVOPS (Apex) for this
feature is slim: there is no new crate (slice 01 lands inside the
DESIGN-chosen pillar's existing crate) and no new dependency
expected. The DEVOPS surface this feature touches:

- **Data collection**: none new. The refusal rides on the existing
  `event=health.startup.refused` structured event; the substrate
  descriptor is a new field on the existing event payload.
- **Dashboards / monitoring**: none new at v0/v1. The platform has
  no live observability stack of its own yet.
- **Alerting thresholds**: none. A startup refusal IS the alert; the
  operator sees the non-zero exit code and the event in stdout / a
  log sink.
- **Baseline measurement**: the workspace grep cited in KPI 1's
  baseline column suffices; no separate baseline collection needed.

## Connection to the residuality analysis

The KPIs above map directly onto the analysis's incidence-matrix
S02 row (the most informative row, per the analysis): every cell
that reads "B silent loss possible (A-U1)" today becomes "S
substrate refused at startup" once slice 01 lands and slice 02+
extends to the remaining pillars. KPI 1 is the rate at which this
matrix transition completes for the slice-01 pillar; KPI 2 + KPI 3
+ KPI 4 are the guardrails that the transition does not cost
anything elsewhere in the matrix.
