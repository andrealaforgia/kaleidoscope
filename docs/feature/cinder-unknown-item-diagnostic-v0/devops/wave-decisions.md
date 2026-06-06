# Wave Decisions — cinder-unknown-item-diagnostic-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-06
- **Mode**: Autonomous overnight run. **SLIM** wave — the slimmest possible:
  a ONE-ARM Display-string fidelity fix to an existing crate (cinder),
  inherited by kaleidoscope-cli with zero CLI-side edits, plus new black-box
  CLI test assertions. NO new crate, NO deploy surface, NO new infrastructure,
  NO new dependency, NO new observability, NO semver bump.
- **Inputs read**: `design/wave-decisions.md` (Decisions 1-5 resolved; the
  one-arm fix, no new ADR, no semver), `discuss/outcome-kpis.md` (KPI-1/KPI-2
  + guardrails), `docs/product/architecture/brief.md` (§"Application
  Architecture — cinder-unknown-item-diagnostic-v0", the brief note + its
  For-Acceptance-Designer + Handoff-to-DEVOPS paragraphs),
  `docs/product/architecture/adr-0005-ci-contract.md` (the five gates),
  `.github/workflows/ci.yml`, `scripts/hooks/{pre-commit,pre-push}`,
  `crates/cinder/src/store.rs:55-58`, `crates/cinder/Cargo.toml`,
  `crates/kaleidoscope-cli/Cargo.toml`, `CLAUDE.md`, and the prior slim-DEVOPS
  shape (`cinder-wal-error-surfacing-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | D1 (render `{:?}` on `item.as_str()`); D2 (Display-impl-on-ItemId REJECTED — wider blast radius); D3 (get-tier shares the same arm, no separate fix); D4 (no new ADR); D5 (no semver bump, NEVER 1.0.0); reuse verdict = EXTEND one arm + REUSE `ItemId::as_str()`, nothing new — all consumed | − none. DESIGN's DEVOPS handoff is explicit: inherits the five gates, Gate 2/3 do not fire, mutation scope = the single line. Confirmed below. |
| `discuss/outcome-kpis.md` | North Star = zero `ItemId(` leaks in the unknown-item stderr; Leading = byte-equal-to-help quoted id + verifier K18 (UC-TIER-008/009) GREEN; Guardrails = exit-1 unchanged, known-item stdout unchanged, no other diagnostic changes, five gates GREEN, 100% mutation kill | − none. The KPIs are in-suite acceptance + mutation outcomes (K6 idiom), not live telemetry — matches the no-new-observability posture. |
| `brief.md` cinder note | the one-line decision; the For-Acceptance-Designer subprocess test seam (both subcommands through the shared arm); Handoff-to-DEVOPS = inherits ADR-0005's five gates, Gate 2/3 do NOT fire, no semver, mutation scoped to `store.rs:57`, no new ADR, no external integrations | − none. The handoff and this wave's CI inspection agree exactly. |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | − Gate 2/Gate 3 enrol only the four graduated packages; cinder/kaleidoscope-cli are not among them (confirmed, consistent with DESIGN handoff). |
| `.github/workflows/ci.yml` | `gate-5-mutants-cinder` (:2249, `--in-diff` on `crates/cinder/**`) and `gate-5-mutants-kaleidoscope-cli` (:1725, `--in-diff` on `crates/kaleidoscope-cli/**`) both exist; Gate 2 (~330) and Gate 3 (~423) list ONLY otlp-conformance-harness/spark/sieve/codex | − none new; the DESIGN handoff assumption (Gate 2/3 do not fire) is CONFIRMED by direct inspection, not corrected. |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = Gate 4 + Gate 1 (local mirror of the CI commit stage); pre-push loops `for pkg in otlp-conformance-harness spark sieve codex` (lines 54, 77) for Gate 2/3 | − confirms cinder/kaleidoscope-cli absent from the local public-api/semver loop too — no local pre-push diff from a Display-arm change. |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and no CI-config change is made by
this wave.** The feature changes ONE line —
`crates/cinder/src/store.rs:57` — turning the leaked `{item:?}`
(Debug-of-`ItemId`-newtype → `ItemId("ghost")`) into `{:?}` on
`item.as_str()` (Debug-of-`&str` → quoted `"ghost"`), and adds black-box CLI
subprocess assertions under `crates/kaleidoscope-cli/tests/`. Both touched
crates (cinder, kaleidoscope-cli) already own a path-filtered
`gate-5-mutants-<crate>` `--in-diff` job that mutates exactly their changed
lines automatically.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production code, NO tests, and NO CI-config
changes exist yet for this feature. That absence is the EXPECTED and CORRECT
state — it is NOT a finding and NOT a rejection reason. This wave's job is to
CONFIRM the existing ADR-0005 CI contract covers a one-line message fix and to
produce `environments.yaml` + this file; review THAT, not the non-existence of
code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers the change.

## Decision summary (D1-D9, all existing / inherited — brownfield, NOT a deploy)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | Library message-text change to an existing crate. Operators run the binary; Kaleidoscope deploys nothing. |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface. |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the modified cinder line and the new CLI tests automatically (Gate 5 via the existing per-crate `--in-diff` jobs). Gate 2/Gate 3 do NOT cover cinder/kaleidoscope-cli — see CI Contract. No new gate. |
| D5 | Observability | **Existing convention — structured logging to STDERR** | The unknown-item diagnostic already rides stderr via `Error::CinderMigrate` Display, non-zero exit. The fix changes ONLY the rendered id token (leaked `ItemId("ghost")` → quoted `"ghost"`). No new metric, no new dashboard, no new stack. |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; the change is message TEXT only, no on-disk format/data is touched, so a revert is zero-implication. |
| D7 | Continuous learning | **N/A** | No live telemetry loop; the KPIs are in-suite acceptance assertions + 100% mutation-kill on the modified line (the K6 raw-observation idiom). |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5)** | Already pinned in CLAUDE.md. Mutation scope = the **single modified line** `crates/cinder/src/store.rs:57` (plus any new CLI test sites). Covered by the existing `gate-5-mutants-cinder` and `gate-5-mutants-kaleidoscope-cli` `--in-diff` jobs. **No CLAUDE.md change needed.** |

## CI Contract — confirmation (no correction needed; matches DESIGN handoff)

### Gate 5 (mutants, 100% kill) — CONFIRMED, no new job

| Touched crate / path | Change in this feature | Existing gate-5 job | ci.yml line | Path filter | Verified |
|----------------------|------------------------|---------------------|-------------|-------------|----------|
| `crates/cinder` (`src/store.rs:57`) | the one Display arm: `{item:?}` (Debug-of-newtype) → `{:?}` on `item.as_str()` (Debug-of-`&str`, quoted) | `gate-5-mutants-cinder` | 2249 | `git diff … -- 'crates/cinder/**'` | ✓ `--package cinder --in-diff "$DIFF_FILE"` (ci.yml:2311-2315) |
| `crates/kaleidoscope-cli` (`tests/…`) | new black-box subprocess assertions (must-contain quoted id; must-NOT-contain `ItemId(`; exit non-zero) for migrate + get-tier | `gate-5-mutants-kaleidoscope-cli` | 1725 | `git diff … -- 'crates/kaleidoscope-cli/**'` | ✓ `--package kaleidoscope-cli --in-diff "$DIFF_FILE"` (ci.yml:1787-1791) |

Both jobs run `cargo mutants --package <crate> --in-diff "$DIFF_FILE"` against
`git diff "$BASELINE" HEAD -- 'crates/<crate>/**'` (baseline cascade
`origin/main` → `HEAD~1` → full). The `--in-diff` filter means each job
mutates ONLY the lines this feature changes — the single Display-arm line in
cinder (ADR-0005 Gate 5's target) and the new test sites in kaleidoscope-cli.
**The mutant that reverts the placeholder to `{item:?}` is killed by the new
must-contain-quoted-id / must-NOT-contain-`ItemId(` assertion pair.** No
per-feature wiring, no new gate-5 job. The two crates were both already
enrolled at the `gate-5-mutants-batch-v0` close, so this feature inherits
gating for free.

### Gate 2 (public-api) + Gate 3 (semver) — do NOT fire (confirmed, matches DESIGN)

Gate 2 (`cargo public-api`, ci.yml ~330) and Gate 3 (`cargo semver-checks`,
ci.yml ~423) are enrolled for ONLY the four **graduated** packages —
`otlp-conformance-harness`, `spark`, `sieve`, `codex`. The local pre-push hook
mirrors exactly that set (`scripts/hooks/pre-push` lines 54 and 77:
`for pkg in otlp-conformance-harness spark sieve codex`). **cinder and
kaleidoscope-cli are NOT enrolled in Gate 2 or Gate 3** — so NO public-api gate
fires for this feature.

Independently of enrollment: **a private `Display`-arm string change is NOT a
public-API change.** No type, trait, or function signature changes; the public
surface of `MigrateError` is unchanged (same variants, same fields, same
`Display`/`Error` impls). `Display` output is documentation/behaviour, not API
signature. So even if cinder were enrolled, Gate 2/Gate 3 would not flag it.

### Gates 1 and 4 — CONFIRMED unchanged

- **Gate 1 (`cargo test --workspace --all-targets --locked`, ci.yml:184)** runs
  the new black-box CLI subprocess assertions (DISTILL authors them; DELIVER
  turns them green) plus the existing guardrail suite, identically in the local
  pre-commit hook and CI. No change.
- **Gate 4 (`cargo deny`)** — no new dependency is introduced (the fix reuses
  the existing `ItemId::as_str()` and `std::fmt`), so Gate 4 is a no-op
  confirmation.

## Infrastructure Summary

- **Inherits the five ADR-0005 gates UNCHANGED.** No new infrastructure: no
  crate, no container, no service, no cloud resource, no IaC, no orchestration.
- **CI changes**: none. The two relevant Gate 5 jobs (`cinder`,
  `kaleidoscope-cli`) already path-filter `--in-diff` onto the modified line
  and the new tests. Gate 1/Gate 4 inherited as-is; Gate 2/Gate 3 do not cover
  these crates (and would not flag a Display-arm string anyway).
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for a
  library message fix, NOT deploy targets. See `environments.yaml`.
- **Acceptance test environment**: a CLI **subprocess** (black-box) test —
  spawn the built kaleidoscope-cli binary, assert on captured stderr/exit for
  both `migrate` and `get-tier` through the shared arm. A TEST concern, no
  infra. Recorded in `environments.yaml > acceptance_test_environment`.
- **Observability**: structured logging to stderr (existing platform
  convention); only the rendered id token changes. No new metric, no new
  dashboard, no new stack.
- **Rollback**: `git revert`; message-text-only change, no on-disk format/data
  touched, so a revert is zero-implication.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** The existing
  `gate-5-mutants-cinder` (ci.yml:2249) and `gate-5-mutants-kaleidoscope-cli`
  (ci.yml:1725) jobs cover the modified line and the new tests via `--in-diff`.
  DELIVER must NOT add a per-feature gate-5 job (do NOT add speculatively).
- **C-DEVOPS-2 — No semver bump; NEVER 1.0.0.** A private `Display`-arm string
  is not a public-API break; cinder/kaleidoscope-cli are not in Gate 2/Gate 3
  and there is NO public-api baseline to update. **cinder stays at 0.2.0,
  kaleidoscope-cli stays at 0.1.0.** Bumping to 1.0.0 is Andrea's call and is
  NOT authorised by this wave (CLAUDE.md / MEMORY).
- **C-DEVOPS-3 — Acceptance assertions must be deterministic and run in BOTH
  the local pre-commit hook AND CI Gate 1.** Boolean substring presence/absence
  on captured stderr + an exit-code check, NO wall-clock threshold — so the
  hook does not flake under overnight load (the p95-flake class does NOT apply).
- **C-DEVOPS-4 — Falsifiability is mandatory.** The new assertion pair MUST fail
  on today's leak (`ItemId("ghost")` emitted) and pass ONLY on the fix (quoted
  `"ghost"`, no `ItemId(`). Do NOT weaken the existing substring test
  (`migrate_subcommand.rs:309-324`, which stays green under both wordings and
  never caught the leak); ADD the two new assertions — that is the gap (the
  ADR-0060 §1 / ADR-0049 false-confidence guard).
- **C-DEVOPS-5 — Guardrails must stay green.** Exit-1 on unknown item is
  UNCHANGED (only wording changes); the known-item success path stdout is
  UNCHANGED; no OTHER cinder diagnostic message changes; Gate 5 must reach 100%
  kill on the modified line.
- **C-DEVOPS-6 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy
  is already pinned (D9).

## Upstream Changes

**None.** No upstream crate, port, trait, dependency, or config change. The fix
is contained in one existing `Display` arm in `crates/cinder/src/store.rs` and
is inherited by `kaleidoscope-cli` with zero CLI-side edits. No new ADR, no new
task, no new crate. This DEVOPS wave confirms the existing ADR-0005 CI contract
covers the change and produces `environments.yaml` + this file; it surfaces NO
correction (the DESIGN handoff assumption that Gate 2/3 do not fire is
CONFIRMED by direct inspection). No DISCUSS/DESIGN re-scoping.

## Production Readiness (scoped to a library message fix)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance assertions defined for both arms (migrate + get-tier) via the
      CLI subprocess seam; DISTILL authors them, DELIVER turns them green.
- [x] Mutation gate (Gate 5, 100% kill) auto-covers the modified line and the
      new tests via the existing `--in-diff` jobs (CI Contract above).
- [x] The diagnostic surfaces through a typed Result rendered to stderr
      (existing), with only the id token changing to the contract-faithful form.
- [x] No new event / metric / dashboard.
- [x] Rollback posture: `git revert`; message-text-only, no data implication.
- [n/a] Canary / blue-green / rolling — no deployment surface.
- [n/a] On-call / runbook — operators run the binary; the stderr unknown-item
      message is the operator-facing signal (now contract-faithful).

## Peer Review

A structured self-review against the `nw-platform-architect-reviewer` (Forge)
DEVOPS dimensions was conducted (see `self-review.md`). The
`nw-platform-architect-reviewer` Agent is not nested-invocable from within this
subagent context (the identical constraint was recorded for the prior
slim-DEVOPS features). Verdict: **APPROVED_PENDING_INDEPENDENT_REVIEW**, 0
blocking issues, per the established slim-DEVOPS precedent. An independent
top-level reviewer run is recommended before DISTILL.

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (the existing per-crate
  `gate-5-mutants-*` jobs are untouched; trunk-based, no required checks).
- Does not enrol cinder/kaleidoscope-cli into Gate 2/Gate 3 (a separate
  graduation decision; not needed — a Display-arm string is not an API change).
- Does not write production code or the acceptance tests (crafter owns DELIVER;
  acceptance-designer owns the test specs in DISTILL).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not bump any `Cargo.toml` version (no semver event; NEVER 1.0.0).
- Does not proceed into DISTILL.
