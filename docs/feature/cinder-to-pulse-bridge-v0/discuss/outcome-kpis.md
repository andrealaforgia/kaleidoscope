# Outcome KPIs — `cinder-to-pulse-bridge-v0`

## Feature

`cinder-to-pulse-bridge-v0` — bridge that wires Cinder's
`MetricsRecorder` events into a Pulse `MetricStore`.

## Objective

Eliminate the Cinder observability void by making every tier-management
event queryable through the same Pulse `MetricStore::query` API the
operator already uses for Lumen events. Library-level outcome; CLI
exposure follows in the next feature.

## Note on KPI granularity at v0

This feature is library-only. The operator persona (Priya) cannot directly
exercise the bridge without the post-v0 CLI wiring feature. Therefore the
"behaviour change" KPIs land at the **library contract level**, measured
through acceptance tests and through the post-v0 CLI feature's adoption.

The KPIs below distinguish:

- **Now-measurable (acceptance-test level)**: 100% of the Cinder event
  types have a documented Pulse query that returns the expected shape.
  This is measured by green tests in
  `crates/self-observe/tests/cinder_to_pulse.rs` at the close of DELIVER.
- **Post-v0-measurable (operator behaviour)**: time-to-first-answer for
  Priya's Cinder tier questions. Measured when the CLI follow-up ships.
  The bridge is the substrate; the CLI is the surface; the operator-
  behaviour metric is necessarily downstream.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1 | Platform operator (Priya) | Receives a queryable `cinder.place.count` Pulse point per tenant per `place` call | 100% of `place` calls produce exactly one point (no drops, no duplicates) | 0% (NoopRecorder swallows everything) | Acceptance tests Slice 01 | Leading (library contract) |
| OK2 | Platform operator (Priya) | Receives a queryable `cinder.migrate.count` Pulse point per tenant per successful `migrate` call, with correct `from`/`to` attributes | 100% of successful `migrate` calls produce exactly one point with attributes matching the call arguments; 0% of failed (`UnknownItem`) calls produce any point | 0% (NoopRecorder) | Acceptance tests Slice 02 | Leading (library contract) |
| OK3 | Platform operator (Priya) | Receives a queryable `cinder.evaluate.migrated.count` Pulse point per (tenant, evaluate_at call) pair where at least one item was migrated for that tenant, with `value` equal to that tenant's migrated count | 100% of (tenant, evaluate) pairs with N>=1 migrations produce exactly one point with value=N; 0% of (tenant, evaluate) pairs with 0 migrations produce a point | 0% (NoopRecorder) | Acceptance tests Slice 03 | Leading (library contract) |
| OK4 | Cinder operations in production | Continue to run with identical behaviour and identical error semantics when CinderToPulseRecorder is substituted for NoopRecorder | Zero observable change in Cinder's user-facing API behaviour | n/a (baseline = current Noop behaviour) | All slices: every test calls the Cinder API the same way it would with NoopRecorder, and asserts unchanged Cinder return values | Guardrail |

## Metric Hierarchy

- **North Star (v0 library scope)**: "Every Cinder event type produces
  a queryable Pulse point under the calling tenant with the documented
  shape." Measured via 100% green acceptance tests in
  `crates/self-observe/tests/cinder_to_pulse.rs`.
- **Leading Indicators**: OK1, OK2, OK3 above — each event type's
  emission contract.
- **Guardrail Metrics**: OK4 — Cinder's user-facing behaviour does not
  change. The bridge is observably transparent at Cinder's API boundary.

## Post-v0 outcome KPIs (deferred to the CLI follow-up feature)

When `kaleidoscope-cli-wires-cinder-bridge-v0` (or its successor name)
ships, these become measurable. Listed here for traceability:

- **OK1-CLI**: Time-to-first-answer for "how many Hot->Warm migrations
  did tenant `acme` see in the last hour?" via the operator CLI. Target:
  <30 seconds for an operator who has used the Lumen query pattern. Baseline:
  N/A (today the question is unanswerable without source modification).
- **OK2-CLI**: Number of operators reporting "I had to add `println!` to
  Cinder to debug tier movements" in the 90 days following CLI ship.
  Target: 0. Baseline: ad hoc (no formal count today).

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1 | `crates/self-observe/tests/cinder_to_pulse.rs` (Slice 01 tests) | `cargo test --package self-observe --test cinder_to_pulse` exit code | At every commit touching the bridge | self-observe crate maintainer (CI feedback per ADR-0005) |
| OK2 | Same test file (Slice 02 tests) | Same | Same | Same |
| OK3 | Same test file (Slice 03 tests) | Same | Same | Same |
| OK4 | The fact that Slice 01/02/03 tests construct Cinder identically to its prior usage with NoopRecorder | Code review at DESIGN handoff + DELIVER review | Per wave close | Reviewer |
| OK1-CLI / OK2-CLI | CLI follow-up feature's instrumentation | TBD by that feature | Post-v0 | TBD |

## Hypothesis

We believe that **wiring Cinder's `MetricsRecorder` events into the same
Pulse `MetricStore` the operator already uses for Lumen** for the
**platform operator** will achieve **a queryable per-tenant + per-event-
type view of Cinder's tier-management activity**.

We will know this is true when:
- 100% of acceptance tests in `cinder_to_pulse.rs` pass green (library
  contract held — measurable at DELIVER close).
- Following the post-v0 CLI follow-up: operators answer Cinder tier
  questions with the same query idiom they already use for Lumen, with
  no source modification of Cinder.

## Handoff to DEVOPS / DESIGN

The DESIGN wave should preserve:

1. The three exact metric names as the public emission contract:
   `cinder.place.count`, `cinder.migrate.count`,
   `cinder.evaluate.migrated.count`.
2. The lowercase tier serialisation convention (`hot`/`warm`/`cold`).
3. The `value=migrated as f64` encoding on
   `cinder.evaluate.migrated.count` (NOT `value=1` + attribute).
4. The best-effort emission posture (matching the Lumen bridge), with a
   forward-compatibility note for v1 when `MetricStoreError` may grow
   real variants.

DEVOPS instrumentation needs (post-v0 CLI feature, not this one): no
new collection infrastructure. The bridge emits into the same Pulse
instance that already collects Lumen metrics; the operator's existing
dashboards extend by adding `cinder.*` panels.
