# Wave Decisions — cli-unknown-flag-rejection-v0 / DEVOPS

- **Wave**: DEVOPS (slim)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-01
- **Mode**: slim. The feature touches only `crates/kaleidoscope-cli`:
  one shared `reject_unknown_flags(args, known)` free function in
  `src/main.rs`, eight one-line call sites in the subcommand wrappers,
  inline `#[cfg(test)]` units over the helper, and one new subprocess
  acceptance test file registered as a `[[test]]` in `Cargo.toml`. No
  new crate, no new workspace member, no new dependency, no clap
  migration, no new CI job, no new deployment artefact, no ADR
  (confirmed DESIGN DD4 / DISCUSS D6). This wave verifies that the
  existing CI contract (ADR-0005) already covers the modified code
  paths and records the inherited posture. Shape and brevity mirror
  the immediate sibling slim precedent at
  `docs/feature/log-body-regex-search-v0/devops/wave-decisions.md`.

## DEVOPS Decisions

| D# | Topic | Value |
|----|-------|-------|
| DD1 | deployment_target | N/A (existing CLI binary; no new binary, no new container, no new deployment target) |
| DD2 | container_orchestration | N/A (no container image produced or changed; this is an argv-validation behaviour change inside the existing `kaleidoscope-cli` binary) |
| DD3 | cicd_platform | inherit GitHub Actions; ADR-0005 gate contract unchanged |
| DD4 | existing_infrastructure | extend; reuse the existing `gate-5-mutants-kaleidoscope-cli` job and the workspace gates; no new dependency, no new infra, no new CI job |
| DD5 | observability | this feature improves the CLI's fail-fast posture (it rejects malformed input with exit 2 and usage on stderr instead of silently ignoring unknown flags); no live observability stack at v0; the exit-code-plus-stderr contract IS the operator-facing signal, asserted by the acceptance suite |
| DD6 | deployment_strategy | N/A (pure trunk-based; recovery is fix-forward / git revert; the change is additive and every currently-valid invocation stays byte-equal per US-04) |
| DD7 | continuous_learning | N/A (no live observability or experimentation stack at v0) |
| DD8 | git_branching | inherit pure trunk-based (project default; main has no required-status-checks and no enforce_admins) |
| DD9 | mutation_testing | inherit per-feature, 100% kill rate (CLAUDE.md, ADR-0005 Gate 5); covered by `gate-5-mutants-kaleidoscope-cli` (line 1725) via `--in-diff` |

## CI Inheritance

The ADR-0005 workspace gates are inherited unchanged. No workflow file
is edited. No new or amended job. The mutation verification is
file-grounded:

- `gate-5-mutants-kaleidoscope-cli` exists at
  `.github/workflows/ci.yml:1725` (CONFIRMED by grep and read). The job
  scopes to `crates/kaleidoscope-cli/**` via the diff filter
  (`git diff "$BASELINE" HEAD -- 'crates/kaleidoscope-cli/**'`) and runs
  `cargo mutants --package kaleidoscope-cli --in-diff "$DIFF_FILE"
  --no-shuffle --jobs 2` (lines 1790-1794). The `--in-diff` filter
  naturally points the runner at `crates/kaleidoscope-cli/src/main.rs`,
  the sole src file this feature touches: the new `reject_unknown_flags`
  helper, its eight call sites, and the exit-2 routing in `main`. The
  job's own comment (lines 1767-1770, per cli-cinder-otlp-wiring-v0
  DEVOPS) records that it covers every src file in kaleidoscope-cli via
  path-filtered `--in-diff`, so the new seam needs no scoping change.

- The other four ADR-0005 gates (Gate 1 `cargo test --workspace`,
  Gate 2 `cargo public-api`, Gate 3 `cargo semver-checks`, Gate 4
  `cargo deny`) are workspace-scope and cover this change unchanged.
  Gate 1 runs the new subprocess acceptance suite plus the inline
  helper unit tests. `reject_unknown_flags` is a private item in the
  binary crate, so it contributes no public-api diff and no
  semver-checks surface. No dependency is added, so Gate 4 sees no new
  licence, banned-crate, or advisory concern.

The new subprocess acceptance tests (spawning the real binary via
`env!("CARGO_BIN_EXE_kaleidoscope-cli")`) provide the observable kills
for body-deletion and condition-flip mutants on the helper: an exit-2
assertion plus a `unknown flag "<token>"` stderr substring assertion
fail the moment a mutant weakens the rejection logic. This is the
mutation surface the gate at line 1725 was designed to audit.

## No new tooling

Zero new workspace crate. Zero new binary. Zero new dependency. No
clap migration (DESIGN rejects clap for this feature; the documented
"clap does not earn it" choice in main.rs:17-21 is honoured). Zero new
`deny.toml` policy change. Zero new graduation tag and no crate bumped
to any version (no 1.0.0). Cargo.toml gains only one `[[test]]`
name/path entry for the new acceptance file, which is not a dependency
or tooling change.

## K11 re-anchor note

This feature re-anchors EDD verifier defect K11
("kaleidoscope-cli rejects an unknown flag"), whose prior anchor
(commit e7fbee0) was dropped en bloc by revert e3a8cad, leaving K11 in
`held`. The DELIVER wave's new subprocess acceptance tests in
`crates/kaleidoscope-cli/tests/` ARE the fresh, non-reverted anchor:
they are committed on a live commit and assert the observable contract
(exit 2 plus the `unknown flag "<token>"` / `unknown subcommand "<x>"`
stderr substrings) that K11 re-verifies against. The DEVOPS posture
here ensures that anchor is exercised by the existing mutation gate
(line 1725) at 100% kill rate, so the contract is not merely present
but defended against silent erosion. On the next verifier run K11
transitions from `held` to satisfied.

## Inherited from slim precedent

This wave inherits the structure and the per-decision shape of
`docs/feature/log-body-regex-search-v0/devops/wave-decisions.md`
(slim DEVOPS, 2026-05-29). Both are targeted growths of an existing
artefact with no new crate, no new dependency in the touched binary,
and no new CI job, verified against the same ADR-0005 contract by an
existing `gate-5-mutants-<crate>` job via `--in-diff`. Where that
sibling extended `lumen::Predicate` and the log-query-api handler,
this one extends the `kaleidoscope-cli` argv validator; the workflow
and deployment layers are identical, and in both cases the existing
per-crate mutation gate is the binary signal that the new behaviour is
exercised.

## Upstream Changes

None. Zero DISCUSS assumptions changed by this DEVOPS wave. Zero
DESIGN assumptions changed: the DESIGN handoff at
`../design/wave-decisions.md` "DEVOPS Handoff" is ratified verbatim
(no new crate, no new dependency, no new workflow;
`gate-5-mutants-kaleidoscope-cli` covers via `--in-diff`; CI is
feedback, not a gate on a pure trunk-based project). The feature
composes additively on top of ADR-0005 without altering it.
