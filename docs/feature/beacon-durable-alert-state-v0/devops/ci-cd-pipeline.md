# CI/CD Pipeline — `beacon-durable-alert-state-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Workflow file**: `.github/workflows/ci.yml` (existing — NOT
  modified by this DEVOPS wave; Crafty lands the new job in DELIVER)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default, pure
  trunk-based per memory `project_kaleidoscope_pure_trunk_based`)
- **Direct precedent**: `docs/feature/strata-v1/devops/ci-cd-pipeline.md`
  (strata-v1 added `gate-5-mutants-strata`, same shape — a never-gated
  pillar gaining its first mutation gate)

## Posture

`beacon-durable-alert-state-v0` inherits Gates 1-4 of the ADR-0005
five-gate contract UNCHANGED, and ADDS one new Gate 5 job,
`gate-5-mutants-beacon`, because the `beacon` crate has never had a
Gate 5 job (verified: `grep -c "gate-5-mutants-beacon"
.github/workflows/ci.yml` returns **0**; existing Gate 5 jobs cover only
aperture, codex, harness, kaleidoscope-cli, pulse, ray, self-observe,
sieve, spark, strata). beacon shipped as a pure evaluation engine with
no durable logic to mutate; this feature introduces the durable
`RuleStateStore`, so this is the moment to add the gate. See
`wave-decisions.md` A1 for the full justification.

## Per-gate mapping

| Gate | Tool | Owns (this feature) | KPI(s) enforced |
|------|------|---------------------|-----------------|
| Gate 1 — `cargo test --workspace --all-targets --locked` | `cargo test` | the durable-store unit + integration tests (round-trip recovery, restart-survival, persist/recover micro-benchmarks, three substrate-lie gold tests) and the beacon-server `run_rule` wiring, all auto-discovered (A2) | **KPI1, KPI2, KPI3, KPI4** — Gate 1 pass/fail IS the measurement |
| Gate 2 — `cargo public-api` | `cargo-public-api` | NOT graduated for beacon (scope set is {harness, spark, sieve, codex}) | none directly |
| Gate 3 — `cargo semver-checks` | `cargo-semver-checks` | NOT graduated for beacon (same scope as Gate 2) | none directly |
| Gate 4 — `cargo deny check` | `cargo-deny` | no-op-for-this-feature; `serde`/`serde_json` already declared in beacon's manifest, zero new external crates (A3) | none directly |
| Gate 5 — `cargo mutants` (**NEW** job `gate-5-mutants-beacon`) | `cargo-mutants` | mutation testing of the new `state_store` module + the additive `RuleState` serde derive via `--in-diff`; 100% kill rate per ADR-0005 Gate 5 + CLAUDE.md | test-quality probe over KPI1/KPI2; the sole enforcer that keyed-latest-wins recovery (DD4) has no divergent twin |

## The workflow change — ONE new job

`beacon-durable-alert-state-v0`'s DELIVER commit (landed by Crafty,
atomic with the implementation) adds exactly one job to
`.github/workflows/ci.yml`: `gate-5-mutants-beacon`. It is
`gate-5-mutants-self-observe` (`ci.yml:862-947`) copied byte-for-byte
with the six substitutions in `wave-decisions.md` A1 — the same
operation strata-v1 performed. Insert it adjacent to the other Gate 5
jobs (e.g. after `gate-5-mutants-strata`, before
`gate-5-mutants-kaleidoscope-cli`). The `--in-diff` baseline cascade is
preserved verbatim, which is what keeps mutation scoped to the diff on a
~1470-line crate (A1).

### Full YAML snippet (copy-paste into ci.yml)

```yaml
  gate-5-mutants-beacon:
    name: Gate 5 — cargo mutants (beacon)
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

      - name: Cache Cargo registry, git index and target/ (beacon)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-beacon-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-beacon-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (beacon, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the harness, aperture,
        # spark, sieve, and codex jobs. An empty diff (commit does
        # not touch crates/beacon/) short-circuits to a
        # zero-second exit.
        #
        # Per cinder-to-pulse-bridge-v0 DEVOPS A3 (commit 49328e7)
        # and cinder-to-otlp-json-bridge-v0 DEVOPS A3 (in-flight),
        # this single job covers every src file added to
        # beacon via path-filtered --in-diff. Per-writer
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
            git diff "$BASELINE" HEAD -- 'crates/beacon/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No beacon-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- beacon diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package beacon \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package beacon \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (beacon)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-beacon
          path: mutants.out/
          retention-days: 30
```

### Substitution audit (the only differences from self-observe)

| # | self-observe token | beacon token |
|---|--------------------|--------------|
| 1 | `gate-5-mutants-self-observe:` (job key) | `gate-5-mutants-beacon:` |
| 2 | `Gate 5 — cargo mutants (self-observe)` (name) | `Gate 5 — cargo mutants (beacon)` |
| 3 | `crates/self-observe/**` (path filter) | `crates/beacon/**` |
| 4 | `--package self-observe` | `--package beacon` |
| 5 | `cargo-mutants-self-observe` (cache key suffix + restore-keys + step name) | `cargo-mutants-beacon` |
| 6 | `mutants-out-self-observe` (artefact name) | `mutants-out-beacon` |

The diff-echo log strings and the step comment naming "beacon" follow
mechanically from substitutions 3/4; the cache-step display name and
`restore-keys` prefix follow mechanically from substitution 5. No
structural difference: `runs-on`, `needs`, `timeout-minutes`, all pinned
action SHAs, the **`--in-diff` baseline cascade** (`origin/main` ->
`HEAD~1` -> full), the empty-diff short-circuit, `--no-shuffle --jobs 2`,
and the 30-day retention are identical to the self-observe template and
to the strata job. The retained baseline cascade is the mechanism that
keeps mutation scoped to the commit's diff, so beacon's ~1470-line size
does not enlarge the job.

## Behaviour on the DELIVER commit

- The commit touches the new `crates/beacon/src/state_store.rs` module,
  `crates/beacon/src/lib.rs` (module wiring + re-exports, DD1), an
  additive `Serialize`/`Deserialize` derive on `RuleState` in
  `crates/beacon/src/state_machine.rs` (DD7, `transition` untouched per
  ADR-0037), `crates/beacon-server/src/main.rs` (the `run_rule` wiring,
  DD8), the new test files, and `.github/workflows/ci.yml` (the new job
  above). `serde`/`serde_json` already in `crates/beacon/Cargo.toml`, so
  no dependency edit (A3).
- The path filter `crates/beacon/**` matches; `--in-diff` scopes
  mutation to the changed hunks in beacon-owned files. `state_store.rs`
  is new, so it is mutated in full; the additive `RuleState` derive hunk
  is mutated; the untouched 1470 lines of pre-existing pure-evaluation
  source are NOT mutated. (Note: the path filter is `crates/beacon/**`,
  so beacon-server's `run_rule` wiring is gated by Gate 1, not this
  beacon-library mutation job.)
- Per-feature 100% kill rate per CLAUDE.md applies: every mutant MUST be
  killed by the tests before DELIVER review approval. A surviving mutant
  in the keyed-latest-wins replay would mean the recovery tests cannot
  detect a "last wins" -> "first wins" or dropped-overwrite regression —
  the precise failure DD4 designs against.

## Gates NOT modified

| Gate | Why not |
|------|---------|
| Gate 1 | new tests + beacon-server wiring auto-discovered by `--workspace --all-targets` (A2) |
| Gate 2 / Gate 3 | beacon not graduated to the {harness, spark, sieve, codex} scope set |
| Gate 4 | zero new external crates; `serde`/`serde_json` already declared in beacon's manifest (A3) |
| Existing Gate 5 jobs (aperture, codex, harness, kaleidoscope-cli, pulse, ray, self-observe, sieve, spark, strata) | independent; the new beacon job runs in parallel and does not touch them |
| Prism Gates 6-11 | Rust-only commit; path filter excludes it |

## Summary

| Question | Answer |
|----------|--------|
| Is the existing workflow sufficient as-is? | **No** — beacon has no Gate 5 job. One new job is required. |
| What is the change? | Add `gate-5-mutants-beacon` (byte-for-byte mirror of `gate-5-mutants-self-observe`, six substitutions, `--in-diff` cascade retained). |
| Who lands it? | `@nw-software-crafter`, atomic with the implementation in the DELIVER commit. |
| New workflow files? | NONE. |
| New CI dependencies? | NONE — `cargo-mutants` already installed for existing Gate 5 jobs. |
| New external crates in the graph? | NONE — `serde`/`serde_json` already declared in beacon's manifest (A3). |
| Which gate enforces each KPI? | Gate 1 enforces KPI1/KPI2/KPI3/KPI4; the new Gate 5 job is the supplemental test-quality probe and the sole DD4 keyed-latest-wins no-drift enforcer. |
