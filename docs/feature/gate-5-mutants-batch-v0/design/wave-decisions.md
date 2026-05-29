# Wave Decisions — gate-5-mutants-batch-v0 / DESIGN

British English. No em dashes in body. (Em dash appears only inside the
job `name:` display field, copied verbatim from the sibling for
consistency.)

- **Wave**: DESIGN (application scope, trivial)
- **Author**: Morgan (`nw-solution-architect`)
- **Date**: 2026-05-29
- **Mode**: propose
- **Input**: DISCUSS handoff by Luna (commit `a757f3a`), US-01 batch,
  three flags raised to DESIGN, K1 to K6 KPIs, ADR-0005 Gate 5
  contract, the `gate-5-mutants-lumen-v0` DESIGN and DEVOPS precedents,
  the `gate-5-mutants-query-http-common-v0` DEVOPS precedent.

This wave resolves the three flags and produces eight copy-paste-ready
YAML blocks plus an alphabetical placement map for the DEVOPS wave
(Apex). No ADR is created. Replicating an established pattern across an
eighteenth-through-twenty-fifth sibling is execution, not an
architectural decision; the `gate-5-mutants-lumen-v0` DESIGN wave set
the precedent of no ADR for the same shape of work, and this feature is
its batch sibling.

## DESIGN Decisions

### DD1: Alphabetical job placement (resolves FLAG 1)

Luna recommended Option A (alphabetic insertion). PIN. Alphabetic
insertion makes the gate-5 block self-indexing: a maintainer scanning
the file for `gate-5-mutants-<X>` finds it by name without a full-file
`grep`, and every future new crate has one and only one correct slot.

The seventeen pre-existing `gate-5-mutants-*` job keys, in file order
(verified by `grep -n "^  gate-5-mutants-" .github/workflows/ci.yml`):

| Line | Existing job key (crate token) |
|------|--------------------------------|
| 453  | harness |
| 503  | aperture |
| 604  | spark |
| 692  | sieve |
| 777  | codex |
| 862  | self-observe |
| 949  | aperture-storage-sink |
| 1036 | query-api |
| 1123 | log-query-api |
| 1210 | lumen |
| 1297 | trace-query-api |
| 1384 | pulse |
| 1467 | ray |
| 1550 | strata |
| 1635 | beacon |
| 1723 | kaleidoscope-cli |
| 1809 | query-http-common |

The file is currently ordered by accretion, not alphabetically. To
keep DD1 honest and low-risk, the placement is defined relative to the
**alphabetical neighbour among the gate-5 jobs that already sit in the
correct relative position**, so that Apex inserts each new block
immediately after its nearest alphabetical predecessor that is present
in the file. The merged alphabetical order of all twenty-five crate
tokens (ASCII; `-` 0x2D sorts before letters) is:

`aegis, aperture, aperture-storage-sink, augur, beacon, beacon-server,
cinder, codex, harness, integration-suite, kaleidoscope-cli,
kaleidoscope-gateway, log-query-api, loom, lumen, pulse, query-api,
query-http-common, ray, self-observe, sieve, sluice, spark, strata,
trace-query-api`.

Because the existing seventeen are NOT in file-order alphabetical, a
strict "insert at the globally correct line" is not achievable without
reordering the existing jobs (which K3 / K6 forbid). The pragmatic,
guardrail-safe resolution: insert each of the eight new blocks
**immediately after the existing sibling that is its nearest
alphabetical predecessor present in the file**, computed below. This
preserves all seventeen existing blocks byte-identical (K3, K6) and
places each new block adjacent to a recognisable alphabetical
neighbour. Apex applies the eight inserts from the bottom of the file
upward so earlier line numbers are not invalidated by earlier inserts.

| # | New crate (job key suffix) | Insert after existing job | Insert before (next existing job in file) | Rationale |
|---|----------------------------|---------------------------|-------------------------------------------|-----------|
| 1 | aegis | aperture (line 503, ends ~600) | spark (line 604) | aegis < aperture alphabetically; aperture is the nearest present sibling. Placing aegis immediately after aperture keeps them adjacent for the reader. |
| 2 | augur | aperture-storage-sink (line 949, ends ~1035) | query-api (line 1036) | augur > aperture-storage-sink, < beacon; aperture-storage-sink is the nearest present alphabetical predecessor in the file. |
| 3 | beacon-server | beacon (line 1635, ends ~1722) | kaleidoscope-cli (line 1723) | beacon-server immediately follows beacon alphabetically; beacon is present. |
| 4 | cinder | beacon-server (the new block from #3) | kaleidoscope-cli (line 1723) | cinder > beacon-server, < codex; placed directly after the new beacon-server block, both following beacon. |
| 5 | integration-suite | harness (line 453, ends ~502) | aperture (line 503) | integration-suite > harness, < kaleidoscope-cli; harness is the nearest present alphabetical predecessor in the file. |
| 6 | kaleidoscope-gateway | kaleidoscope-cli (line 1723, ends ~1808) | query-http-common (line 1809) | kaleidoscope-gateway immediately follows kaleidoscope-cli alphabetically; kaleidoscope-cli is present. |
| 7 | loom | log-query-api (line 1123, ends 1208) | lumen (line 1210) | loom > log-query-api, < lumen; both present and adjacent in the file, so loom slots cleanly between them. |
| 8 | sluice | sieve (line 692, ends ~776) | codex (line 777) | sluice > sieve, < spark; sieve is the nearest present alphabetical predecessor. |

Note on imperfect global order: because the existing file is
accretion-ordered, the eight inserts achieve **local** alphabetical
adjacency to a recognisable neighbour, not a globally sorted gate-5
block. A globally sorted block would require reordering the seventeen
existing jobs, which K3 and K6 forbid (zero rename, zero re-scope,
every non-new block byte-identical). DD1 therefore optimises for
guardrail safety first and readability second: each new job is findable
next to its alphabetical neighbour, and the seventeen existing jobs are
untouched. If a future feature chooses to reorder the whole block, that
is a separate, larger edit out of this feature's scope.

### DD2: `needs:` graph (resolves FLAG 2)

Luna recommended Option A (verbatim copy). PIN. All eight new jobs use,
verbatim:

```yaml
    needs:
      - gate-2-public-api
      - gate-3-semver
```

Verified against the `gate-5-mutants-lumen` sibling at
`.github/workflows/ci.yml` lines 1213 to 1215. This is the uniform
`needs:` graph across all seventeen existing `gate-5-mutants-*` jobs.
Deviating for any of the eight would introduce a maintenance asymmetry
with no justifying argument: every one of the eight crates has the same
build-graph position as its siblings (Gate 1 runs
`cargo test --workspace`; Gate 2 and Gate 3 lock the published crates'
public API; Gate 5 fires against the per-package diff). None of the
eight is in the Gate 2 / Gate 3 locked set, so no semver-checks
coupling argues for a different upstream.

### DD3: Token swaps per job (verbatim template, four token classes)

Copy the entire `gate-5-mutants-lumen` block (lines 1210 to 1295)
verbatim, then apply the four token-class swaps from Luna's
substitution table. Expanded to every textual occurrence, the swap
touches the following thirteen slots. For each new crate, every
occurrence of the token `lumen` below is replaced by the new crate's
`package.name` (which equals its directory name for all eight,
confirmed from each `crates/<dir>/Cargo.toml` line 2).

| # | Slot | Sibling (`lumen`) value | Sibling line |
|---|------|-------------------------|--------------|
| 1 | Job key | `gate-5-mutants-lumen:` | 1210 |
| 2 | `name:` display | `Gate 5 — cargo mutants (lumen)` | 1211 |
| 3 | Cache step name | `Cache Cargo registry, git index and target/ (lumen)` | 1228 |
| 4 | Cache key (primary) | `${{ runner.os }}-cargo-mutants-lumen-${{ hashFiles('**/Cargo.lock') }}` | 1235 |
| 5 | Cache restore-key shard | `${{ runner.os }}-cargo-mutants-lumen-` | 1237 |
| 6 | `cargo mutants` step name | `cargo mutants (lumen, in-diff)` | 1245 |
| 7 | Diff filter path glob | `'crates/lumen/**'` | 1268 |
| 8 | Short-circuit log line | `No lumen-touching changes vs $BASELINE; skipping mutation testing.` | 1270 |
| 9 | Diff head log line | `--- lumen diff vs $BASELINE (head) ---` | 1273 |
| 10 | `--package` (in-diff branch) | `--package lumen` | 1277 |
| 11 | `--package` (full branch) | `--package lumen` | 1285 |
| 12 | Artefact step name | `Upload mutants.out artefact (lumen)` | 1289 |
| 13 | Artefact `name:` | `mutants-out-lumen` | 1293 |

Per-crate package names (all equal to the directory name, verified):

| Crate dir | `package.name` | Diff filter path glob |
|-----------|----------------|-----------------------|
| aegis | `aegis` | `crates/aegis/**` |
| augur | `augur` | `crates/augur/**` |
| sluice | `sluice` | `crates/sluice/**` |
| beacon-server | `beacon-server` | `crates/beacon-server/**` |
| cinder | `cinder` | `crates/cinder/**` |
| loom | `loom` | `crates/loom/**` |
| integration-suite | `integration-suite` | `crates/integration-suite/**` |
| kaleidoscope-gateway | `kaleidoscope-gateway` | `crates/kaleidoscope-gateway/**` |

The descriptive comment in the `cargo mutants` step (sibling lines 1246
to 1256) names `lumen`-specific mutation targets. For the eight new
jobs, Apex MAY adapt the second comment paragraph to name each crate's
primary mutation targets (for example aegis: the JWT signature
predicate and RBAC match arms; augur: the Z-score comparator and
threshold; sluice: the bounded-capacity check and FIFO order; cinder:
the age-based lifecycle comparator; loom: the TOML catalogue diff and
plan ordering; beacon-server and kaleidoscope-gateway: the host wiring
and sink injection). This is cosmetic; the structural script body
(baseline cascade, short-circuit, `cargo mutants` invocation) is
byte-identical to the sibling modulo the package and path tokens. The
first comment paragraph (sibling lines 1246 to 1250, the `--in-diff`
strategy note) is kept verbatim.

### DD4: Zero mutants is green for small crates (resolves FLAG 3)

Luna recommended Option A (ship all eight, including the test-only
`integration-suite`). PIN. Ship the job for all eight crates with no
policy exclusion and no placeholder comment. The verdict is green in
two distinct no-op situations, both of which are already exercised by
the seventeen siblings:

1. **Empty diff (PR does not touch the crate).** The shell
   short-circuit fires first: `git diff "$BASELINE" HEAD -- 'crates/<crate-dir>/**'`
   produces an empty file, the `[ ! -s "$DIFF_FILE" ]` guard prints
   `No <crate>-touching changes vs $BASELINE; skipping mutation testing.`
   and the step runs `exit 0`. `cargo mutants` is never invoked. This is
   the same short-circuit verified at the sibling line 1270 to 1271 and
   documented in the `query-http-common-v0` DEVOPS wave-decisions (line
   117, `exit 0`). Sub-minute, green.

2. **Diff present but zero viable mutants in scope.** When the diff
   touches the crate but `--in-diff "$DIFF_FILE"` resolves to zero
   mutable expressions (a tiny crate such as `integration-suite` at ~50
   src LOC, or a diff touching only non-mutable lines such as comments,
   `Cargo.toml`, or test fixtures), `cargo mutants` finds no mutants to
   test and exits with status 0. A run with no surviving mutants is a
   pass by definition: the 100% kill-rate invariant (ADR-0005 Gate 5)
   is vacuously satisfied when the set of mutants is empty. cargo-mutants
   reserves its non-zero exit (code 2, MUTANTS_FOUND) for surviving
   mutants and code 4 for usage/test-build failures; "found nothing to
   mutate" is not an error. So a small crate ships green and becomes
   active automatically once a future PR adds mutable production code,
   with no workflow edit required.

This is future-proof: `integration-suite` (test-only, ~50 src LOC) and
`kaleidoscope-gateway` (host binary, ~486 src LOC) both ship now and
start producing a real mutation signal the moment they gain mutable
production logic. No ADR exclusion, no commented placeholder slot, no
asymmetry against the other six.

## Reuse Analysis

The feature is 100% reuse. No new component is designed; eight verbatim
copies of one established block are inserted.

| Component | File | Decision | Justification |
|-----------|------|----------|---------------|
| `gate-5-mutants-*` job template | `.github/workflows/ci.yml` lines 1210 to 1295 (sibling `gate-5-mutants-lumen`) | REUSE-COPY x8 | Seventeen sibling jobs share an identical shape; verbatim replication with the four token-class swaps is the documented pattern (ADR-0047, ADR-0048, ADR-0052; `lumen` and `query-http-common` DEVOPS precedents) |
| `cargo-mutants` installer (`taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090`, `tool: cargo-mutants`) | Sibling jobs (e.g. line 1241) | REUSE | Already pinned and used by seventeen sibling jobs; precompiled binary, no `deny.toml` change, no new workspace dependency (K3 / AC-12 guardrail) |
| `ubuntu-latest` runner | Sibling jobs (line 1212) | REUSE | Uniform runner across every CI job |
| `--in-diff "$DIFF_FILE"` filter + `origin/main -> HEAD~1 -> full` baseline cascade + empty-diff short-circuit | Sibling jobs (lines 1257 to 1287) | REUSE | Per-crate `--in-diff` scoping per ADR-0047; identical script body across every sibling |
| `actions/checkout@de0fac2e...` with `fetch-depth: 0` | Sibling jobs (lines 1218 to 1221) | REUSE | Required for `origin/main` baseline resolution |
| `dtolnay/rust-toolchain@e97e2d8c...` `toolchain: stable` | Sibling jobs (lines 1223 to 1226) | REUSE | Workspace-wide toolchain pin |
| `actions/cache@27d5ce7f...` for `~/.cargo` and `target/` | Sibling jobs (lines 1228 to 1238) | REUSE | Shard-keyed by package name; only the shard token changes per crate |
| `actions/upload-artifact@043fb46d...` `if: success() || failure()`, `retention-days: 30` | Sibling jobs (lines 1289 to 1295) | REUSE | Uniform artefact upload posture |
| `needs: [gate-2-public-api, gate-3-semver]` | Every existing `gate-5-mutants-*` job | REUSE | DD2 |

No CREATE NEW. No EXTEND on production source. The single workflow-file
edit is eight structural inserts of a verbatim copy.

## Architecture Summary

Eight new job blocks are inserted into `.github/workflows/ci.yml` at
their alphabetical-neighbour slots (DD1). Nothing else changes. No Rust
source is touched, no `Cargo.toml`, `Cargo.lock`, `deny.toml`, or
`rust-toolchain.toml` is touched, no ADR is created or modified. The
eight residual crates gain per-crate Gate 5 enforcement, bringing the
workspace to uniform 25 / 25 coverage. The seventeen-of-seventeen
invariant (K3, K6) is preserved by pure addition: every existing job
block is byte-identical pre vs post.

## Technology Stack

No new external dependency. `cargo-mutants` is already installed by the
seventeen sibling jobs via `taiki-e/install-action` from a precompiled
binary, and the installer does not consult the workspace dependency
graph. The runner is `ubuntu-latest`, the toolchain is `stable`, and
the five action pins (checkout, rust-toolchain, cache, install-action,
upload-artifact) are all already in use by the sibling jobs.

## DEVOPS Handoff

Apex inserts eight blocks. Each block is the `gate-5-mutants-lumen`
template (lines 1210 to 1295) with `lumen` swapped to the crate token
in all thirteen slots of DD3. Apply the inserts bottom-up by file line
number so earlier line offsets are not invalidated. The placement map
(DD1) and the swap map (DD3) are repeated here as the copy-paste index:

| Order to apply (bottom-up) | New job key | Package name | Diff glob | Insert immediately after the block ending of |
|----------------------------|-------------|--------------|-----------|----------------------------------------------|
| 1 | `gate-5-mutants-kaleidoscope-gateway` | `kaleidoscope-gateway` | `crates/kaleidoscope-gateway/**` | `gate-5-mutants-kaleidoscope-cli` (line 1723) |
| 2 | `gate-5-mutants-beacon-server` then `gate-5-mutants-cinder` | `beacon-server`, `cinder` | `crates/beacon-server/**`, `crates/cinder/**` | `gate-5-mutants-beacon` (line 1635); cinder follows the new beacon-server block |
| 3 | `gate-5-mutants-loom` | `loom` | `crates/loom/**` | `gate-5-mutants-log-query-api` (line 1123, ends 1208) |
| 4 | `gate-5-mutants-augur` | `augur` | `crates/augur/**` | `gate-5-mutants-aperture-storage-sink` (line 949) |
| 5 | `gate-5-mutants-sluice` | `sluice` | `crates/sluice/**` | `gate-5-mutants-sieve` (line 692) |
| 6 | `gate-5-mutants-aegis` | `aegis` | `crates/aegis/**` | `gate-5-mutants-aperture` (line 503) |
| 7 | `gate-5-mutants-integration-suite` | `integration-suite` | `crates/integration-suite/**` | `gate-5-mutants-harness` (line 453) |

Apply order is bottom-up by insertion line so each insert leaves the
earlier (lower-line-number) targets unshifted. Within a single insert
point with two crates (beacon-server then cinder), insert cinder first
then beacon-server above it so the final file order reads
`beacon, beacon-server, cinder`.

The canonical block to copy (then swap thirteen tokens per crate) is the
verbatim `gate-5-mutants-lumen` block at `.github/workflows/ci.yml`
lines 1210 to 1295. The full template, already token-swapped, for one
crate (`aegis`) as the worked example for Apex:

```yaml
  gate-5-mutants-aegis:
    name: Gate 5 — cargo mutants (aegis)
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

      - name: Cache Cargo registry, git index and target/ (aegis)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-aegis-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-aegis-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (aegis, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the seventeen sibling jobs.
        # An empty diff (commit does not touch crates/aegis/)
        # short-circuits to a zero-second exit.
        #
        # Primary mutation targets: the JWT signature predicate, the
        # RBAC match arms, and the TOML tenant catalogue parser.
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
            git diff "$BASELINE" HEAD -- 'crates/aegis/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No aegis-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- aegis diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package aegis \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package aegis \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (aegis)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-aegis
          path: mutants.out/
          retention-days: 30
```

The other seven blocks are byte-identical to this with `aegis` swapped
to `augur`, `sluice`, `beacon-server`, `cinder`, `loom`,
`integration-suite`, `kaleidoscope-gateway` respectively across all
thirteen slots, and the descriptive comment's second paragraph adapted
per crate (cosmetic only).

## Upstream Changes

None. DESIGN does not revise any DISCUSS assumption. All three flags
are resolved to Luna's recommended option. The four-token substitution
table from `discuss/wave-decisions.md` is expanded (without
contradiction) to thirteen textual swaps in DD3.

No ADR is created. The five-gate ADR-0005 contract is unchanged; this
is the addition of eight per-crate instances of Gate 5 by an
already-established pattern documented in ADR-0047 (per-crate
`--in-diff` scoping), ADR-0048 (per-crate Gate 5 precedent), and
ADR-0052 (per-crate graduation pattern). Replicating an established
pattern is execution, not a decision; the `gate-5-mutants-lumen-v0`
DESIGN wave created no ADR for the identical shape of work, and this
feature is its batch sibling.
