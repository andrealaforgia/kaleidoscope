# Wave Decisions - pulse-series-identity-v0 / DEVOPS

British English. No em dashes.

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-22
- **Mode**: slim DEVOPS. This wave confirms that the existing CI contract
  already covers a library-only, in-crate data-model correction; it
  designs no new infrastructure. The decision to run a slim wave, and its
  shape, are Apex's own judgement from the DESIGN handoff, not pre-taken.

## Why this wave is slim

The feature is a library-only, in-crate data-model correction in the
`pulse` crate: a metric series becomes identified by its full label set
(`MetricName` + `resource_attributes`) instead of by name alone. There is
no new component, no new crate, no new public API (the `MetricStore` trait
signature is unchanged), and no deployment artefact. The on-disk snapshot
format may change but there is no production data, so no migration is
needed. The DESIGN "DEVOPS Handoff Annotation"
(`../design/wave-decisions.md`) anticipated every DEVOPS conclusion; the
job of this wave is to VERIFY those conclusions against the live CI
workflow and record the verification, not to re-litigate them.

This follows the `cinder-to-pulse-bridge-v0` slim-DEVOPS precedent. That
precedent produced four files (environments.yaml, wave-decisions.md,
kpi-instrumentation.md, ci-cd-pipeline.md). This wave produces two; the
two it omits are justified as N/A under "Artefacts judged N/A" below,
because here there is genuinely less to say than for the bridge (which
ADDED a new public item and a new mutation job).

## Inputs read (in dependency order)

1. `CLAUDE.md` - paradigm (Rust idiomatic) and the per-feature mutation
   testing strategy at 100% kill rate (declared; not modified here).
2. `../discuss/outcome-kpis.md` - the north star plus seven correctness
   guardrail KPIs.
3. `../design/wave-decisions.md` - DESIGN decisions D1-D9 and the
   explicit DEVOPS Handoff Annotation (library-only, no new gate, ADR-0005
   five gates inherited, per-feature mutation on the three modified files
   via `gate-5-mutants-pulse`, no new dependency, no external integration).
4. `../design/application-architecture.md` - C4 L1+L2, the three changed
   files, no external integrations.
5. `docs/product/architecture/adr-0045-pulse-series-identity-is-the-full-label-set.md`
   - the companion ADR (Accepted).
6. `docs/feature/cinder-to-pulse-bridge-v0/devops/{environments.yaml,wave-decisions.md}`
   - the slim-DEVOPS shape precedent for a library-only feature.
7. `.github/workflows/ci.yml` - the existing five-gate workflow, read to
   CONFIRM (not modify) the gate scopes (see "Verification against ci.yml").

## Pre-wave decisions (carried in from project convention, not re-litigated)

| D# | Decision | Value | Source |
|----|----------|-------|--------|
| P1 | `deployment_target` | None (library only; no deploy artefact) | DESIGN handoff + ADR-0045 |
| P2 | `container_orchestration` | N/A | library only |
| P3 | `cicd_platform` | GitHub Actions (existing, unchanged) | ADR-0005 |
| P4 | `existing_infrastructure` | Yes (workspace + five-gate CI; `gate-5-mutants-pulse` already present) | ci.yml |
| P5 | `git_branching_strategy` | Trunk-based, pure (main has no required-status-checks; CI is feedback, not a gate) | memory `project_kaleidoscope_pure_trunk_based` |
| P6 | `mutation_testing_strategy` | Per-feature, 100% kill rate | CLAUDE.md, ADR-0005 Gate 5 |

## In-wave decisions (A = Apex / DEVOPS Decision)

### [A1] No new CI gate; ADR-0005's five gates inherited unchanged

The change touches three files of an already-gated crate. Each gate is
satisfied by existing machinery:

- **Gate 1 (cargo test --workspace)**: runs the new pulse acceptance file
  with zero workflow edit (the file is auto-discovered under
  `crates/pulse/tests/`). This is the KPI collection surface (see mapping).
- **Gate 2 (cargo public-api)** and **Gate 3 (cargo semver-checks)**:
  scope to harness/spark/sieve/codex only; pulse is not in the locked set
  (verified below). No diff applies. The trait signature is unchanged
  regardless (DESIGN D8).
- **Gate 4 (cargo deny)**: no new external dependency, so no scan change.
- **Gate 5 (cargo mutants)**: covered by the existing
  `gate-5-mutants-pulse` job (A2 below).

No new or amended gate is warranted. No new CI workflow file is created;
no existing gate is added to, removed, or modified by this feature.

### [A2] Mutation testing: the existing `gate-5-mutants-pulse` job covers it; no workflow edit

**Options considered**:

1. **Rely on the existing `gate-5-mutants-pulse` job** (which already runs
   `cargo mutants --package pulse --in-diff` against `crates/pulse/**`).
2. Add a new file-scoped job pinned to the three modified files.
3. Reuse a different crate's job.

**Decision**: Option 1.

**Rationale**: `gate-5-mutants-pulse` already exists in `ci.yml` (line
1123) and runs the `--in-diff` cascade against `crates/pulse/**` with the
`origin/main -> HEAD~1 -> full` baseline, short-circuiting to a
zero-second exit on an empty diff. Because this feature touches
`crates/pulse/src/{store.rs, file_backed.rs, metric.rs}`, the diff filter
naturally limits mutation to exactly those files - which is precisely the
DESIGN-scoped mutation set. Option 2 would duplicate the existing job's
behaviour for no benefit and would require a workflow edit the feature
does not need. Option 3 loses per-package fail-fast isolation. The 100%
kill-rate gate (CLAUDE.md, ADR-0005 Gate 5) is enforced by the job's
non-zero exit on any surviving mutant.

**Mutation scope (per DESIGN)**: `crates/pulse/src/store.rs`,
`crates/pulse/src/file_backed.rs`, `crates/pulse/src/metric.rs`. The
re-keying lines, the removed overwrite, and the query fan-out all carry
assertable behaviour (a surviving mutant on the `SeriesKey.name` match in
the fan-out, or on the dropped overwrite, would represent a real
test-suite gap), so mutation is informative here, not a thin-shell case.

### [A3] `SeriesKey` stays crate-private (`pub(crate)`); no public surface added

**Decision**: `SeriesKey` is internal series-identity machinery and is to
be declared `pub(crate)` at DELIVER, not `pub`.

**Rationale**: keeping the key crate-private means this feature adds zero
public surface. Combined with the unchanged `MetricStore` trait signature
(DESIGN D8, ADR-0045 Decision 5), the public API of `pulse` is
byte-identical before and after the feature. This keeps the door clean
should pulse later graduate to Gates 2/3: there would be no surprise new
public item to baseline. This is a constraint on DELIVER, recorded here so
the crafter does not inadvertently export the key.

### [A4] No new external dependency; Gate 4 unaffected

`SeriesKey` uses only `std::collections::BTreeMap` and derive macros.
`serde`, `serde_json`, and `aegis` are already present in the crate. No
new `[dependencies]` entry, no `deny.toml` change.

### [A5] Snapshot format may change; no migration

Per DESIGN D7 and ADR-0045 Decision 4: the in-memory bucket rebuild key
changes from name to full label set, and the on-disk format may change
freely. No production Pulse data exists (library-only at v0/v1, no
daemon), so no migration, shim, or version negotiation is designed.
Recorded so DELIVER does not invent a migration story.

### [A6] No observability/monitoring/alerting instrumentation for this feature

Pulse is part of the platform's observability substrate, but this feature
adds no instrumentation of its own (no bridge-latency counter, no
events-dropped gauge). For a no-deployment library the CI gates ARE the
alerting surface: a regression fails Gate 1 (test) or Gate 5 (mutants) at
the next push. The empirical probe is the existing recovery durability
test, unchanged in shape and now also exercising distinct-series survival
(DESIGN Earned Trust note). No separate observability stack is designed.

### [A7] No deployment/rollback procedure

There is no deployment artefact, so there is nothing to roll back at the
deployment layer. The project is pure trunk-based with no merge gate
(memory `project_kaleidoscope_pure_trunk_based`); the recovery is
fix-forward on `main`. This satisfies the rollback-first principle
vacuously: the only "rollback" available and needed is a git revert of the
keying commit, and because no production data exists, a revert has no data
consequence (A5).

## Verification against ci.yml (CONFIRM, not modify)

Read of `.github/workflows/ci.yml` in this wave confirmed:

| Claim | Verified location | Result |
|-------|-------------------|--------|
| Gate 2 (`cargo public-api`) scopes to harness/spark/sieve/codex; pulse excluded | lines 326-347 (`-p otlp-conformance-harness`, `-p spark`, `-p sieve`, `-p codex`) | CONFIRMED, pulse not present |
| Gate 3 (`cargo semver-checks`) scopes to the same four; pulse excluded | lines 420-433 (`--package` for the same four) | CONFIRMED, pulse not present |
| `gate-5-mutants-pulse` job exists and runs `cargo mutants --in-diff` | line 1123; invocation `cargo mutants --package pulse --in-diff "$DIFF_FILE"` (lines 1185-1189) with `origin/main -> HEAD~1 -> full` cascade and empty-diff short-circuit | CONFIRMED present |

No workflow file was modified by this wave. No gate was added, removed, or
amended.

## KPI to gate mapping

All outcome KPIs (`../discuss/outcome-kpis.md`) are correctness indicators
collected by green acceptance tests under **Gate 1** (`cargo test
--workspace`) running the new pulse series-identity test file. The trait
signature KPI is additionally collected by the compile of existing
consumers under Gate 1; Gate 5 (`gate-5-mutants-pulse`) guards the
test-suite strength behind these assertions.

| KPI (from outcome-kpis.md) | Target | Gate | Collection |
|----------------------------|--------|------|------------|
| North star: per-service provenance survives ingest and recovery | 100% distinct sets preserved; 0 overwritten | Gate 1 | US-01 live path + US-02 durable paths |
| Distinct series preserved at ingest | 100% of distinct label sets under a name | Gate 1 | US-01 acceptance scenarios |
| No cross-service label overwrite | 0 overwrites | Gate 1 | US-01 assertion that neither service overwrites the other |
| Identical label set merges, not duplicates | 1 series, points ascending | Gate 1 | US-01 boundary scenario |
| Distinct series survive snapshot + reopen | 100% present, correctly labelled | Gate 1 | US-02 snapshot path on a real FileBackedMetricStore (tempdir) |
| Distinct series survive WAL-only reopen | 100% present, correctly labelled | Gate 1 | US-02 WAL-replay path |
| MetricStore trait signature unchanged | 0 signature changes | Gate 1 | compile of query-api, aperture-storage-sink, self-observe |
| Point attributes untouched | per-point, unchanged | Gate 1 | US-01 edge scenario (point attrs do not split a series) |
| Test-suite strength behind the above | 100% mutant kill | Gate 5 | `gate-5-mutants-pulse` --in-diff over the three modified files |

## Infrastructure summary

- **Deployment**: none (library only, no artefact).
- **CI/CD**: GitHub Actions, ADR-0005 five gates, inherited unchanged.
  `gate-5-mutants-pulse` already present; no new or amended job.
- **Branching**: pure trunk-based (project default, unchanged).
- **Mutation testing**: per-feature, 100% kill rate, scoped by `--in-diff`
  to `crates/pulse/src/{store.rs, file_backed.rs, metric.rs}`.
- **External integrations**: none. No contract tests apply.
- **Observability**: no new instrumentation; CI gates are the alerting
  surface.
- **Public surface**: unchanged. `SeriesKey` is crate-private (A3).

## Artefacts produced by this wave

| Artefact | Path |
|----------|------|
| Environment inventory (library-only, durable substrate) | `docs/feature/pulse-series-identity-v0/devops/environments.yaml` |
| DEVOPS wave decisions log (this file) | `docs/feature/pulse-series-identity-v0/devops/wave-decisions.md` |

## Artefacts judged N/A (with reason)

| Skipped artefact | Reason |
|------------------|--------|
| `kpi-instrumentation.md` | The bridge precedent produced this because it ADDED a new public item and a new mutation job needing a per-KPI gate design. Here the KPI to gate mapping is short and fully contained in the table above; every KPI maps to Gate 1 on one new test file (plus Gate 5 for suite strength), with no instrumentation to design. A separate file would only restate this table. |
| `ci-cd-pipeline.md` | The bridge precedent produced this to specify a NEW Gate 5 job. This feature adds no job and edits no workflow; the existing `gate-5-mutants-pulse` covers it as-is. The "Verification against ci.yml" section above is the entire pipeline content for this feature; a separate addendum would be empty. |
| `platform-architecture.md` | No platform infrastructure to architect (no cloud, no orchestration, no service mesh). Morgan's `application-architecture.md` is sufficient. |
| `observability-design.md` / `monitoring-alerting.md` | No runtime monitoring for a no-deployment library; CI gates are the alerting surface (A6). |
| `infrastructure-integration.md` | No external integrations at runtime (DESIGN: external integrations = none). |
| `branching-strategy.md` | Pure trunk-based is the project default; no per-feature deviation (P5). |
| `deployment-strategy.md` / `rollback.md` | No deployment artefact; recovery is git revert with no data consequence (A7). |

## Constraints established for downstream waves (DISTILL, DELIVER)

| When | What | Why |
|------|------|-----|
| At DISTILL | Write the acceptance tests in a new `crates/pulse/tests/` file (mirroring the existing snapshot/recovery test), opening a real `FileBackedMetricStore` in a tempdir; cover the eight acceptance criteria including snapshot-path and WAL-only reopen | The `clean` environment with the durable substrate is the only environment to parametrise over (environments.yaml); durable survival is part of the contract |
| At DISTILL | DO NOT edit `.github/workflows/ci.yml` | No new gate; Gate 1 auto-discovers the test, `gate-5-mutants-pulse` already covers mutation (A1, A2) |
| At DISTILL | DO NOT add `pulse` to Gate 2 or Gate 3 | They scope to harness/spark/sieve/codex; pulse graduation is out of scope (A1) |
| At DELIVER | Declare `SeriesKey` as `pub(crate)`, not `pub` | No public surface is added (A3) |
| At DELIVER | Turn the modified files' mutants 100% killed before close | CLAUDE.md per-feature MT strategy and ADR-0005 Gate 5 (A2) |
| At DELIVER | Do not write a migration path for the snapshot format | No production data; format may change freely (A5) |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**What DISTILL receives**: the mandatory `environments.yaml` for Mandate 4
(the `clean` environment over the real `FileBackedMetricStore`, snapshot
and WAL-only recovery, no external services); the confirmation that no CI
edit is needed (A1, A2); the constraint that `SeriesKey` stays
crate-private (A3); and the KPI to gate mapping above.

**Peer review**: required before DISTILL handoff. The orchestrator
dispatches `@nw-platform-architect-reviewer` separately upon receipt of
this wave's outputs.

## Note on the replaced placeholders

The two files at this path before this wave (`environments.yaml`,
`wave-decisions.md`) were hand-authored placeholders, one attributed to
the orchestrator. They have been overwritten with this genuine nWave
DEVOPS output. The conclusions are largely the same (the DESIGN handoff
was accurate), but the verification against `ci.yml`, the full KPI
mapping, the crate-private `SeriesKey` constraint, and the N/A
justifications are now Apex's own and traceable.
