# Wave Decisions — log-query-pagination-v0 / DEVOPS

- **Wave**: DEVOPS (slim)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-30
- **Mode**: slim. The feature is a parse plus wire growth of TWO optional
  query-string parameters (`limit`, `offset`) on `GET /api/v1/logs`,
  applied as a handler-side `skip(offset).take(limit)` slice over the
  `Vec<LogRecord>` the store already returns. The change is confined to
  `crates/log-query-api/src/lib.rs` (two private `LogsParams` fields, two
  new parse helpers, two parse arms, one slice expression). `lumen` is
  NOT touched: no trait method, no predicate field, no adapter edit. No
  new crate, no new workspace member, no new CI job, no new dependency,
  no `Cargo.lock` diff, no new deployment artefact. This wave verifies
  that the existing five-gate CI contract (ADR-0005) covers the modified
  code path and records the inherited posture. Shape and brevity mirror
  the immediate sibling slim precedent at
  `docs/feature/log-body-regex-search-v0/devops/wave-decisions.md`.

## DEVOPS Decisions

| DD# | Topic | Value |
|-----|-------|-------|
| DD1 | deployment_target | N/A (library extension; no new binary, no new container) |
| DD2 | container_orchestration | N/A (this slice produces no container image; the pre-existing Dockerfile is untouched) |
| DD3 | cicd_platform | inherit GitHub Actions; ADR-0005 five-gate contract unchanged |
| DD4 | existing_infrastructure | extend; no new dep, no new infra, no new CI job |
| DD5 | observability | inherit workspace gates; no new instrumentation (no new metric, no new dashboard, no new alert; ADR-0050 Decision 8, ADR-0057) |
| DD6 | deployment_strategy | N/A (pure trunk-based; recovery is fix-forward / git revert; both parameters are optional and the parameter-less path is byte-equal to the slice-prior response) |
| DD7 | continuous_learning | N/A (no live observability stack at v0/v1) |
| DD8 | git_branching | inherit pure trunk-based (project default; main has no required-status-checks and no enforce_admins) |
| DD9 | mutation_testing | inherit per-feature, 100% kill rate (CLAUDE.md, ADR-0005 Gate 5); covered by `gate-5-mutants-log-query-api` (line 1123) via `--in-diff` |

## CI Inheritance

The ADR-0005 five workspace gates (Gate 1 `cargo test --workspace`,
Gate 2 `cargo public-api`, Gate 3 `cargo semver-checks`, Gate 4
`cargo deny`, Gate 5 `cargo mutants`) are inherited unchanged. No
workflow file edit. No new or amended job. The verification is
file-grounded: `gate-5-mutants-log-query-api` exists at
`.github/workflows/ci.yml:1123` (CONFIRMED by grep). The job filters
the diff with `git diff "$BASELINE" HEAD -- 'crates/log-query-api/**'`
and runs `cargo mutants --in-diff` over that diff. The `--in-diff`
filter naturally points the runner at
`crates/log-query-api/src/lib.rs`, the sole file touched by this slice
(the `parse_limit` and `parse_offset` helpers, the two new `LogsParams`
fields, the two parse arms, and the `skip(offset).take(limit)` slice).
Primary mutation targets follow the design handoff: the `>` to `>=`
over-cap boundary (US-05c), the zero-rejection on `limit` (US-05a),
the `skip` / `take` off-by-one (US-04 honesty), the
per-tenant-scope-before-slice order (US-07), and the no-store-call
order on the invalid-parse arms (K4). Because `lumen` is NOT touched,
`gate-5-mutants-lumen` (line 1210) sees an empty diff for this feature
and short-circuits; it is not relevant to this slice.

## No new tooling

Zero new workspace crate. Zero new binary. Zero new public event name.
Zero new graduation tag. Zero new dependency: the slice uses only the
standard library (`str::parse`, `Iterator::skip`, `Iterator::take`)
plus the already-present `axum` / `serde` and the in-workspace
`query-http-common`. No `Cargo.toml` edit in any crate; no `Cargo.lock`
diff. Zero `deny.toml` policy change. The two new `LogsParams` fields
are private (`pub(crate)`) and do not appear in any `cargo public-api`
diff; Gate 2 on `log-query-api` shows zero drift.

## Outcome KPIs instrumentation

No new runtime instrumentation. The five outcome KPIs from
`../discuss/outcome-kpis.md` are covered by Gate 1 acceptance tests and
Gate 5 mutation testing, not by a runtime counter or dashboard. K1
(behaviour invariance when `limit` / `offset` absent) and K2
(pagination honesty: no duplicate, no gap, no cross-tenant leak) and K5
(cap interaction; refuse not truncate) are covered by the new DISTILL
acceptance suite `crates/log-query-api/tests/slice_01_pagination.rs`
plus the unchanged pre-existing suites (`slice_01_logs_read`,
`slice_02_caps`, `slice_01_severity_filter`, `slice_01_body_contains`,
`slice_01_body_regex`), which stay green unchanged. K3
(`query-http-common` reuse confirmed) is covered by Gate 1 plus the
static-grep CI assertions. K4 (invalid `limit` / `offset` return 400
fast, no store hit) is covered by the US-05 acceptance tests with a
no-store-call assertion. Consistent with ADR-0050 Decision 8 and
ADR-0057: at v0/v1 the platform has no live observability stack of its
own; a contract-shaped outcome IS the signal.

## Inherited from slim precedent

This wave inherits the structure and the per-decision shape of
`docs/feature/log-body-regex-search-v0/devops/wave-decisions.md` (slim
DEVOPS, 2026-05-29). That body-regex slice is the immediate sibling: a
parse-and-wire growth of one optional query-string parameter on the
same route, verified against the same ADR-0005 contract, with the same
GREEN-suites-plus-mutation-gate posture. This slice differs in KIND
(pagination is a windowing slice over the result vector, not a filter
over `lumen::Predicate`), so it touches ONLY `log-query-api` and leaves
`lumen` untouched; the DEVOPS posture at the workflow and deployment
layers is otherwise identical.

## Upstream Changes

None. Zero DISCUSS assumptions changed by this DEVOPS wave. Zero DESIGN
assumptions changed: the DESIGN handoff at `../design/wave-decisions.md`
(the `gate-5-mutants-log-query-api` coverage claim and the no-new-dep,
lumen-untouched posture) is ratified verbatim. The slice composes
additively on top of ADR-0047, ADR-0050, ADR-0052, ADR-0054, ADR-0055,
ADR-0056, and ADR-0057 without altering any of them.
