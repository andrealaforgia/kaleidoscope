# Wave Decisions - trace-lookup-by-id-v0 / DEVOPS

British English. No em dashes.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-27
- **Mode**: slim DEVOPS. The feature is a parse + wire growth of ONE
  sibling route on the existing `crates/trace-query-api` crate. No
  new crate, no new dependency, no new binary, no new graduation tag,
  no new CI job, no new workflow file, no new env variable. Apex's
  job is to record the inheritance verdicts, not re-litigate them.

## DEVOPS Decisions

| #   | Decision                          | Verdict       | Rationale                                                                                                                                                                            |
|-----|-----------------------------------|---------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| DD1 | deployment_target                 | N/A           | No new deployment artefact. The existing `trace-query-api` binary serves the new sibling route on the same listener; no new container, no new listening port.                        |
| DD2 | container_orchestration           | N/A           | Slice 01 produces no container image. The pre-existing `kaleidoscope-cli` Dockerfile is unrelated and untouched.                                                                     |
| DD3 | cicd_platform                     | inherit       | GitHub Actions per ADR-0005's five-gate workspace contract. No workflow file is added or amended.                                                                                    |
| DD4 | existing_infrastructure           | inherit       | Workspace + five-gate CI present. `gate-5-mutants-trace-query-api` job already exists; the diff filter `crates/trace-query-api/**` naturally covers the modified `lib.rs`.           |
| DD5 | observability_and_logging         | inherit       | No new instrumentation. The path is observable via `ray::TraceStore`'s pre-existing `record_query` recorder seam and the existing `traces.store.failed` `tracing::error!` event.     |
| DD6 | deployment_strategy               | N/A           | No new deployment. Recovery is git revert; the new route is purely additive on a sibling path so a revert has no data consequence on the existing route.                             |
| DD7 | continuous_learning               | N/A           | At v0/v1 there is no live observability stack of its own; the contract IS the signal (ADR-0050 Decision 8, ADR-0052 Decision 10, ADR-0053 Consequences).                             |
| DD8 | git_branching_strategy            | trunk-based   | Project default per project memory `project_kaleidoscope_pure_trunk_based`. Main has no required-status-checks and no enforce_admins; CI is feedback, not a gate.                    |
| DD9 | mutation_testing_strategy         | inherit       | Per-feature, 100% kill rate per project CLAUDE.md and ADR-0005 Gate 5. No change to project CLAUDE.md; the existing `gate-5-mutants-trace-query-api` job covers the modified `lib.rs`. |

## CI Inheritance

ADR-0005's five workspace gates are inherited unchanged. Gate 1
(`cargo test --workspace`) picks up the new acceptance suite at
`crates/trace-query-api/tests/slice_02_traces_lookup_by_id.rs` via
file-name discovery. Gate 2 (`cargo public-api`) and Gate 3
(`cargo semver-checks`) scope to the workspace's locked set; the
`trace-query-api` `pub` surface (`router`, `TRACES_ROUTE`,
`MAX_WINDOW_SECONDS`, `MAX_RESULT_ROWS`) is byte-identical to the
prior tag, and the new items (`TRACES_BY_ID_ROUTE` const,
`handle_traces_by_id`, `TracesByIdParams`, `parse_trace_id`) are all
module-private. The `ray::TraceStore` trait signatures are
byte-identical (DESIGN `wave-decisions.md` D9). Gate 4
(`cargo deny`) is unaffected: no new external dependency, no
`deny.toml` edit. Gate 5 (`cargo mutants`) runs via the existing
`gate-5-mutants-trace-query-api` job whose diff filter
`crates/trace-query-api/**` and `cargo mutants --package
trace-query-api --in-diff "$DIFF_FILE"` invocation naturally pick up
the modified `lib.rs` (the sole `src/` file changed by this slice)
and the new sibling test file. No new workflow file, no new job, no
modification to `.github/workflows/`.

## No new tooling

No new external crate dependency. No new feature flag on existing
dependencies. No new binary. No new crate. No new graduation tag
(`trace-query-api` is not in Gate 2 / Gate 3's locked set; no `pub`
surface diff regardless). The outcome KPIs in
`../discuss/outcome-kpis.md` are already covered by the
strumentation internal to `ray::TraceStore`: the `record_query`
recorder seam on the `InMemoryTraceStore` adapter at
`crates/ray/src/store.rs:190` continues to record query duration and
returned span count for `get_trace`; the `FileBackedTraceStore`
adapter inherits the same recorder surface. No new recorder method,
no new label, no new dashboard.

## Outcome KPIs instrumentation

No additional instrumentation. The five outcome KPIs land as
correctness signals on the new acceptance suite (DISTILL output) and
as latency/error observability via the existing trace-query-api
structured logs:

- KPI-1 (one HTTP call pivot from `trace_id` to spans), KPI-2 (100%
  field fidelity), and KPI-4 (tenant fail-closed and zero cross-
  tenant leak) are CI-gated by Gate 1 on the new acceptance suite
  (the by-id walking skeleton, the field-fidelity scenario, the no-
  tenant 401 scenario, the cross-tenant isolation scenario, the no-
  store-call assertion via the existing `FailingTraceStore` double).
- KPI-3 (p95 lookup latency at most 200 ms on GitHub Actions
  ubuntu-latest for a trace of <= 1000 spans) is collected by a
  timed acceptance test in CI (ubuntu-latest) cross-checked with the
  store's own `record_query` recorder; no new recorder, no new
  histogram.
- KPI-5 (redaction on the 400 arm; no raw `trace_id` echoed; no
  store call on the 400 path) is CI-gated by Gate 1's redaction
  acceptance test plus the `FailingTraceStore` no-store-call
  assertion, and CI-gated by Gate 5's mutation budget on the
  modified `lib.rs` (the literal class label `"invalid trace_id"`,
  the `raw.len() != 32` boundary, the per-byte hex decode, and the
  order of checks tenancy -> presence -> parse -> store -> cap ->
  serialise).

The 400/401/200-empty arms are observable via the existing
structured logs of `trace-query-api`; no new event vocabulary, no
new metric, no new alert threshold. Refusals ride on the existing
`{status:"error", error:"<reason>"}` envelope (ADR-0048 Decision 2;
preserved on the new arm per ADR-0053 Decision 2).

## Inherited from slim precedent

This wave mirrors the slim shape of
`docs/feature/log-query-severity-filter-v0/devops/wave-decisions.md`
verbatim: two artefacts (this file and `environments.yaml`), no
`kpi-instrumentation.md`, no `ci-cd-pipeline.md`, no
`observability-design.md`, no `monitoring-alerting.md`, no
`branching-strategy.md`. The precedent's "Artefacts judged N/A"
verdicts apply unchanged: a parse + wire growth on one existing
crate with no new deployment surface, no external integrations, and
no new dependency does not warrant per-artefact restatement of the
same inheritance verdicts.

## Upstream Changes

None. The DESIGN wave's decisions (the four DISCUSS-wave flags PINNED
as recommended, the parse + wire micro-decisions D5-D9, the DEVOPS
Handoff Annotation) are honoured verbatim. No DEVOPS finding
contradicts a DESIGN or DISCUSS assumption.
