# Outcome KPIs — query-http-common-v0

## Feature: query-http-common-v0

### Objective

Single-source the read-side HTTP scaffolding (cap constants, time-range
parser, error envelope helper, fail-closed tenant pattern) across the three
pillar read APIs, so that any future scaffolding change is a one-file edit
rather than a three-place lockstep edit, without changing any user-observable
read-side behaviour.

### Outcome KPIs

| #  | Who                                          | Does what                                                                       | By how much                                                       | Baseline                                                        | Measured by                                                                                                            | Type    |
|----|----------------------------------------------|---------------------------------------------------------------------------------|-------------------------------------------------------------------|-----------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------|---------|
| K1 | The workspace test suite                     | Stays green with no test-count regression after each slice                      | Zero failing tests; pre-feature test count = post-feature count   | `cargo test --workspace` test count at the head of feature start | `cargo test --workspace` exit code 0 and test-count parity (counts read from `cargo test --workspace --no-fail-fast` summary) | Leading |
| K2 | The three consumer read APIs                 | Emit byte-identical response bodies on every existing 400 and 401 acceptance test | Zero byte differences; status codes identical                     | Pre-feature acceptance suite recordings on 400/401 arms          | A snapshot diff per arm (or equivalent assertion in the existing acceptance tests; they already pin literal body strings) | Leading |
| K3 | The read-side scaffolding LOC count          | Shrinks across the three consumer crates                                          | From approximately 90 lines to ≤ 30 lines (≥ 67% reduction)        | Approximately 90 lines pre-feature                              | A small grep-and-sum shell pipeline counting lines matching the six scaffold patterns in the three consumers' `src/lib.rs` (pattern list lives in the slice brief produced by DESIGN) | Leading |
| K4 | The new `query-http-common` crate            | Reaches the workspace mutation gate                                              | 100% kill rate (per ADR-0005 Gate 5)                              | N/A (new crate)                                                 | `cargo mutants -p query-http-common --no-shuffle` reporting zero MISSED, zero TIMEOUT, zero UNVIABLE survivors           | Leading |

All four KPIs are LEADING indicators because they measure properties
observable immediately at slice landing, not downstream business metrics.
This is an internal refactor; there are no business-KPI consequences to
measure.

### Metric hierarchy

- **North star**: K3 (scaffolding LOC reduction) is the headline outcome.
  It is the single metric that captures whether the refactor actually
  collapsed duplication or merely shuffled it.
- **Leading indicators**: K1 (test regressions), K2 (byte identity), and
  K4 (mutation kill rate) are the safety properties; together they assert
  the refactor changed structure without changing behaviour.
- **Guardrail metrics**:
  - K1 must be `0 failing tests` and `test count >= pre-feature count`.
    A drop in test count (even with a green run) is a regression because
    it suggests a test was silently dropped during migration.
  - K2 must be `0 byte differences` on every recorded 400/401 arm.
  - K4 must be `100% kill rate` on `query-http-common`. Anything less
    blocks closure of US-05.

### Measurement plan

| KPI | Data source                                                         | Collection method                                                                                                  | Frequency                                       | Owner                |
|-----|---------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|----------------------|
| K1  | `cargo test --workspace --no-fail-fast` exit code and summary line  | Run pre-feature (record test count) and after each slice (compare)                                                 | Per slice                                       | nw-software-crafter   |
| K2  | Existing acceptance suite assertions across the three consumer crates | The pre-extraction assertions already pin the literal response body bytes; running them post-extraction is the measurement | Per slice (post-extraction)                     | nw-software-crafter   |
| K3  | Source files in `crates/{query,log-query,trace-query}-api/src/`     | Shell pipeline (`grep -E "<pattern1>\|<pattern2>\|..." crates/*-api/src/lib.rs \| wc -l`); exact pattern list in slice brief | Pre-feature (baseline) and after US-05          | nw-product-owner (DISCUSS) records pre; nw-software-crafter records post |
| K4  | `cargo mutants -p query-http-common`                                | Run in CI on the `query-http-common-v0` branch and locally before close                                            | Once at US-05 (Gate 5 per ADR-0005)             | nw-software-crafter (DELIVER); nw-quality-orchestrator (Gate 5) |

### Hypothesis

We believe that extracting the read-side HTTP scaffolding into
`query-http-common` for the read-side maintainer will achieve a one-file
seam for the cap-policy, parser, error-envelope, and fail-closed-tenancy
concerns.

We will know this is true when the read-side scaffolding LOC across the
three consumer crates (K3) falls to ≤ 30, the workspace test suite stays
green with no test-count regression (K1), every existing 400 and 401
response body is byte-identical pre and post (K2), and the new crate
reaches 100% mutation kill rate (K4).

### KPIs NOT covered (and why)

- No user-behaviour KPI is defined because this feature has no
  user-observable behaviour change. Adding a synthetic user-behaviour KPI
  would be either dishonest (the feature does not move user behaviour) or
  decorative (a metric that always equals 1.0 because the feature is
  silent on the wire).
- No business-impact (lagging) KPI is defined for the same reason. The
  business impact, if any, is in MAINTAINER VELOCITY on future read-side
  changes, which K3 captures indirectly (fewer scattered edits => faster
  future edits).

### Handoff to DEVOPS

The `nw-platform-architect` (DEVOPS) reads this file. The instrumentation
implications are minimal:

- No new dashboards or alerts. The four KPIs are measured by `cargo` and
  by shell grep, not by runtime telemetry.
- The Gate 5 mutation kill rate (K4) is the only KPI that touches CI: a
  `gate-5-mutants-query-http-common` job is the expected addition,
  following the pattern established by ADR-0048 ("the
  `gate-5-mutants-trace-query-api` job and a new per-crate tag at
  graduation"). This is a DEVOPS concern at slice closure, not a runtime
  observability concern.
