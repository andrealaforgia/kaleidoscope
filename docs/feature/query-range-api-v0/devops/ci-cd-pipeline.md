# CI/CD Pipeline - query-range-api-v0

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Workflow file**: `.github/workflows/ci.yml` (existing - NOT modified
  by this DEVOPS wave; Crafty lands the new job in DELIVER)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default, pure
  trunk-based per memory `project_kaleidoscope_pure_trunk_based`)
- **Direct precedent**:
  `docs/feature/aperture-storage-sink-v0/devops/ci-cd-pipeline.md` and
  `docs/feature/strata-v1/devops/ci-cd-pipeline.md` - each added a single
  `gate-5-mutants-<crate>` job, same shape

## Posture

`query-range-api-v0` inherits Gates 1-4 of the ADR-0005 five-gate
contract UNCHANGED, and ADDS one new Gate 5 job, `gate-5-mutants-query-api`,
because the net-new `query-api` crate has never had a Gate 5 job
(verified: `grep -c "gate-5-mutants-query-api" .github/workflows/ci.yml`
returns 0; the crate does not exist yet). The thin query-api binary gets
NO separate Gate 5 job - it is composition / wiring with its testable
logic (the `selector` parser and the `matrix` translation) living in the
lib (see `wave-decisions.md` A1 for the full reasoning).

## Per-gate mapping

| Gate | Tool | Owns (this feature) | KPI(s) enforced |
|------|------|---------------------|-----------------|
| Gate 1 - `cargo test --workspace --all-targets --locked` | `cargo test` | the consumer-driven contract test (four pinned shapes vs Prism's validators), the E2E ingest-to-query-to-render test, the timed latency test, the fail-closed tenancy tests, and the probe gold-test - auto-discovered via the new member and its `[[test]]` blocks (A2) | **KPI-1, KPI-2, KPI-3, KPI-4** - Gate 1 pass/fail IS the measurement |
| Gate 2 - `cargo public-api` | `cargo-public-api` | NOT graduated (scope set is {harness, spark, sieve, codex}) | none directly |
| Gate 3 - `cargo semver-checks` | `cargo-semver-checks` | NOT graduated (same scope as Gate 2) | none directly |
| Gate 4 - `cargo deny check` | `cargo-deny` | no-op-for-this-feature; `axum` / `hyper` / `tokio` / `serde_json` all already in the resolved workspace graph (via aperture and the workspace deps), so zero new external crates (A3) | none directly |
| Gate 5 - `cargo mutants` (**NEW** job `gate-5-mutants-query-api`) | `cargo-mutants` | mutation testing of `crates/query-api/src/**` (the `selector` bare-name parser, the `matrix` label-merge + time + value translation, the half-open range conversion, fail-closed tenancy) via `--in-diff`; 100% kill rate per ADR-0005 Gate 5 + CLAUDE.md | test-quality probe supplementing KPI-1..KPI-4; proves the contract, round-trip, and refusal tests kill a behaviourally-mutated parser + translator |

## The workflow change - ONE new job

`query-range-api-v0`'s DELIVER commit (landed by Crafty, atomic with the
implementation) adds exactly one job to `.github/workflows/ci.yml`:
`gate-5-mutants-query-api`. It is `gate-5-mutants-self-observe`
(`ci.yml:862-947`) copied byte-for-byte with the six substitutions in
`wave-decisions.md` A1 - the same operation aperture-storage-sink-v0,
strata-v1, ray-v1, and pulse-v1 performed. Insert it adjacent to the
other Gate 5 jobs (e.g. after `gate-5-mutants-kaleidoscope-cli`, the
current last Gate 5 job). The `--in-diff` baseline cascade is preserved
verbatim.

### Full YAML snippet (copy-paste into ci.yml)

```yaml
  gate-5-mutants-query-api:
    name: Gate 5 — cargo mutants (query-api)
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

      - name: Cache Cargo registry, git index and target/ (query-api)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-query-api-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-query-api-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (query-api, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the harness, aperture,
        # spark, sieve, and codex jobs. An empty diff (commit does
        # not touch crates/query-api/) short-circuits to a
        # zero-second exit.
        #
        # Per cinder-to-pulse-bridge-v0 DEVOPS A3 (commit 49328e7)
        # and cinder-to-otlp-json-bridge-v0 DEVOPS A3 (in-flight),
        # this single job covers every src file added to
        # query-api via path-filtered --in-diff. Per-writer
        # job fan-out is deferred to N=8 source files.
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
            git diff "$BASELINE" HEAD -- 'crates/query-api/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No query-api-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- query-api diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package query-api \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package query-api \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (query-api)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-query-api
          path: mutants.out/
          retention-days: 30
```

### Substitution audit (the only differences from self-observe)

| # | self-observe token | query-api token |
|---|--------------------|-----------------|
| 1 | `gate-5-mutants-self-observe:` (job key) | `gate-5-mutants-query-api:` |
| 2 | `Gate 5 — cargo mutants (self-observe)` (name) | `Gate 5 — cargo mutants (query-api)` |
| 3 | `crates/self-observe/**` (path filter) | `crates/query-api/**` |
| 4 | `--package self-observe` | `--package query-api` |
| 5 | `cargo-mutants-self-observe` (cache key suffix + restore-keys + step name) | `cargo-mutants-query-api` |
| 6 | `mutants-out-self-observe` (artefact name) | `mutants-out-query-api` |

The diff-echo log strings and the step comment naming the crate follow
mechanically from substitutions 3/4; the cache-step display name and
`restore-keys` prefix follow mechanically from substitution 5. No
structural difference: `runs-on`, `needs`, `timeout-minutes`, all pinned
action SHAs, the `--in-diff` baseline cascade, `--no-shuffle --jobs 2`,
and the 30-day retention are byte-identical to the self-observe template.

## Gates NOT modified

| Gate | Why not |
|------|---------|
| Gate 1 | new workspace member + `[[test]]` blocks auto-discovered by `--workspace --all-targets` (A2) |
| Gate 2 / Gate 3 | the new crate is not graduated to the {harness, spark, sieve, codex} scope set |
| Gate 4 | zero new external crates in the resolved graph; `axum`/`hyper`/`tokio`/`serde_json` already present (A3) |
| Existing Gate 5 jobs | independent; the new job runs in parallel and does not touch them |
| Prism Gates 6-11 | Rust-only commit; path filter excludes it |

## Summary

| Question | Answer |
|----------|--------|
| Is the existing workflow sufficient as-is? | **No** - the net-new query-api crate has no Gate 5 job. One new job is required. |
| What is the change? | Add `gate-5-mutants-query-api` (byte-for-byte mirror of `gate-5-mutants-self-observe`, six substitutions, `--in-diff` kept). |
| Does the thin binary get a gate? | **No** - it is composition/wiring; its testable logic lives in the lib (A1). |
| Who lands it? | `@nw-software-crafter`, atomic with the implementation in the DELIVER commit. |
| New workflow files? | NONE. |
| New CI dependencies? | NONE - `cargo-mutants` already installed for existing Gate 5 jobs. |
| New external crates in the graph? | NONE - `axum`/`hyper`/`tokio`/`serde_json` already resolved (A3). |
| Which gate enforces each KPI? | Gate 1 enforces KPI-1..KPI-4; the new Gate 5 job is the supplemental test-quality probe over the parser + translation. |
```
