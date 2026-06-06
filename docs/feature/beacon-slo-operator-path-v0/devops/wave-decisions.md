# Wave Decisions — beacon-slo-operator-path-v0 (DEVOPS)

British English throughout, no em dashes.

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-06
- **Mode**: Autonomous overnight run. **SLIM** wave — an ADDITIVE, brownfield
  WIRING change to TWO EXISTING live crates (`beacon` loader + `beacon-server`);
  NO new crate, NO new dependency, NO deploy surface, NO new infrastructure, NO
  public-API break.
- **Inputs read**: `design/wave-decisions.md` (F1-F5 resolved; Reuse = EXTEND;
  the merge / validation / reload semantics),
  `docs/product/architecture/adr-0067-beacon-slo-operator-path.md`,
  `docs/product/architecture/brief.md` (§"Application Architecture —
  `beacon-slo-operator-path-v0`", incl. its DEVOPS handoff +
  For-Acceptance-Designer notes), `discuss/outcome-kpis.md` (KPI 1..5),
  ADR-0005 (the five workspace gates), `.github/workflows/ci.yml`,
  `scripts/hooks/{pre-commit,pre-push}`, `CLAUDE.md`, and the prior slim-DEVOPS
  shape (`aperture-serve-loop-error-surfacing-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | - (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | F1-F5 resolved (the `[[slo]]` schema + private `RawSlo` + `FileShape` defaulted field; refuse-on-collision merge; the two validations with exact messages; the expansion-aware count by construction; deliver the F5 cross-validation test); Reuse = EXTEND-only, net-new surface = one private `RawSlo` + `into_slo` + one `FileShape` field + five `BLESSED_FIELDS` entries + one duplicate-name scan; the test seam reuses the beacon-sighup-reload-v0 harness | - none; DESIGN explicitly confirms ADDITIVE / no-public-API-break and hands DEVOPS the mutation scope (the modified `loader.rs` / `slo.rs` / `main.rs`), addressed below |
| ADR-0067 | the WIRE decision (parse -> validate -> convert -> synthesise -> merge); the public-API + semver posture (additive; private wire shape; beacon not in Gate 2/3; pre-1.0, NEVER 1.0.0); the reconciliation of ADR-0036 (three corrections, an immutable-ADR correction NOTE handed to DELIVER); the reuse table; no external integration / no contract-test | - none; ADR-0067 states the no-break / no-bump posture, confirmed against the live source and the workflow below |
| `brief.md` beacon-slo section | the DEVOPS handoff (NO new infrastructure; inherits ADR-0005's five gates; mutation scope = modified `loader.rs` / `slo.rs`, beacon-server unchanged at the reload site; beacon not in Gate 2/3; semver additive-or-none, NEVER 1.0.0; trunk-based no-CI-gates); the For-Acceptance-Designer driving ports (`--rules` TOML + beacon-server binary + POSIX SIGHUP) and per-AC observables | - none |
| `discuss/outcome-kpis.md` | KPI 1 (one `[[slo]]` -> four loaded rules, 0% -> 100%), KPI 2 (target/budget typo refused at load, degenerate always-fire stays 0), KPI 3 (mixed-dir coexistence, silent shadowing stays 0), KPI 4 (SIGHUP edit applied / bad edit refused, partial-apply stays 0), KPI 5 (slo.rs:49-51 doc-lie 1 -> 0); the DEVOPS handoff: the `rules_loaded` + `beacon.reload.succeeded` / `.refused` events ALREADY exist, NO new instrumentation / dashboard / runtime alert is required | - none; the KPIs are categorical in-suite / mutation outcomes, not live telemetry — explicitly no pre-release baseline collection |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | - Gate 2/Gate 3 enrolled for only four graduated packages; beacon is not among them (consistent with no-break, finding below) |
| `.github/workflows/ci.yml` | `gate-5-mutants-beacon` (:1637-1723) AND `gate-5-mutants-beacon-server` (:2166-2247) both exist, both `--in-diff` path-filtered (`crates/beacon/**` and `crates/beacon-server/**`); Gate 1 (:136-184); Gate 4 (:83-114) | - Gate 2 / Gate 3 list ONLY otlp-conformance-harness, spark, sieve, codex; neither beacon nor beacon-server is enrolled (noted, not a blocker) |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = fmt + clippy + Gate 4 + Gate 1 (the local mirror, runs the SLO acceptance tests + slice_05 + rule-load negative controls); pre-push = Gate 2 / Gate 3 for the four graduated pkgs | - pre-push (lines 54, 77) confirms beacon / beacon-server absent from the public-api / semver package loop (consistent with additive / no-break) |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and no CI-config change is made by
this wave.** The feature modifies the existing `beacon` library loader
(`crates/beacon/src/loader.rs`: a new private `RawSlo`, its `into_slo`
validation/conversion, the `FileShape` defaulted `slo` field, five
`BLESSED_FIELDS` entries, one duplicate-name scan) and `slo.rs` (DOC-COMMENT
lines only; the `synthesise_slo` engine is untouched). `beacon-server` is
UNCHANGED at the reload site (SLO support falls out of the shared `load_rules`
re-read). beacon AND beacon-server each already own a path-filtered, `--in-diff`
Gate 5 mutation job that mutates exactly their changed lines automatically.

**Confirmed against the live source**: `gate-5-mutants-beacon`
(ci.yml:1637-1723) and `gate-5-mutants-beacon-server` (ci.yml:2166-2247) both
exist and both `--in-diff` path-filter (`crates/beacon/**`,
`crates/beacon-server/**`). `crates/beacon/Cargo.toml:3` and
`crates/beacon-server/Cargo.toml:3` are both `version = "0.1.0"`. The reload
events `beacon.reload.succeeded` / `beacon.reload.refused` and the startup
`rules_loaded` field are present (main.rs:94, 307-410). The
beacon-sighup-reload-v0 harness exists (`crates/beacon-server/tests/sighup_reload.rs`).
The four graduated Gate 2/Gate 3 packages are `otlp-conformance-harness spark
sieve codex` (pre-push lines 54, 77); beacon is not among them.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production code, NO tests, and NO CI-config
changes exist yet for this feature. That absence is the EXPECTED and CORRECT
state — it is not a finding. This wave's job is to CONFIRM the existing ADR-0005
CI contract covers the feature and to produce `environments.yaml` + this file;
review THAT, not the non-existence of code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers the change.

## Decision summary (D1-D9, all existing / inherited — brownfield wiring, NOT a deploy)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | Additive wiring in a live library (`beacon`) + a live binary (`beacon-server`). beacon-server is the operator-run / orchestrator-run alert daemon; Kaleidoscope deploys nothing. No deploy step is added or required. |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface added by this wave. |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the modified beacon / beacon-server files automatically (Gate 5 via the existing `gate-5-mutants-beacon` and `gate-5-mutants-beacon-server` `--in-diff` jobs). Gate 2/Gate 3 do NOT cover beacon — see CI Contract finding (consistent with no-break). No new gate. |
| D5 | Observability | **Existing — REUSES the beacon reload + startup events, no new stream, no new field** | The feature reuses the EXISTING startup `rules_loaded` field (main.rs:94, now counting the four synthesised rules per SLO) and the EXISTING SIGHUP `beacon.reload.succeeded` / `beacon.reload.refused` events with EXPANSION-AWARE counts BY CONSTRUCTION (added = 4 for one new SLO, main.rs:338-340, 408-410; refusal via the existing `broken_edit_added_nothing` guard, main.rs:343). NO new event field (a dedicated `slos_added` was REJECTED in DESIGN F4 as redundant — the `slo_service` label already groups the four rules). NO new metric, NO new dashboard, NO new observability stack, NO runtime alerting threshold (outcome-kpis.md DEVOPS note: the guardrails are enforced by acceptance tests + the 100% mutation gate, not runtime alerts). |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; the change is additive to the rule-file FORMAT (a file may now hold `[[slo]]` alongside `[[rules]]`), and a revert restores the prior loader (after which an `[[slo]]` file is once again refused — the prior honest poisons-its-file behaviour, not silent loss). No WAL / snapshot / durable-state format change; the reload event shapes are unchanged. |
| D7 | Continuous learning | **N/A** | No live telemetry loop; the KPIs are in-suite acceptance + 100% mutation-kill (the K6 raw-observation idiom). |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5 / CLAUDE.md)** | Already pinned in CLAUDE.md (per-feature, scoped to modified files, 100% kill gate). Mutation scope = the modified `crates/beacon/src/loader.rs` (`RawSlo`, `into_slo`, the `FileShape` `slo` field, the `BLESSED_FIELDS` additions, the duplicate-name scan) and `crates/beacon/src/slo.rs` (doc-comment lines only); `crates/beacon-server/src/main.rs` is covered if touched (no reload-logic change expected). Covered by the existing `gate-5-mutants-beacon` and `gate-5-mutants-beacon-server` `--in-diff` jobs. **No CLAUDE.md change needed.** |

## CI Contract — confirmation and findings

### Gate 5 (mutants, 100% kill) — CONFIRMED, no new job

| Touched path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|--------------|------------------------|---------------------|-------------|----------|
| `crates/beacon/src/loader.rs` | new private `RawSlo` (`deny_unknown_fields`); `RawSlo::into_slo` (the two validations: `target_availability` strictly in (0,1); `error_budget_period == 30d`); the `FileShape` `#[serde(default)] slo: Vec<RawSlo>` field; five `BLESSED_FIELDS` SLO-key entries; the merge second-pass + the duplicate-name collision scan | `gate-5-mutants-beacon` | 1637-1723 | yes — `--in-diff` on `crates/beacon/**` |
| `crates/beacon/src/slo.rs` | doc-comment fixes ONLY (slo.rs:49-51 made true; slo.rs:24-26 backed by the F5 test); the `synthesise_slo` / `MWMBR_TABLE` / `Slo` engine is UNTOUCHED | `gate-5-mutants-beacon` | 1637-1723 | yes — same job, same glob |
| `crates/beacon-server/src/main.rs` | none expected (reload reuses the shared `load_rules`; expansion-aware counts fall out by construction); covered if any line is touched | `gate-5-mutants-beacon-server` | 2166-2247 | yes — `--in-diff` on `crates/beacon-server/**` |

`gate-5-mutants-beacon` runs `cargo mutants --package beacon --in-diff
"$DIFF_FILE"` against `git diff "$BASELINE" HEAD -- 'crates/beacon/**'`;
`gate-5-mutants-beacon-server` runs the same against `crates/beacon-server/**`.
Both use the baseline cascade `origin/main` -> `HEAD~1` -> full and short-circuit
to a zero-second exit on an empty diff. The `--in-diff` filter means each job
mutates ONLY the lines this feature changes — a mutant weakening the `(0,1)`
target check, the `== 30d` budget check, the duplicate-name scan, the
`FileShape` field default, or a `BLESSED_FIELDS` entry must be killed by the SLO
load / validation / collision acceptance tests; a mutant at any touched
beacon-server line by the reload acceptance tests (KPI 1..5). **No per-feature
wiring, no new gate-5 job.** beacon was enrolled in the per-crate `--in-diff`
model by `beacon-durable-alert-state-v0` DELIVER (recorded in the
`gate-5-mutants-beacon` job comment, ci.yml:1679-1684), and `gate-5-mutants-beacon-server`
was added alongside; this feature inherits both for free. The recollection that
`beacon-sighup-reload-v0` / `store-fsync-durability` touched beacon /
beacon-server is consistent with the jobs already existing.

### Gate 2 (public-api) + Gate 3 (semver) — CONFIRMED: do NOT fire, NO semver bump (the load-bearing confirmation)

**DESIGN confirmed the change is ADDITIVE with NO public-API break, and CI
inspection confirms beacon / beacon-server are not even enrolled in Gate 2 /
Gate 3 — the two facts agree.**

1. **beacon is NOT enrolled.** Gate 2 (`cargo public-api`) and Gate 3 (`cargo
   semver-checks`) are enrolled for ONLY the four **graduated** packages —
   `otlp-conformance-harness`, `spark`, `sieve`, `codex`. The local pre-push hook
   mirrors exactly that set (`scripts/hooks/pre-push` lines 54, 77:
   `for pkg in otlp-conformance-harness spark sieve codex`). Neither beacon nor
   beacon-server is in the loop.
2. **And there is no break to flag anyway.** Per ADR-0067 §"Public-API and semver
   posture" and `design/wave-decisions.md`: the net-new surface is PRIVATE —
   `RawSlo` and `RawSlo::into_slo` are private to `loader.rs` (like the existing
   `RawRule` / `into_rule`); `FileShape` gains a `#[serde(default)] slo` field
   (rules-only files parse byte-identically); the public `Rule`, `LoadOutcome`,
   `load_rules`, `synthesise_slo`, and `Slo` surfaces are UNCHANGED. **No public
   type is introduced or changed.** So even if beacon WERE enrolled, Gate 2 /
   Gate 3 would find no diff.
3. **Therefore NO semver bump is needed. beacon and beacon-server stay `0.1.0`**
   (verified: `crates/beacon/Cargo.toml:3`, `crates/beacon-server/Cargo.toml:3`).
   This is the explicit CONTRAST with a genuine signature break (e.g.
   `cinder-wal-error-surfacing-v0`, which required a manual `0.1.0 -> 0.2.0`
   bump): beacon's ripple is entirely additive and behind the loader boundary.
   DELIVER must NOT bump `crates/beacon/Cargo.toml` or
   `crates/beacon-server/Cargo.toml`. (Were a wire-format-capability minor bump
   ever wanted, it would be semver-MINOR at most, pre-1.0, **NEVER 1.0.0** —
   Andrea's call; out of scope here.)
4. **Decision: do NOT enrol beacon into Gate 2 / Gate 3 in this wave.** Graduating
   a crate into the public-surface lock is a separate, deliberate decision (as it
   was for spark / sieve / codex). Flagged, not actioned.

### Gates 1 and 4 — CONFIRMED unchanged

- **Gate 1 (`cargo test --workspace --all-targets --locked`, ci.yml:136-184)**
  runs the SLO load / validate / merge acceptance tests and the SIGHUP reload
  acceptance tests (DISTILL authors them; DELIVER turns them green) plus the
  regression guardrails — `slice_05_slo_burn_rate.rs` (20/20) and the existing
  beacon-server rule tests — identically in the local pre-commit hook and CI. No
  change.
- **Gate 4 (`cargo deny --all-features check`, ci.yml:83-114)** — no new
  dependency is introduced (ADR-0067 reuses `serde`, `toml`, the existing
  `humantime` `parse_duration`, and the existing `RawSink`; the pins are
  untouched), so Gate 4 is a no-op confirmation.

## Infrastructure Summary

- **New infrastructure**: none. No crate, no binary, no container, no service, no
  cloud resource, no IaC, no orchestration, no new dependency.
- **CI changes**: none. The five ADR-0005 gates are inherited unchanged; the two
  relevant Gate 5 jobs (`gate-5-mutants-beacon`, `gate-5-mutants-beacon-server`)
  already path-filter `--in-diff` onto the modified beacon / beacon-server files.
  No new job, no edit to an existing job.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for an
  additive single-loader wiring change, NOT deploy targets. See
  `environments.yaml`.
- **SLO / SIGHUP-reload test environment**: real on-disk `[[slo]]` (and mixed
  `[[rules]]` + `[[slo]]`) TOML in a temp dir + the real `load_rules` + a real
  `beacon-server` child process with a backend stub driven by a real POSIX
  `kill -HUP` — a TEST concern, IN-PROCESS / real-temp-file, REUSING the
  beacon-sighup-reload-v0 harness (`crates/beacon-server/tests/sighup_reload.rs`),
  NO infra. Recorded in `environments.yaml > slo_reload_test_environment`.
- **Observability**: REUSES the existing `rules_loaded` startup field and the
  `beacon.reload.succeeded` / `beacon.reload.refused` events (expansion-aware
  counts by construction); no new event stream, no new field, no new metric, no
  new dashboard, no new stack, no runtime alert.
- **Rollback**: `git revert` (trunk-based); the change is additive to the
  rule-file format and touches no durable-state / WAL / snapshot format, so a
  revert restores the prior loader (an `[[slo]]` file becomes once again refused —
  the prior honest behaviour) with no data or wire-format consideration.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** The existing
  `gate-5-mutants-beacon` (ci.yml:1637-1723) and `gate-5-mutants-beacon-server`
  (ci.yml:2166-2247) jobs cover the modified files via `--in-diff`. DELIVER must
  NOT add a per-feature gate-5 job.
- **C-DEVOPS-2 — ADDITIVE, NO public-API break, NO semver bump.** The net-new
  surface is private (`RawSlo` / `into_slo`) plus a defaulted `FileShape` field;
  the public `Rule` / `LoadOutcome` / `load_rules` / `synthesise_slo` / `Slo`
  surfaces are unchanged. beacon / beacon-server are NOT enrolled in Gate 2 /
  Gate 3, AND there is no break to flag anyway. DELIVER must NOT bump
  `crates/beacon/Cargo.toml` or `crates/beacon-server/Cargo.toml` (both stay
  `0.1.0`) and there is NO public-api baseline to update (none exists for
  beacon). NEVER 1.0.0 — Andrea's call; out of scope.
- **C-DEVOPS-3 — SLO + SIGHUP-reload tests must be deterministic and run in BOTH
  the local pre-commit hook AND CI Gate 1.** Real temp-file load + a real
  `beacon-server` + a real POSIX SIGHUP, reusing the beacon-sighup-reload-v0
  harness; categorical assertions on named rules / refusal diagnostics /
  collision diagnostics / `beacon.reload.succeeded` vs `.refused` / firing-vs-not,
  NO wall-clock threshold — so the hook does not flake under overnight load (the
  p95-flake class does NOT apply; these are categorical, not p95 latency).
- **C-DEVOPS-4 — Falsifiability is mandatory.** Each AC MUST fail on today's
  poisons-its-file before state (an `[[slo]]` table makes `toml::from_str` fail
  "unknown field `slo`", skipping the whole file; no validation exists at
  slo.rs:114; no SLO reload path) and pass ONLY on the wired-validated-merged fix.
  Do NOT inherit an SLO test that passes on the before state.
- **C-DEVOPS-5 — Guardrails must stay green.** The engine's existing
  `slice_05_slo_burn_rate.rs` (20/20) and the existing beacon-server rule tests
  must not regress, proving the rules-only path is byte-identical (the new
  `FileShape` `slo` vector defaults empty); a mixed dir loads both without
  dropping or shadowing the hand-authored rules (KPI 3); the F5 within-budget 24h
  trace MUST NOT fire any rule; a firing synthesised rule survives an unrelated
  SLO edit by stable name and does NOT re-page (US-05); Gate 5 must reach 100%
  kill on the modified `loader.rs` / `slo.rs` lines (CLAUDE.md / ADR-0005 Gate 5).
- **C-DEVOPS-6 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy is
  already pinned (D9).
- **C-DEVOPS-7 — The ADR-0036 "Corrected by ADR-0067" annotation is a DELIVER
  doc act, NOT a DEVOPS concern.** ADRs are immutable; DELIVER appends the
  three-reconciliation correction note (FOUR rules not five; `slo_source` LABEL
  not an `annotations` field; the Rust TOML loader not a CUE schema) to ADR-0036.
  This wave neither performs nor blocks on that doc edit; it is recorded here so
  DELIVER does not lose it.

## Upstream Changes

**None expected.** DESIGN resolved F1-F5 into locked ACs for DISTILL; this
DEVOPS wave CONFIRMS (rather than corrects) the existing ADR-0005 CI contract
covers them and that the additive / no-public-API-break / no-semver-bump posture
holds against the live source and the workflow. No shared assumption needed
correcting; ADR-0067 and the brief already state the posture and CI inspection
agrees. No story re-scoping; no DISCUSS/DESIGN delta. **Note** the ADR-0036
"Corrected by ADR-0067" annotation handed to DELIVER (three reconciliations) is
a DELIVER documentation act, NOT a DEVOPS upstream change (C-DEVOPS-7).

## Production Readiness (scoped to an additive, library-wiring change, no deploy)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance tests defined for SLO load (four named rules), validation refusal
      (target / budget), mixed-dir coexistence + collision, and SIGHUP reload
      (succeeded / refused) via the reused beacon-sighup-reload-v0 harness; DISTILL
      authors them, DELIVER turns them green (KPI 1..5).
- [x] Mutation gate (Gate 5, 100% kill) auto-covers the modified `loader.rs` /
      `slo.rs` via `gate-5-mutants-beacon --in-diff`, and any touched
      beacon-server line via `gate-5-mutants-beacon-server --in-diff`.
- [x] Operator signals reuse the EXISTING channels (D5): the `rules_loaded`
      startup field (expansion-aware), the `beacon.reload.succeeded` /
      `beacon.reload.refused` events (expansion-aware counts by construction); no
      new field, no new event stream.
- [x] No new metric / dashboard / observability stack / runtime alert
      (outcome-kpis.md DEVOPS note); the guardrails (always-fire = 0, silent
      shadowing = 0, partial apply = 0) are enforced by acceptance tests + the
      100% mutation gate.
- [x] Rollback posture: `git revert`; additive rule-file format, no durable-state
      / WAL / snapshot format change, so a revert is clean (an `[[slo]]` file
      becomes once again refused — the prior honest behaviour).
- [x] ADDITIVE / no-public-API-break / no-semver-bump confirmed against the live
      source and the workflow (beacon + beacon-server stay 0.1.0).
- [n/a] Canary / blue-green / rolling — no deployment surface.
- [n/a] On-call / runbook — operators run / orchestrate beacon-server; the
      existing reload events + the refusal diagnostics ARE the operator-facing
      signals.

## Peer Review

The `nw-platform-architect-reviewer` Agent could not be invoked as a nested
subagent from within this subagent context (the identical constraint was
recorded for the prior slim-DEVOPS features, e.g.
`aperture-serve-loop-error-surfacing-v0/devops/wave-decisions.md` §"Peer
Review"). Per the established slim-DEVOPS precedent on this project, a structured
self-review was conducted against the reviewer's exact dimensions (external
validity -> evidence-based findings -> severity-driven -> DORA -> handoff
completeness); see "Self-Review" below. The dispatch carried the nWave-order
reminder (no code / tests / CI exist at DEVOPS time — that absence is expected,
not a rejection reason). Verdict: **APPROVED_PENDING_INDEPENDENT_REVIEW**, 0
blocking issues. An independent top-level `nw-platform-architect-reviewer` run is
recommended before DISTILL.

### Self-Review (against the platform reviewer's dimensions)

```yaml
reviewer: nw-platform-architect-reviewer (self-review, nested-invocation unavailable)
feature: beacon-slo-operator-path-v0
wave: devops
mode: slim
nwave_order_reminder_applied: true   # DEVOPS precedes DISTILL/DELIVER; absence of code/tests/CI is expected, not a finding
verdict: APPROVED_PENDING_INDEPENDENT_REVIEW
blocking_issues: 0
dimensions:
  external_validity:
    status: pass
    note: >
      Every CI claim is grounded in a verified file:line — gate-5-mutants-beacon
      (ci.yml:1637-1723), gate-5-mutants-beacon-server (ci.yml:2166-2247), the
      four graduated Gate 2/3 pkgs (pre-push:54,77), both crate versions 0.1.0
      (Cargo.toml:3 each), the reload events + rules_loaded (main.rs:94,307-410),
      the harness (tests/sighup_reload.rs). No claim rests on the ADR's prose
      alone; the probe was run.
  evidence_based_findings:
    status: pass
    note: >
      The load-bearing no-new-job / no-semver-bump confirmations are each backed
      by an inspected artefact, not asserted. The mutation-scope-to-job mapping is
      tabulated per touched path with the covering glob.
  severity_driven:
    status: pass
    note: 0 blocking, 0 high. Two NOTED items (beacon not in Gate 2/3; the ADR-0036 correction is a DELIVER doc act) are flagged-not-actioned, correctly.
  dora_alignment:
    status: pass
    note: >
      Trunk-based, change is feedback-gated not merge-gated (project memory). The
      additive wiring lowers lead-time risk (no new pipeline, no deploy). Change
      failure rate is guarded by the 100% mutation gate + the byte-identical
      rules-only regression guardrail. No DORA regression.
  handoff_completeness:
    status: pass
    note: >
      Seven C-DEVOPS constraints handed to DISTILL/DELIVER, incl. the falsifiability
      requirement (must fail on the poisons-its-file before state), the guardrail
      set, the no-bump instruction, and the ADR-0036 correction reminder. The test
      seam (reuse the sighup harness, black-box the real `_slo_` names) is recorded.
  simplest_solution:
    status: pass
    note: >
      No infrastructure proposed; the wave confirms existing gates rather than
      adding any. The DESIGN-rejected dedicated *.slo.toml / separate --slos dir
      and the rejected slos_added event field are honoured (no new surface).
```

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (the existing
  `gate-5-mutants-beacon` and `gate-5-mutants-beacon-server` jobs are untouched;
  trunk-based, no required checks).
- Does not enrol beacon / beacon-server into Gate 2 / Gate 3 (a separate
  graduation decision; flagged, not actioned).
- Does not write production code or the SLO / reload acceptance tests (crafter
  owns DELIVER; acceptance-designer owns the test specs in DISTILL).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not bump any `Cargo.toml` version (beacon + beacon-server stay 0.1.0 — NO
  break, NO bump, NEVER 1.0.0).
- Does not perform the ADR-0036 "Corrected by ADR-0067" annotation (a DELIVER doc
  act; recorded for DELIVER, not done here).
- Does not proceed into DISTILL.
