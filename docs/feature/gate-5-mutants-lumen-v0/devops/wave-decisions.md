# Wave Decisions - gate-5-mutants-lumen-v0 / DEVOPS

British English. No em dashes in body.

- **Wave**: DEVOPS (CI-only; closure of feature)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-29
- **Mode**: NOT slim. This wave adds ONE new CI job
  (`gate-5-mutants-lumen`) into `.github/workflows/ci.yml`. No
  production source change. No new tooling. No new dependency. No new
  tag. The DEVOPS commit is the closure of the feature; there is no
  DISTILL or DELIVER wave because there is no production code to
  scaffold.

## DEVOPS Decisions

| DD# | Decision area | Value | Rationale / source |
|-----|---------------|-------|--------------------|
| DD1 | Job placement | New `gate-5-mutants-lumen:` job key landed at `.github/workflows/ci.yml` line 1210, immediately after the sibling `gate-5-mutants-log-query-api` block (which ends at line 1208) and immediately before `gate-5-mutants-trace-query-api` (line 1297) | DESIGN DD1 pinned line 1209 as the insertion slot (blank separator after sibling end). The job key lands on the next line, 1210, by YAML structural necessity. Semantic adjacency: lumen is the storage engine backing log-query-api |
| DD2 | `needs:` graph | `[gate-2-public-api, gate-3-semver]` copied verbatim from sibling lines 1126 to 1128 | DESIGN DD2; uniform across all sixteen pre-existing `gate-5-mutants-*` jobs; deviation would introduce maintenance asymmetry without justification |
| DD3 | Token swaps | Package name `log-query-api` -> `lumen` applied across thirteen textual slots (job key, `name:` display, cache step name, cache key primary, cache restore-key, mutants step name, diff filter path glob, short-circuit log line, diff head log line, `--package` arg in in-diff branch, `--package` arg in full branch, artefact step name, artefact `name:`) | DESIGN DD3 substitution table. Crate name confirmed by `crates/lumen/Cargo.toml` line 2 (`name = "lumen"`) |
| DD4 | No tag | Zero new crate version tag at DEVOPS close. No `1.0.0` on any crate (project policy: semver 1.0.0 is Andrea's call) | This feature is pure CI infrastructure; no published crate version is affected. The `lumen` crate is not in Gate 2 / Gate 3's locked set so semver-checks is not consulted |

## CI Inheritance + One Addition

Four of the five ADR-0005 gates are inherited verbatim from the
existing workflow. The fifth gate (mutation testing) gains ONE new
per-crate job:

| Gate | Coverage for `gate-5-mutants-lumen-v0` | Workflow delta |
|------|----------------------------------------|----------------|
| Gate 1 (`cargo test --workspace`) | Inherited; runs every crate's existing test suite | Zero |
| Gate 2 (`cargo public-api`) | Inherited; locked set unchanged (`harness`, `spark`, `sieve`, `codex`). `lumen` is not in the locked set | Zero |
| Gate 3 (`cargo semver-checks`) | Inherited; same locked set as Gate 2 | Zero |
| Gate 4 (`cargo deny`) | Inherited; no new dependency added; `deny.toml` byte-identical | Zero |
| Gate 5 (`cargo mutants`) | One NEW per-crate job `gate-5-mutants-lumen` modelled on the existing `gate-5-mutants-log-query-api` job. Scope: `cargo mutants --package lumen --in-diff "$DIFF_FILE"` with the `origin/main -> HEAD~1 -> full` baseline cascade and the empty-diff short-circuit | ADD one job |

The new job follows the EXACT pattern of the sixteen existing
`gate-5-mutants-*` jobs (`harness`, `aperture`, `spark`, `sieve`,
`codex`, `self-observe`, `aperture-storage-sink`, `query-api`,
`log-query-api`, `trace-query-api`, `pulse`, `ray`, `strata`,
`beacon`, `kaleidoscope-cli`, `query-http-common`). All sixteen
sibling jobs are byte-identical pre vs post (K3 guardrail
preserved). The seventeenth-of-seventeen invariant is achieved by
pure addition.

## Workflow edit verification

Post-edit, Apex executed three verification checks:

1. Occurrence count of the literal `gate-5-mutants-lumen` in the
   workflow:

    ```sh
    grep -c "gate-5-mutants-lumen" .github/workflows/ci.yml
    ```

    returns `1` (the job key on its own line). The cache shard, step
    names, and artefact name carry the bare token `lumen` without the
    `gate-5-mutants-` prefix; sixteen additional `lumen` occurrences
    in the inserted block are verified by
    `grep -c "lumen" .github/workflows/ci.yml` which returns `17`
    (post-edit) against `0` (pre-edit).

2. Pinpoint of the new and adjacent job keys:

    ```sh
    grep -nE "^  gate-5-mutants-(log-query-api|lumen|trace-query-api):" .github/workflows/ci.yml
    ```

    returns:

    ```text
    1123:  gate-5-mutants-log-query-api:
    1210:  gate-5-mutants-lumen:
    1297:  gate-5-mutants-trace-query-api:
    ```

    confirming semantic adjacency: lumen sits immediately between its
    consumer-side sibling (log-query-api at 1123) and the next sibling
    (trace-query-api at 1297, pushed down by exactly 87 lines from
    its pre-edit position of 1210).

3. YAML syntactic validation:

    ```sh
    ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml'); puts 'YAML OK'"
    ```

    completed with `YAML OK`. Python's `yaml.safe_load` was attempted
    first but `pyyaml` is not installed in the externally-managed
    system Python; Ruby is preinstalled on macOS and provides
    equivalent strict YAML 1.1 parsing.

The total count of gate-5 mutants jobs is `17` (was `16`
pre-feature), verified by
`grep -c "^  gate-5-mutants-" .github/workflows/ci.yml`. The file
grew from `2065` to `2152` lines, matching the DESIGN estimate of
`~87 LOC` for the inserted block.

## No new tooling

- No new external dependency. `cargo-mutants` is already installed
  by sixteen sibling jobs via
  `taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090`
  from a precompiled binary; the workspace dependency graph
  (`Cargo.toml`, `Cargo.lock`, `deny.toml`) is not consulted.
- No new binary. No file under `crates/lumen/src/` is touched.
- No new crate. No file under `crates/` is created or modified.
- No new tag. No crate version bump.
- No new observability surface. The mutation signal rides on the
  existing PR status panel check produced by the new job.
- No new container, no new image, no new runtime target. The
  workflow runs on the existing `ubuntu-latest` runner.

## DISTILL/DELIVER skipped

This feature is DEVOPS-only by construction. The user stories
(US-01) specify a single CI workflow edit with no production source
change. There is no application code to scaffold, no acceptance
scenario over a runtime component, and no DELIVER commit on a Rust
crate. The DEVOPS commit closes the feature. DISTILL Mandate 4
(Environmental Realism) is satisfied by the trivial
`environments.yaml` enumerating the single `clean` GitHub Actions
runner. The K1 / K2 / K3 / K4 KPIs are verified at the
feature-close commit by `grep` and `git diff`, not by a runtime
acceptance suite.

This pattern matches the precedent set by `query-http-common-v0`
where the DEVOPS wave was the only wave that touched
`.github/workflows/ci.yml`, with the difference that
`query-http-common-v0` also shipped a new crate (production source)
whereas this feature ships none.

## Upstream Changes

None. Every DEVOPS conclusion is consistent with the DESIGN handoff
recorded in `docs/feature/gate-5-mutants-lumen-v0/design/wave-decisions.md`
(DEVOPS Handoff section, lines 143 to 252). No DESIGN assumption is
revised. No
`docs/feature/gate-5-mutants-lumen-v0/devops/upstream-changes.md`
is required.
