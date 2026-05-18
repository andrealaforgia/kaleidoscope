# Wave Decisions — `cinder-to-otlp-json-bridge-v0` / DEVOPS

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
2. `docs/feature/cinder-to-otlp-json-bridge-v0/discuss/outcome-kpis.md`
   — OK1/OK2/OK3 (leading, library-contract) + OK4 (Cinder behaviour
   guardrail) + OK5 (NDJSON-validity guardrail).
3. `docs/feature/cinder-to-otlp-json-bridge-v0/design/wave-decisions.md`
   — DESIGN-wave decisions DD1–DD5 + the explicit DEVOPS handoff
   block (one new source file `crates/self-observe/src/cinder_otlp_json.rs`,
   one new test file `crates/self-observe/tests/cinder_to_otlp_json.rs`,
   one `Cargo.toml` `[[test]]` block, one `lib.rs` mod+pub-use line,
   one ADR (already shipped as ADR-0039); the `cinder` dep already
   added by the Pulse-sink sibling).
4. `docs/feature/cinder-to-otlp-json-bridge-v0/design/application-architecture.md`
   — C4-equivalent walkthrough, external-integrations = none,
   no-substrate adapter posture, Earned-Trust layers spec.
5. `docs/product/architecture/adr-0039-cinder-to-otlp-json-bridge-public-api-and-crate-layout.md`
   — locks public surface (§1), per-event emission contract (§2),
   acceptance-test seam (§3), file location (§4), recommended internal
   structure (§5), and Cargo manifest additions (§6).
6. `docs/product/architecture/adr-0005-ci-contract.md` — the five-
   gate CI contract that this wave inherits unchanged.
7. `docs/product/architecture/brief.md > ## Application Architecture
   — cinder-to-otlp-json-bridge-v0` (lines 488–707) — reuse-of-
   platform-level-decisions block confirms CI contract inheritance +
   per-feature MT scope.
8. `.github/workflows/ci.yml` — the existing five-gate workflow
   (1116 lines as of 2026-05-18). Gate 5 currently has five per-
   package jobs (harness, aperture, spark, sieve, codex);
   `gate-5-mutants-self-observe` is specified by the Pulse-sink
   sibling feature's DEVOPS (`cinder-to-pulse-bridge-v0`'s A3) and
   lands in that feature's DISTILL commit. **By the time this
   feature's DISTILL commit lands, the `gate-5-mutants-self-observe`
   job already exists; this feature requires ZERO further CI
   workflow edits.** See A3 below.
9. `scripts/hooks/pre-push` — Gates 2 and 3 mirror locally for the
   four currently-graduated packages. Not modified by this feature.
10. `crates/self-observe/Cargo.toml` — current dependency list (aegis,
    cinder, lumen, pulse, serde, serde_json) — the `cinder` line was
    added by the Pulse-sink sibling. Current `[[test]]` blocks:
    lumen_to_pulse, lumen_to_otlp_json, cinder_to_pulse (the third
    was added by the Pulse-sink sibling).
11. `docs/feature/cinder-to-pulse-bridge-v0/devops/{wave-decisions.md, environments.yaml, kpi-instrumentation.md, ci-cd-pipeline.md}`
    — the immediate precedent. The OTLP-JSON sibling inherits every
    A-decision identically except for the three named differences
    enumerated under "Differences from the Pulse-sink sibling" below.

## Pre-wave decisions (carried in, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| D1 | `deployment_target` | N/A (library only; no deploy) | Andrea, pre-wave |
| D2 | `container_orchestration` | N/A | Andrea, pre-wave |
| D3 | `cicd_platform` | GitHub Actions (existing) | Andrea, pre-wave + ADR-0005 |
| D4 | `existing_infrastructure` | Yes, both (workspace + five-gate CI) | Andrea, pre-wave |
| D5 | `observability_and_logging` | None / N/A (the writer IS observability infra) | Andrea, pre-wave |
| D6 | `deployment_strategy` | N/A | Andrea, pre-wave |
| D7 | `continuous_learning` | No (single-feature delivery, no A/B, no flags) | Andrea, pre-wave |
| D8 | `git_branching_strategy` | Trunk-Based Development | Andrea, pre-wave + memory `project_kaleidoscope_pure_trunk_based` |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill rate per ADR-0005 Gate 5 | CLAUDE.md (declared; not modified by this wave) |

## Differences from the Pulse-sink sibling cinder-to-pulse-bridge-v0

This DEVOPS wave is a near-clone of the Pulse-sink sibling's DEVOPS
wave. The same nine pre-wave decisions; the same posture on
graduation, gate inheritance, and skipped artefacts; the same
slim-output shape. Three differences only:

1. **New source file path**: `crates/self-observe/src/cinder_otlp_json.rs`
   (NOT `cinder_bridge.rs`, which is the Pulse-sink sibling's file
   and already shipped).
2. **New test file path**: `crates/self-observe/tests/cinder_to_otlp_json.rs`
   (NOT `cinder_to_pulse.rs`, already shipped).
3. **CI workflow edit count is ZERO** (the Pulse-sink sibling adds
   one `gate-5-mutants-self-observe` job; this feature inherits that
   job — its `--in-diff` path filter `crates/self-observe/**`
   matches the new file automatically). See A3 below.

The KPI set also has one extra row relative to the Pulse-sink
sibling — OK5 (NDJSON-validity guardrail) — because the OTLP-JSON
sink is a byte stream where validity at the line boundary is a
distinct contract beyond per-event correctness. See
`kpi-instrumentation.md` for the per-KPI mapping.

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] Do NOT graduate `self-observe` to Gates 2 and 3 in this feature

**Options considered**:

1. **Graduate now**: add `-p self-observe` to Gate 2 (`cargo
   public-api`) and Gate 3 (`cargo semver-checks`) in the same
   DISTILL commit that lands the source file. Lock the writer's
   public surface (ADR-0039 §1) plus the already-shipped Lumen and
   Cinder bridges' surfaces.
2. **Defer until self-observe stabilises**: leave Gate 2 and Gate 3
   scoped to {harness, spark, sieve, codex} as-is. Rely on ADR-0039
   §1 as the audit-trail for the new public surface; rely on code
   review and Gate 1 for behavioural correctness.
3. **Graduate the whole crate (all four writers) plus a follow-up
   commit**: bring `LumenToPulseRecorder`, `LumenToOtlpJsonWriter`,
   `CinderToPulseRecorder`, and `CinderToOtlpJsonWriter` under the
   same gate at the same time.

**Recommendation**: **Option 2 — defer**.

**Rationale**:
- Identical rationale to the Pulse-sink sibling's A1. The
  `self-observe` crate is not yet a published library and has no
  external consumers beyond the workspace itself. Per ADR-0005 Gates
  2/3 commentary, those gates exist to "lock public surface for
  downstream consumers" — at v0, the post-v0 CLI feature is the only
  downstream consumer and it is in-workspace (it will detect surface
  drift at compile time, not at CI semver-check time).
- The Aperture precedent (`.github/workflows/ci.yml` Gate 2/3
  comments at lines 302–308, 405–407) explicitly defers crates whose
  public surface is "dev-only seam, not consumer-facing API". The
  writer family in `self-observe` has the same posture today.
- ADR-0039 §1 locks the public surface as an audit artefact. A
  surface change would require an ADR amendment and a peer review,
  not a silent CI bypass. The lock is in the ADR layer, not the CI
  layer, at this maturity stage.
- Graduating self-observe to Gates 2/3 would lock FOUR public
  surfaces (the two Lumen writers + the two Cinder writers) without
  prior public-API baseline review of the Lumen surfaces. That is
  out-of-scope work for this feature.
- **Cost of deferral**: the audit-trail for `CinderToOtlpJsonWriter`'s
  surface lives in ADR-0039 §1 + the explicit `pub use` line in
  `lib.rs`. A future change requires both an ADR amendment and a
  crafter-side `pub use` edit. The two-place change is the
  detection mechanism in the interim.

**Trade-off accepted**: a surface-drift commit that lands without
ADR amendment will not be caught by CI in this feature's window. It
will be caught at peer review (every commit to `main` is reviewed
under nWave) and at the point self-observe graduates to Gates 2/3
in a future "self-observe public API lock" feature.

**Forward-compatible posture**: when self-observe graduates (future
feature; expected when external consumers exist or when the writer
family stabilises across all five planned source bridges per
`lib.rs:44-47`), the graduation commit will pick up the current
public surface — INCLUDING `CinderToOtlpJsonWriter` — as the
baseline. ADR-0039 §1's locked surface is the contract that the
baseline must match.

### [A2] Gate 1 (`cargo test --workspace`) inherits the new test file with ZERO workflow edit

**Options considered**:

1. **No workflow edit**: rely on `cargo test --workspace --all-targets
   --locked` naturally discovering the new
   `tests/cinder_to_otlp_json.rs` via the new `[[test]]` block in
   `crates/self-observe/Cargo.toml`.
2. **Explicit `-p self-observe --test cinder_to_otlp_json`**: add a
   dedicated step that re-runs the writer tests in isolation,
   mirroring the harness's KPI 4 artefact-capture pattern.

**Recommendation**: **Option 1** (identical to the Pulse-sink
sibling's A2).

**Rationale**:
- `cargo test --workspace --all-targets --locked` (line 182 of
  `.github/workflows/ci.yml`) already runs every `[[test]]` in every
  workspace member. The three previously-shipped writer test files
  (`lumen_to_pulse.rs`, `lumen_to_otlp_json.rs`, `cinder_to_pulse.rs`)
  are picked up by this invocation today without per-test steps; the
  Cinder OTLP-JSON writer follows the same pattern.
- The writer tests do not produce a CI artefact (unlike the harness's
  KPI 4 verdict-counts). The pass/fail of the test IS the KPI; no
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

### [A3] Gate 5 — inherit `gate-5-mutants-self-observe` from the Pulse-sink sibling; no new job

**Options considered**:

1. **Inherit unchanged**: the Pulse-sink sibling's DISTILL commit
   (per `cinder-to-pulse-bridge-v0`'s A3) adds the
   `gate-5-mutants-self-observe` parallel job with `--in-diff` path
   filter `crates/self-observe/**`. By the time this feature's
   DISTILL commit lands, that job already exists; the new file
   `crates/self-observe/src/cinder_otlp_json.rs` is matched by the
   existing path filter automatically. ZERO further CI workflow
   edits.
2. **Add a second per-file job** (`gate-5-mutants-self-observe-cinder-otlp-json`)
   scoped to the new file only.
3. **Replace the inherited single job with one job per writer**
   (`gate-5-mutants-self-observe-lumen-bridge`,
   `gate-5-mutants-self-observe-lumen-otlp-json`,
   `gate-5-mutants-self-observe-cinder-bridge`,
   `gate-5-mutants-self-observe-cinder-otlp-json`).
4. **Skip mutation testing on the OTLP-JSON writer**: argue that
   the duplicated serde structs and the thin `emit` helper are
   "trivial" enough that mutation testing is uninformative.

**Recommendation**: **Option 1** — inherit unchanged.

**Rationale**:
- The Pulse-sink sibling's A3 already established the
  `gate-5-mutants-self-observe` job with `--in-diff` path filter
  `crates/self-observe/**` and `--package self-observe`. The
  `--in-diff` cascade naturally limits the run to whichever files
  the commit actually touched: on a commit that touches only
  `cinder_otlp_json.rs`, mutation runs against that file only; on a
  commit that touches both `cinder_bridge.rs` and
  `cinder_otlp_json.rs` (the Pulse-sink sibling's DISTILL slice and
  this feature's DISTILL slice respectively, ordered in time), each
  file is mutated only on the commit that introduces it.
- Option 2 (per-file second job) would double the Gate 5 fan-out
  cost on self-observe-touching commits without scoping benefit
  (the per-file `cargo mutants --file` flag scopes more precisely
  than `--package` but the `--in-diff` cascade already provides
  per-commit precision).
- Option 3 (per-writer fan-out) is premature optimisation: when the
  writer count reaches ~6-8 (Sluice/Augur/Ray/Strata writers + their
  OTLP-JSON variants), per-writer parallelism may earn the
  CI-minute trade. At N=4 writers, one shared job with `--in-diff`
  is the correct granularity.
- Option 4 (skip) is wrong: the writer has real branchful logic
  (the `tier_lowercase` match, the per-event attribute construction
  in the three `record_*` methods, the `migrated.to_string()` in
  `record_evaluate`, the `Mutex<W>::lock` + best-effort triple, the
  `serde_json::to_string` result handling) that mutation can probe.
  A surviving mutation would represent a real test-suite gap. The
  100% kill rate per CLAUDE.md applies to every new source file,
  including this one.

**Decision A3 spec**:
- Job name: `gate-5-mutants-self-observe` (PRE-EXISTING from the
  Pulse-sink sibling — no new job in this feature)
- DESIGN-scoped mutation target for this feature: the new file
  `crates/self-observe/src/cinder_otlp_json.rs` (matched by the
  pre-existing `--in-diff` path filter `crates/self-observe/**`).
- No edit to the job specification; no edit to the path filter; no
  edit to the cache key namespace.
- DELIVER wave responsibility: turn every mutant on
  `cinder_otlp_json.rs` 100% killed before review approval (per
  CLAUDE.md per-feature MT strategy).

**Trade-off accepted**: identical to the Pulse-sink sibling's A3 —
the `gate-5-mutants-self-observe` job's wall-clock grows slowly as
more files land in `crates/self-observe/src/`. The `--in-diff`
cascade keeps the per-commit wall-clock bounded; the full-mutation
fallback path (no baseline available) grows with the file count but
runs only on first-commit-on-branch scenarios.

### [A4] No new CI workflow files; no contract amendment

The DEVOPS-wave deliverable for THIS feature changes ZERO files in
`.github/workflows/`. The Pulse-sink sibling's DISTILL commit lands
the only Gate 5 spec change required for the OTLP-JSON sibling to
inherit. ADR-0005's five-gate contract is inherited unchanged; no
new contract is written.

This satisfies the brief's constraint: "Do NOT add new CI workflow
files — the existing GitHub Actions workflow at
`.github/workflows/ci.yml` carries all five gates already." It also
exceeds it: this feature's DISTILL commit makes ZERO edits to that
file (whereas the Pulse-sink sibling's DISTILL adds one job block).

### [A5] No new external dependencies

The Cargo manifest delta (already locked by ADR-0039 §6) is:

```toml
# crates/self-observe/Cargo.toml
[dependencies]
# existing deps preserved (aegis, cinder, lumen, pulse, serde, serde_json)
# NO new [dependencies] line — the cinder = { path = "../cinder", version = "0.1.0" }
# line was added by the Pulse-sink sibling cinder-to-pulse-bridge-v0.

[[test]]
name = "cinder_to_otlp_json"
path = "tests/cinder_to_otlp_json.rs"
```

The `serde` and `serde_json` dependencies are already present (used
by the Lumen OTLP-JSON writer at v0 and reaffirmed by the Pulse-sink
sibling — though the Pulse-sink sibling does not itself emit JSON,
the Cinder OTLP-JSON writer here does). Zero new external
dependencies; zero new entries in `deny.toml` required; zero impact
on Gate 4 (`cargo deny check`).

### [A6] No new toolchain pin

The writer inherits the workspace's stable Rust toolchain (per
`rust-toolchain.toml`) for build/test/mutation testing, and the
workflow's `NIGHTLY_PIN` (`nightly-2026-04-15` as of writing) for
the Gate 2/3 toolchain — neither of which are exercised on
self-observe in this feature per A1. Zero toolchain change.

### [A7] No infrastructure-integration document required

External integrations = NONE per DESIGN wave's `application-
architecture.md > External integrations` section (lines 340–354).
The writer has no network surface, no third-party API, no webhook,
no OAuth, no subprocess. The downstream OTLP/HTTP collector that
Priya's sidecar forwards to IS an external integration, but it is at
the operator's deployment boundary, not at this library's boundary.
The DEVOPS skill's `infrastructure-integration.md` artefact is
explicitly N/A; this is recorded for traceability.

### [A8] No observability/monitoring/alerting document required

Per pre-wave Decision D5: the writer IS observability infrastructure
itself. There is no separate observability stack to design for this
feature; the writer's outputs (OTLP-JSON ResourceMetrics NDJSON
lines) ARE the observability data downstream consumers would alert
on, but the writer does not emit its own telemetry about itself (no
"writer p99 latency" counter, no "writer events dropped" gauge — the
best-effort emission posture per DISCUSS D5 explicitly accepts silent
drops on Mutex poisoning, serialisation failure, or write failure).
The DEVOPS skill's `observability-design.md` and `monitoring-
alerting.md` artefacts are explicitly N/A.

The CI gates ARE the alerting surface for this library-only feature:
a regression in the writer fails Gate 1 (cargo test) or Gate 5
(cargo mutants on self-observe) at the next commit. Operationally,
this is the correct surface for a no-deployment library.

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
  inherited UNCHANGED; ZERO CI workflow edits in this feature's
  DISTILL commit (the `gate-5-mutants-self-observe` job already
  exists from the Pulse-sink sibling, and its `--in-diff` path
  filter `crates/self-observe/**` matches the new file
  automatically).
- **Branching**: Trunk-Based Development (project default,
  unchanged).
- **Mutation testing**: per-feature, scoped to
  `crates/self-observe/src/cinder_otlp_json.rs` via the inherited
  `--in-diff` cascade. 100% kill rate per ADR-0005 Gate 5.
- **External integrations**: NONE (no contract tests apply).
- **Observability**: the writer IS observability infrastructure; no
  separate stack.

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Add `crates/self-observe/src/cinder_otlp_json.rs` (panicking skeleton or empty no-op body per DESIGN DD4) + `tests/cinder_to_otlp_json.rs` (RED scenarios from BDD feature + slice files) + the `[[test]]` block + the `mod cinder_otlp_json;` + the `pub use` line — all in one atomic commit per ADR-0039 §6 | Keeps `main` green and CI machinery consistent with source state. |
| At DISTILL | DO NOT edit `.github/workflows/ci.yml` | A3: the `gate-5-mutants-self-observe` job already exists from the Pulse-sink sibling's DISTILL commit; its `--in-diff` path filter matches the new file automatically. ZERO CI workflow edits in this feature. |
| At DISTILL | DO NOT modify Gate 1's `cargo test --workspace` invocation | The writer inherits Gate 1 with zero edit per A2. |
| At DISTILL | DO NOT add `-p self-observe` to Gate 2 or Gate 3 | A1 defers the graduation (mirrors the Pulse-sink sibling's A1). |
| At each DELIVER slice | Turn the slice's mutants 100% killed before review approval | Per CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5. The DESIGN-scoped mutation target is `crates/self-observe/src/cinder_otlp_json.rs`. |
| Post-DELIVER (close) | No additional DEVOPS step required | Mutation testing is enforced by the (pre-existing) CI gate; outcome KPIs are measured by green tests (also gated by CI). The narrative.md + slides.md update happens per the project memory `feedback_narrative_per_wave_closure`. |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced by this wave**:

| Artefact | Path |
|----------|------|
| Environment inventory (library-only) | `docs/feature/cinder-to-otlp-json-bridge-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/cinder-to-otlp-json-bridge-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation design | `docs/feature/cinder-to-otlp-json-bridge-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline addendum (per-gate mapping + zero-edit confirmation) | `docs/feature/cinder-to-otlp-json-bridge-v0/devops/ci-cd-pipeline.md` |

**Deliverables explicitly NOT produced** (N/A per library-only +
pre-wave decisions; rationale per A7–A11):

| Skipped artefact | Reason |
|------------------|--------|
| `platform-architecture.md` | Morgan's `application-architecture.md` is sufficient; no platform infrastructure to architect (A11) |
| `observability-design.md` | The writer IS observability infra; no separate stack (D5, A8) |
| `monitoring-alerting.md` | No runtime monitoring needed for a library; CI gates are the alerting surface (A8) |
| `infrastructure-integration.md` | No external integrations at runtime; the downstream OTLP/HTTP collector is at the operator's boundary, not the library's (A7) |
| `branching-strategy.md` | Trunk-based is project default; no per-feature deviation (D8, A9) |
| `continuous-learning.md` | No A/B testing, no feature flags (D7, A10) |

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt
of this wave's outputs (per brief).

**What DISTILL receives**:

- The mandatory environments.yaml for Mandate 4 (Environmental
  Realism). The `clean` environment is the only target; tests run
  in-process with no external dependency.
- The constraint that NO CI workflow edits are needed (A3) — the
  `gate-5-mutants-self-observe` job already exists from the
  Pulse-sink sibling's DISTILL commit, and its `--in-diff` path
  filter `crates/self-observe/**` matches the new file.
- The constraint that Gates 2 and 3 stay scoped to {harness, spark,
  sieve, codex} for this feature (A1) — DISTILL must not add
  `self-observe` to those gates.
- The constraint that Gate 1 takes no edit (A2) — the new test file
  is auto-discovered via the new `[[test]]` block.
- The per-KPI instrumentation mapping (in
  `kpi-instrumentation.md`): which acceptance test gates which KPI,
  including the OK5 NDJSON-validity guardrail (new relative to the
  Pulse-sink sibling).

---

## Forward-compatibility notes (added during Forge's peer review)

These notes do not alter A1–A11 but record handoff conditions that
Forge identified as worth flagging explicitly so downstream features
do not silently miss them.

### Pre-push hook graduation (cross-feature handoff)

A1 defers `self-observe` from Gates 2/3 (`cargo public-api`,
`cargo semver-checks`). When that graduation lands in a future
feature, that feature's DEVOPS wave MUST also update
`scripts/hooks/pre-push` to include `self-observe` in the per-package
loop (currently `[otlp-conformance-harness, spark, sieve, codex]`).
The graduation feature's DEVOPS wave is responsible for keeping the
pre-push hook in sync with the CI workflow's Gates 2/3 scope.

### Mutation kill-rate measurement protocol (DELIVER-wave clarification)

A3 inherits the sibling Pulse-sink's `gate-5-mutants-self-observe`
job (now landed via post-merge correction commit b5fa550) and
CLAUDE.md's per-feature 100% kill-rate gate. The measurement
protocol for the DELIVER crafter:

1. After Slice N's tests turn GREEN, run
   `cargo mutants --package self-observe --in-diff <(git diff origin/main HEAD -- crates/self-observe/src/cinder_otlp_json.rs)`
   locally (mirrors the CI job invocation).
2. Inspect `mutants.out/summary.txt`. The "undetected" count MUST
   be zero.
3. If any mutation survives: strengthen the relevant test, adjust
   the implementation, or escalate to the review with explicit
   justification.
4. The CI-layer measurement is the `gate-5-mutants-self-observe`
   job on the merge commit; it serves as the authoritative oracle.

Same protocol the Pulse-sink sibling followed (commit 4d20c31
achieved 6/6 = 100% kill rate).
