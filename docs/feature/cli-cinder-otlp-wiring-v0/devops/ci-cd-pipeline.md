# CI/CD Pipeline - `cli-cinder-otlp-wiring-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Workflow file**: `.github/workflows/ci.yml` (existing; ONE new
  job block added by this feature; ZERO edits to existing job
  blocks)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default)

## Posture

Inherits the ADR-0005 five-gate contract UNCHANGED. ONE workflow
edit: addition of `gate-5-mutants-kaleidoscope-cli` parallel job
alongside the other per-package Gate 5 jobs, mirrored byte-for-byte
from `gate-5-mutants-self-observe` (ci.yml lines 862–947) with six
substitutions.

## Per-gate mapping to outcome KPIs

| Gate | Tool | Owns (for this feature) | KPI(s) enforced |
|------|------|--------------------------|-----------------|
| Gate 4 - `cargo deny check` | `cargo-deny` | Dependency policy. The wiring edit adds ZERO new external deps. | none directly (transitive: regression in deny.toml blocks merge) |
| Gate 1 - `cargo test --workspace --all-targets --locked` | `cargo test` | Acceptance tests: `tests/observe_otlp_cinder_wiring.rs` (new, OK6 + OK7) + `tests/observe_otlp_flag.rs` (existing, OK8). | **OK6**, **OK7**, **OK8** |
| Gate 2 - `cargo public-api` | `cargo-public-api` | `kaleidoscope-cli` is a binary; not graduated to Gate 2. | none |
| Gate 3 - `cargo semver-checks` | `cargo-semver-checks` | Same as Gate 2. | none |
| Gate 5 - `cargo mutants` (NEW per-package job `gate-5-mutants-kaleidoscope-cli`) | `cargo-mutants` | Mutation of the wiring surface in `crates/kaleidoscope-cli/src/lib.rs` via `--in-diff` cascade. 100% kill rate per ADR-0005 Gate 5 + CLAUDE.md. | Test-quality probe supplementing OK6/OK7. |

## The workflow change - full YAML snippet for Crafty

Add the following job block to `.github/workflows/ci.yml` adjacent
to `gate-5-mutants-self-observe`. Byte-for-byte mirror of lines
862–947 with six substitutions; no other changes.

```yaml
  gate-5-mutants-kaleidoscope-cli:
    name: Gate 5 — cargo mutants (kaleidoscope-cli)
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

      - name: Cache Cargo registry, git index and target/ (kaleidoscope-cli)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-kaleidoscope-cli-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-kaleidoscope-cli-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (kaleidoscope-cli, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the harness, aperture,
        # spark, sieve, codex, and self-observe jobs. An empty
        # diff (commit does not touch crates/kaleidoscope-cli/)
        # short-circuits to a zero-second exit.
        #
        # Per cli-cinder-otlp-wiring-v0 DEVOPS A1/A3, this job
        # covers every src file in kaleidoscope-cli via
        # path-filtered --in-diff. Per-file job fan-out is
        # deferred.
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
            git diff "$BASELINE" HEAD -- 'crates/kaleidoscope-cli/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No kaleidoscope-cli-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- kaleidoscope-cli diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package kaleidoscope-cli \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package kaleidoscope-cli \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (kaleidoscope-cli)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-kaleidoscope-cli
          path: mutants.out/
          retention-days: 30
```

## Gates and hooks NOT modified

- Gate 4 (`cargo deny`): zero new external deps.
- Gate 1 (`cargo test --workspace`): auto-discovers new test via
  `[[test]]` block (A2).
- Gates 2/3: kaleidoscope-cli is a binary; not graduated.
- Pre-existing Gate 5 jobs (harness, aperture, spark, sieve, codex,
  self-observe): independent; new job runs in parallel.
- Prism Gates 6-11: out of scope (Rust-only commit).
- `scripts/hooks/pre-commit`: no edit (test auto-discovered).
- `scripts/hooks/pre-push`: no edit (kaleidoscope-cli is binary).

Trunk-Based Development: workflow already encodes TBD; every push
to `main` triggers the full pipeline including the new Gate 5 job.
Per memory `project_kaleidoscope_pure_trunk_based`: CI is feedback,
not a gate.

## Summary

| Question | Answer |
|----------|--------|
| Is the existing 5-gate workflow sufficient? | Yes, with one new per-package Gate 5 job. |
| Which gate enforces each KPI? | Gate 1 → OK6/OK7/OK8. New Gate 5 job → supplemental test-quality probe. |
| New workflow files | NONE |
| Modifications to existing workflow | ONE pure-addition (the new Gate 5 job block) |
| Modifications to hooks | NONE |
| New CI dependencies | NONE |
| Files touched by DELIVER commit | `crates/kaleidoscope-cli/src/lib.rs` + `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` (new) + `crates/kaleidoscope-cli/Cargo.toml` (one `[[test]]` block) + `.github/workflows/ci.yml` (one new job block per A3) |
