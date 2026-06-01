# Wave Decisions — read-api-tracing-subscriber-v0 / DEVOPS

- **Wave**: DEVOPS (slim, doc-only)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-29
- **Mode**: slim. This feature installs a `tracing` subscriber in the three
  read binaries (`query-api`, `log-query-api`, `trace-query-api`) via one
  shared `query_http_common::init_tracing()` helper (`OnceLock`-guarded,
  JSON to stderr, `EnvFilter` keyed on `RUST_LOG`), called as the first
  statement of each `main`. It makes the existing lifecycle events
  (`*_starting`, `listener_bound`, `health.startup.refused`) visible on
  operator stderr, where today they are silently discarded. Origin: the
  EDD black-box verifier issue 005 (operability). No new crate, no new
  workspace member, no new binary, no new CI job, no new ADR (the change
  aligns the read tier to aperture's already-blessed ADR-0009 posture).
  All source (the helper, the three `main` calls, and the two new
  dependency lines on `query-http-common`'s `Cargo.toml`) is written by
  Crafty in one atomic DELIVER pass. Shape and brevity mirror the
  immediate sibling slim precedent at
  `docs/feature/log-body-regex-search-v0/devops/wave-decisions.md`.

## DEVOPS Decisions

| D# | Topic | Value |
|----|-------|-------|
| DD1 | deployment_target | N/A (library helper plus three pre-existing binaries; no new deployable artefact, no new container) |
| DD2 | container_orchestration | N/A (this feature produces no container image; no Dockerfile is added or amended) |
| DD3 | cicd_platform | inherit GitHub Actions; the ADR-0005 five-gate contract is unchanged |
| DD4 | existing_infrastructure | extend; the only addition is two dependency edges (`tracing = "0.1"`, `tracing-subscriber` 0.3) on `query-http-common`; no new infra, no new CI job |
| DD5 | observability | this feature IS observability: it installs the subscriber that renders the read tier's lifecycle events; it improves operability rather than leaving it invariant; no new metric, no new dashboard, no new alert (events were already emitted, just never rendered) |
| DD6 | deployment_strategy | N/A (pure trunk-based; recovery is fix-forward / git revert; the change is additive to startup output only and the HTTP contract is byte-identical) |
| DD7 | continuous_learning | N/A (no live observability stack at v0; stderr is the operator's surface) |
| DD8 | git_branching | inherit pure trunk-based (project default; main has no required-status-checks and no enforce_admins) |
| DD9 | mutation_testing | inherit per-feature, 100% kill rate (CLAUDE.md, ADR-0005 Gate 5); covered by `gate-5-mutants-query-http-common` (line 1811) plus `gate-5-mutants-query-api` (line 1038), `gate-5-mutants-log-query-api` (line 1125), and `gate-5-mutants-trace-query-api` (line 1299) via `--in-diff`. The `#[mutants::skip]` posture on each `main` and on the `OnceLock`-guarded global-install body (DESIGN C6) is preserved |

## CI Inheritance

The ADR-0005 five workspace gates (Gate 1 `cargo test --workspace`, Gate 2
`cargo public-api`, Gate 3 `cargo semver-checks`, Gate 4 `cargo deny`,
Gate 5 `cargo mutants`) are inherited unchanged. No workflow file edit. No
new or amended job. The four relevant Gate 5 jobs already exist and each
path-filters its own crate via `--in-diff`, so they cover this feature's
modified files automatically:

- `gate-5-mutants-query-http-common` at `.github/workflows/ci.yml:1811`
  (CONFIRMED). It diffs `crates/query-http-common/**` and runs
  `cargo mutants --package query-http-common --in-diff "$DIFF_FILE"
  --no-shuffle --jobs 2`. The `--in-diff` filter points the runner at the
  new `init_tracing()` helper added to that crate.

- `gate-5-mutants-query-api` at `.github/workflows/ci.yml:1038`
  (CONFIRMED). It diffs `crates/query-api/**`; it covers the one-line
  `init_tracing()` call and the pre-init `eprintln!` conversion in
  `crates/query-api/src/main.rs`.

- `gate-5-mutants-log-query-api` at `.github/workflows/ci.yml:1125`
  (CONFIRMED). It diffs `crates/log-query-api/**`; same coverage shape for
  `crates/log-query-api/src/main.rs`.

- `gate-5-mutants-trace-query-api` at `.github/workflows/ci.yml:1299`
  (CONFIRMED). It diffs `crates/trace-query-api/**`; same coverage shape
  for `crates/trace-query-api/src/main.rs`.

Per DESIGN C6, each `main` keeps its `#[mutants::skip]`, and the
`OnceLock`-guarded global-install body of `init_tracing()` carries the same
posture as unkillable global-install wiring exercised only by the black-box
acceptance run. The four jobs above remain the binding gate-5 signal; no
gate is added or removed.

## No new tooling

Zero new workspace crate. Zero new binary. Zero new public event name (the
three event names already exist; this feature only makes them render). Zero
new graduation tag. Zero new `deny.toml` policy change.

Two new dependency edges, both on `crates/query-http-common/Cargo.toml`
`[dependencies]`:

- `tracing = "0.1"` (MIT/Apache-2.0).
- `tracing-subscriber = { version = "0.3", default-features = false,
  features = ["fmt", "json", "env-filter", "registry"] }` (MIT/Apache-2.0).

Both specifiers are non-wildcard, so Gate 4 `cargo deny` raises no
wildcard-pin concern. The workspace `Cargo.lock` already resolves a
`tracing-subscriber` 0.3.x and `tracing` 0.1.x via aperture, so the two new
edges add only the edges themselves with no fresh transitive resolution and
near-zero `Cargo.lock` churn. The three read crates pick the subscriber up
transitively through `query_http_common::init_tracing()` and do not each
declare their own `tracing-subscriber` edge (DESIGN DD1/DD2). No version is
bumped, and nothing approaches a 1.0.0 promise.

## Observability note

This feature is itself an observability improvement, which is why DD5 reads
"improves" rather than "invariant". The three read binaries already emit
their lifecycle events through `tracing::`, but with no subscriber
installed every event is dropped and the operator's container stderr is
empty. Installing the shared subscriber as the first statement of each
`main` renders those events as JSON lines on stderr: `*_starting` and
`listener_bound` on a clean start, and `health.startup.refused` with its
`reason` before the non-zero exit on a fail-closed start. This brings the
read tier into uniformity with the gateway (aperture, ADR-0009): one
subscriber format (JSON to stderr), one filter (`EnvFilter` / `RUST_LOG`),
one rendered line shape that a single JSON parser covers across all four
read-tier binaries. The signal is the stderr contract itself; at v0 there
is no separate metrics or dashboard layer to wire, and the black-box
acceptance run plus the EDD verifier are the consumers of that contract.

## Inherited from slim precedent

This wave inherits the structure and per-decision shape of
`docs/feature/log-body-regex-search-v0/devops/wave-decisions.md` (slim
DEVOPS, 2026-05-29). That sibling was a parse-and-wire growth of one
query-string parameter on `GET /api/v1/logs` verified against the same
ADR-0005 contract with no new crate and no new CI job. The DEVOPS posture
is identical at the workflow and deployment layers: inherit the five gates,
edit no workflow file, rely on the per-crate `--in-diff` gate-5 jobs to
cover the modified files. The one shape difference is the breadth of
coverage: where the regex sibling leaned on two crate jobs (`lumen` and
`log-query-api`), this feature leans on four (`query-http-common` plus the
three read APIs), because the helper lives in the shared crate and is
consumed by all three binaries.

## Upstream Changes

None. Zero DISCUSS assumptions changed by this DEVOPS wave. Zero DESIGN
assumptions changed; the DESIGN handoff at `../design/wave-decisions.md`
("No new crate, no new workspace member, no new binary, no new CI job ...
the three read crates' existing gate-5 mutant runs already scope the
modified files; query-http-common's gate-5 run scopes the new helper") is
ratified verbatim: all four jobs exist (lines 1811, 1038, 1125, 1299), all
four cover via `--in-diff`, and no CI edit is required. The feature composes
additively on top of ADR-0009 and ADR-0054 without altering either.
