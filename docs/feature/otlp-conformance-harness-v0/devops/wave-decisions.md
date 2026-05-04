# Wave Decisions — `otlp-conformance-harness-v0` (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-03.
> **Author**: Apex.
> **Companion documents**: `platform-architecture.md`, `ci-cd-pipeline.md`,
> `branching-strategy.md`, `kpi-instrumentation.md`, `environments.yaml`.
> **Workflow file**: `.github/workflows/ci.yml`.
> **Toolchain pin**: `rust-toolchain.toml` (repository root).

---

## Mode

This was an **execute-and-resolve** wave, not a propose wave. Andrea
resolved the nine decision-points the skill template normally walks
the architect through (D1–D9, see "Andrea's pre-resolved decisions"
below) before invoking the wave. Apex's job was to execute on those
resolutions and to answer the six explicit open questions Crafty
handed up from the DELIVER wave.

The wave produced no new architectural ground; every load-bearing
choice traces to either Andrea's pre-resolution, ADR-0005, the
implementation roadmap's Section A, the DISCUSS wave's
constraints, or the harness's library-not-service shape. This is
appropriate for a DEVOPS wave that wires a small, well-specified
library into a CI runner.

---

## Andrea's pre-resolved decisions

| # | Topic | Resolution | Source |
|---|---|---|---|
| D1 | Deployment target | N/A — pure-function Rust library, distribution via GitHub repository (and crates.io eventually). | DISCUSS D1 (library not service); roadmap A.2. |
| D2 | Container orchestration | None. | Library, not service. |
| D3 | CI/CD platform | **GitHub Actions.** | Repository lives on GitHub; ADR-0005 defers runner choice to this wave; Roadmap FOSS-replacement table excludes GitHub Actions only as a *bundled* dependency, not for the project's own CI. |
| D4 | Existing infrastructure | Greenfield. No prior workflows or pipelines exist. | Verified: no `.github/` directory before this wave. |
| D5 | Observability and logging | None. The harness emits no telemetry by design. CI logs exist by virtue of running; no aggregation set up. | Roadmap Section A.2 (no-telemetry-on-telemetry); DISCUSS US System Constraint 4. |
| D6 | Deployment strategy | N/A. | Library, not service. |
| D7 | Continuous learning | No. | Outside this wave's scope. |
| D8 | Git branching strategy | **Trunk-Based Development.** | Project rule. DELIVER already operated this way (eight commits direct to `main`, no feature branches, Conventional Commits). |
| D9 | Mutation testing strategy | **Per-feature**, 100 % kill-rate gate. | ADR-0005 Gate 5. DELIVER reached 100 % (33/33 viable mutants caught). Codified in `CLAUDE.md` `## Mutation Testing Strategy`. |

---

## Resolution of Crafty's six open questions

### Q1 — CI runner choice

**Resolved by Andrea before the wave**: GitHub Actions.

Apex's contribution: the runner-specific YAML at
`.github/workflows/ci.yml`, with each of the five ADR-0005 gates
mapped to a job, fail-fast ordering, two cache namespaces (stable and
nightly), and SHA-pinned third-party actions.

### Q2 — Toolchain provisioning for Gates 2 and 3

**Decision**: Install nightly Rust via `dtolnay/rust-toolchain@master`
with an explicit `toolchain:` input set to a pinned date stored in the
`NIGHTLY_PIN` env var (currently `nightly-2026-04-15`). The pin is at
the workflow level, not at a `rust-toolchain.toml` level, because
nightly is needed only by Gates 2 and 3, not by day-to-day
development.

**Rationale**: pinning a specific nightly date keeps `cargo public-api`
output reproducible across runs (its diff format is sensitive to
nightly compiler internals) and insulates the project from a nightly
compiler regression breaking the pipeline overnight. The cost of the
pin is a one-line edit when bumping; the benefit is a deterministic
gate.

**Alternatives considered**:

- (A) `dtolnay/rust-toolchain@nightly` (latest). Rejected: non-reproducible.
- (B) Pinned nightly date via env var (recommended, accepted).
- (C) Run Gates 2 and 3 only on tags. Rejected: tags do not exist in v0
  (`publish = false`); waiting until v0.1 to start enforcing the
  surface-lock contract would let surface drift accumulate
  un-reviewed for the lifetime of v0.

### Q3 — `rust-toolchain.toml` policy

**Decision**: Ship `rust-toolchain.toml` at the repository root pinning
the stable channel to `1.78` (matching the workspace MSRV). Ship the
file in this wave.

**Rationale**: pinning at the repo root guarantees reproducibility for
contributors (every `cargo build` and `cargo test` runs on the same
compiler as the CI's stable jobs) and closes the gap that would
otherwise exist if MSRV were policed only via a CI matrix. This
trades a small loss of flexibility (a contributor on a newer
toolchain cannot accidentally use a newer feature) for a worthwhile
gain in local-vs-CI symmetry. Trunk-Based Development relies on local
CI symmetry being high.

**Alternatives considered**:

- (A) Pin via `rust-toolchain.toml` (recommended, accepted).
- (B) MSRV via CI matrix only. Rejected: opens MSRV drift; a contributor
  can land code that breaks 1.78 if the CI matrix only tests stable
  latest.

The file ships with `components = ["rustfmt", "clippy"]` and
`profile = "minimal"` so the contributor's first `rustup` invocation
fetches what is needed and nothing more.

### Q4 — Mutation-test budget for CI

**Decision**: Start with full `cargo mutants` per push. Switch to
`--in-diff origin/main` when a single feature's full mutation run
exceeds **60 seconds wall-clock** for two consecutive merges to
`main`.

**Rationale**: the 60-second threshold sits well above the v0 baseline
(45 s, observed by Crafty in DELIVER) and well below "developer
experience starts to suffer" territory. The "two consecutive merges"
clause prevents flapping (a single anomalous run does not flip the
switch). The PR that flips the switch must include a manual full-run
local check showing the kill rate is still 100 % at the time of the
switch, attached as a CI artefact.

**Alternatives considered**:

- (A) Full run unconditionally. Rejected: scales badly as the corpus
  grows; the threshold provides a graceful escalation.
- (B) `--in-diff` from the start. Rejected: weaker signal for v0; the
  full run is fast.
- (C) `--in-diff` plus a nightly full-run job. Rejected as
  unnecessary in v0; nothing in DISCUSS or DESIGN demands a nightly
  full-run, and the maintainer can run `cargo mutants` locally in 45 s.

The threshold and switch-over rule are documented in
`ci-cd-pipeline.md > Mutation-test budget rule`. The workflow's
`gate-5-mutants` job carries an explicit `timeout-minutes: 30` upper
bound as a runaway-safety net.

### Q5 — Verdict-counts artefact

**Decision**: Capture the corpus's per-signal and per-rule verdict
counts as a CI artefact named `verdict-counts`, in JSON
(schema_version 1), with **90-day retention**. Produced by a small
Python step in the `gate-1-test` job that walks
`crates/otlp-conformance-harness/tests/vectors/`, parses the
`*.expected.json` descriptors, and writes
`target/kpi/verdict-counts.json`.

**Rationale**: 90 days covers a full quarterly review window without
overspending GitHub artefact storage. The schema is intentionally
self-describing (`schema_version` field) so a future iteration that
adds, e.g., per-vector latency samples can extend without breaking
existing readers. The artefact is uploaded with
`if: success() || failure()` so a failed CI run still leaves a
forensic trail.

**Schema (v1)**: see `kpi-instrumentation.md > KPI 4`.

**Alternatives considered**:

- (A) JSON artefact with 90-day retention (recommended, accepted).
- (B) JSON artefact with 365-day retention. Rejected: storage cost grows
  linearly with retention; the marginal value of 9 extra months of
  history is low because KPI 1 is binary per commit and a quarterly
  sweep covers any drift.
- (C) Inline the counts in the workflow's job summary
  (`$GITHUB_STEP_SUMMARY`) instead of an artefact. Rejected: job
  summaries are not durably retrievable via the API the way artefacts
  are; KPI 4 reporting wants a stable shape.

### Q6 — Upstream feature-gate issue at `opentelemetry-proto`

**Decision**: File a courtesy issue at the upstream OpenTelemetry Rust
project requesting a `messages-only` feature split. **Non-blocking on
this wave's completion.** Apex provides the issue body as a stub at
`upstream-issue-opentelemetry-proto-feature-split.md` (this directory)
for Andrea (or a delegated agent) to paste into the upstream tracker
when convenient.

**Rationale**: the build-graph constraint Crafty observed (the `logs`
/ `trace` / `metrics` features pull `opentelemetry_sdk` transitively)
is real and impacts every Rust consumer of `opentelemetry-proto` who
wants only the prost-generated message types. Filing the issue is a
courtesy to the upstream community and a small contribution to the
ecosystem; resolving it is on upstream's timeline, not the harness's.

The harness's licence (CC0-1.0) is more permissive than upstream's
(Apache-2.0), so any patch the project chooses to write later can be
contributed back without a CLA conflict, should the maintainers be
willing to accept one. That is a future option, not a current
commitment.

The issue stub follows; no further action this wave.

---

## Decisions made by Apex (not pre-resolved by Andrea or Crafty)

### A1 — Workflow trigger surface

**Decision**: `on: push: branches: [main]` and
`on: pull_request: branches: [main]`. No path filters in v0.

**Rationale**: aligns with Trunk-Based Development. Path filters would
let a `docs/`-only commit skip CI, but the gates are fast (a few
minutes wall-clock) and the cost of running them on a docs change is
negligible compared with the cost of accidentally landing a code
change masked as a docs change. Adding path filters is reversible if
CI minutes become a concern.

### A2 — `concurrency: cancel-in-progress: true`

**Decision**: Set `concurrency.group: ${{ github.workflow }}-${{ github.ref }}`
with `cancel-in-progress: true`.

**Rationale**: a contributor pushing two commits in quick succession
should not double-burn CI minutes. The previous in-flight run is
cancelled when a newer commit lands on the same branch / PR.
Standard practice; no project-specific consideration.

### A3 — `permissions: read`

**Decision**: Workflow-level `permissions: contents: read`. No job
opts in to write permissions.

**Rationale**: the workflow does not push, tag, or comment on PRs.
Read is sufficient. Setting it explicitly is supply-chain hygiene
(the default `GITHUB_TOKEN` permissions vary across repository
settings; explicit is safer than implicit).

### A4 — Action version pinning

**Decision**: First-party actions (`actions/checkout`,
`actions/cache`, `actions/upload-artifact`) pinned to major version
tags (`@v4`). Third-party actions
(`EmbarkStudios/cargo-deny-action`,
`obi1kenobi/cargo-semver-checks-action`,
`dtolnay/rust-toolchain`) pinned to commit SHAs where reasonable.

**Rationale**: GitHub maintains the `@v4`-style tags on its own
actions and re-tags them only for security fixes; SHA-pinning
first-party actions provides marginal hygiene gain at significant
maintenance cost. Third-party actions do not have the same trust
posture, so SHA pinning is appropriate.

`dtolnay/rust-toolchain@stable` and `dtolnay/rust-toolchain@master`
are pinned by branch / tag (not SHA) because the action's stability
posture is well-known and the action itself is small enough to audit
inline; a SHA pin would force a manual bump every time `dtolnay`
ships a patch, which is high-frequency for that action.

### A5 — Single OS in CI for v0

**Decision**: `ubuntu-latest` only. No matrix.

**Rationale**: the harness is pure Rust, no platform-specific code.
Adding `macos-latest` and `windows-latest` to the matrix would triple
CI minutes for no observable contract benefit in v0. Adding them is
reversible if a platform-specific regression appears (which has not).

### A6 — Local quality gates: recommendation only

**Decision**: Do not introduce a `lefthook` / `pre-commit` / `husky`
configuration. Recommend the equivalent commands in
`CONTRIBUTING.md` instead (when next updated).

**Rationale**: the harness's full local test suite runs in seconds
(`cargo test` plus `cargo deny check`), so contributors' natural
reflex to run them before pushing is sufficient. A pre-commit
framework adds setup friction (every contributor needs to install
the framework and its language runtime) and a hard-to-debug class of
mismatches between local hooks and CI. The "mirror, not duplicate"
principle from the `cicd-and-deployment` skill is honoured by
recommending the same shell commands rather than wiring a separate
hook framework.

If a future feature introduces slow local checks that contributors
forget, this decision is reversible.

---

## Risk register

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| The pinned nightly (`NIGHTLY_PIN`) goes stale and Gate 2 / Gate 3 break against newer crates.io tooling | Medium | Low | Quarterly review bumps the pin; CI failure clearly identifies which gate broke and the fix is a one-line PR. |
| `cargo mutants` runtime exceeds the 60 s threshold as the corpus grows | Medium | Low | Documented switch-over rule to `--in-diff origin/main`; the manual full-run check at switch-over time preserves the 100 % kill-rate signal. |
| GitHub Actions usage exceeds the free-tier ceiling | Low | Low | Concurrency cancellation, single-OS matrix, fail-fast ordering keep minute consumption low. The harness's CI is approximately 10 minutes of compute per commit; the free tier is generous. |
| `actions/upload-artifact@v4` deprecation cycle | Low | Low | Standard maintenance: bump when GitHub announces deprecation. |
| `dtolnay/rust-toolchain` action takes a hostile turn | Low | Medium | The action is small (a shell script wrapping `rustup`); a fork-and-pin response is straightforward. SHA-pinning the action is the next-step hardening if the project's threat model changes. |
| KPI 4 verdict-counts JSON schema needs to evolve | Low | Low | Schema is versioned (`schema_version: 1`); a v2 reader can fall back to v1 if needed. The artefact is regenerated every CI run, so a schema bump propagates immediately. |
| Branch-protection-rule drift (someone disables a required check in the GitHub UI) | Low | High | The required-status-check list is documented in `branching-strategy.md`; quarterly review re-checks the GitHub Settings page against this document. |

---

## Quality gate self-check

Per the `production-readiness` skill's "Quality gates for production
readiness" checklist, adapted for a library:

- [x] All acceptance tests passing (73/73, verified by DELIVER)
- [x] Unit coverage meets project standard — implicit via 100 % mutation kill rate (33/33 viable, DELIVER)
- [x] Integration tests validated — corpus runner, slice 07
- [ ] Performance validated under realistic load — N/A for v0 (KPI 7 is informational)
- [x] Security scan completed — `cargo deny check` passes (Gate 4)
- [ ] Monitoring and alerting configured — N/A (no runtime)
- [ ] Logging structured and searchable — N/A (no runtime, no telemetry)
- [ ] Rollback procedure documented and tested — N/A (no deployment)
- [x] Runbook for operational procedures — `branching-strategy.md` and the wave's combined documents serve this role
- [x] On-call team trained on new feature — Andrea is the sole maintainer; the documents above are the training material

The four "N/A" items are structurally non-applicable, not unaddressed.
Each maps to a runtime concern the harness does not have.

---

## Hand-off

This wave produces no further hand-off. The next wave — peer review
by `nw-platform-architect-reviewer`, dispatched by the orchestrator —
reads the artefacts in this directory, the workflow file, the
`rust-toolchain.toml`, and the bootstrap `CLAUDE.md`, and either
approves or returns a finding list.

After the peer review approves, the wave is complete and the
harness's first CI run can be triggered by the next merge to `main`.
There is no separate "go-live" event because the harness has no
runtime; the pipeline becomes active the moment `.github/workflows/ci.yml`
is on `main` and the branch-protection rules are in place.

---

## DEVOPS wave summary

- 1 workflow file (`.github/workflows/ci.yml`) with 5 jobs mapping 1:1 to the 5 ADR-0005 gates.
- 1 `rust-toolchain.toml` pinning stable Rust 1.78.
- 1 `CLAUDE.md` with the Development Paradigm and Mutation Testing Strategy declarations.
- 5 nWave devops artefacts in this directory (`platform-architecture.md`, `ci-cd-pipeline.md`, `branching-strategy.md`, `kpi-instrumentation.md`, `environments.yaml`).
- 1 upstream-issue stub for Crafty's Q6 (`upstream-issue-opentelemetry-proto-feature-split.md`).
- 6 of Crafty's 6 open questions resolved.
- 6 Apex-side decisions recorded (A1–A6), each with rationale.
- 0 changes to the harness source, the harness tests, the corpus, or the existing `deny.toml`.
- 0 new cloud resources, no orchestrator, no observability stack — by design.
- Trunk-Based Development codified.
- 100 % mutation kill rate gate inherited; 60-second budget rule documented.

The harness now has a CI contract honoured by an actual workflow,
reproducibility for contributors, and KPI 4 instrumentation for the
quarterly review. Every other open thread is structurally absent (no
runtime) or deferred to a future iteration with an explicit reason.

---

## Post-merge correction — Gate 4 vs `wit-bindgen-core` 0.51

**Date**: 2026-05-04 (same day as DEVOPS-wave close).

The first real CI run after branch-protection went live failed at
Gate 4 with:

```
error: failed to parse manifest at .../wit-bindgen-core-0.51.0/Cargo.toml
Caused by: feature `edition2024` is required
```

### Root cause

The lockfile contains a target-conditional tail
`wasip3` → `wit-bindgen 0.51.0` → `wit-bindgen-core 0.51.0`, materialised
only for `wasm32-wasip3`. `wit-bindgen-core` 0.51's manifest declares
`edition = "2024"`, which Cargo 1.78 cannot parse.

`EmbarkStudios/cargo-deny-action@v2.0.4` runs in a Docker container
that honours the workspace's `rust-toolchain.toml` (1.78) when shelling
out to `cargo metadata`, and `cargo deny … --all-features` walks every
target in the lockfile by default. The wasm tail therefore reached the
1.78 cargo and the gate failed before any policy was checked.

### Fix

Two changes, applied together as belt-and-braces:

1. **`deny.toml` `[graph].targets`** — pinned the dependency-graph
   walk to four triples we actually ship for
   (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
   `x86_64-apple-darwin`, `aarch64-apple-darwin`). The wasm tail is
   no longer materialised. WSL is covered by the linux triple. Native
   Windows can be added when a real consumer needs it.

2. **`.github/workflows/ci.yml` Gate 4** — replaced the Docker action
   with a precompiled-binary install via `taiki-e/install-action`,
   running `cargo deny … check` directly with
   `RUSTUP_TOOLCHAIN=stable` set at the job level. cargo-deny does not
   compile our code; it only walks the graph, so the MSRV pin is
   irrelevant for this gate. Setting the env var bypasses the
   project-pin discovery cleanly.

Either change alone is sufficient. Together they are durable: target
filtering keeps the graph tight regardless of toolchain, and the
toolchain override keeps Gate 4 robust against future modern manifests
that may appear in scoped targets.

### Why this is a fix-forward, not a new feature

The failing CI run is the "test" that demanded the fix. There was no
design space to explore and no production code to change. Per
trunk-based discipline, the correction lands as a single commit on
`main`, the next CI run validates it, and `main` returns to green.
This is the post-merge correction pattern; the DEVOPS wave remains
closed.

### Carryover

- Forge's H1 (action-pinning by tag rather than SHA) now also covers
  `taiki-e/install-action@v2`. Tightening to commit SHAs remains the
  same recommendation, deferred until external contribution opens.
