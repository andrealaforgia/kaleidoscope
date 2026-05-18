# Wave Decisions — `cinder-to-pulse-bridge-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-18
- **Mode**: Decisions pre-taken with Andrea (D1–D9, see brief);
  in-wave Apex decisions recorded below as A-decisions.

## Inputs read (in dependency order)

1. `CLAUDE.md` — paradigm declaration (Rust idiomatic, data + free
   functions + traits where polymorphism is genuinely needed); per-
   feature mutation testing strategy at 100% kill rate (declared,
   not modified by this wave).
2. `docs/feature/cinder-to-pulse-bridge-v0/discuss/outcome-kpis.md`
   — OK1/OK2/OK3 (leading, library-contract) + OK4 (guardrail).
3. `docs/feature/cinder-to-pulse-bridge-v0/design/wave-decisions.md`
   — DESIGN-wave decisions DD1–DD4 + the explicit DEVOPS handoff
   block (workspace changes, CI inherited unchanged, mutation scope
   = `crates/self-observe/src/cinder_bridge.rs`).
4. `docs/feature/cinder-to-pulse-bridge-v0/design/application-architecture.md`
   — C4 L1+L2, external-integrations = none, no-substrate adapter
   posture.
5. `docs/product/architecture/adr-0038-cinder-to-pulse-bridge-public-api-and-crate-layout.md`
   — locks public surface, per-event emission contract, file
   location, Cargo manifest additions.
6. `docs/product/architecture/adr-0005-ci-contract.md` — the five-
   gate CI contract that this wave inherits unchanged.
7. `docs/product/architecture/brief.md > ## Application Architecture
   — cinder-to-pulse-bridge-v0` — reuse-of-platform-level-decisions
   block confirms CI contract inheritance + per-feature MT scope.
8. `.github/workflows/ci.yml` — the existing five-gate workflow
   (lines 1–860 cover Gates 1–5 per-package). The workflow already
   covers harness, aperture, spark, sieve, codex (Rust) and the Prism
   apps (TS). `self-observe` is NOT currently in Gates 2/3/5; see A1
   below.
9. `scripts/hooks/pre-push` — Gates 2 and 3 mirror locally for the
   four currently-graduated packages.
10. `crates/self-observe/Cargo.toml` — current dependency list (aegis,
    lumen, pulse) and existing `[[test]]` blocks (lumen_to_pulse,
    lumen_to_otlp_json).
11. `docs/feature/beacon-v0/devops/{environments.yaml,wave-decisions.md,ci-cd-pipeline.md}`
    — precedent for library-side DEVOPS shape on a Kaleidoscope
    feature.

## Pre-wave decisions (carried in, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| D1 | `deployment_target` | N/A (library only; no deploy) | Andrea, pre-wave |
| D2 | `container_orchestration` | N/A | Andrea, pre-wave |
| D3 | `cicd_platform` | GitHub Actions (existing) | Andrea, pre-wave + ADR-0005 |
| D4 | `existing_infrastructure` | Yes, both (workspace + five-gate CI) | Andrea, pre-wave |
| D5 | `observability_and_logging` | None / N/A (the bridge IS observability infra) | Andrea, pre-wave |
| D6 | `deployment_strategy` | N/A | Andrea, pre-wave |
| D7 | `continuous_learning` | No (single-feature delivery, no A/B, no flags) | Andrea, pre-wave |
| D8 | `git_branching_strategy` | Trunk-Based Development | Andrea, pre-wave + memory `project_kaleidoscope_pure_trunk_based` |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md (declared; not modified by this wave) |

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] Do NOT graduate `self-observe` to Gates 2 and 3 in this feature

**Options considered**:

1. **Graduate now**: add `-p self-observe` to Gate 2 (`cargo
   public-api`) and Gate 3 (`cargo semver-checks`) in the same
   DISTILL commit that lands the source file. Mirror Beacon's D2/D3
   posture.
2. **Defer until self-observe stabilises**: leave Gate 2 and Gate 3
   scoped to {harness, spark, sieve, codex} as-is. Rely on ADR-0038
   §1 as the audit-trail for the public surface; rely on code review
   and Gate 1 for behavioural correctness.
3. **Graduate the whole crate (both bridges) plus a follow-up commit**:
   bring the existing `LumenToPulseRecorder` + `LumenToOtlpJsonWriter`
   surfaces under the same gate at the same time.

**Recommendation**: **Option 2 — defer**.

**Rationale**:
- The `self-observe` crate is not yet a published library and has no
  external consumers beyond the workspace itself. Per ADR-0005 Gates
  2/3 commentary, those gates exist to "lock public surface for
  downstream consumers" — at v0, the post-v0 CLI feature is the only
  downstream consumer and it is in-workspace (it will detect surface
  drift at compile time, not at CI semver-check time).
- The Aperture precedent (`.github/workflows/ci.yml` Gate 2/3
  comments at lines 302–308, 405–407) explicitly defers crates whose
  public surface is "dev-only seam, not consumer-facing API". The
  bridge family in `self-observe` has the same posture today.
- ADR-0038 §1 locks the public surface as an audit artefact. A
  surface change would require an ADR amendment and a peer review,
  not a silent CI bypass. The lock is in the ADR layer, not the CI
  layer, at this maturity stage.
- Graduating self-observe to Gates 2/3 would lock TWO new public
  surfaces (Lumen + the new Cinder bridge) without prior public-API
  baseline review of the Lumen surface. That is out-of-scope work
  for this feature.
- **Cost of deferral**: the audit-trail for `CinderToPulseRecorder`'s
  surface lives in ADR-0038 §1 + the explicit `pub use` line in
  `lib.rs`. A future change requires both an ADR amendment and a
  crafter-side `pub use` edit. The two-place change is the
  detection mechanism in the interim.

**Trade-off accepted**: a surface-drift commit that lands without
ADR amendment will not be caught by CI in this feature's window. It
will be caught at peer review (every commit to `main` is reviewed
under nWave) and at the point self-observe graduates to Gates 2/3
in a future "self-observe public API lock" feature.

**Forward-compatible posture**: when self-observe graduates (future
feature; expected when external consumers exist or when the bridge
family stabilises across all five planned bridges per `lib.rs:44-47`),
the graduation commit will pick up the current public surface as the
baseline. ADR-0038 §1's locked surface is the contract that the
baseline must match.

### [A2] Gate 1 (`cargo test --workspace`) inherits the new test file with ZERO workflow edit

**Options considered**:

1. **No workflow edit**: rely on `cargo test --workspace --all-targets
   --locked` naturally discovering the new
   `tests/cinder_to_pulse.rs` via the new `[[test]]` block in
   `crates/self-observe/Cargo.toml`.
2. **Explicit `-p self-observe --test cinder_to_pulse`**: add a
   dedicated step that re-runs the bridge tests in isolation, mirroring
   the harness's KPI 4 artefact-capture pattern.

**Recommendation**: **Option 1**.

**Rationale**:
- `cargo test --workspace --all-targets --locked` (line 182 of
  `.github/workflows/ci.yml`) already runs every `[[test]]` in every
  workspace member. The Lumen bridge's two test files
  (`lumen_to_pulse.rs`, `lumen_to_otlp_json.rs`) are picked up by
  this invocation today without per-test steps; the Cinder bridge
  follows the same pattern.
- The bridge tests do not produce a CI artefact (unlike the harness's
  KPI 4 verdict-counts). The pass/fail of the test is the KPI; no
  separate artefact step is warranted.
- Zero workflow YAML edit reduces the risk of an inadvertent
  regression to the existing five-gate contract — a key Andrea
  requirement (project memory: `project_kaleidoscope_pure_trunk_based`
  + the constraint "Do NOT add new CI workflow files").

**Trade-off accepted**: failure to compile the new test file (e.g.
typo in the `[[test]]` block) breaks Gate 1 for the whole workspace
rather than for one targeted job. This is correct fail-fast behaviour
— a malformed `Cargo.toml` should fail the workspace build, not
silently skip one test.

### [A3] Gate 5 (`cargo mutants`) — ADD a new parallel job `gate-5-mutants-self-observe`

**Options considered**:

1. **Add a new parallel job** scoped to the `self-observe` package,
   following the existing per-package shape (one job per crate, 30-
   minute timeout, `--in-diff` cascade against `origin/main → HEAD~1
   → full`). Mirror the Beacon D4 / Sieve / Codex / Spark pattern.
2. **Reuse an existing job**: extend `gate-5-mutants-harness` (the
   only non-`--in-diff` job today) to also mutate `self-observe`.
3. **Skip mutation testing on self-observe**: argue that the bridge
   is a "thin adapter" similar to `beacon-server` (excluded per
   Beacon D7) and the kill-rate signal is uninformative.
4. **Add the new job, scope it to the bridge file only** via
   `cargo mutants --file crates/self-observe/src/cinder_bridge.rs`
   (no `--in-diff` cascade).

**Recommendation**: **Option 1** (mirror Beacon D4 / Sieve / Codex
shape, `--in-diff` cascade).

**Rationale**:
- ADR-0005 Gate 5 + CLAUDE.md mandate per-feature mutation testing
  at 100% kill rate. DESIGN handoff explicitly scoped the run to
  `crates/self-observe/src/cinder_bridge.rs`. The `--in-diff`
  cascade naturally achieves this scoping: on a PR that touches only
  `cinder_bridge.rs`, the diff filter limits mutation to that file;
  on a push to main, the cascade compares against `HEAD~1`.
- Option 2 (extend `gate-5-mutants-harness`) would change the
  harness job's wall-clock without scoping benefit — the harness
  runs without `--in-diff` because the harness is small enough to
  always mutate fully; conflating two crates in one job loses that
  per-package fail-fast isolation.
- Option 3 (skip) is wrong: the bridge has real branchful logic
  (the `record_evaluate` cast `migrated as f64`, the `tier_attr`
  match, the BTreeMap construction) that mutation can probe. A
  surviving mutation would represent a real test-suite gap. Unlike
  `beacon-server`'s `tokio::main` orchestration shell, the bridge
  has assertable behaviour at the unit-test level.
- Option 4 (explicit `--file` flag without `--in-diff`) would run
  the full bridge mutation suite on every commit, even commits that
  do not touch `cinder_bridge.rs`. This wastes CI minutes for the
  ~95% of workspace commits unrelated to this feature.

**Decision A3 spec**:
- Job name: `gate-5-mutants-self-observe`
- Runs on: `ubuntu-latest`
- `needs: [gate-2-public-api, gate-3-semver]` (matches other Gate 5 jobs)
- `timeout-minutes: 30` (matches other Gate 5 jobs)
- Cache key namespace: `cargo-mutants-self-observe`
- Invocation: `cargo mutants --package self-observe --in-diff "$DIFF_FILE" --no-shuffle --jobs 2`
- `--in-diff` path filter: `crates/self-observe/**`
- Baseline cascade: `origin/main → HEAD~1 → full` (identical to
  beacon/aperture/spark/sieve/codex)
- Artefact upload: `mutants-out-self-observe`, retention 30 days

**Trade-off accepted**: the new job adds one parallel runner to the
existing five Gate 5 jobs (harness, aperture, spark, sieve, codex).
Total Gate 5 fan-out becomes six. Each job is independent and runs
in parallel; the critical-path wall-clock is bounded by the slowest
single job (typically harness's full-mutation run at ~5–10 min,
unchanged by this addition).

### [A4] No new CI workflow files; no contract amendment

The DEVOPS-wave deliverable changes one existing file
(`.github/workflows/ci.yml`) by **adding** one job block, applied
in the DISTILL commit (per A3). Zero workflow files are created.
Zero existing gates are removed or modified. ADR-0005's five-gate
contract is inherited unchanged; no new contract is written.

This satisfies the brief's constraint: "Do NOT add new CI workflow
files — the existing GitHub Actions workflow at
`.github/workflows/ci.yml` carries all five gates already."

### [A5] No new external dependencies

The Cargo manifest delta (already locked by ADR-0038 §6) is:

```toml
# crates/self-observe/Cargo.toml
[dependencies]
# existing deps preserved (aegis, lumen, pulse, serde, serde_json)
cinder = { path = "../cinder", version = "0.1.0" }

[[test]]
name = "cinder_to_pulse"
path = "tests/cinder_to_pulse.rs"
```

The `cinder` crate is already a workspace member; this is an
in-workspace path dependency. Zero new external dependencies; zero
new entries in `deny.toml` required; zero impact on Gate 4
(`cargo deny check`).

### [A6] No new toolchain pin

The bridge inherits the workspace's stable Rust toolchain (per
`rust-toolchain.toml`) for build/test/mutation testing, and the
workflow's `NIGHTLY_PIN` (`nightly-2026-04-15` as of writing) for
the Gate 2/3 toolchain — neither of which are exercised on
self-observe in this feature per A1. Zero toolchain change.

### [A7] No infrastructure-integration document required

External integrations = NONE per DESIGN wave's `application-
architecture.md > External integrations` section. The bridge has no
network surface, no third-party API, no webhook, no OAuth, no
subprocess. The DEVOPS skill's `infrastructure-integration.md`
artefact is explicitly N/A; this is recorded for traceability.

### [A8] No observability/monitoring/alerting document required

Per pre-wave Decision D5: the bridge IS observability infrastructure
itself. There is no separate observability stack to design for this
feature; the bridge's outputs (Pulse points) ARE the observability
data downstream consumers would alert on, but the bridge does not
emit its own telemetry about itself (no "bridge p99 latency"
counter, no "bridge events dropped" gauge). The DEVOPS skill's
`observability-design.md` and `monitoring-alerting.md` artefacts are
explicitly N/A.

The CI gates ARE the alerting surface for this library-only feature:
a regression in the bridge fails Gate 1 (cargo test) or Gate 5
(cargo mutants) at the next commit. Operationally, this is the
correct surface for a no-deployment library.

### [A9] No branching-strategy document required

Per pre-wave Decision D8: Trunk-Based Development is the project-
wide default per memory `project_kaleidoscope_pure_trunk_based`. The
CI workflow already encodes TBD (`.github/workflows/ci.yml` lines
44–52, with concurrency-group cancellation on `${{ github.ref }}`).
No per-feature branching deviation; the DEVOPS skill's
`branching-strategy.md` artefact is explicitly N/A.

### [A10] No continuous-learning document required

Per pre-wave Decision D7: no A/B testing, no feature flags, no
multi-arm bandits. This is a single-feature library addition with
deterministic behaviour pinned by acceptance tests. The DEVOPS
skill's `continuous-learning.md` artefact is explicitly N/A.

### [A11] No platform-architecture document required

Per the brief's explicit guidance: Morgan's `application-
architecture.md` is sufficient. There is no platform infrastructure
to architect (no cloud resources, no container orchestration, no
service mesh). The DEVOPS skill's `platform-architecture.md`
artefact is explicitly N/A.

## Infrastructure summary

- **Deployment**: out of scope (library only; no deploy artefact).
- **CI**: GitHub Actions ubuntu-latest, ADR-0005's five gates
  inherited unchanged; ONE new parallel Gate 5 job
  (`gate-5-mutants-self-observe`) added.
- **Branching**: Trunk-Based Development (project default,
  unchanged).
- **Mutation testing**: per-feature, scoped to
  `crates/self-observe/src/cinder_bridge.rs` via `--in-diff`
  cascade. 100% kill rate per ADR-0005 Gate 5.
- **External integrations**: NONE (no contract tests apply).
- **Observability**: the bridge IS observability infrastructure; no
  separate stack.

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Add `crates/self-observe/src/cinder_bridge.rs` (panicking skeleton or `unimplemented!()` body) + `tests/cinder_to_pulse.rs` (RED scenarios from BDD feature + slice files) + the `[[test]]` block + the `cinder` path dep + the `mod cinder_bridge;` + the `pub use` line — all in one atomic commit per ADR-0038 §6 | Keeps `main` green and CI machinery consistent with source state. |
| At DISTILL | Add `gate-5-mutants-self-observe` job to `.github/workflows/ci.yml` mirroring the beacon/spark/sieve/codex shape (per A3) | Mutation testing covers the new source file from its first commit. |
| At DISTILL | DO NOT modify Gate 1's `cargo test --workspace` invocation | The bridge inherits Gate 1 with zero edit per A2. |
| At DISTILL | DO NOT add `-p self-observe` to Gate 2 or Gate 3 | A1 defers the graduation. |
| At each DELIVER slice | Turn the slice's mutants 100% killed before review approval | Per CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5. |
| Post-DELIVER (close) | No additional DEVOPS step required | Mutation testing is enforced by the CI gate added at DISTILL; outcome KPIs are measured by green tests (also gated by CI). |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced by this wave**:

| Artefact | Path |
|----------|------|
| Environment inventory (library-only) | `docs/feature/cinder-to-pulse-bridge-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/cinder-to-pulse-bridge-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation design | `docs/feature/cinder-to-pulse-bridge-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline addendum (per-gate mapping + new job spec) | `docs/feature/cinder-to-pulse-bridge-v0/devops/ci-cd-pipeline.md` |

**Deliverables explicitly NOT produced** (N/A per library-only +
pre-wave decisions; rationale per A7–A11):

| Skipped artefact | Reason |
|------------------|--------|
| `platform-architecture.md` | Morgan's `application-architecture.md` is sufficient; no platform infrastructure to architect (A11) |
| `observability-design.md` | The bridge IS observability infra; no separate stack (D5, A8) |
| `monitoring-alerting.md` | No runtime monitoring needed for a library; CI gates are the alerting surface (A8) |
| `infrastructure-integration.md` | No external integrations at runtime (A7) |
| `branching-strategy.md` | Trunk-based is project default; no per-feature deviation (D8, A9) |
| `continuous-learning.md` | No A/B testing, no feature flags (D7, A10) |

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt
of this wave's outputs (per brief).

**What DISTILL receives**:

- The mandatory environments.yaml for Mandate 4 (Environmental
  Realism). The `clean` environment is the only target; tests run
  in-process with no external dependency.
- The CI extension spec (one new Gate 5 job) that DISTILL must apply
  in the same atomic commit that creates `cinder_bridge.rs` +
  `tests/cinder_to_pulse.rs` + the Cargo manifest delta.
- The constraint that Gates 2 and 3 stay scoped to {harness, spark,
  sieve, codex} for this feature (A1) — DISTILL must not add
  `self-observe` to those gates.
- The constraint that Gate 1 takes no edit (A2) — the new test file
  is auto-discovered via the new `[[test]]` block.
- The per-KPI instrumentation mapping (in
  `kpi-instrumentation.md`): which acceptance test gates which KPI.
