# Wave Decisions - `cli-cinder-otlp-wiring-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (D1–D8, see brief);
  in-wave Apex decisions recorded below as A-decisions.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill.
2. `discuss/outcome-kpis.md` - OK6 (principal cross-writer NDJSON
   validity), OK7 (per-call Cinder line presence), OK8 (Lumen
   non-regression).
3. `design/wave-decisions.md` - DD1 (`File::try_clone`), DD2-DD5,
   DEVOPS handoff annotation.
4. `docs/feature/cinder-to-otlp-json-bridge-v0/devops/{wave-decisions, environments, kpi-instrumentation, ci-cd-pipeline}.{md, yaml}`
   - template; this feature inherits every A-posture except the
   workflow-edit posture (one new Gate 5 job here vs zero there).
5. `.github/workflows/ci.yml` lines 862–947 -
   `gate-5-mutants-self-observe` job, byte-for-byte template.
6. `crates/kaleidoscope-cli/Cargo.toml:24` - `self-observe` is
   already a workspace dep; no `[dependencies]` change.

## Pre-wave decisions (carried in, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| D1 | `deployment_target` | N/A (CLI wiring; binary unchanged) | Andrea, pre-wave |
| D2 | `container_orchestration` | N/A | Andrea, pre-wave |
| D3 | `cicd_platform` | GitHub Actions (existing) | Andrea, pre-wave + ADR-0005 |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI) | Andrea, pre-wave |
| D5 | `observability_and_logging` | The feature IS observability infra | Andrea, pre-wave |
| D6 | `deployment_strategy` | N/A | Andrea, pre-wave |
| D7 | `continuous_learning` | No (single-feature delivery, no A/B, no flags) | Andrea, pre-wave |
| D8 | `git_branching_strategy` | Trunk-Based Development | Andrea, pre-wave + memory `project_kaleidoscope_pure_trunk_based` |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill rate per ADR-0005 Gate 5 | CLAUDE.md (declared) |

## Differences from the cinder-to-otlp-json-bridge-v0 template

1. **CI workflow edit count is ONE** (prior wave was ZERO).
   Different crate (`kaleidoscope-cli`); no precedent Gate 5 job
   covers it. A1 adds `gate-5-mutants-kaleidoscope-cli` as a
   sibling per-package job.
2. **No new source-tree dependency** (prior wave added `cinder`
   via its Pulse-sink sibling). A4 confirms zero new deps.

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] Add sibling `gate-5-mutants-kaleidoscope-cli` job

**Options considered**:

1. **Add sibling per-package job** mirroring
   `gate-5-mutants-self-observe` byte-for-byte.
2. **Extend an existing Gate 5 job** to cover both packages.
3. **Skip Gate 5 on kaleidoscope-cli** as "small enough".

**Recommendation**: **Option 1** - sibling job.

**Rationale**:

- **Precedent**: every per-package Gate 5 job today is
  single-package-scoped (`gate-5-mutants-codex`, `-spark`,
  `-sieve`, `-self-observe`, `-harness`, `-aperture`). One job
  per package, with `--in-diff` path filter scoping per commit.
  Option 2 breaks the cache-key namespace (keyed by package name)
  and the artefact-naming convention (`mutants-out-{package}`).
- **Kill-rate enforcement**: Option 3 violates CLAUDE.md's
  per-feature MT contract. Mutating the wiring (e.g. eliding
  `try_clone()?`, or forcing the `Some(path)` arm to construct
  `NoopRecorder`) is exactly the regression class that compiles
  green; the acceptance test must kill these mutants and Gate 5
  is the mechanical oracle.
- **`gate-5-mutants-self-observe` stays scoped to self-observe**.
  Two independent jobs, each owning one package's mutation
  surface. No coupling.

**Verdict**: add as sibling job in the DELIVER commit that
lands the wiring edit + new test. Per-package precedent
preserved.

### [A2] Gate 1 inherits via `[[test]]` block; workflow edit IS A3's new job

**Recommendation**: no Gate 1 workflow edit. `cargo test
--workspace --all-targets --locked` (ci.yml:182) auto-discovers
the new test via its `[[test]]` block in
`crates/kaleidoscope-cli/Cargo.toml`. Identical posture to the
prior wave's A2.

**Clarification on workflow-edit accounting**: the single
workflow edit required by this feature is the NEW Gate 5 job
from A3, NOT any edit to Gate 1's invocation. ci.yml:182 is
untouched. Crafty-side change to ci.yml is a pure addition of
one new job block (mirrored from lines 862–947) alongside the
other Gate 5 jobs. Crafty lands the job block, the wiring edit,
the new test file, and the `[[test]]` block in ONE atomic
commit per ADR-0005's "tests and source land together" rule.

**Trade-off accepted**: a malformed `[[test]]` block fails Gate
1 for the whole workspace. Correct fail-fast behaviour.

### [A3] New `gate-5-mutants-kaleidoscope-cli` job - byte-for-byte mirror

**Decision spec** (six substitutions vs `gate-5-mutants-self-observe`):

| Field | Value |
|-------|-------|
| Job key | `gate-5-mutants-kaleidoscope-cli` |
| `name` | `Gate 5 - cargo mutants (kaleidoscope-cli)` |
| `--in-diff` path filter | `crates/kaleidoscope-cli/**` |
| `--package` | `kaleidoscope-cli` |
| Cache key suffix | `kaleidoscope-cli` |
| Artefact name | `mutants-out-kaleidoscope-cli` |

All other fields (`runs-on`, `needs`, `timeout-minutes`,
toolchain, baseline cascade, `--no-shuffle --jobs 2`, retention)
are copied unchanged. Full YAML in `ci-cd-pipeline.md`; Crafty
copy-pastes into ci.yml in the DELIVER commit.

**Trade-off accepted**: one extra Gate 5 parallel job. `--in-diff`
short-circuits to zero-second exit on commits that do not touch
`crates/kaleidoscope-cli/`.

### [A4] Zero new external dependencies

Verified: `self-observe = { path = "../self-observe", version =
"0.1.0" }` already present at `crates/kaleidoscope-cli/Cargo.toml:24`.
Wiring gains an import name (`CinderToOtlpJsonWriter`,
re-exported from `self-observe` per ADR-0039 §1). The new test
uses `serde_json` (already a dev-dep). Zero `[dependencies]`
edit, zero `deny.toml` change. Only `Cargo.toml` addition:

```toml
[[test]]
name = "observe_otlp_cinder_wiring"
path = "tests/observe_otlp_cinder_wiring.rs"
```

### [A5] No new toolchain pin

Inherits workspace stable Rust (`rust-toolchain.toml`). The
nightly pin is not exercised (no Gate 2/3 graduation).

## Skipped artefacts (N/A per library-shape + pre-wave decisions)

`platform-architecture.md` (Morgan's app-architecture sufficient),
`observability-design.md` (D5: feature IS observability),
`monitoring-alerting.md` (CI gates ARE alerts),
`infrastructure-integration.md` (no external integrations),
`branching-strategy.md` (D8 trunk-based default),
`continuous-learning.md` (D7 no A/B, no flags).

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Author the new test file `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` with RED scenarios for OK6/OK7 + the `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml` | Keeps `main` green and CI machinery consistent with source state. |
| At DELIVER | Land the wiring edit, the new test file, the `[[test]]` block, AND the new `gate-5-mutants-kaleidoscope-cli` job in ONE atomic commit | A3: the new job's `--in-diff` cascade requires the commit to be self-contained so the mutation run on the merge commit sees both source and test diffs. |
| At DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 or Gate 3 | No graduation trigger for a binary crate. |
| At DELIVER | Turn every mutant on the changed surface in `crates/kaleidoscope-cli/src/lib.rs` 100% killed before review approval | Per CLAUDE.md per-feature MT strategy + ADR-0005 Gate 5. |
| At DELIVER | The existing `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` MUST pass unchanged | OK8 guardrail per outcome-kpis.md. |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced by this wave**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-cinder-otlp-wiring-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/cli-cinder-otlp-wiring-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation design | `docs/feature/cli-cinder-otlp-wiring-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline addendum + full YAML for new Gate 5 job | `docs/feature/cli-cinder-otlp-wiring-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### Pre-push hook graduation (cross-feature handoff)

Pre-push per-pkg loop currently iterates
`[otlp-conformance-harness, spark, sieve, codex]`. If/when
kaleidoscope-cli gains a library-shaped public surface (e.g. a
future feature extracts `ingest` into a `kaleidoscope-cli-core`
library crate), that feature's DEVOPS wave MUST add the new
crate name to (1) CI workflow Gates 2+3 per-package matrix,
(2) the pre-push hook's per-pkg loop, (3) pre-commit if
relevant. The graduation feature owns that synchronisation.

### Mutation kill-rate measurement protocol (DELIVER clarification)

For the DELIVER crafter, mirroring the prior wave:

1. After wiring tests turn GREEN, run locally:
   `cargo mutants --package kaleidoscope-cli --in-diff <(git
   diff origin/main HEAD -- crates/kaleidoscope-cli/src/lib.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero.
3. Survivors → strengthen the test (concurrent-random-pause for
   OK6 or happy-path for OK7), or escalate.
4. CI-layer oracle: `gate-5-mutants-kaleidoscope-cli` on merge.

Prior wave precedent: commit 4d20c31 hit 6/6 = 100% kill.
