# Wave Decisions - `cli-read-observe-otlp-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (D1-D8, see brief);
  in-wave Apex decisions recorded below as A-decisions.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill.
2. `discuss/outcome-kpis.md` - OK1 (principal, `lumen.query.count`
   line per `read` invocation), OK2 (no-flag non-regression
   guardrail), OK3 (cross-subcommand symmetry).
3. `design/wave-decisions.md` - DD1 (single OpenOptions::append,
   no try_clone), DD2 (no helper extraction at N=2), DD3
   (`read()` gains `Option<&Path>` 4th param), DD4 (RCA: reuse
   only), DD5 (out-of-scope confirmations), DEVOPS handoff
   annotation.
4. `docs/feature/cli-cinder-otlp-wiring-v0/devops/{wave-decisions,
   environments, kpi-instrumentation, ci-cd-pipeline}.{md, yaml}`
   - template; this feature inherits every A-posture AND now
   inherits the workflow-edit posture (ZERO edits here vs ONE
   there: the prior wave's gate-5-mutants-kaleidoscope-cli job
   already covers this feature via `--in-diff`).
5. `crates/kaleidoscope-cli/Cargo.toml:24` - `self-observe` is
   already a workspace dep; the import line already names
   `LumenToOtlpJsonWriter` (used by `ingest`). No
   `[dependencies]` change.

## Pre-wave decisions (carried in, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| D1 | `deployment_target` | N/A (CLI wiring; binary unchanged) | Andrea, pre-wave |
| D2 | `container_orchestration` | N/A | Andrea, pre-wave |
| D3 | `cicd_platform` | GitHub Actions (existing) | Andrea, pre-wave + ADR-0005 |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-package Gate 5 jobs) | Andrea, pre-wave |
| D5 | `observability_and_logging` | The feature IS observability infra | Andrea, pre-wave |
| D6 | `deployment_strategy` | N/A | Andrea, pre-wave |
| D7 | `continuous_learning` | No (single-feature delivery, no A/B, no flags) | Andrea, pre-wave |
| D8 | `git_branching_strategy` | Trunk-Based Development | Andrea, pre-wave + memory `project_kaleidoscope_pure_trunk_based` |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill rate per ADR-0005 Gate 5 | CLAUDE.md (declared) |

## Differences from the cli-cinder-otlp-wiring-v0 template

1. **CI workflow edit count is ZERO** (prior wave was ONE).
   The Gate 5 job `gate-5-mutants-kaleidoscope-cli` added by
   commit 2baa05c is path-filtered on
   `crates/kaleidoscope-cli/**` via `--in-diff`; any commit
   touching `src/lib.rs` or `src/main.rs` (this feature touches
   both) is automatically mutated. The prior wave's investment
   in that job pays off here in full: zero workflow churn.
2. **One writer participates in `read`, not two.** No
   `try_clone`, no concurrent-writer scenario, no
   concurrent-random-pause test. KPI surface accordingly
   shrinks from 3 (OK6/OK7/OK8) to 3 (OK1/OK2/OK3) but the
   character changes: OK3 is sequential, not concurrent.
3. **No new source-tree dependency** (same as the prior wave).
   A3 confirms zero new deps.

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] No new CI workflow edit - inherit existing Gate 5 job

**Options considered**:

1. **Inherit existing `gate-5-mutants-kaleidoscope-cli` job**
   via its `--in-diff` path filter on
   `crates/kaleidoscope-cli/**`.
2. **Add a per-file Gate 5 fan-out** (e.g.
   `gate-5-mutants-kaleidoscope-cli-read`) for finer-grained
   reporting.
3. **Skip Gate 5 on this feature** as "wiring edit, small
   surface".

**Recommendation**: **Option 1** - inherit.

**Rationale**:

- **The prior wave's investment pays off.** Commit 2baa05c
  added `gate-5-mutants-kaleidoscope-cli` precisely so that
  subsequent CLI wiring features (this one, future ones) cost
  zero workflow edits. The job's `--in-diff` cascade
  (`origin/main` -> `HEAD~1` -> full) auto-picks up the diff
  on `src/lib.rs` (the `read()` body + signature) and
  `src/main.rs` (the `run_read` dispatcher edit) on the merge
  commit. No new job means no new cache-key namespace, no new
  artefact-naming convention, no new `needs:` graph entry.
- **Per-file fan-out is premature** at N=2 modified files in
  one crate. The existing job mutates both files in one pass;
  the artefact `mutants-out-kaleidoscope-cli` carries both
  files' surviving-mutant reports. Fan-out adds parallel runner
  spend with no diagnostic gain at this scale.
- **Skipping Gate 5** violates CLAUDE.md's per-feature MT
  contract. Mutating the wiring (e.g. eliding the
  `OpenOptions::append(true)` flag, or flipping the
  `Some(path)` arm to construct `LumenToPulseRecorder` instead
  of `LumenToOtlpJsonWriter`) is exactly the regression class
  that compiles green; the acceptance test must kill these
  mutants and Gate 5 is the mechanical oracle.

**Verdict**: NO edit to `.github/workflows/ci.yml`. The
existing job covers this feature in full. Crafty's DELIVER
commit touches only source + test + `Cargo.toml`'s `[[test]]`
block. Per-package Gate 5 precedent preserved.

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Recommendation**: no Gate 1 workflow edit. `cargo test
--workspace --all-targets --locked` (ci.yml:182) auto-discovers
the new test via its `[[test]]` block in
`crates/kaleidoscope-cli/Cargo.toml`. Identical posture to the
prior wave's A2.

**Clarification on workflow-edit accounting**: A1 + A2 together
mean **ZERO workflow edits** for this feature. ci.yml is
byte-untouched. Crafty lands the wiring edit (`src/lib.rs` +
`src/main.rs`), the new test file (`tests/observe_otlp_read_flag.rs`),
and the `[[test]]` block in `Cargo.toml` in ONE atomic commit
per ADR-0005's "tests and source land together" rule. This is
the explicit case where the prior wave's per-package Gate 5
job and the workspace's `cargo test --workspace` posture
collectively absorb the entire CI footprint of a wiring feature
within the same crate.

**Trade-off accepted**: a malformed `[[test]]` block fails
Gate 1 for the whole workspace. Correct fail-fast behaviour.

### [A3] Zero new external dependencies

Verified: `self-observe = { path = "../self-observe", version =
"0.1.0" }` already present at
`crates/kaleidoscope-cli/Cargo.toml:24`. The import line at
`crates/kaleidoscope-cli/src/lib.rs:65` already names
`LumenToOtlpJsonWriter` (used by `ingest`); the new `read()`
construction reuses that import unchanged. The new test uses
`serde_json` (already a dev-dep) and `tempfile` (already used
by the prior test files in this crate). Zero
`[dependencies]` edit, zero `[dev-dependencies]` edit, zero
`deny.toml` change. Only `Cargo.toml` addition:

```toml
[[test]]
name = "observe_otlp_read_flag"
path = "tests/observe_otlp_read_flag.rs"
```

### [A4] No new toolchain pin

Inherits workspace stable Rust (`rust-toolchain.toml`). The
nightly pin is not exercised (no Gate 2/3 graduation;
kaleidoscope-cli remains a binary crate).

## Skipped artefacts (N/A per library-shape + pre-wave decisions)

`platform-architecture.md` (Morgan's app-architecture
sufficient), `observability-design.md` (D5: feature IS
observability), `monitoring-alerting.md` (CI gates ARE alerts),
`infrastructure-integration.md` (no external integrations),
`branching-strategy.md` (D8 trunk-based default),
`continuous-learning.md` (D7 no A/B, no flags).

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Author the new test file `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` with RED scenarios for OK1/OK2/OK3 + the `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml` | Keeps `main` green and CI machinery consistent with source state. |
| At DELIVER | Land the wiring edits (`src/lib.rs` + `src/main.rs`), the new test file, AND the `[[test]]` block in ONE atomic commit | The Gate 5 job's `--in-diff` cascade on the merge commit needs source + test diff to coincide for the mutation run to bind correctly. |
| At DELIVER | DO NOT edit `.github/workflows/ci.yml` | A1 + A2: zero workflow edits required for this feature. |
| At DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 or Gate 3 | No graduation trigger for a binary crate. |
| At DELIVER | Turn every mutant on the changed surface in `crates/kaleidoscope-cli/src/lib.rs` AND `crates/kaleidoscope-cli/src/main.rs` 100% killed before review approval | Per CLAUDE.md per-feature MT strategy + ADR-0005 Gate 5. |
| At DELIVER | The existing `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` and `tests/observe_otlp_cinder_wiring.rs` MUST pass unchanged | Cross-feature non-regression (the `ingest` side and the Cinder wiring side stay green). |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced by this wave**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-read-observe-otlp-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/cli-read-observe-otlp-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation design | `docs/feature/cli-read-observe-otlp-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation (no edits) | `docs/feature/cli-read-observe-otlp-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### Pre-push hook graduation (cross-feature handoff)

Pre-push per-pkg loop currently iterates
`[otlp-conformance-harness, spark, sieve, codex]`. If/when
kaleidoscope-cli gains a library-shaped public surface (e.g. a
future feature extracts `ingest` + `read` into a
`kaleidoscope-cli-core` library crate), that feature's DEVOPS
wave MUST add the new crate name to (1) CI workflow Gates 2+3
per-package matrix, (2) the pre-push hook's per-pkg loop, (3)
pre-commit if relevant. The graduation feature owns that
synchronisation. This feature does NOT trigger the graduation;
the binary crate posture is preserved.

### Mutation kill-rate measurement protocol (DELIVER clarification)

For the DELIVER crafter, mirroring the prior wave:

1. After wiring tests turn GREEN, run locally:
   `cargo mutants --package kaleidoscope-cli --in-diff <(git
   diff origin/main HEAD -- crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero.
3. Survivors -> strengthen the test (happy-path for OK1,
   no-flag for OK2, ingest-then-read for OK3), or escalate.
4. CI-layer oracle: existing `gate-5-mutants-kaleidoscope-cli`
   on merge - no new job, mutation surface auto-discovered via
   `--in-diff`.

Prior wave precedent: commit 4d20c31 hit 6/6 = 100% kill on
the Cinder wiring; commit 2baa05c established the per-package
Gate 5 job that this feature inherits at zero cost.
