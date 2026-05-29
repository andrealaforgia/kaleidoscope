# Wave Decisions — log-body-regex-search-v0 / DEVOPS

- **Wave**: DEVOPS (slim)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-29
- **Mode**: slim. The feature is a parse + wire growth of ONE optional
  query-string parameter on `GET /api/v1/logs` plus a four-line additive
  extension to `lumen::Predicate` (one field, one builder, one `matches`
  arm, one `is_empty` clause). No new crate, no new workspace member, no
  new CI job, no new deployment artefact. This wave verifies that the
  existing five-gate CI contract (ADR-0005) covers the modified code
  paths and records the inherited posture. Shape and brevity mirror the
  immediate sibling slim precedent at
  `docs/feature/log-body-text-search-v0/devops/wave-decisions.md`.

## DEVOPS Decisions

| D# | Topic | Value |
|----|-------|-------|
| DD1 | deployment_target | N/A (library extension; no new binary, no new container) |
| DD2 | container_orchestration | N/A (slice 01 produces no container image; the pre-existing `kaleidoscope-cli` Dockerfile is untouched) |
| DD3 | cicd_platform | inherit GitHub Actions; ADR-0005 five-gate contract unchanged |
| DD4 | existing_infrastructure | extend; one new lumen direct dep (`regex = "1"`); no new infra; no new CI job |
| DD5 | observability | inherit workspace gates; observability invariata; existing instrumentation covers (no new metric, no new dashboard, no new alert, consistent with ADR-0050 Decision 8, ADR-0055 Decision 13, ADR-0056 Decision 14) |
| DD6 | deployment_strategy | N/A (pure trunk-based; recovery is fix-forward / git revert; the parameter is optional and the parameter-less path is byte-equal to the slice-prior response shape) |
| DD7 | continuous_learning | N/A (no live observability stack at v0/v1) |
| DD8 | git_branching | inherit pure trunk-based (project default; main has no required-status-checks and no enforce_admins) |
| DD9 | mutation_testing | inherit per-feature, 100% kill rate (CLAUDE.md, ADR-0005 Gate 5); covered by `gate-5-mutants-lumen` (line 1210) + `gate-5-mutants-log-query-api` (line 1123) via `--in-diff` |

## CI Inheritance, Both Gates Cover

The ADR-0005 five workspace gates (Gate 1 `cargo test --workspace`,
Gate 2 `cargo public-api`, Gate 3 `cargo semver-checks`, Gate 4
`cargo deny`, Gate 5 `cargo mutants`) are inherited unchanged. No
workflow file edit. No new or amended job. The verification is
file-grounded:

- `gate-5-mutants-log-query-api` exists at
  `.github/workflows/ci.yml:1123` (CONFIRMED by grep). The job scopes
  to `crates/log-query-api/**` via the diff filter and runs
  `cargo mutants --package log-query-api --in-diff "$DIFF_FILE"
  --no-shuffle --jobs 2`. The `--in-diff` filter naturally points the
  runner at `crates/log-query-api/src/lib.rs`, which is the sole file
  touched by this slice inside that crate (the `parse_body_regex`
  helper, the `MAX_BODY_REGEX_LEN` constant, the new `LogsParams`
  field, the mutual-exclusion check, the new dispatch arms).

- `gate-5-mutants-lumen` exists at `.github/workflows/ci.yml:1210`
  (CONFIRMED by grep). The job scopes to `crates/lumen/**` via the
  diff filter and runs `cargo mutants --package lumen --in-diff
  "$DIFF_FILE" --no-shuffle --jobs 2`. The `--in-diff` filter
  naturally points the runner at `crates/lumen/src/predicate.rs`,
  which is the sole src file touched by this slice inside that crate
  (the new `body_regex: Option<Regex>` field, the new `body_regex`
  builder, the new arm in `matches`, the new clause in `is_empty`,
  and the `#[derive(...)]` relaxation per ADR-0056 Decision 4).

This is the CLOSURE OF THE LOOP. The gap noted in
`docs/feature/log-body-text-search-v0/devops/wave-decisions.md`
(commit `cf0ac15`, slim sibling that surfaced the absent
`gate-5-mutants-lumen` job) was closed two wakeups ago by the
`gate-5-mutants-lumen-v0` feature (commit `d96a807`), which shipped
the workflow at line 1210. This slice is the FIRST consumer of that
gate: the four-line lumen `Predicate` extension introduced here is
exactly the mutation surface d96a807 was designed to audit. The
investment in CI hygiene made one feature ago now pays its first
rendita without any further action on this wave. The legible chain
is the value of the discipline: `cf0ac15` (gap noted) -> `d96a807`
(gap closed) -> here (gap exercised).

## No new tooling

Zero new workspace crate. Zero new binary. Zero new public event
name. Zero new graduation tag. Zero new `deny.toml` policy change
(no new transitive licence, no new banned-crate concern, no new
advisory window).

ONE new direct dependency: `regex = "1"` is added to
`crates/lumen/Cargo.toml` `[dependencies]`. The version specifier
`"1"` is spelled identically to `crates/query-api/Cargo.toml:62`
(ADR-0046 Decision 1). The workspace's `Cargo.lock` already pins
`regex = "1.12.3"` via `query-api`'s direct dep; the new direct
edge on `lumen` resolves to the same lock pin with ZERO `Cargo.lock`
diff. Licence `MIT/Apache-2.0` is compatible with `lumen`'s
`AGPL-3.0-or-later`.

The `lumen::Predicate` `body_regex` builder is an additive
public-surface item; `lumen` is not in Gate 2 / Gate 3's locked set,
so no `semver-checks` failure is produced, but the crafter MUST
snapshot the new `cargo public-api` baseline as part of DELIVER per
ADR-0056 Decision 10 (and the `#[derive(PartialEq, Eq)]` relaxation
per ADR-0056 Decision 4 is part of the same snapshot). The new
`body_regex: Option<String>` field on `LogsParams` is private
(`pub(crate)`) and does NOT appear in any public-api diff.

## Outcome KPIs instrumentation

No new runtime instrumentation. The five outcome KPIs from
`../discuss/outcome-kpis.md` are covered by Gate 1 acceptance tests
and Gate 5 mutation testing:

- **K1 (honest regex matches)** and **K2 (zero regression when
  `body_regex` absent)** and **K3 (400 BEFORE the store on empty /
  over-cap / invalid-syntax)**: Gate 1 on the new acceptance suite
  `crates/log-query-api/tests/slice_01_body_regex.rs` (DISTILL
  output) plus unchanged pre-existing suites
  `tests/slice_01_logs_read.rs`, `tests/slice_02_caps.rs`,
  `tests/slice_01_severity_filter.rs`,
  `tests/slice_01_body_contains.rs` (no test deletion, no test
  rewrite).
- **K4 (`query-http-common` reuse confirmed; under-40-LOC budget)**:
  Gate 1 on `crates/log-query-api/src/lib.rs` plus the static-grep
  CI assertions enumerated in K4 Measurement, identical in shape to
  the ones established for `log-body-text-search-v0`.
- **K5 (`gate-5-mutants-lumen` exercises the new arm at 100% kill
  rate)**: Gate 5 at `.github/workflows/ci.yml:1210`, the explicit
  binary signal that the loop closure above is exercised.

Consistent with ADR-0050 Decision 8, ADR-0055 Decision 13, and
ADR-0056 Decision 14: at v0/v1 the platform has no live observability
stack of its own; a contract-shaped outcome IS the signal.

## Inherited from slim precedent

This wave inherits the structure and the per-decision shape of
`docs/feature/log-body-text-search-v0/devops/wave-decisions.md`
(slim DEVOPS, 2026-05-27). The body-text-search slice is the
immediate sibling: a lumen-predicate-extending feature without a
new crate, without a new workspace member, verified against the
same ADR-0005 contract. Both slices add one optional query-string
parameter to `GET /api/v1/logs`, both extend `lumen::Predicate`
additively, both compose conjunctively with `min_severity` via
`Predicate::matches`. The DEVOPS posture is identical at the
workflow / deployment layers, with ONE honest improvement recorded
above: where the body-text-search slim wave had to record the
absence of `gate-5-mutants-lumen` as a forward-looking item, this
slim wave records its presence (committed in `d96a807`) and its
first exercised use, closing the loop end-to-end.

## Upstream Changes

None. Zero DISCUSS assumptions changed by this DEVOPS wave. Zero
DESIGN assumptions changed (the DESIGN handoff at
`../design/wave-decisions.md` Reuse Analysis rows for
`gate-5-mutants-lumen` and `gate-5-mutants-log-query-api` is
ratified verbatim: both jobs exist, both cover via `--in-diff`, no
CI edit is required). The slice composes additively on top of
ADR-0046, ADR-0047, ADR-0050, ADR-0052, ADR-0054, ADR-0055,
ADR-0056, and `gate-5-mutants-lumen-v0` without altering any of
them.
