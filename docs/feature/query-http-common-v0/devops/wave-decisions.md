# Wave Decisions - query-http-common-v0 / DEVOPS

British English. No em dashes in body.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-27
- **Mode**: NOT slim. This wave adds ONE new CI job
  (`gate-5-mutants-query-http-common`) because the feature introduces
  ONE new workspace crate (`crates/query-http-common/`). Every other
  CI gate is inherited verbatim from ADR-0005 and the existing
  `.github/workflows/ci.yml`. No new container, no new external
  dependency, no new observability surface, no new deployment target.

## DEVOPS Decisions

| DD# | Decision area | Value | Rationale / source |
|-----|---------------|-------|--------------------|
| DD1 | Deployment target | None new; library-only refactor | DESIGN handoff: `query-http-common` is `publish = false`, library-only, consumed in-process by the three read APIs |
| DD2 | Container orchestration | N/A | No binary in the new crate; no new image to build; the existing `kaleidoscope-cli` Dockerfile is untouched |
| DD3 | CI/CD platform | GitHub Actions, ADR-0005 five gates inherited | ADR-0005 is the workspace contract; `.github/workflows/ci.yml` already implements all five gates |
| DD4 | Existing infrastructure | Yes; extended with ONE addition (new `gate-5-mutants-query-http-common` job) | DESIGN handoff explicitly names the new job, modelled on `gate-5-mutants-trace-query-api` (ADR-0048 precedent) |
| DD5 | Observability | None new; existing workspace gates suffice | The new crate is a library; no runtime instrumentation, no dashboard, no alert. The four KPIs (K1, K2, K3, K4) are build-time measurements |
| DD6 | Deployment strategy | N/A | No new deployment artefact; recovery is git revert with no data-format consequence |
| DD7 | Continuous learning | N/A | No live observability stack to feed; no A/B story; no feature flag |
| DD8 | Git branching | Pure trunk-based (project default, unchanged) | Memory `project_kaleidoscope_pure_trunk_based`: main has no required-status-checks and no enforce_admins; CI is feedback, not a gate |
| DD9 | Mutation testing | Per-feature, 100% kill rate, scoped by `--in-diff` to `crates/query-http-common/**` | CLAUDE.md per-feature MT strategy; ADR-0005 Gate 5; outcome KPI K4 |

## CI Inheritance + One Addition

Four of the five ADR-0005 gates are inherited verbatim from the
existing workflow at `.github/workflows/ci.yml`. The fifth gate
(mutation testing) gains ONE new per-crate job:

| Gate | Coverage for `query-http-common-v0` | Workflow delta |
|------|-------------------------------------|----------------|
| Gate 1 (`cargo test --workspace`) | Runs the new crate's inline `#[cfg(test)] mod tests` and every consumer crate's existing acceptance suite; the K1 (test-count parity) and K2 (byte-identical 400/401 bodies) KPIs are observed here | Zero (Gate 1 auto-discovers the new crate via the workspace `members` array) |
| Gate 2 (`cargo public-api`) | Scoped to `otlp-conformance-harness`, `spark`, `sieve`, `codex`; `query-http-common` is NOT a graduated crate and is `publish = false`, so it is NOT added to the locked set | Zero |
| Gate 3 (`cargo semver-checks`) | Same locked set as Gate 2; `query-http-common` is NOT added | Zero |
| Gate 4 (`cargo deny`) | No new external dependency (every dep of the new crate is already in the workspace: `axum`, `serde`, `serde_json`, `aegis`); `deny.toml` unchanged | Zero |
| Gate 5 (`cargo mutants`) | One NEW per-crate job `gate-5-mutants-query-http-common`, modelled on the existing `gate-5-mutants-trace-query-api` job at `.github/workflows/ci.yml` lines 1210-1295 (ADR-0048 precedent). Scope: `cargo mutants --package query-http-common --in-diff "$DIFF_FILE"` with the `origin/main -> HEAD~1 -> full` baseline cascade and the empty-diff short-circuit | ADD one job |

The new job follows the EXACT pattern of the fourteen existing
`gate-5-mutants-*` jobs (harness, aperture, spark, sieve, codex,
self-observe, aperture-storage-sink, query-api, log-query-api,
trace-query-api, pulse, ray, strata, beacon, kaleidoscope-cli).
Differences are limited to: the package name (`query-http-common`),
the diff-filter path (`crates/query-http-common/**`), the cache key
shard (`...-query-http-common-...`), and the artefact name
(`mutants-out-query-http-common`). Toolchain pin
(`dtolnay/rust-toolchain@e97e2d8cc328f1b50210efc529dca0028893a2d9`),
`cargo-mutants` installer
(`taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090`),
runner (`ubuntu-latest`), timeout (30 minutes), `needs` graph
(`gate-2-public-api`, `gate-3-semver`), `fetch-depth: 0` checkout,
`--no-shuffle --jobs 2` invocation, and the upload-artefact action
are byte-identical to the sibling jobs.

## New job specification

Workflow file: `.github/workflows/ci.yml`. The new job is inserted
at the END of the Rust gate-5 block, immediately after
`gate-5-mutants-kaleidoscope-cli` (line 1720) and immediately before
the `# Prism v0 gates 6-11` comment block (line 1722).

```yaml
  gate-5-mutants-query-http-common:
    name: Gate 5 — cargo mutants (query-http-common)
    runs-on: ubuntu-latest
    needs:
      - gate-2-public-api
      - gate-3-semver
    timeout-minutes: 30
    steps:
      - name: Check out repository
        uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
        with:
          fetch-depth: 0

      - name: Install stable Rust toolchain
        uses: dtolnay/rust-toolchain@e97e2d8cc328f1b50210efc529dca0028893a2d9 # v1
        with:
          toolchain: stable

      - name: Cache Cargo registry, git index and target/ (query-http-common)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-query-http-common-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-query-http-common-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (query-http-common, in-diff)
        run: |
          DIFF_FILE=$(mktemp)
          BASELINE=""
          if git rev-parse --verify origin/main >/dev/null 2>&1 && \
             [ "$(git rev-parse origin/main)" != "$(git rev-parse HEAD)" ]; then
            BASELINE="origin/main"
          elif git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
            BASELINE="HEAD~1"
          fi

          if [ -n "$BASELINE" ]; then
            git diff "$BASELINE" HEAD -- 'crates/query-http-common/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No query-http-common-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            cargo mutants \
              --package query-http-common \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            cargo mutants \
              --package query-http-common \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (query-http-common)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-query-http-common
          path: mutants.out/
          retention-days: 30
```

## No new tooling beyond the gate

- No new observability stack. The four KPIs (K1 test-count parity,
  K2 byte identity, K3 LOC reduction, K4 mutation kill rate) are all
  measured at build time by `cargo test`, `cargo mutants`, and a
  shell `grep | wc -l` pipeline. There is no runtime telemetry to
  emit and no dashboard to provision.
- No new binary. `query-http-common` is library-only (`publish =
  false`, no `[[bin]]`, no `src/main.rs`).
- No new external dependency. The new crate consumes only
  workspace-pinned crates: `axum 0.7`, `serde`, `serde_json`, and
  `aegis` (path dependency). `deny.toml` is unchanged. Gate 4 passes
  with the same policy that passed at the previous feature close.
- No new container image. The pre-existing `kaleidoscope-cli`
  Dockerfile is untouched; the new crate is not a binary and has no
  Dockerfile of its own.
- No new alert or threshold. Refusal envelopes ride on the existing
  `{status:"error", error:"<reason>"}` shape with no new vocabulary
  (the four reason text consts are existing literals lifted into a
  single source of truth, not new wire values).

## Tag plan

- `query-http-common/v0.1.0` lands at DELIVER close, not at DESIGN
  close and not at DEVOPS close. This follows the per-crate
  graduation pattern set by ADR-0048 / ADR-0052 (one new per-crate
  tag at slice graduation). The tag is created by the DELIVER agent
  after the K4 mutation gate is green and after the workspace is
  green on `main`.
- No `1.0.0` bump on any crate. Project policy (semver 1.0.0 is
  Andrea's call).
- No tag is created at DESIGN close and no tag is created at DEVOPS
  close.
- The three consumer crates (`query-api`, `log-query-api`,
  `trace-query-api`) are NOT in the graduated set (Gate 2 / Gate 3
  locked set is harness / spark / sieve / codex only); no consumer
  tag is created.

## Upstream Changes

None. Every DEVOPS conclusion is consistent with the DESIGN handoff
recorded in `docs/feature/query-http-common-v0/design/wave-decisions.md`
(DEVOPS Handoff section). No DESIGN assumption is revised by this
wave. No
`docs/feature/query-http-common-v0/devops/upstream-changes.md` is
required.

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**What DISTILL receives**: the mandatory `environments.yaml` for
Mandate 4 (the `clean` environment; the new crate is a library
consumed by three read APIs through workspace path dependencies);
the confirmation that one and only one workflow edit is made (the
new `gate-5-mutants-query-http-common` job); the confirmation that
`deny.toml` is unchanged; the constraint that `query-http-common`
is NOT added to Gate 2 / Gate 3's locked set; and the K4 100%
kill-rate target on the new crate at DELIVER close.
