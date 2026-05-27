# Wave Decisions - earned-trust-fsync-probe-v0 / DEVOPS

British English. No em dashes.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-27
- **Mode**: slim DEVOPS. This wave confirms that the existing CI contract
  already covers the pulse-side changes of a library-and-wiring slice, and
  records the one nuance the prompt framing under-stated (the
  gateway-side wiring is NOT mutation-gated by any existing job at slice
  01). It designs no new infrastructure. The decision to run slim, and
  its shape, are Apex's own judgement from the DESIGN handoff, not
  pre-taken.

## Why this wave is slim

The feature is a library-and-wiring change that refines ADR-0042's
Earned-Trust principle to honour fsync, not just open-and-read. ADR-0049
records the refinement. The deliverable is:

- A new `pulse::fsync_probe` module (FsyncBackend trait, RealFsyncBackend,
  fsync_probe() free function) inside the existing library crate
  `pulse`.
- Three surgical `sync_all` additions in `crates/pulse/src/file_backed.rs`
  (one in `append_wal` per record; one on the snapshot writer; one or
  two parent-directory fsyncs on the snapshot rename path).
- One wiring call in the existing binary
  `crates/kaleidoscope-gateway/src/main.rs` invoking
  `pulse::fsync_probe(&pulse::RealFsyncBackend, &pulse_path)` BEFORE the
  listener binds, with the existing `event=health.startup.refused`
  emission on `Err`.

There is NO new workspace crate, NO new external dependency (the entire
fsync surface is in std), NO new public event name (the existing
`event=health.startup.refused` is reused with an informational
`substrate=<descriptor>` payload field), NO new CI gate, and NO new
graduation tag (no new crate to tag). The DESIGN "DEVOPS Handoff
Annotation" (`../design/wave-decisions.md` lines 233-289) anticipated
every DEVOPS conclusion bar one nuance recorded under "Honest
contradiction with prompt framing" below; the job of this wave is to
VERIFY those conclusions against the live CI workflow and `deny.toml`,
record the verification, and flag the one nuance honestly, not to
re-litigate the design.

This follows the `pulse-series-identity-v0` slim-DEVOPS precedent. That
precedent produced two files (environments.yaml, wave-decisions.md);
this wave produces the same two, for the same reasons recorded there
under "Artefacts judged N/A".

## Inputs read (in dependency order)

1. `CLAUDE.md` - paradigm (Rust idiomatic) and the per-feature mutation
   testing strategy at 100% kill rate (declared; not modified here).
2. `../discuss/wave-decisions.md` and `../discuss/user-stories.md` -
   the four DISCUSS flags Morgan resolved at DESIGN, and the US-01 /
   US-02 acceptance scenarios.
3. `../design/wave-decisions.md` - DESIGN decisions 1-8 and the explicit
   DEVOPS Handoff Annotation (no new crate, no new dependency, the
   existing `gate-5-mutants-pulse` job covers the pulse-side changes
   via `--in-diff`, no new event name, three orthogonal Earned-Trust
   enforcement layers).
4. `../design/application-architecture.md` - C4 L1+L2, the Changes Per
   File table (the exact line loci in file_backed.rs and main.rs).
5. `docs/product/architecture/adr-0049-earned-trust-honour-fsync.md` -
   the companion ADR (Accepted).
6. `docs/feature/pulse-series-identity-v0/devops/{environments.yaml,wave-decisions.md}`
   - the slim-DEVOPS shape precedent for a library-only pulse feature.
7. `.github/workflows/ci.yml` - the existing five-gate workflow, read to
   CONFIRM (not modify) the `gate-5-mutants-pulse` job's existence and
   `--in-diff` scope (see "Verification against ci.yml" below).
8. `deny.toml` - read to CONFIRM no licence / ban / advisory policy
   change is needed (no new dependency).

## Pre-wave decisions (carried in from project convention, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| P1 | `deployment_target` | None new (pulse stays library-only; kaleidoscope-gateway stays the existing binary, one wiring call added) | DESIGN handoff + ADR-0049 |
| P2 | `container_orchestration` | N/A (slice 01 produces no container image; the pre-existing kaleidoscope-cli Dockerfile is untouched) | environments.yaml |
| P3 | `cicd_platform` | GitHub Actions (existing, unchanged) | ADR-0005 |
| P4 | `existing_infrastructure` | Yes (workspace + five-gate CI; `gate-5-mutants-pulse` already present at ci.yml line 1297) | ci.yml |
| P5 | `git_branching_strategy` | Trunk-based, pure (main has no required-status-checks; CI is feedback, not a gate) | memory `project_kaleidoscope_pure_trunk_based` |
| P6 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md, ADR-0005 Gate 5 |

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] No new CI gate; ADR-0005's five gates inherited unchanged

The change touches one file in pulse (`file_backed.rs`), adds one new
file in pulse (`fsync_probe.rs`), adds two lines to `pulse/src/lib.rs`
(a `mod` and a `pub use`), and adds one wiring block in the gateway
main. Each gate is satisfied by existing machinery:

- **Gate 1 (`cargo test --workspace`)**: runs the new
  `crates/pulse/tests/slice_01_fsync_probe.rs` with zero workflow edit
  (the file is auto-discovered under `crates/pulse/tests/`). This is
  the KPI collection surface for the four behavioural KPIs (honest +
  three lie classes).
- **Gate 2 (`cargo public-api`)** and **Gate 3 (`cargo semver-checks`)**:
  scope to harness / spark / sieve / codex only; pulse and
  kaleidoscope-gateway are not in the locked set (verified below). No
  diff applies. `MetricStore` / `LogStore` / `TraceStore` / beacon
  `RuleStateStore` trait signatures are unchanged regardless (DESIGN
  Decision 7).
- **Gate 4 (`cargo deny`)**: no new external dependency, so no scan
  change. VERIFIED in `deny.toml`: no edit required (A4 below).
- **Gate 5 (`cargo mutants`)**: covered by the existing
  `gate-5-mutants-pulse` job for the pulse-side changes; the gateway
  wiring is not mutation-gated at slice 01 (see A2 and the honest
  contradiction note).

No new or amended gate is warranted. No new CI workflow file is
created; no existing gate is added to, removed, or modified by this
feature.

### [A2] Mutation testing: the existing `gate-5-mutants-pulse` covers the pulse-side changes; no workflow edit

**Options considered**:

1. **Rely on the existing `gate-5-mutants-pulse` job** (which already
   runs `cargo mutants --package pulse --in-diff` against
   `crates/pulse/**`).
2. Add a new file-scoped job pinned to the new `fsync_probe.rs` and the
   modified `file_backed.rs`.
3. Add a new `gate-5-mutants-kaleidoscope-gateway` job to cover the
   gateway wiring.

**Decision**: Option 1.

**Rationale**: `gate-5-mutants-pulse` already exists in `ci.yml` (line
1297) and runs the `--in-diff` cascade against `crates/pulse/**` with
the `origin/main -> HEAD~1 -> full` baseline, short-circuiting to a
zero-second exit on an empty diff. Because this feature touches
`crates/pulse/src/file_backed.rs` and adds
`crates/pulse/src/fsync_probe.rs`, the diff filter naturally limits
mutation to exactly those files - which is precisely the DESIGN-scoped
mutation set on the pulse side. Option 2 would duplicate the existing
job's behaviour for no benefit and would require a workflow edit the
feature does not need. Option 3 (adding a gateway mutation job) is
REJECTED for slice 01 on proportionality grounds: a single wiring call
is a poor mutation target (the call either exists or it does not, and
its presence is enforced by ADR-0049 Verification Layers (a) subtype
and (b) AST structural; the behavioural payload of the gateway main is
that the listener does not bind when the probe returns `Err`, which is
covered by the Layer-(c) behavioural acceptance test).

**Mutation scope (per DESIGN)**: `crates/pulse/src/fsync_probe.rs`
(the new module) and `crates/pulse/src/file_backed.rs` (the
`append_wal` `sync_all` line, the `snapshot` `sync_all` line, and the
two parent-directory fsyncs on the rename path). The 100% kill-rate
gate (CLAUDE.md, ADR-0005 Gate 5) is enforced by the job's non-zero
exit on any surviving mutant. Primary mutation targets per the DESIGN
DEVOPS Handoff Annotation:

- The probe's bytes-differ branch (`!=` -> `==` must be killed).
- The per-record `sync_all` on `append_wal` (the call must not be
  deletable without a surviving test).
- The parent-directory fsync calls on the snapshot rename.
- The three lie classes in the substrate descriptor mapping (no-op vs
  truncating vs corrupting must remain distinguishable).

### [A3] No new public event name; no new dashboard

Refusal rides on the existing `event=health.startup.refused` (ADR-0042
Decision 8 vocabulary, reused verbatim by ADR-0047 / 0048 / 0049). The
informational payload field `substrate=<descriptor>` is added but no
operator alert or dashboard work is needed at v0/v1: the platform has
no live operator-facing observability stack of its own yet (per
`../discuss/outcome-kpis.md` "Handoff to DEVOPS"), and the refusal is
self-documenting in the gateway's exit logs. Recorded so DELIVER does
not invent an alert routing story.

### [A4] No new external dependency; Gate 4 unaffected (`deny.toml` unchanged)

`std::fs::File::sync_all` and `std::fs::File::open` cover the entire
fsync probe and write-path surface. `serde` and `serde_json` are
already present in pulse for the existing WAL / snapshot. No `nix`,
no `libc`, no `tempfile` in non-test code. VERIFIED by reading
`deny.toml`:

- The `[graph].targets` (x86_64-linux-gnu, aarch64-linux-gnu,
  x86_64-darwin, aarch64-darwin) already cover the supported
  platforms.
- The `[licenses].allow` list is unaffected (no new transitive crate
  is added).
- The `[bans]` list is unaffected (no new dependency, so no new
  duplicate-version concern; the `multiple-versions = "allow"`
  relaxation is unchanged).
- The `[advisories]` and `[sources]` policies are unaffected.

**Verdict**: zero change to `deny.toml`.

### [A5] No new graduation tag; no per-crate release

There is no new crate, so there is no new per-crate tag at graduation.
The change lands as a `git commit` on `main` (pure trunk-based per P5)
under the existing pulse and kaleidoscope-gateway crate manifests.
No `pulse-vX.Y.Z` or `kaleidoscope-gateway-vX.Y.Z` tag is created by
this slice. Recorded so DELIVER does not invent a release story.

### [A6] No observability / monitoring / alerting instrumentation beyond the refusal event

The refusal event `event=health.startup.refused` is the entire
observability surface for slice 01. There is no new bridge-latency
counter, no events-dropped gauge, no histogram. The substrate
descriptor is an informational payload field on the existing event.
For a no-new-deployment library-and-wiring slice the CI gates ARE the
alerting surface: a regression fails Gate 1 (test) or Gate 5 (mutants
for the pulse files) at the next push, and the behavioural gold-test
of ADR-0049 Verification Layer (c) is the regression net for the
gateway wiring.

### [A7] No deployment / rollback procedure beyond git revert

There is no new deployment artefact, so there is nothing to roll back
at the deployment layer. kaleidoscope-gateway is the existing binary;
its main.rs gains one wiring block. The project is pure trunk-based
with no merge gate (memory `project_kaleidoscope_pure_trunk_based`);
the recovery is fix-forward on `main`. This satisfies the
rollback-first principle vacuously for the deployment layer: the only
"rollback" available and needed is a git revert of the slice commit,
and because the probe is a startup check (it adds no on-disk
state-format change), a revert has no data consequence. The new
sentinel file `pillar_root/pulse/.fsync-probe` is overwritten per
gateway start (64 bytes, fixed path), not accumulating, and is left in
place by a revert (an operator may delete it manually; it is harmless
if left).

## Verification against ci.yml (CONFIRM, not modify)

Read of `.github/workflows/ci.yml` in this wave confirmed:

| Claim | Verified location | Result |
|-------|-------------------|--------|
| Gate 2 (`cargo public-api`) scopes to harness / spark / sieve / codex; pulse and kaleidoscope-gateway excluded | lines 326-347 (`-p otlp-conformance-harness`, `-p spark`, `-p sieve`, `-p codex`) | CONFIRMED, pulse and gateway not present |
| Gate 3 (`cargo semver-checks`) scopes to the same four; pulse and kaleidoscope-gateway excluded | lines 420-433 (`--package` for the same four) | CONFIRMED, pulse and gateway not present |
| `gate-5-mutants-pulse` job exists and runs `cargo mutants --in-diff` over `crates/pulse/**` | line 1297; invocation `cargo mutants --package pulse --in-diff "$DIFF_FILE"` (lines 1359-1363) with `origin/main -> HEAD~1 -> full` cascade (lines 1343-1356) and empty-diff short-circuit (lines 1352-1355); diff filter is `git diff "$BASELINE" HEAD -- 'crates/pulse/**'` (line 1351) | CONFIRMED present, covers `crates/pulse/src/fsync_probe.rs` and `crates/pulse/src/file_backed.rs` |
| `gate-5-mutants-kaleidoscope-gateway` job exists | (searched gate-5-mutants-* jobs; only `kaleidoscope-cli` is present at line 1636, no `kaleidoscope-gateway` job) | NOT PRESENT - the gateway wiring is not mutation-gated at slice 01; see "Honest contradiction with prompt framing" |

No workflow file was modified by this wave. No gate was added, removed,
or amended.

## Verification against deny.toml (CONFIRM, not modify)

Read of `deny.toml` in this wave confirmed that no policy change is
needed:

- `[graph].targets`: the four supported triples (linux x86_64,
  linux aarch64, darwin x86_64, darwin aarch64) are unchanged.
- `[licenses].allow`: zero new transitive licence (no new dependency
  introduced; std is unlicenced for purposes of `cargo deny`).
- `[bans]`: zero new dependency, so the `multiple-versions = "allow"`
  relaxation and the `deny = [{ name = "openssl", ... }]` clauses are
  unaffected.
- `[advisories]`: `yanked = "deny"` policy is unaffected.
- `[sources]`: `unknown-registry = "deny"`, `unknown-git = "deny"`
  policies are unaffected.

**Verdict**: zero change to `deny.toml`. The Gate 4 (`cargo deny`) run
on the slice's commit will pass with the same policy that passed at
`pulse-series-identity-v0` close.

## Honest contradiction with prompt framing

The DEVOPS-wave prompt to Apex framed `gate-5-mutants-pulse` as
covering "the modified files (file_backed.rs, the new fsync_probe.rs,
the gateway main) ... via `--in-diff`". After reading ci.yml line 1351,
this framing is over-inclusive: the pulse job's diff filter is
`git diff "$BASELINE" HEAD -- 'crates/pulse/**'`, which by design
EXCLUDES `crates/kaleidoscope-gateway/src/main.rs`. A grep over
`ci.yml` for `gate-5-mutants-*` returns 15 mutation jobs; the only
binary-crate job is `gate-5-mutants-kaleidoscope-cli` (line 1636);
there is NO `gate-5-mutants-kaleidoscope-gateway` job.

The DESIGN handoff itself was careful and CORRECT: the DEVOPS Handoff
Annotation (`../design/wave-decisions.md` line 244-249) names ONLY
`crates/pulse/src/fsync_probe.rs` and the additions in
`crates/pulse/src/file_backed.rs` as the files
`gate-5-mutants-pulse --in-diff` covers; the gateway wiring is not
listed in that scope. The Reuse Analysis line on the same point (line
172) similarly says the existing job "covers `crates/pulse/src/`",
not the gateway. The contradiction is between the prompt framing and
the actual ci.yml, not between the DESIGN and the DEVOPS wave.

**Resolution**: ADR-0049 Decision 6 / Verification already designs
THREE orthogonal Earned-Trust enforcement layers, only one of which is
mutation testing. The gateway wiring is enforced by:

- Layer (a) **subtype**: `pulse::RealFsyncBackend` `impl FsyncBackend`
  is consumed at the gateway's composition root; removing the
  implementation fails the build (caught by Gate 1).
- Layer (b) **AST structural pre-commit check**: a hook scans
  `crates/kaleidoscope-gateway/src/main.rs` for a call to
  `pulse::fsync_probe` ABOVE the `axum::serve` / listener bind. ADR-0049
  Verification names this hook as Apex's during DEVOPS. The hook is
  recorded as a constraint on DELIVER (see below) but is NOT itself
  introduced by this slim DEVOPS wave: a slim DEVOPS wave that adds a
  new pre-commit hook would be contradictory in name. The hook lives
  one step further along in the project's hook stack, to be
  implemented when DISTILL or DELIVER finds the Layer (a) and Layer (c)
  pair insufficient on the gateway side.
- Layer (c) **behavioural gold-test**: an acceptance test in
  `crates/pulse/tests/slice_01_fsync_probe.rs` exercises the three lie
  classes via `LyingFsyncBackend` and asserts the probe returns `Err`
  and the synthetic composition-root caller emits
  `event=health.startup.refused`. Covered by Gate 1.

So the gateway wiring is enforced by two of the three layers at slice
01 close (subtype + behavioural), with the AST structural check
queued behind them. The prompt's framing was simply more optimistic
about mutation coverage than ci.yml warrants; the ADR-0049 design is
robust either way.

## KPI to gate mapping

All outcome KPIs (`../discuss/outcome-kpis.md`) are correctness
indicators collected by green acceptance tests under **Gate 1** (`cargo
test --workspace`) running the new
`crates/pulse/tests/slice_01_fsync_probe.rs` test file, with Gate 5
(`gate-5-mutants-pulse`) guarding the test-suite strength behind the
pulse-side assertions. The trait signature KPI is additionally
collected by the compile of existing pulse consumers under Gate 1.

| KPI (from outcome-kpis.md) | Target | Gate | Collection |
|----------------------------|--------|------|------------|
| North star: the pulse Earned-Trust claim is honest under a lying substrate | 1 honest pass + 3 lie classes refused | Gate 1 | `slice_01_fsync_probe.rs` (honest + three lie scenarios) |
| Honest substrate passes the probe | 1 Ok return over a real tempdir | Gate 1 | US-01 Scenario 1 |
| fsync-no-op lie is detected | 1 Err with substrate=fsync-noop | Gate 1 | US-01 Scenario 2 via LyingFsyncBackend no-op mode |
| fsync-truncating lie is detected | 1 Err with substrate=fsync-truncating | Gate 1 | US-01 Scenario 3 via LyingFsyncBackend truncating mode |
| fsync-corrupting lie is detected | 1 Err with substrate=fsync-corrupting | Gate 1 | US-02 lie class via LyingFsyncBackend byte-flipping mode |
| MetricStore / LogStore / TraceStore / beacon RuleStateStore trait signatures unchanged | 0 signature changes | Gate 1 | compile of query-api, log-query-api, trace-query-api, aperture-storage-sink, self-observe |
| WAL append per-record sync_all call is not deletable | 0 surviving mutants on the `append_wal` sync_all line | Gate 5 | `gate-5-mutants-pulse --in-diff` over `file_backed.rs` |
| Snapshot parent-directory fsyncs are not deletable | 0 surviving mutants on the snapshot rename path | Gate 5 | `gate-5-mutants-pulse --in-diff` over `file_backed.rs` |
| Probe substrate descriptor classes remain distinguishable | 0 surviving mutants on the bytes-differ branch and the descriptor mapping | Gate 5 | `gate-5-mutants-pulse --in-diff` over `fsync_probe.rs` |
| Gateway wiring is present BEFORE listener bind | the gateway main calls `pulse::fsync_probe` before `axum::serve` | Layer (a) subtype + Layer (c) behavioural | compile of the gateway under Gate 1 + the behavioural acceptance test |

## Infrastructure summary

- **Deployment**: none new (pulse stays library-only; kaleidoscope-gateway
  stays the existing binary, one wiring call added).
- **CI/CD**: GitHub Actions, ADR-0005 five gates, inherited unchanged.
  `gate-5-mutants-pulse` already present at ci.yml line 1297; no new or
  amended job. Gateway wiring is NOT mutation-gated at slice 01 (Layer
  (a) subtype + Layer (c) behavioural cover it).
- **Branching**: pure trunk-based (project default, unchanged).
- **Mutation testing**: per-feature, 100% kill rate, scoped by `--in-diff`
  to `crates/pulse/src/fsync_probe.rs` and `crates/pulse/src/file_backed.rs`.
- **External integrations**: none. No contract tests apply.
- **External dependencies**: none new. `deny.toml` unchanged.
- **Observability**: no new instrumentation beyond the reused
  `event=health.startup.refused` with `substrate=<descriptor>` payload
  field; CI gates and the in-process refusal event are the alerting
  surface.
- **Public surface (pulse)**: three new public items - `FsyncBackend` (trait),
  `RealFsyncBackend` (struct), `fsync_probe` (free function) - all
  re-exported from `pulse::*`. Trait surfaces of the four storage stores
  are unchanged.
- **Public surface (kaleidoscope-gateway)**: unchanged (the binary has no
  public library surface).
- **Graduation tag**: none (no new crate).
- **Docker**: out of scope for slice 01; the pre-existing
  kaleidoscope-cli Dockerfile is untouched.

## Artefacts produced by this wave

| Artefact | Path |
|----------|------|
| Environment inventory (clean target environment, in-process probe, no external services) | `docs/feature/earned-trust-fsync-probe-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/earned-trust-fsync-probe-v0/devops/wave-decisions.md` |

## Artefacts judged N/A (with reason)

| Skipped artefact | Reason |
|------------------|--------|
| `kpi-instrumentation.md` | Every KPI maps to Gate 1 on one new test file (plus Gate 5 for suite strength) and a Layer (a) / Layer (c) pair for the gateway wiring; no instrumentation to design. A separate file would only restate the KPI to gate mapping table above. |
| `ci-cd-pipeline.md` | This feature adds no job and edits no workflow; the existing `gate-5-mutants-pulse` covers the pulse-side as-is, and the gateway wiring is covered by the Earned-Trust enforcement layers (a) and (c). The "Verification against ci.yml" section above is the entire pipeline content for this feature; a separate addendum would be empty. |
| `platform-architecture.md` | No platform infrastructure to architect (no cloud, no orchestration, no service mesh). Morgan's `../design/application-architecture.md` is sufficient. |
| `observability-design.md` / `monitoring-alerting.md` | No runtime monitoring beyond the reused refusal event (A3, A6); CI gates and the in-process event are the alerting surface. |
| `infrastructure-integration.md` | No external integrations at runtime (DESIGN: external integrations = none). |
| `branching-strategy.md` | Pure trunk-based is the project default; no per-feature deviation (P5). |
| `deployment-strategy.md` / `rollback.md` | No new deployment artefact; recovery is git revert with no data-format consequence (A7). |
| `docker.md` / `containers.md` | Out of scope for slice 01 (environments.yaml); the pre-existing kaleidoscope-cli Dockerfile is untouched. |

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Write the acceptance suite in a new `crates/pulse/tests/slice_01_fsync_probe.rs` file, exercising one honest case (real tempdir) and three lie classes (LyingFsyncBackend no-op, truncating, byte-flipping); assert the substrate descriptor for each lie class | The `clean` environment with the durable substrate AND the in-memory `LyingFsyncBackend` double are the only environments to parametrise over (environments.yaml); the probe is the contract |
| At DISTILL | DO NOT edit `.github/workflows/ci.yml` | No new gate; Gate 1 auto-discovers the test, `gate-5-mutants-pulse` already covers mutation on the pulse files (A1, A2) |
| At DISTILL | DO NOT add `pulse` or `kaleidoscope-gateway` to Gate 2 or Gate 3 | They scope to harness / spark / sieve / codex; graduation is out of scope (A1) |
| At DISTILL | DO NOT propose a new `gate-5-mutants-kaleidoscope-gateway` job | Slice 01 enforces the gateway wiring via Earned-Trust Layer (a) subtype + Layer (c) behavioural (the honest contradiction note above); a new mutation job for a single wiring call is disproportionate at v0 |
| At DELIVER | Declare `FsyncBackend`, `RealFsyncBackend`, and `fsync_probe` as `pub` in `pulse::*` (re-exported from `lib.rs`) | DESIGN application-architecture.md lib.rs change; the surface is intended public so successor slices can reuse the trait from other composition roots |
| At DELIVER | Declare `LyingFsyncBackend` inside `#[cfg(test)] mod tests` only | Per ADR-0049 Decision 6 and the LyingLogStore / LyingTraceStore precedent; the double is test-only and not part of the public surface |
| At DELIVER | Turn the modified files' mutants 100% killed before close (the bytes-differ branch, the per-record sync_all, the parent-directory fsyncs, the three substrate descriptor classes) | CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5 (A2) |
| At DELIVER | Do not invent a migration path for any on-disk format | The WAL and snapshot file formats do not change; only their durability semantics (sync_all) change (A7) |
| At DELIVER | Do not invent a new event name | Refusal rides on the existing `event=health.startup.refused` with a `substrate=<descriptor>` payload field (A3) |
| At DELIVER (or DISTILL) | Optionally, add the Layer (b) AST structural pre-commit hook scanning `crates/kaleidoscope-gateway/src/main.rs` for `pulse::fsync_probe` above `axum::serve` | ADR-0049 Verification names this hook as Apex's during DEVOPS; the slim wave defers it as proportionate (the two other layers cover the wiring at slice 01); the wave-closure may add it as a small follow-up commit per `feedback_fix_forward_post_merge_correction` memory |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**What DISTILL receives**: the mandatory `environments.yaml` for
Mandate 4 (the `clean` environment, in-process via the `FsyncBackend`
seam, no external services); the confirmation that no CI edit is
needed (A1, A2); the confirmation that `deny.toml` is unchanged (A4);
the honest contradiction note about gateway mutation coverage (the
gateway wiring is enforced by ADR-0049 Verification Layers (a) and (c),
not by mutation); the constraint that `FsyncBackend`, `RealFsyncBackend`,
and `fsync_probe` are `pub` while `LyingFsyncBackend` is test-only;
and the KPI to gate mapping above.

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt of
this wave's outputs.
