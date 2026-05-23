# CI/CD pipeline addendum — ray-query-api-v0 / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-23
- **Mode**: propose. British English. No em dashes.

This addendum specifies the ONE CI change this feature requires: a new
Gate 5 (`cargo mutants`) job for the new crate `crates/trace-query-api`.
It does NOT modify `.github/workflows/ci.yml`; the block below is a
precise specification for the DELIVER crafter to ADD, in the same
atomic commit that creates the crate (mirroring the lumen-query-api-v0
DEVOPS A2 posture, and learning from the cinder-bridge post-merge
correction: the CI edit MUST land WITH the source, not be assumed
pre-existing).

## Inherited five-gate contract (ADR-0005), unchanged

| Gate | Tool | Effect of this feature |
|------|------|------------------------|
| 1 | `cargo test --workspace --all-targets --locked` | Auto-discovers the new crate's tests; ZERO workflow edit. |
| 2 | `cargo public-api` | NOT graduated for `trace-query-api` this feature (see wave-decisions). ZERO edit. |
| 3 | `cargo semver-checks` | NOT graduated for `trace-query-api` this feature. ZERO edit. |
| 4 | `cargo deny check` | No new external dependency; NO deny.toml change. ZERO edit. |
| 5 | `cargo mutants` (per-crate jobs) | ADD one new parallel job `gate-5-mutants-trace-query-api`. |

Per project memory (`project_kaleidoscope_pure_trunk_based`), CI is
feedback, not a merge gate: `main` has no required-status-checks and no
`enforce_admins`. These gates are correctness signals.

## The new Gate 5 job

Mirrored byte-for-byte from the REAL existing
`gate-5-mutants-log-query-api` job
(`.github/workflows/ci.yml:1123-1208`), the directly symmetric
precedent (same axum/tokio HTTP read-path shape over a durable
per-tenant store, same `--in-diff` cascade). Only the per-crate
substitutions change: package name, the `crates/<crate>/**` path
filter, the cache-key namespace, the artefact name, the job/step
names, and the mutation-target comment (which adds the missing-service
400 to the existing half-open / empty-vs-error / bounds-parser /
fail-closed list, the one structural divergence ADR-0048 Decision 1
introduces). The `needs:`, `timeout-minutes`, toolchain, the pinned
action SHAs, the `origin/main -> HEAD~1 -> full` baseline cascade, the
empty-diff short-circuit, and `--no-shuffle --jobs 2` are all preserved
identically.

Place this block alongside the other `gate-5-mutants-*` jobs in
`.github/workflows/ci.yml`:

```yaml
  gate-5-mutants-trace-query-api:
    name: Gate 5 — cargo mutants (trace-query-api)
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

      - name: Cache Cargo registry, git index and target/ (trace-query-api)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-trace-query-api-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-trace-query-api-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (trace-query-api, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the query-api, log-query-api,
        # harness, aperture, spark, sieve, and codex jobs. An empty diff
        # (commit does not touch crates/trace-query-api/) short-circuits
        # to a zero-second exit.
        #
        # Per ray-query-api-v0 DEVOPS (ADR-0048), this single job covers
        # every src file added to trace-query-api via path-filtered
        # --in-diff. Primary mutation targets: the half-open boundary,
        # the empty-vs-error distinction, the missing-service 400, the
        # bounds parser, and the fail-closed refusal.
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
            git diff "$BASELINE" HEAD -- 'crates/trace-query-api/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No trace-query-api-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- trace-query-api diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package trace-query-api \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package trace-query-api \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (trace-query-api)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-trace-query-api
          path: mutants.out/
          retention-days: 30
```

### Per-crate substitutions (the only differences from the mirror)

| Field | `gate-5-mutants-log-query-api` (mirrored) | `gate-5-mutants-trace-query-api` (new) |
|-------|-------------------------------------------|----------------------------------------|
| Job key | `gate-5-mutants-log-query-api` | `gate-5-mutants-trace-query-api` |
| `name:` | `Gate 5 — cargo mutants (log-query-api)` | `Gate 5 — cargo mutants (trace-query-api)` |
| Cache key namespace | `cargo-mutants-log-query-api` | `cargo-mutants-trace-query-api` |
| `--package` | `log-query-api` | `trace-query-api` |
| `--in-diff` path filter | `crates/log-query-api/**` | `crates/trace-query-api/**` |
| Artefact name | `mutants-out-log-query-api` | `mutants-out-trace-query-api` |
| Mutation-target comment | half-open / empty-vs-error / bounds / fail-closed | half-open / empty-vs-error / **missing-service 400** / bounds / fail-closed |

Everything else (action SHAs, `needs`, timeout, `--no-shuffle --jobs 2`,
the baseline cascade and short-circuit) is identical.

The one substantive list addition is the missing-service 400, the one
structural divergence ADR-0048 Decision 1 introduces over the
log-query-api shape: traces alone require an explicit `service`
parameter (the store mandates `&ServiceName`), and a missing or empty
`service` is a 400 (named, no store query run), NOT an empty result.
That arm is a primary mutation target because the empty-vs-400
distinction is precisely the honest-outcome guarantee US-04 demands.

## Gate 4 (cargo deny) verdict: NO change required

The new crate adds NO new external dependency. axum (0.7), hyper (1.4),
serde, serde_json, tokio, tower/tower-http (dev posture), aegis, and
ray are ALL already in the workspace and in `Cargo.lock` (verified by
grep: each resolves to existing entries; `trace-query-api` itself is
absent from the lock, confirming it is new). `regex` is NOT pulled in
(no label matchers in slice 01). No new licence, advisory, or yanked
crate enters the tree.

CONSTRAINT for the crafter: `deny.toml` sets `wildcards = "deny"` (line
84). Every dependency in `crates/trace-query-api/Cargo.toml` MUST be
pinned with an explicit version, never a `*` wildcard, exactly as
`crates/query-api/Cargo.toml` and `crates/log-query-api/Cargo.toml`
already do (`axum = { version = "0.7", ... }`, `hyper = "1.4"`,
in-workspace path deps with explicit `version = "0.1.0"`). A wildcard
would fail Gate 4 even though no new crate is introduced.

## Gate 2 / Gate 3 scope: NOT graduated this feature

`trace-query-api` is NOT added to the Gate 2 (`cargo public-api`) or
Gate 3 (`cargo semver-checks`) scope in this feature. This is
consistent with the self-observe and log-query-api precedents
(cinder-to-pulse-bridge-v0 DEVOPS A1; lumen-query-api-v0 DEVOPS A4): a
thin v0 crate whose only consumer is the workspace itself is not
locked under the public-api / semver gates until it stabilises or a
real external consumer appears. ADR-0048 Decision 5 is the surface
audit trail in the interim; the public port is just
`router(store, tenant)`. The pre-push hook's per-package Gate 2/3 loop
is NOT extended either.

## Graduation tag

Closing this feature requires a NEW per-crate tag
`trace-query-api/v0.1.0`, matching the crate's manifest version
(`version = "0.1.0"`), exactly as the sibling crates are tagged (NOT
`1.0.0`; this is a v0 slice). See wave-decisions.md for the recorded
decision.
