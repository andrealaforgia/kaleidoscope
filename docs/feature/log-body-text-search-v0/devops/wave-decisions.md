# Wave Decisions — log-body-text-search-v0 / DEVOPS

- **Wave**: DEVOPS (slim)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-27
- **Mode**: slim. The feature is a parse + wire growth of ONE optional
  query-string parameter on `GET /api/v1/logs` plus a four-line
  additive extension to `lumen::Predicate`. No new crate, no new
  external dependency, no new CI job, no new deployment artefact.
  This wave verifies the existing five-gate CI contract (ADR-0005)
  already covers the modified code paths and records the inherited
  posture. Shape and brevity mirror the immediate sibling slim
  precedent at `docs/feature/log-query-severity-filter-v0/devops/`.

## DEVOPS Decisions

| D# | Topic | Value |
|----|-------|-------|
| DD1 | deployment_target | N/A (library + thin `main.rs`; no new binary, no new container) |
| DD2 | container_orchestration | N/A (slice 01 produces no container image; the pre-existing `kaleidoscope-cli` Dockerfile is untouched) |
| DD3 | cicd_platform | inherit GitHub Actions; ADR-0005 five-gate contract unchanged |
| DD4 | existing_infrastructure | extend; no new infra; no new CI job (inherit `gate-5-mutants-log-query-api`) |
| DD5 | observability | inherit workspace gates; `lumen` already carries its internal `tracing` posture; no new metric, no new dashboard, no new alert (consistent with ADR-0050 Decision 8, ADR-0052 Decision 10, ADR-0055 Decision 13) |
| DD6 | deployment_strategy | N/A (pure trunk-based; recovery is fix-forward / git revert; the parameter is optional and the parameter-less path is byte-equal to the slice-prior response shape) |
| DD7 | continuous_learning | N/A (no live observability stack at v0/v1) |
| DD8 | git_branching | inherit pure trunk-based (project default; main has no required-status-checks and no enforce_admins) |
| DD9 | mutation_testing | inherit per-feature, 100% kill rate (CLAUDE.md, ADR-0005 Gate 5) |

## CI Inheritance

The ADR-0005 five workspace gates (Gate 1 `cargo test --workspace`,
Gate 2 `cargo public-api`, Gate 3 `cargo semver-checks`, Gate 4
`cargo deny`, Gate 5 `cargo mutants`) are inherited unchanged. No
workflow file edit. No new or amended job. The verification is
file-grounded:

- `gate-5-mutants-log-query-api` exists at
  `.github/workflows/ci.yml:1123` (CONFIRMED by grep). The job
  scopes to `crates/log-query-api/**` via the diff filter at
  `.github/workflows/ci.yml:1181`
  (`git diff "$BASELINE" HEAD -- 'crates/log-query-api/**' >
  "$DIFF_FILE"`) and runs
  `cargo mutants --package log-query-api --in-diff "$DIFF_FILE"
  --no-shuffle --jobs 2` at lines 1189-1193. The
  `--in-diff` filter naturally points the runner at
  `crates/log-query-api/src/lib.rs`, which is the sole file
  touched by this slice inside that crate (the
  `parse_body_contains` helper, the `LogsParams` field, and the
  dispatch arm). Coverage at the 100% kill-rate gate is
  inherited unchanged.

- `gate-5-mutants-lumen` does NOT exist in
  `.github/workflows/ci.yml`. The grep over the workflow file
  returns one match for `lumen`, at line 1165, inside a comment
  ("Per lumen-query-api-v0 DEVOPS (ADR-0047), this single job
  covers every src file added to log-query-api via path-filtered
  --in-diff."), NOT a job definition. The full enumeration of
  `gate-5-mutants-*` jobs in the workflow is: harness (453),
  aperture (503), spark (604), sieve (692), codex (777),
  self-observe (862), aperture-storage-sink (949), query-api
  (1036), log-query-api (1123), trace-query-api (1210), pulse
  (1297), ray (1380), strata (1463), beacon (1548),
  kaleidoscope-cli (1636), query-http-common (1722). The
  `lumen` crate is not in the list.

This is an HONEST finding that contradicts the DESIGN handoff at
`../design/wave-decisions.md:295-302`, which referenced "the
workspace-default mutants gate at the lumen crate" alongside
`gate-5-mutants-log-query-api`. The DESIGN handoff was speculative
on that point. The truthful state of the world is that the
four-line lumen `Predicate` extension (`crates/lumen/src/predicate.rs`:
one new field, one new builder, one new `matches` arm, one new
`is_empty` clause) is NOT mutation-tested by any existing
per-crate Gate 5 job. It IS covered by Gate 1 (`cargo test
--workspace`) compile-and-link of the new acceptance suite
`crates/log-query-api/tests/slice_01_body_contains.rs` (KPI-1,
KPI-2, KPI-4) and by the inline lumen predicate unit tests
declared in `../design/application-architecture.md` "Changes Per
File" (the lumen `tests` mod extension covering the matches arm,
the is_empty clause, and the conjunctive composition with
`min_severity` and `service`). A successor maintenance commit MAY
add a `gate-5-mutants-lumen` job to close this gap; that is OUT
of this slim slice's authorised scope and is recorded here as a
forward-looking item, NOT a blocker. The slice's behavioural
correctness is enforced by Gate 1 on the new acceptance + unit
suites; the slice's body-contains-arm mutation surface inside
`log-query-api` (the `parse_body_contains` helper boundaries, the
dispatch arm, the redaction substring) is enforced at 100% by
the existing `gate-5-mutants-log-query-api` job via `--in-diff`.

No new workflow file. No new job. No edit to any existing job.

## No new tooling

Zero new external dependency. Zero new workspace crate. Zero new
binary. Zero new public event name. Zero new graduation tag (the
`lumen` `Predicate` `body_contains` builder is an additive
public-surface item; `lumen` is not in Gate 2 / Gate 3's locked
set, so no semver-checks failure is produced, but the crafter
MUST snapshot the new `cargo public-api` baseline as part of
DELIVER per ADR-0055 Decision 10 and the Gate 2 posture). The
new field on `LogsParams` is private (`pub(crate)`) and does NOT
appear in any public-api diff. No `deny.toml` policy change (no
new transitive licence, no new banned-crate concern, no new
advisory window). The parse helper uses `str::contains` and a
byte-length comparison from `core`; the predicate arm uses
`String::contains` from `std`; both are already in the workspace
dependency graph.

## Outcome KPIs instrumentation

No new runtime instrumentation. The four outcome KPIs from
`../discuss/outcome-kpis.md` map directly to existing CI gates:

- **KPI-1 (substring match honesty)**: Gate 1 on the new
  acceptance suite `crates/log-query-api/tests/slice_01_body_contains.rs`,
  per-record and per-fixture-completeness assertions.
- **KPI-2 (behaviour invariance when `body_contains` absent)**:
  Gate 1 on the unchanged pre-existing suites
  `tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`,
  `tests/slice_01_severity_filter.rs`. No test deletion. No
  test rewrite. The DISTILL wave MUST add the new file as
  additive coverage and MUST NOT touch the three existing
  files.
- **KPI-3 (`query-http-common` reuse, zero re-implementation)**:
  Gate 1 on the modified `crates/log-query-api/src/lib.rs` plus
  static-grep CI assertions enumerated in
  `../discuss/outcome-kpis.md` Measurement Plan row KPI-3
  (`! grep -n 'MAX_RESULT_ROWS\s*:\s*usize' crates/log-query-api/src/`,
  `! grep -rE 'window exceeds 86400 seconds\|result exceeds 100000 rows\|no tenant resolvable' crates/log-query-api/src/`,
  `! grep -n 'fn error_response\|fn parse_time_range\|fn resolve_tenant' crates/log-query-api/src/`)
  and the under-30-LOC line-count diff on
  `crates/log-query-api/src/lib.rs`. The crafter's commit
  MUST honour the budget; the assertions execute against the
  slice-close commit relative to the slice-prior tag.
- **KPI-4 (case-sensitivity discoverable via acceptance test)**:
  Gate 1 on the named scenario
  `case_sensitive_matching_is_pinned_by_acceptance_test` (or
  the equivalent named function carrying the literal
  `case_sensitive` in its name) in
  `crates/log-query-api/tests/slice_01_body_contains.rs`.

No new dashboard, no new counter, no new tracing event beyond the
existing `tracing::error!` calls in the 500 arm. Consistent with
ADR-0050 Decision 8 and ADR-0055 Decision 13: at v0/v1 the
platform has no live observability stack of its own; a
contract-shaped outcome IS the signal.

## Inherited from slim precedent

This wave inherits the structure and the per-decision shape of
`docs/feature/log-query-severity-filter-v0/devops/wave-decisions.md`
(slim DEVOPS, 2026-05-27). The severity-filter slice is the
immediate sibling: a lumen-predicate-extending feature without a
new crate, without a new dependency, and without a new CI job,
verified against the same ADR-0005 contract. The structural
parallel is intentional: both slices add one optional query-string
parameter to `GET /api/v1/logs`, both extend `lumen::Predicate`
additively (severity-filter consumed the pre-existing
`min_severity` builder; body-contains adds one new builder), and
both compose conjunctively via `Predicate::matches`. The DEVOPS
posture is therefore identical at the workflow / dependency /
deployment layers, with one honest divergence recorded above:
the severity-filter slice did NOT touch any file in
`crates/lumen/` (it consumed the pre-existing
`Predicate::min_severity` builder), so its slim wave did not
need to observe a lumen-coverage gap. This slice DOES touch
`crates/lumen/src/predicate.rs` and so surfaces the
`gate-5-mutants-lumen` absence as a CI Inheritance note above.

## Upstream Changes

None. Zero DISCUSS assumptions changed by this DEVOPS wave. Zero
DESIGN assumptions changed (the DESIGN handoff's reference to a
`gate-5-mutants-lumen` job is corrected as a CI observation, NOT
a design retraction: the design's correctness is independent of
which Gate 5 job exercises mutations on `lumen/src/predicate.rs`,
and the design's KPI-3 + Gate 1 coverage is sufficient for slice
01). The slice composes additively on top of ADR-0047, ADR-0050,
ADR-0052, ADR-0054, and ADR-0055 without altering any of them.
