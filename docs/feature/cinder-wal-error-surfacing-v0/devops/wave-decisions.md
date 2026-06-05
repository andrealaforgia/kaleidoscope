# Wave Decisions — cinder-wal-error-surfacing-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-05
- **Mode**: Autonomous overnight run. **SLIM** wave — a library + CLI change
  to existing crates (cinder, sluice, kaleidoscope-cli); NO new crate, NO
  deploy surface, NO new infrastructure.
- **Inputs read**: `design/wave-decisions.md` (D1-D4 resolved),
  `docs/product/architecture/adr-0065-cinder-wal-error-surfacing-trait-signature.md`,
  `docs/product/architecture/brief.md` (§"Application Architecture —
  cinder-wal-error-surfacing-v0", incl. its For-Acceptance-Designer +
  failing-substrate notes), `discuss/outcome-kpis.md`,
  `docs/product/architecture/adr-0005-ci-contract.md` (the five gates),
  `.github/workflows/ci.yml`, `scripts/hooks/{pre-commit,pre-push}`,
  `CLAUDE.md`, and the prior slim-DEVOPS shapes
  (`cli-ingest-atomic-v0/devops`, `store-fsync-durability-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | D1-D4 resolved (trait sig change; fail-the-ingest; fail-whole sweep; sluice Queue fallible); caller-ripple map (20 files, 1 live `flush`); reuse verdict = EXTEND, zero net-new types — all consumed | − none; DESIGN explicitly hands DEVOPS the public-api baseline + cinder minor-bump flag, addressed below |
| ADR-0065 | the trait-signature change, semver-MINOR (NEVER 1.0.0), failing-substrate seam via `open_with_fsync_backend`, External-integration handoff = none (no new metric/dashboard) | − ADR (and the DEVOPS brief) ASSUME Gate 2/Gate 3 fire on cinder; CI inspection shows cinder is NOT enrolled — corrected in the CI Contract finding below |
| `brief.md` cinder section | For-Acceptance-Designer driving ports; failing-substrate falsifiability requirement; stderr `error: cinder place: persistence failed: io: …` (D2); no new dashboard | − none |
| `discuss/outcome-kpis.md` | KPI-1..4 (swallow sites 4 -> 0, falsifiable per site); guardrails (healthy path unchanged, no torn memory, write-ahead, 100% mutation); handoff = no new runtime metric/dashboard | − none |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | − Gate 2/Gate 3 are enrolled for only 4 graduated packages; cinder/sluice are not among them (finding below) |
| `.github/workflows/ci.yml` | `gate-5-mutants-cinder` (:2249) + `gate-5-mutants-sluice` (:2584) + `gate-5-mutants-kaleidoscope-cli` (:1725) all exist, `--in-diff` path-filtered | − Gate 2 (:330-343) and Gate 3 (:423-433) list ONLY otlp-conformance-harness, spark, sieve, codex |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = Gate 4 + Gate 1 (the local mirror); pre-push = Gate 2/Gate 3 for the 4 graduated pkgs | − pre-push (lines 54, 77) confirms cinder/sluice absent from the public-api/semver package loop |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and no CI-config change is made by
this wave.** The feature modifies four existing source files
(`crates/cinder/src/store.rs`, `crates/cinder/src/file_backed.rs`,
`crates/sluice/src/queue.rs`, `crates/sluice/src/file_backed.rs`) plus two
thin CLI `Error` variants (`crates/kaleidoscope-cli/src/lib.rs`). Each of the
three touched crates already owns a path-filtered `gate-5-mutants-<crate>`
`--in-diff` job that mutates exactly its changed lines automatically.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production code, NO tests, and NO CI-config
changes exist yet for this feature. That absence is the EXPECTED and CORRECT
state — it is not a finding. This wave's job is to CONFIRM the existing
ADR-0005 CI contract covers the feature and to produce `environments.yaml` +
this file; review THAT, not the non-existence of code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers the change.

## Decision summary (D1-D9, all existing / inherited — brownfield, NOT a deploy)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | Library + CLI change to existing crates. Operators run the binary; Kaleidoscope deploys nothing. |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface. |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the modified cinder, sluice, and kaleidoscope-cli files automatically (Gate 5 via the existing per-crate `--in-diff` jobs). Gate 2/Gate 3 do NOT cover cinder/sluice — see CI Contract finding. No new gate. |
| D5 | Observability | **Existing convention — structured logging to STDERR** | D2 surfaces the previously-swallowed error to stderr (`error: cinder place: persistence failed: io: <reason>`), consistent with aperture / gateway / read-APIs / beacon, which all log to stderr. No new metric, no new dashboard, no new observability stack (ADR-0065 + KPI handoff). |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; the on-disk WAL/snapshot format is unchanged, so a revert reads existing data unchanged. |
| D7 | Continuous learning | **N/A** | No live telemetry loop; the KPIs are in-suite falsifiability + 100% mutation-kill (the K6 raw-observation idiom). |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5)** | Already pinned in CLAUDE.md. Mutation scope = the modified cinder files (`src/store.rs`, `src/file_backed.rs`), sluice (`src/queue.rs`, `src/file_backed.rs`), and the kaleidoscope-cli Error variants. Covered by the existing per-crate `--in-diff` jobs. **No CLAUDE.md change needed.** |

## CI Contract — confirmation and the ONE corrected finding

### Gate 5 (mutants, 100% kill) — CONFIRMED, no new job

| Touched crate / path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|----------------------|------------------------|---------------------|-------------|----------|
| `crates/cinder` (`src/store.rs`, `src/file_backed.rs`) | `TieringStore::{place, evaluate_at}` fallible + write-ahead-ordered; `InMemory` impl; `FileBacked` `?`-on-append | `gate-5-mutants-cinder` | 2249 | ✓ `--in-diff` on `crates/cinder/**` |
| `crates/sluice` (`src/queue.rs`, `src/file_backed.rs`) | `Queue::{dequeue, ack, nack}` fallible + write-ahead-ordered (D4, R3) | `gate-5-mutants-sluice` | 2584 | ✓ `--in-diff` on `crates/sluice/**` |
| `crates/kaleidoscope-cli` (`src/lib.rs`) | two thin `Error` variants (`CinderPlace`, `CinderEvaluate`); `flush`/`place`/`evaluate_policy` map_err+`?` | `gate-5-mutants-kaleidoscope-cli` | 1725 | ✓ `--in-diff` on `crates/kaleidoscope-cli/**` |

All three jobs run `cargo mutants --package <crate> --in-diff "$DIFF_FILE"`
against `git diff "$BASELINE" HEAD -- 'crates/<crate>/**'` (baseline cascade
`origin/main` → `HEAD~1` → full). The `--in-diff` filter means each job
mutates ONLY the lines this feature changes — the `?` on `append_wal` and the
append-before-apply ordering (ADR-0065's primary mutants). **No per-feature
wiring, no new gate-5 job.** This is the workspace-wide per-crate
`--in-diff` model `gate-5-mutants-batch-v0` completed; cinder and sluice were
both already enrolled at that close, so this feature inherits gating for free.

### Gate 2 (public-api) + Gate 3 (semver) — the ONE corrected finding

**The DEVOPS brief (and ADR-0065 §"Public-API + semver impact") ASSUME Gate 2
and Gate 3 fire on the cinder trait change. CI inspection shows they do NOT:**
Gate 2 (`cargo public-api`, ci.yml:330-343, 345-349) and Gate 3
(`cargo semver-checks`, ci.yml:423-433) are enrolled for ONLY the four
**graduated** packages — `otlp-conformance-harness`, `spark`, `sieve`,
`codex`. The local pre-push hook mirrors exactly that set
(`scripts/hooks/pre-push` lines 54 and 77:
`for pkg in otlp-conformance-harness spark sieve codex`). **cinder and sluice
are NOT enrolled in Gate 2 or Gate 3.**

Consequences (decided, not deferred):

1. The `TieringStore` / `Queue` trait-signature change IS a real public-API
   break, but it will **not be machine-flagged** — there is no `cargo
   public-api` baseline for cinder or sluice to trip, and `cargo semver-checks`
   never inspects them. The "Gate 2/Gate 3 WILL fire — expected and correct"
   line in ADR-0065/brief is aspirational, not the current CI reality.
2. Therefore **there is NO cinder (or sluice) public-api baseline/snapshot to
   update in DELIVER** — none exists to drift. The DESIGN handoff note "the
   public-api baseline/snapshot for cinder will need updating during DELIVER"
   is **superseded by this finding: no such baseline exists**, so the DELIVER
   task reduces to the manual version bump only.
3. The semver-MINOR posture stays correct as a **manual versioning discipline**
   (the only enforcement is the team's own ADR-0065 commitment, not CI):
   DELIVER bumps `crates/cinder/Cargo.toml` `version = "0.1.0"` → `"0.2.0"`
   and `crates/sluice/Cargo.toml` accordingly (pre-1.0, breaking-in-minor per
   Cargo semantics). **NEVER 1.0.0** — Andrea's call (CLAUDE.md / MEMORY); this
   wave does NOT authorise it.
4. **Decision: do NOT enrol cinder/sluice into Gate 2/Gate 3 in this wave.**
   Graduating a crate into the public-surface lock is a separate, deliberate
   decision (as ADR-0005 / the per-crate graduation notes in ci.yml show it was
   for spark/sieve/codex). Adding it here would be a speculative CI change on a
   pure-trunk-based repo with no required checks, for a crate that is still
   pre-1.0 and churning. If the team later wants the surface locked, that is its
   own DESIGN/DEVOPS decision. Flagged, not actioned.

### Gates 1 and 4 — CONFIRMED unchanged

- **Gate 1 (`cargo test --workspace --all-targets --locked`, ci.yml:184)** runs
  the failing-substrate surfacing tests (DISTILL authors them; DELIVER turns
  them green) and the ~1194-test guardrail suite, identically in the local
  pre-commit hook and CI. No change.
- **Gate 4 (`cargo deny`, ci.yml:114)** — no new dependency is introduced
  (ADR-0065 reuses `serde_json`, `wal-recovery`, `std::io`), so Gate 4 is a
  no-op confirmation.

## Infrastructure Summary

- **New infrastructure**: none. No crate, no container, no service, no cloud
  resource, no IaC, no orchestration.
- **CI changes**: none. The five ADR-0005 gates are inherited unchanged; the
  three relevant Gate 5 jobs (`cinder`, `sluice`, `kaleidoscope-cli`) already
  path-filter `--in-diff` onto the modified files.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for a
  library + CLI change, NOT deploy targets. See `environments.yaml`.
- **Failing-substrate test environment**: in-process io::Error injection
  through the EXISTING `open_with_fsync_backend` + `FsyncBackend` seam — a
  TEST concern, no infra, no host disk-fill (C5). Recorded in
  `environments.yaml > failing_substrate_test_environment`.
- **Observability**: structured logging to stderr (existing platform
  convention); no new metric, no new dashboard, no new stack.
- **Rollback**: `git revert` (trunk-based); on-disk WAL/snapshot format
  unchanged, so a revert reads existing data unchanged.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** The existing
  `gate-5-mutants-{cinder,sluice,kaleidoscope-cli}` jobs cover the modified
  files via `--in-diff`. DELIVER must NOT add a per-feature gate-5 job.
- **C-DEVOPS-2 — cinder/sluice are NOT in Gate 2/Gate 3.** The trait-signature
  break is real but unenforced by CI. **There is NO cinder/sluice public-api
  baseline to update in DELIVER** (this supersedes the DESIGN
  baseline-update-due-in-DELIVER flag — no such baseline exists). The only
  DELIVER versioning act is the MANUAL semver-MINOR bump of
  `crates/cinder/Cargo.toml` (0.1.0 → 0.2.0) and `crates/sluice/Cargo.toml`,
  pre-1.0, **NEVER 1.0.0** (Andrea's call).
- **C-DEVOPS-3 — Failing-substrate tests must be deterministic and run in BOTH
  the local pre-commit hook AND CI Gate 1.** In-process io::Error injection,
  presence/absence + memory==disk assertions, no wall-clock threshold — so the
  hook does not flake under overnight load.
- **C-DEVOPS-4 — Falsifiability is mandatory.** Each failure AC MUST fail on
  today's swallow bug and pass only on the surfaced-and-consistent fix (assert
  the error AND memory == disk after reopen). Do NOT inherit a test that
  cannot fail on the swallow (ADR-0060 §1 / ADR-0049 false-confidence lesson).
- **C-DEVOPS-5 — Guardrails must stay green.** Every healthy-`RealFsyncBackend`
  scenario and the ~1194-test graceful-restart durability suite must not
  regress; Gate 5 must reach 100% kill on the modified files.
- **C-DEVOPS-6 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy
  is already pinned (D9).

## Upstream Changes

**None expected.** DESIGN resolved D1-D4 into locked ACs for DISTILL; this
DEVOPS wave confirms the CI contract covers them and surfaces ONE correction
to a shared assumption (cinder/sluice are not in Gate 2/Gate 3), which changes
nothing for DISCUSS/DESIGN scope — it only sharpens the DELIVER versioning task
(manual bump, no baseline update). No story re-scoping; the carpaccio cut-line
(US-04/sluice in R3) is preserved.

## Production Readiness (scoped to a library + CLI change)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance tests defined for both surfaced paths (cinder place /
      evaluate_at, sluice dequeue/ack/nack) via the failing-substrate seam;
      DISTILL authors them, DELIVER turns them green.
- [x] Mutation gate (Gate 5, 100% kill) auto-covers every touched crate via the
      existing `--in-diff` jobs (CI Contract above).
- [x] Error surfaced through a typed Result and rendered to stderr (D2),
      consistent with the platform stderr-logging convention.
- [x] No new event / metric / dashboard (ADR-0065 + KPI handoff).
- [x] Rollback posture: `git revert`; on-disk WAL/snapshot format unchanged, so
      a revert reads existing data unchanged.
- [n/a] Canary / blue-green / rolling — no deployment surface.
- [n/a] On-call / runbook — operators run the binary; no Kaleidoscope-operated
      service. The stderr persistence-failure message is the operator-facing
      signal.

## Peer Review

The `nw-platform-architect-reviewer` Agent was invoked via the Task tool (see
`self-review.md` for the dispatch note and the reviewer's YAML verdict). The
dispatch carried the nWave-order reminder (no code/tests/CI exist at DEVOPS
time — that absence is expected, not a rejection reason). Findings were
addressed; the wave completes on reviewer **APPROVED**.

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (the existing per-crate
  `gate-5-mutants-*` jobs are untouched; trunk-based, no required checks).
- Does not enrol cinder/sluice into Gate 2/Gate 3 (a separate graduation
  decision; flagged, not actioned).
- Does not write production code or the surfacing/failing-substrate tests
  (crafter owns DELIVER; acceptance-designer owns the test specs in DISTILL).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not bump any `Cargo.toml` version (the manual semver-MINOR bump is a
  DELIVER act; this wave only flags it).
- Does not proceed into DISTILL.
