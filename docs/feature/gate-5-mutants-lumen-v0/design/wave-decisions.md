# Wave Decisions — gate-5-mutants-lumen-v0 / DESIGN

British English. No em dashes in body.

- **Wave**: DESIGN (application scope, trivial)
- **Author**: Morgan (`nw-solution-architect`)
- **Date**: 2026-05-29
- **Mode**: propose
- **Input**: DISCUSS handoff by Luna (commit `a11910f`), US-01, two
  flags raised to DESIGN, K1 to K4 KPIs, ADR-0005 Gate 5 contract,
  `query-http-common-v0` DEVOPS precedent.

This wave resolves two minor flags and produces a copy-paste-ready
YAML template for the DEVOPS wave (Apex). No ADR is created.
Replicating an established pattern across a sixteenth-then-seventeenth
sibling is execution, not architectural decision.

## DESIGN Decisions

### DD1: Job placement in `.github/workflows/ci.yml`

**Resolves FLAG 1 (Luna recommended Option A).**

Insert the new `gate-5-mutants-lumen:` job block starting at
**line 1209** of `.github/workflows/ci.yml`. Verified by grep:

- `gate-5-mutants-log-query-api:` is at line `1123`.
- Its last step (`Upload mutants.out artefact (log-query-api)`) ends
  at line `1208` (`retention-days: 30`).
- Line `1209` is the blank separator before
  `gate-5-mutants-trace-query-api:` at line `1210`.

The new job block occupies the slot at line `1209` and pushes
`gate-5-mutants-trace-query-api` and all subsequent jobs down by
exactly the length of the inserted block (one blank line plus the
job body, approximately 87 lines).

**Rationale**: semantic adjacency. `lumen` is the storage engine that
backs `log-query-api`; placing the new job immediately after its
consumer-side sibling reads naturally to a maintainer scanning the
file. The alphabetic slot (`log-query-api` < `lumen` < `pulse`) and
the temporal append slot (after `gate-5-mutants-query-http-common` at
line `1722`) both lose to semantic adjacency for readability.

### DD2: `needs:` graph

**Resolves FLAG 2 (Luna recommended Option A).**

Copy verbatim from the sibling `gate-5-mutants-log-query-api`:

```yaml
    needs:
      - gate-2-public-api
      - gate-3-semver
```

Verified by grep on lines `1126` to `1128` of the sibling block. This
is the uniform `needs:` graph across all sixteen existing
`gate-5-mutants-*` jobs (`harness`, `aperture`, `spark`, `sieve`,
`codex`, `self-observe`, `aperture-storage-sink`, `query-api`,
`log-query-api`, `trace-query-api`, `pulse`, `ray`, `strata`,
`beacon`, `kaleidoscope-cli`, `query-http-common`). Deviating would
introduce a maintenance asymmetry without justification. `lumen`'s
build-graph position is identical to its sibling consumers: Gate 1
runs `cargo test --workspace`, Gate 2 and Gate 3 lock the published
crates' public API, and Gate 5 fires against the per-package diff.

### DD3: Job content — verbatim template with four token swaps

Copy the entire sibling block (lines `1123` to `1208`) verbatim. Swap
four tokens, all derived from the substitution table in Luna's
`wave-decisions.md`:

| # | Slot | Sibling value | Swap to | Sibling line(s) |
|---|------|---------------|---------|-----------------|
| 1 | Job key | `gate-5-mutants-log-query-api:` | `gate-5-mutants-lumen:` | 1123 |
| 2 | `name:` field | `Gate 5 — cargo mutants (log-query-api)` | `Gate 5 — cargo mutants (lumen)` | 1124 |
| 3 | Cache step name | `Cache Cargo registry, git index and target/ (log-query-api)` | `Cache Cargo registry, git index and target/ (lumen)` | 1141 |
| 4 | Cache key (primary) | `${{ runner.os }}-cargo-mutants-log-query-api-${{ hashFiles('**/Cargo.lock') }}` | `${{ runner.os }}-cargo-mutants-lumen-${{ hashFiles('**/Cargo.lock') }}` | 1148 |
| 5 | Cache restore-key | `${{ runner.os }}-cargo-mutants-log-query-api-` | `${{ runner.os }}-cargo-mutants-lumen-` | 1150 |
| 6 | `cargo mutants` step name | `cargo mutants (log-query-api, in-diff)` | `cargo mutants (lumen, in-diff)` | 1158 |
| 7 | Diff filter path glob | `'crates/log-query-api/**'` | `'crates/lumen/**'` | 1181 |
| 8 | Short-circuit log line | `No log-query-api-touching changes vs $BASELINE; skipping mutation testing.` | `No lumen-touching changes vs $BASELINE; skipping mutation testing.` | 1183 |
| 9 | Diff head log line | `--- log-query-api diff vs $BASELINE (head) ---` | `--- lumen diff vs $BASELINE (head) ---` | 1186 |
| 10 | `cargo mutants --package` (in-diff branch) | `log-query-api` | `lumen` | 1190 |
| 11 | `cargo mutants --package` (full branch) | `log-query-api` | `lumen` | 1197 |
| 12 | Artefact step name | `Upload mutants.out artefact (log-query-api)` | `Upload mutants.out artefact (lumen)` | 1202 |
| 13 | Artefact `name:` | `mutants-out-log-query-api` | `mutants-out-lumen` | 1206 |

The package name `lumen` is confirmed by `crates/lumen/Cargo.toml`
line 2 (`name = "lumen"`). Luna's substitution table named four
token classes (package, diff path, cache key shard, artefact); the
table above lists every textual occurrence required by that fourfold
class swap, including the comment-level log strings the sibling
job emits to make its job log self-describing. Inline comments
(sibling lines 1159 to 1169) are kept verbatim; they document the
`--in-diff` strategy and baseline cascade and apply unchanged to the
`lumen` job.

## Reuse Analysis

The feature is 100% reuse. No new component is designed.

| Component | File | Decision | Justification |
|-----------|------|----------|---------------|
| `gate-5-mutants-*` job template | `.github/workflows/ci.yml` lines 1123 to 1208 (sibling `gate-5-mutants-log-query-api`) | REUSE-COPY | Sixteen sibling jobs share an identical shape; verbatim replication with four token-class swaps is the documented pattern (ADR-0048, ADR-0052, `query-http-common-v0` DEVOPS precedent) |
| `cargo-mutants` installer (`taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090` with `tool: cargo-mutants`) | Sibling jobs (e.g. line 1154) | REUSE | Already pinned and used by sixteen sibling jobs; no new tool, no `deny.toml` change, no new workspace dependency (K4 guardrail) |
| `ubuntu-latest` runner | Sibling jobs (line 1125) | REUSE | Uniform runner across every CI job in the workflow |
| `actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd` with `fetch-depth: 0` | Sibling jobs (lines 1131 to 1134) | REUSE | Required for the `origin/main` baseline branch resolution in the `--in-diff` cascade |
| `dtolnay/rust-toolchain@e97e2d8cc328f1b50210efc529dca0028893a2d9` with `toolchain: stable` | Sibling jobs (lines 1136 to 1139) | REUSE | Workspace-wide toolchain pin |
| `actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae` for `~/.cargo` and `target/` | Sibling jobs (lines 1142 to 1151) | REUSE | Shard-keyed by package name; the only delta is the shard token (`log-query-api` → `lumen`) |
| `--in-diff "$DIFF_FILE"` filter, `origin/main → HEAD~1 → full` baseline cascade, empty-diff short-circuit | Sibling jobs (lines 1170 to 1200) | REUSE | Per-crate `--in-diff` scoping per ADR-0047; identical script body across every sibling |
| `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` with `if: success() || failure()`, `retention-days: 30` | Sibling jobs (lines 1202 to 1208) | REUSE | Uniform artefact upload posture across every sibling |
| `needs: [gate-2-public-api, gate-3-semver]` | Every existing `gate-5-mutants-*` job | REUSE | DD2 above |

No CREATE NEW decisions. No EXTEND decisions on production source.
The single workflow-file edit is a structural insert of a verbatim
copy.

## Architecture Summary

One new job block is inserted into `.github/workflows/ci.yml` at line
1209. Nothing else changes. No Rust source file is touched, no
`Cargo.toml`, `Cargo.lock`, `deny.toml`, or `rust-toolchain.toml` is
touched, no ADR is created or modified. The `lumen` crate gains
per-crate Gate 5 enforcement, matching the posture of the sixteen
sibling crates. The seventeen-of-seventeen invariant (K3 guardrail)
is preserved by pure addition: every sibling job is byte-identical
pre vs post.

## Technology Stack

No new external dependency. `cargo-mutants` is already installed by
sixteen sibling jobs via `taiki-e/install-action` from a precompiled
binary, and the workspace dependency graph (`Cargo.toml`,
`Cargo.lock`, `deny.toml`) is not consulted by the installer. The
runner is `ubuntu-latest`, the toolchain is `stable`, the cache
action is `actions/cache@27d5ce7f...`, the upload-artefact action is
`actions/upload-artifact@043fb46d...`, the checkout action is
`actions/checkout@de0fac2e...`. All five pins are already in use by
the sibling jobs.

## DEVOPS Handoff

The DEVOPS wave (Apex) inserts the following block at line 1209 of
`.github/workflows/ci.yml`, verbatim, with no further substitution.
This is the sibling block (lines 1123 to 1208) with the thirteen
token swaps from DD3 applied. Inline comments are preserved verbatim
because they document the `--in-diff` strategy and baseline cascade.

```yaml
  gate-5-mutants-lumen:
    name: Gate 5 — cargo mutants (lumen)
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

      - name: Cache Cargo registry, git index and target/ (lumen)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-lumen-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-lumen-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (lumen, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the query-api, harness,
        # aperture, spark, sieve, and codex jobs. An empty diff (commit
        # does not touch crates/lumen/) short-circuits to a
        # zero-second exit.
        #
        # Per lumen-query-api-v0 DEVOPS (ADR-0047), this single job
        # covers every src file added to lumen via path-filtered
        # --in-diff. Primary mutation targets: the Predicate matches
        # arms (body / host / service / severity), the
        # is_empty short-circuit, and the in-memory store adapter.
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
            git diff "$BASELINE" HEAD -- 'crates/lumen/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No lumen-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- lumen diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package lumen \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package lumen \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (lumen)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-lumen
          path: mutants.out/
          retention-days: 30
```

The comment block beginning `# Per lumen-query-api-v0 DEVOPS` adapts
the sibling's free-form descriptive comment to name `lumen`'s primary
mutation targets (`Predicate::matches` arms, the `is_empty`
short-circuit, the in-memory store adapter). The structural body of
the script (baseline cascade, short-circuit, `cargo mutants`
invocation) is byte-identical to the sibling modulo the package and
path tokens.

Apex inserts this block at line 1209 and pushes
`gate-5-mutants-trace-query-api` and every subsequent job down by
the length of the inserted block. K3 verification (sixteen pre-existing
jobs unchanged) is a `diff` excluding the new block.

## Upstream Changes

None. DESIGN does not revise any DISCUSS assumption. Both flags are
resolved to Luna's recommended option; the four-token substitution
table from `discuss/wave-decisions.md` is expanded (without
contradiction) to thirteen textual swaps in DD3 above.

No ADR is created. The five-gate ADR-0005 contract is unchanged; this
is the addition of a seventeenth per-crate instance of Gate 5, by an
already-established pattern documented in ADR-0047 (per-crate
`--in-diff` scoping), ADR-0048 (per-crate Gate 5 precedent), and
ADR-0052 (per-crate graduation pattern). Replicating an established
pattern is execution; creating a sixth gate or changing the
substitution rule would be a decision.
