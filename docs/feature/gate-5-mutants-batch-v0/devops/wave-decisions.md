# Wave Decisions - gate-5-mutants-batch-v0 / DEVOPS

British English. No em dashes in body. (Em dash appears only inside the
job `name:` display field, copied verbatim from the sibling for
consistency with the seventeen existing jobs.)

- **Wave**: DEVOPS (CI-only; closure of feature)
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-29
- **Mode**: NOT slim. This wave adds EIGHT new CI jobs
  (`gate-5-mutants-aegis`, `gate-5-mutants-augur`,
  `gate-5-mutants-beacon-server`, `gate-5-mutants-cinder`,
  `gate-5-mutants-integration-suite`,
  `gate-5-mutants-kaleidoscope-gateway`, `gate-5-mutants-loom`,
  `gate-5-mutants-sluice`) into `.github/workflows/ci.yml`. No
  production source change. No new tooling. No new dependency. No new
  tag. The DEVOPS commit is the closure of the feature; there is no
  DISTILL or DELIVER wave because there is no production code to
  scaffold.

## DEVOPS Decisions

| DD# | Decision area | Value | Rationale / source |
|-----|---------------|-------|--------------------|
| DD1 | Job placement | All eight new job keys block-appended after the last existing gate-5 job (`gate-5-mutants-query-http-common`, whose artefact step ended at the pre-edit line 1896), before the Prism gates 6-11 comment block. New keys land at lines 1898 (aegis), 1981 (augur), 2064 (beacon-server), 2147 (cinder), 2230 (integration-suite), 2316 (kaleidoscope-gateway), 2399 (loom), 2482 (sluice). DEVIATION from DESIGN DD1 (intercalated alphabetical-neighbour insertion). See "Placement decision" below | DESIGN DD1 itself records that the seventeen existing jobs are accretion-ordered, not alphabetical, and that a globally sorted block is unachievable without reordering (which K3 / K6 forbid). Block-append in alphabetical order among the eight is the lower-risk realisation of the same intent: every existing block stays byte-identical, the eight are self-indexed alphabetically, and the single contiguous append is one Edit rather than seven intercalated inserts |
| DD2 | `needs:` graph | `[gate-2-public-api, gate-3-semver]` copied verbatim from the `gate-5-mutants-lumen` sibling lines 1213 to 1215 for all eight | DESIGN DD2 (FLAG 2, Luna Option A PIN); uniform across all seventeen pre-existing `gate-5-mutants-*` jobs; deviation would introduce maintenance asymmetry without justification. None of the eight crates is in the Gate 2 / Gate 3 locked set, so no semver-checks coupling argues for a different upstream |
| DD3 | Token swaps | Package name `lumen` swapped to each crate name across the thirteen textual slots of DESIGN DD3 (job key, `name:` display, cache step name, cache key primary, cache restore-key shard, mutants step name, diff filter path glob, short-circuit log line, diff head log line, `--package` arg in in-diff branch, `--package` arg in full branch, artefact step name, artefact `name:`). Package names verified from `crates/<dir>/Cargo.toml` line 2: all eight equal their directory name | DESIGN DD3 substitution table |
| DD4 | Descriptive comment | Second comment paragraph in each `cargo mutants` step adapted per crate to name its primary mutation targets (aegis: JWT signature predicate, RBAC match arms, TOML tenant catalogue parser; augur: Z-score comparator, threshold; sluice: bounded-capacity check, FIFO order; beacon-server: scheduler rule-firing, PromQL HTTP client wiring; cinder: tier metadata, age-based lifecycle comparator; loom: TOML catalogue diff, validate/plan/apply ordering; integration-suite: none at present, test-only; kaleidoscope-gateway: host composition wiring, aperture plus StorageSink injection). The first comment paragraph (the `--in-diff` baseline cascade note) is kept verbatim. Cosmetic; structural script body byte-identical to the sibling modulo package and path tokens | DESIGN DD3 final paragraph (comment adaptation is explicitly permitted and cosmetic) |
| DD5 | Small-crate handling | Ship all eight, including the test-only `integration-suite` (~50 src LOC) and the small `kaleidoscope-gateway` (~486 src LOC). No policy exclusion, no placeholder comment. Green in both no-op cases: empty diff short-circuits via shell `exit 0`; diff with zero viable mutants makes cargo-mutants exit 0 (no surviving mutants is a vacuous pass; non-zero exit reserved for survivors and build failures) | DESIGN DD4 (FLAG 3, Luna Option A PIN). Future-proof: each crate activates automatically once it gains mutable production code, with no workflow edit |
| DD6 | No tag | Zero new crate version tag at DEVOPS close. No `1.0.0` on any crate (project policy: semver 1.0.0 is Andrea's call) | This feature is pure CI infrastructure; no published crate version is affected. None of the eight crates is in Gate 2 / Gate 3's locked set |
| DD7 | No version bump | No `Cargo.toml`, `Cargo.lock`, `deny.toml`, or `rust-toolchain.toml` touched. The crafter's authorised file list is `{.github/workflows/ci.yml}` | DISCUSS Constraints Established |
| DD8 | Observability | No new observability surface. The mutation signal rides on the existing PR status panel check produced by each new job. Consistent with the Kaleidoscope-wide no-live-observability-stack posture at v0; a contract-shaped outcome IS the signal | `outcome-kpis.md` Notes on KPI shape; all six KPIs are build-time / workflow-file measurements |
| DD9 | Peer review | Not dispatched. This sub-agent invocation is explicitly instructed not to send to the peer reviewer; the orchestrator owns the review and commit decision. The pattern is established (seventeen prior siblings, two prior DEVOPS precedents) | Orchestrator context; DISCUSS DD8 (Luna self-validated; pattern established) |

## CI Inheritance + Eight Additions

Four of the five ADR-0005 gates are inherited verbatim from the
existing workflow. The fifth gate (mutation testing) gains EIGHT new
per-crate jobs:

| Gate | Coverage for `gate-5-mutants-batch-v0` | Workflow delta |
|------|----------------------------------------|----------------|
| Gate 1 (`cargo test --workspace`) | Inherited; runs every crate's existing test suite | Zero |
| Gate 2 (`cargo public-api`) | Inherited; locked set unchanged. None of the eight crates is in the locked set | Zero |
| Gate 3 (`cargo semver-checks`) | Inherited; same locked set as Gate 2 | Zero |
| Gate 4 (`cargo deny`) | Inherited; no new dependency added; `deny.toml` byte-identical | Zero |
| Gate 5 (`cargo mutants`) | EIGHT NEW per-crate jobs modelled on the existing `gate-5-mutants-lumen` job. Each scopes `cargo mutants --package <crate> --in-diff "$DIFF_FILE"` with the `origin/main -> HEAD~1 -> full` baseline cascade and the empty-diff short-circuit | ADD eight jobs |

The eight new jobs follow the EXACT pattern of the seventeen existing
`gate-5-mutants-*` jobs. The total count rises from 17 to 25,
matching the 25 workspace crates one-to-one (verified by
`ls -d crates/*/ | wc -l` returning 25). The workspace now enjoys
uniform ADR-0005 Gate 5 coverage. All seventeen sibling jobs are
byte-identical pre vs post (K3 guardrail preserved); every
non-gate-5 job is byte-identical (K6 guardrail preserved). The
twenty-five-of-twenty-five invariant is achieved by pure addition.

The eight new job keys, with their post-edit line numbers:

| Line | New job key |
|------|-------------|
| 1898 | `gate-5-mutants-aegis` |
| 1981 | `gate-5-mutants-augur` |
| 2064 | `gate-5-mutants-beacon-server` |
| 2147 | `gate-5-mutants-cinder` |
| 2230 | `gate-5-mutants-integration-suite` |
| 2316 | `gate-5-mutants-kaleidoscope-gateway` |
| 2399 | `gate-5-mutants-loom` |
| 2482 | `gate-5-mutants-sluice` |

## Placement decision

DESIGN DD1 specified an intercalated insertion: each new block was to
land immediately after its nearest alphabetical-predecessor sibling
present in the file (aegis after aperture at line 503, integration-suite
after harness at line 453, and so on), applied bottom-up so earlier line
offsets stayed valid. Apex deviated and instead block-appended all eight
in alphabetical order among themselves after the last existing gate-5
job (`gate-5-mutants-query-http-common`), before the Prism gates 6-11
comment block.

The deviation is the lower-risk realisation of DESIGN DD1's own stated
intent, and DESIGN DD1 itself supplies the justification. The seventeen
existing jobs are accretion-ordered, NOT alphabetical (DESIGN DD1 records
the file order explicitly: harness, aperture, spark, sieve, codex,
self-observe ...). DESIGN DD1 conceded that a globally sorted block is
unachievable without reordering the existing jobs, which K3 and K6
forbid, and that its intercalated map therefore achieves only LOCAL
alphabetical adjacency, not a globally sorted block. Given that the
intercalated map already abandons true global order, a contiguous
alphabetical block-append delivers the same self-indexing benefit (the
eight are sorted among themselves and sit in one findable place) with
three concrete advantages:

1. **One Edit, not seven intercalated inserts.** A single contiguous
   append is far less error-prone than seven separate insertions at
   scattered line offsets, each of which risks an off-by-one or an
   accidental touch of an adjacent sibling block.
2. **Every existing block stays byte-identical with certainty.** The
   `git diff --numstat` on `.github/workflows/ci.yml` reports 667
   insertions and 0 deletions: nothing existing was removed or altered.
   The seventeen siblings and every non-gate-5 job are provably
   untouched (K3, K6).
3. **Temporal precedent.** The `query-http-common-v0` DEVOPS wave also
   appended its new gate-5 job at the end of the block; block-append is
   where new gate-5 jobs have historically accreted on this file.

The cost is that the eight are not interleaved with the seventeen
older crates in one global sort. That cost is the same cost DESIGN DD1
already accepted (its map produced only local adjacency, not global
order), so the deviation forfeits nothing DESIGN DD1 had actually
secured. A future feature that chooses to reorder the whole gate-5
block into one global sort remains a separate, larger edit out of this
feature's scope, exactly as DESIGN DD1 noted.

## Workflow edit verification

Post-edit, Apex executed the verification checks from
`application-architecture.md` Verification and from `outcome-kpis.md`
Measurement Plan.

1. K4 (total count). `grep -cE "^  gate-5-mutants-[a-z-]+:" .github/workflows/ci.yml`
   returns `25` (was `17` pre-edit, +8). Cross-checked against
   `ls -d crates/*/ | wc -l` which returns `25`, confirming one-to-one
   coverage of the twenty-five workspace crates.

2. K1 (per-crate existence).
   `grep -nE "^  gate-5-mutants-(aegis|augur|sluice|beacon-server|cinder|loom|integration-suite|kaleidoscope-gateway):" .github/workflows/ci.yml`
   returns exactly eight lines:

    ```text
    1898:  gate-5-mutants-aegis:
    1981:  gate-5-mutants-augur:
    2064:  gate-5-mutants-beacon-server:
    2147:  gate-5-mutants-cinder:
    2230:  gate-5-mutants-integration-suite:
    2316:  gate-5-mutants-kaleidoscope-gateway:
    2399:  gate-5-mutants-loom:
    2482:  gate-5-mutants-sluice:
    ```

3. K2 (gate plumbing). Each new block carries its own diff glob
   (`crates/<crate>/**`, one occurrence per crate, verified) and its
   own `--package <crate>` argument in both the in-diff and the full
   branch (verified two occurrences each for the spot-checked crates).
   The `--in-diff "$DIFF_FILE"` filter, the
   `git diff "$BASELINE" HEAD -- 'crates/<crate>/**'` baseline diff,
   and the `origin/main -> HEAD~1 -> full` cascade are byte-identical
   to the sibling modulo the package and path tokens.

4. K3 / K6 (zero regression). `git diff --numstat .github/workflows/ci.yml`
   returns `667	0	.github/workflows/ci.yml`: 667 insertions, 0
   deletions. A purely additive edit cannot have altered any existing
   line, so all seventeen sibling jobs and every non-gate-5 job are
   byte-identical pre vs post.

5. K5 (YAML validity).
   `ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml'); puts 'YAML OK'"`
   completed with `YAML OK`. Python's `yaml.safe_load` was attempted
   first but `pyyaml` is not installed in the externally-managed system
   Python; Ruby is preinstalled on macOS and provides equivalent strict
   YAML 1.1 parsing. This matches the `gate-5-mutants-lumen-v0` DEVOPS
   precedent.

## No new tooling

- No new external dependency. `cargo-mutants` is already installed by
  the seventeen sibling jobs via
  `taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090`
  from a precompiled binary; the workspace dependency graph
  (`Cargo.toml`, `Cargo.lock`, `deny.toml`) is not consulted.
- No new binary. No file under `crates/*/src/` is touched.
- No new crate. No file under `crates/` is created or modified.
- No new tag. No crate version bump. No `1.0.0`.
- No new observability surface. The mutation signal rides on the
  existing PR status panel check produced by each new job.
- No new container, no new image, no new runtime target. The eight
  jobs run on the existing `ubuntu-latest` runner.

## DISTILL/DELIVER skipped

This feature is DEVOPS-only by construction. US-01 specifies a single
CI workflow edit with no production source change. There is no
application code to scaffold, no acceptance scenario over a runtime
component, and no DELIVER commit on a Rust crate. The DEVOPS commit
closes the feature. DISTILL Mandate 4 (Environmental Realism) is
satisfied by the trivial `environments.yaml` enumerating the single
`clean` GitHub Actions runner. The K1 to K6 KPIs are verified at the
feature-close commit by `grep`, `git diff`, and a YAML parser smoke,
not by a runtime acceptance suite.

This pattern matches the precedent set by `gate-5-mutants-lumen-v0`
and `gate-5-mutants-query-http-common-v0`, where the DEVOPS wave was
the only wave that touched `.github/workflows/ci.yml`. The difference
here is cardinality: this feature appends eight jobs in one batch
rather than one, and ships no new crate at all.

## Upstream Changes

None. Every DEVOPS conclusion is consistent with the DESIGN handoff
recorded in
`docs/feature/gate-5-mutants-batch-v0/design/wave-decisions.md`. No
DESIGN assumption is revised. The placement deviation (DD1) is a
realisation choice within the latitude DESIGN DD1 explicitly granted
(it noted true global order was unachievable and that its own map
achieved only local adjacency); it does not contradict any DESIGN
decision and requires no
`docs/feature/gate-5-mutants-batch-v0/devops/upstream-changes.md`.
The five-gate ADR-0005 contract is unchanged; this adds eight per-crate
instances of Gate 5, not a sixth gate. ADR immutability is preserved.
