# CI/CD Pipeline — `ray-v1`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Workflow file**: `.github/workflows/ci.yml` (existing — NOT
  modified by this DEVOPS wave; Crafty lands the new job in DELIVER)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default, pure
  trunk-based per memory `project_kaleidoscope_pure_trunk_based`)
- **Direct precedent**: `docs/feature/pulse-v1/devops/ci-cd-pipeline.md`
  (pulse-v1 added `gate-5-mutants-pulse` in commit 4232c4a, same shape)

## Posture

`ray-v1` inherits Gates 1-4 of the ADR-0005 five-gate contract
UNCHANGED, and ADDS one new Gate 5 job, `gate-5-mutants-ray`, because
the `ray` crate has never had a Gate 5 job (verified:
`grep -c "gate-5-mutants-ray" .github/workflows/ci.yml` returns 0;
existing Gate 5 jobs cover only aperture, codex, harness,
kaleidoscope-cli, pulse, self-observe, sieve, spark). See
`wave-decisions.md` A1 for the full justification.

## Per-gate mapping

| Gate | Tool | Owns (this feature) | KPI(s) enforced |
|------|------|---------------------|-----------------|
| Gate 1 — `cargo test --workspace --all-targets --locked` | `cargo test` | the two acceptance suites `v1_slice_01_wal_durability` + `v1_slice_02_snapshot`, auto-discovered via the new `[[test]]` blocks (A2) | **KPI1**, **KPI2**, **KPI3** — Gate 1 pass/fail IS the measurement |
| Gate 2 — `cargo public-api` | `cargo-public-api` | NOT graduated for ray (scope set is {harness, spark, sieve, codex}) | none directly |
| Gate 3 — `cargo semver-checks` | `cargo-semver-checks` | NOT graduated for ray (same scope as Gate 2) | none directly |
| Gate 4 — `cargo deny check` | `cargo-deny` | no-op-for-this-feature; `serde`/`serde_json` enter ray's manifest but are already in the resolved workspace graph, so zero new external crates (A3) | none directly |
| Gate 5 — `cargo mutants` (**NEW** job `gate-5-mutants-ray`) | `cargo-mutants` | mutation testing of `crates/ray/src/file_backed.rs` + touched `store.rs`/`span.rs` lines via `--in-diff`; 100% kill rate per ADR-0005 Gate 5 + CLAUDE.md | test-quality probe supplementing KPI1/KPI2/KPI3; the sole enforcer that the single `apply_ingest` (DD4) has no divergent twin |

## The workflow change — ONE new job

`ray-v1`'s DELIVER commit (landed by Crafty, atomic with the
implementation) adds exactly one job to `.github/workflows/ci.yml`:
`gate-5-mutants-ray`. It is `gate-5-mutants-self-observe`
(`ci.yml:862-947`) copied byte-for-byte with the six substitutions in
`wave-decisions.md` A1 — the same operation pulse-v1 performed in
commit 4232c4a. Insert it adjacent to the other Gate 5 jobs (e.g.
after `gate-5-mutants-pulse`, before `gate-5-mutants-kaleidoscope-cli`).

### Full YAML snippet (copy-paste into ci.yml)

```yaml
  gate-5-mutants-ray:
    name: Gate 5 — cargo mutants (ray)
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

      - name: Cache Cargo registry, git index and target/ (ray)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-ray-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-ray-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (ray, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the self-observe, pulse and
        # kaleidoscope-cli jobs. An empty diff (commit does not touch
        # crates/ray/) short-circuits to a zero-second exit.
        #
        # Added by ray-v1 DEVOPS A1: ray was never mutation-gated
        # before v1 introduced the durable FileBackedTraceStore. This
        # job is the sole enforcer of the single-apply_ingest no-drift
        # guarantee (DESIGN DD4).
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
            git diff "$BASELINE" HEAD -- 'crates/ray/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No ray-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- ray diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package ray \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package ray \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (ray)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-ray
          path: mutants.out/
          retention-days: 30
```

### Substitution audit (the only differences from self-observe)

| # | self-observe token | ray token |
|---|--------------------|-----------|
| 1 | `gate-5-mutants-self-observe:` (job key) | `gate-5-mutants-ray:` |
| 2 | `Gate 5 — cargo mutants (self-observe)` (name) | `Gate 5 — cargo mutants (ray)` |
| 3 | `crates/self-observe/**` (path filter) | `crates/ray/**` |
| 4 | `--package self-observe` | `--package ray` |
| 5 | `cargo-mutants-self-observe` (cache key suffix + restore-keys + step name) | `cargo-mutants-ray` |
| 6 | `mutants-out-self-observe` (artefact name) | `mutants-out-ray` |

The diff-echo log strings and the step comment naming "ray" follow
mechanically from substitutions 3/4; the cache-step display name and
`restore-keys` prefix follow mechanically from substitution 5. No
structural difference: `runs-on`, `needs`, `timeout-minutes`, all
pinned action SHAs, the baseline cascade, `--no-shuffle --jobs 2`, and
the 30-day retention are identical to both the self-observe template
and the pulse job added in commit 4232c4a.

## Behaviour on the DELIVER commit

- The commit touches `crates/ray/src/file_backed.rs` (new),
  `crates/ray/src/lib.rs` (module wiring), `crates/ray/src/store.rs`
  (additive `TraceStoreError` variant per DD7), `crates/ray/src/span.rs`
  (serde derives + custom `TraceId`/`SpanId` impls + hand-rolled hex
  module per DD5), `crates/ray/Cargo.toml` (serde deps + two `[[test]]`
  blocks), the two new test files, and `.github/workflows/ci.yml` (the
  new job above).
- The path filter `crates/ray/**` matches; `--in-diff` scopes mutation
  to the changed hunks in ray-owned files. `file_backed.rs` is new, so
  it is mutated in full; the touched `store.rs`/`span.rs` hunks are
  mutated; untouched ray lines are not.
- Per-feature 100% kill rate per CLAUDE.md applies: every mutant MUST
  be killed by the acceptance tests before DELIVER review approval. A
  surviving mutant in `apply_ingest` would mean the durability tests
  cannot detect index drift — the precise failure DD4 designs against.

## Gates NOT modified

| Gate | Why not |
|------|---------|
| Gate 1 | new `[[test]]` blocks auto-discovered by `--workspace --all-targets` (A2) |
| Gate 2 / Gate 3 | ray not graduated to the {harness, spark, sieve, codex} scope set |
| Gate 4 | zero new external crates in the resolved graph; `serde`/`serde_json` already present via aegis, hex codec hand-rolled (A3) |
| Existing Gate 5 jobs (aperture, codex, harness, kaleidoscope-cli, pulse, self-observe, sieve, spark) | independent; the new ray job runs in parallel and does not touch them |
| Prism Gates 6-11 | Rust-only commit; path filter excludes it |

## Pre-commit and pre-push hooks

| Hook | Action required |
|------|-----------------|
| `scripts/hooks/pre-commit` | None — runs `cargo test --workspace`; new tests auto-discovered (A2) |
| `scripts/hooks/pre-push` | None — Gates 2/3 per-pkg loop does not include ray; mutation testing is not a local hook |

## Summary

| Question | Answer |
|----------|--------|
| Is the existing workflow sufficient as-is? | **No** — ray has no Gate 5 job. One new job is required. |
| What is the change? | Add `gate-5-mutants-ray` (byte-for-byte mirror of `gate-5-mutants-self-observe`, six substitutions). |
| Who lands it? | `@nw-software-crafter`, atomic with the implementation in the DELIVER commit. |
| New workflow files? | NONE. |
| New CI dependencies? | NONE — `cargo-mutants` already installed for existing Gate 5 jobs. |
| New external crates in the graph? | NONE — `serde`/`serde_json` already resolved; hex codec hand-rolled (A3). |
| Which gate enforces each KPI? | Gate 1 enforces KPI1/KPI2/KPI3; the new Gate 5 job is the supplemental test-quality probe and the sole DD4 no-drift enforcer. |
