# Wave Decisions — aperture-presubscriber-probe-stderr-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-07
- **Mode**: Autonomous. **SLIM** wave — a NET-DELETION fix inside the EXISTING
  live `aperture` crate; NO new crate, NO deploy surface, NO new infrastructure,
  NO new dependency, NO public-API break, NO semver bump.
- **Inputs read**: `design/wave-decisions.md` (mechanism (c): drop the redundant
  pre-subscriber probe; the post-subscriber probe at `compose.rs:157-167`
  carries the refusal; the ordering finding; Reuse = EDIT-AND-DELETE only),
  `docs/product/architecture/adr-0071-aperture-presubscriber-probe-refusal-visibility.md`,
  `discuss/outcome-kpis.md` (KPI-1/2 + the DEVOPS handoff note), ADR-0005 (the
  five workspace gates), `.github/workflows/ci.yml` (gate-5-mutants-aperture
  :562-653; Gate 2/3 enrollment), `scripts/hooks/{pre-commit,pre-push}`,
  `crates/aperture/src/compose.rs` (the probe sites), `crates/aperture/Cargo.toml`
  (version), `CLAUDE.md`/MEMORY, and the prior slim-DEVOPS shape
  (`aperture-serve-loop-error-surfacing-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | Mechanism (c) resolved (delete the pre-subscriber probe from both `wire_sink` arms; rely on the unchanged post-subscriber probe); the decisive post-subscriber-probe-vs-bind ordering finding (probe at `compose.rs:157-167` runs after `install_subscriber` `compose.rs:134` and before `spawn_grpc` `compose.rs:196`); the double-probe finding; Reuse verdict = EDIT-AND-DELETE only, zero net-new types/deps/events; the mutation scope (modified compose.rs + the new test file) — all consumed | − none; DESIGN explicitly confirms NO public-API / semver concern and hands DEVOPS the mutation scope, addressed below incl. the deletion-surface nuance |
| ADR-0071 | the mechanism (option (c)); the public-API/semver confirmation (`wire_sink`/`spawn_with_readiness`/`probe_or_refuse` all `pub(crate)`; aperture not in Gate 2/3; NEVER 1.0.0); the test seam (probe-substrate-lie subprocess test at the binary-start surface reusing the 200-OPTIONS/503-POST gold fixture); NO new event/metric/dashboard | − none; ADR-0071 §"Public API / semver" already states NO version bump — confirmed against the live `Cargo.toml` below |
| `discuss/outcome-kpis.md` | KPI-1 (100% of probe-refusal starts emit `event=health.startup.refused`, baseline 0%), KPI-2 (zero silent startup exits, down from 1); the guardrails (healthy = no refusal line; config-error line unchanged; ADR-0066 post-init path unchanged; fail-closed exit non-zero, no listener bound); the DEVOPS handoff (black-box stderr assertion, no new runtime instrumentation; the silent-exit-with-no-line is the guardrail-breach alert worth wiring LATER) | − none; the "wire a fleet alert" note is explicitly a future, out-of-this-wave concern |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | − Gate 2/Gate 3 enrolled for only 4 graduated packages; aperture is not among them (consistent with no-break, finding below) |
| `.github/workflows/ci.yml` | `gate-5-mutants-aperture` (:562-653) exists, `--in-diff` path-filtered on `crates/aperture/**` with the origin/main -> HEAD~1 -> full baseline cascade (:626-653); Gate 4 + Gate 1 run in the same workflow; the COMMENTED service-gates 6/7/8 sketch | − Gate 2 / Gate 3 do not list aperture; gates 6/7/8 are not-yet-wired sketches (both noted, neither a blocker) |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = Gate 4 + Gate 1 (the local mirror); pre-push (line 54) = Gate 2/Gate 3 for `otlp-conformance-harness spark sieve codex` only | − confirms aperture absent from the public-api/semver package loop (consistent with internal-only) |
| `crates/aperture/src/compose.rs` (live) | the two pre-subscriber probe calls to delete (`:73` Stub, `:81` Forwarding); the post-subscriber probe call (`:164`, the `compose.rs:157-167` block) that stays; `probe_or_refuse` body (`:96-104`) that stays | − none; the live source matches the ADR exactly |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and no CI-config change is made by
this wave.** The feature is a NET DELETION inside the single live crate
`aperture`: it removes the two redundant pre-subscriber `probe_or_refuse(...)`
calls from `wire_sink` (`compose.rs:73` Stub arm, `compose.rs:81` Forwarding
arm) and updates `wire_sink`'s doc comment, plus DELIVER adds ONE new
probe-substrate-lie subprocess test file. aperture already owns a path-filtered
`gate-5-mutants-aperture --in-diff` job that mutates exactly its changed lines
automatically.

**Confirmed against the live source**: the two pre-subscriber probe calls are
present (`crates/aperture/src/compose.rs:73` and `:81`); the post-subscriber
probe call is present (`compose.rs:164`, the `probe_or_refuse(&forwarding)` in
the `compose.rs:157-167` block) and `probe_or_refuse` (`:96-104`) emits
`event=health.startup.refused reason=%e` and returns `Err`. aperture's
`Cargo.toml` is `version = "0.1.0"`. The pre-push hook (line 54) enrolls only
`otlp-conformance-harness spark sieve codex` in Gate 2/Gate 3 — aperture is
absent.

**nWave-order note (for the reviewer):** in nWave, DEVOPS runs BEFORE DISTILL
and DELIVER, so at DEVOPS time NO production change, NO new test, and NO
CI-config change exist yet for this feature. That absence is the EXPECTED and
CORRECT state — it is not a finding. This wave's job is to CONFIRM the existing
ADR-0005 CI contract covers the feature and to produce `environments.yaml` +
this file; review THAT, not the non-existence of code or new pipeline files.

Kaleidoscope `main` is pure trunk-based: NO required status checks, NO
`enforce_admins` (project memory). CI is feedback, not a merge gate. This wave
wires nothing into a branch-protection contract; it confirms the existing
feedback signal covers the change.

## Decision summary (D1-D9, all existing / inherited — brownfield, NOT a deploy)

| # | Topic | Decision | Rationale |
|---|-------|----------|-----------|
| D1 | Deployment target | **N/A** | Internal deletion inside a live library + binary. aperture is the operator-run / orchestrator-run OTLP gateway; Kaleidoscope deploys nothing. No deploy step added or required. |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface added by this wave. (The change makes aperture a BETTER orchestration citizen — a refused start now PRINTS its reason instead of exiting silently — but introduces no orchestration artefact.) |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the modified aperture file automatically (Gate 5 via the existing `gate-5-mutants-aperture --in-diff` job). Gate 2/Gate 3 do NOT cover aperture — see CI Contract finding (consistent with no-break). No new gate. |
| D5 | Observability | **Existing channel — the already-existing STDERR structured event** | The fix makes the EXISTING `event=health.startup.refused reason=%e` (ADR-0009 closed vocab, `observability.rs:49`) actually REACH stderr, by letting the POST-subscriber probe carry the refusal. No NEW event constant, no `eprintln!`, no new metric, no new dashboard, no new observability stack. The fleet-level alert on "non-zero exit with no refusal/config line" is a FUTURE separate feature (outcome-kpis.md DEVOPS note), not this wave. |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; aperture is stateless (no WAL/snapshot/on-disk format), and the wire/probe contracts + the probe DECISION + the fail-closed exit are unchanged (the only delta — a refusal now prints its reason — is purely additive visibility), so a revert restores the prior silent behaviour with no data or wire-format consideration. |
| D7 | Continuous learning | **N/A** | No live telemetry loop; the KPIs are in-suite falsifiability (the new subprocess test + Bea A19/A20 evidence), the K6 raw-observation idiom. |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5 / CLAUDE.md)** | Already pinned in CLAUDE.md (`## Mutation Testing Strategy`). Mutation scope = the modified `crates/aperture/src/compose.rs` (the `wire_sink` deletion + the remaining post-subscriber probe logic the `--in-diff` filter still touches) + the new test file. Covered by the existing `gate-5-mutants-aperture --in-diff` job. **No CLAUDE.md change needed.** See the deletion-surface nuance below. |

## CI Contract — confirmation and findings

### Gate 5 (mutants, 100% kill) — CONFIRMED, no new job, with the deletion-surface note

| Touched path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|--------------|------------------------|---------------------|-------------|----------|
| `crates/aperture/src/compose.rs` | DELETE the pre-subscriber `probe_or_refuse(...)` from `wire_sink`'s Stub arm (`:73`) and Forwarding arm (`:81`); update `wire_sink`'s doc comment. The post-subscriber probe (`:157-167`, call on `:164`) and `probe_or_refuse` (`:96-104`) are UNCHANGED. | `gate-5-mutants-aperture` | 562-653 | ✓ `--in-diff` on `crates/aperture/**` |
| `crates/aperture/tests/<new>.rs` (DELIVER) | NEW probe-substrate-lie subprocess test (reuses the gold-runner fixture pattern) | `gate-5-mutants-aperture` only mutates `src/` by default; tests run under Gate 1 | 562-653 / 137-184 | ✓ test runs under Gate 1; mutants targets src |

The job runs `cargo mutants --package aperture --in-diff "$DIFF_FILE"` against
`git diff "$BASELINE" HEAD -- 'crates/aperture/**'` (baseline cascade
`origin/main` -> `HEAD~1` -> full; ci.yml:626-653). The `--in-diff` filter means
the job mutates ONLY the lines this feature changes in compose.rs. **No
per-feature wiring, no new gate-5 job.** aperture was already enrolled in the
per-crate `--in-diff` model (the close of `gate-5-mutants-batch-v0`), so this
feature inherits gating for free.

**Deletion mutation-surface note (the one nuance specific to this feature).**
This is a NET DELETION. cargo-mutants generates mutants from EXISTING
(post-change) source — it cannot mutate a line that no longer exists. So the two
removed `probe_or_refuse(...)` calls (`:73`, `:81`) contribute NO mutants
themselves; the live mutation surface on the changed file is the REMAINING /
changed logic the `--in-diff` filter still touches:

- the `wire_sink` match arms in their new PURE sink-selection shape
  (`compose.rs:68-85` after the deletion — a mutant that, say, swaps the Stub /
  Forwarding arm bodies or drops the `Arc::new` wrap must still be killed by the
  existing wiring tests);
- the now-single post-subscriber probe path (`compose.rs:157-167`, the
  `probe_or_refuse(&forwarding)` call on `:164`) and the `probe_or_refuse` body
  (`:96-104`) — the `sink.probe()` error branch, the
  `event=health.startup.refused` emit, and the `Err` return — which the new
  probe-substrate-lie subprocess test plus the existing gold-runner tests kill.

If the deletion leaves the CHANGED lines with a small or zero generated-mutant
count (a deletion-shaped diff can do that — there may simply be little new
mutable code on the exact changed lines), the BEHAVIOUR is still guarded by the
NEW subprocess test, which FAILS on today's silent exit-1 and passes ONLY when
the refusal is surfaced. That test is a behaviour guard, not a mutant guard, and
it is the load-bearing protection for this fix. **No new gate-5 job and NO new
gate is added speculatively** (trunk-based; CI is feedback, not a gate; do not
add gates speculatively — project memory).

### Gate 2 (public-api) + Gate 3 (semver) — CONFIRMED: do NOT fire, NO semver bump (the load-bearing confirmation)

**DESIGN/ADR-0071 confirmed NO public-API concern, and CI inspection confirms
aperture is not even enrolled in Gate 2/Gate 3 — the two facts agree.**

1. **aperture is NOT enrolled.** Gate 2 (`cargo public-api`) and Gate 3 (`cargo
   semver-checks`) are enrolled for ONLY the four **graduated** packages —
   `otlp-conformance-harness`, `spark`, `sieve`, `codex` — confirmed in the local
   pre-push hook (`scripts/hooks/pre-push` line 54:
   `for pkg in otlp-conformance-harness spark sieve codex`). aperture is not in
   the loop.
2. **And there is no break to flag anyway.** Per ADR-0071 §"Public API / semver"
   and `design/wave-decisions.md`: `wire_sink`, `spawn_with_readiness`, and
   `probe_or_refuse` are all `pub(crate)`. The change DELETES two crate-private
   call sites and adds NO public type, NO new event constant (the
   `HEALTH_STARTUP_REFUSED` constant already exists, `observability.rs:49`), and
   NO new stderr path. **No public type is introduced or changed.** So even if
   aperture WERE enrolled, Gate 2/Gate 3 would find no diff.
3. **Therefore NO semver bump is needed. aperture stays `0.1.0`** (verified
   `crates/aperture/Cargo.toml:3`). DELIVER must NOT bump
   `crates/aperture/Cargo.toml`, and there is NO public-api baseline to update
   (none exists for aperture). (Were a public type ever to leak in a future
   change, it would be semver-MINOR at most, pre-1.0, **NEVER 1.0.0** — Andrea's
   call; not in scope here.)
4. **Decision: do NOT enrol aperture into Gate 2/Gate 3 in this wave.**
   Graduating a crate into the public-surface lock is a separate, deliberate
   decision (as it was for spark/sieve/codex). aperture's only public library
   surface is `aperture::testing` (a dev-only seam, ADR-0007); locking it is its
   own decision, not this internal deletion. Flagged, not actioned.

### Gates 1 and 4 — CONFIRMED unchanged

- **Gate 1 (`cargo test --workspace --all-targets --locked`)** runs the new
  probe-substrate-lie subprocess test and the healthy / config-error negative
  controls (DISTILL authors them; DELIVER turns them green) plus the existing
  `tests/probe_gold_runner.rs` gold suite (the probe-surface guardrail),
  identically in the local pre-commit hook and CI. No change.
- **Gate 4 (`cargo deny --all-features check`)** — **no new dependency** is
  introduced. The new subprocess test reuses the gold-runner's substrate-lie
  fixture pattern and the existing test dependencies (the local liar HTTP server
  is the same shape the gold runner already uses); the production change is a
  pure deletion. `Cargo.toml` is untouched, so Gate 4 is a no-op confirmation.

### Gates 6/7/8 (aperture service-specific) — NOTED, not perturbed, not wired by this wave

aperture has three SERVICE-specific gates SKETCHED (commented) in ci.yml:
`gate-6-aperture-architectural-rules`, `gate-7-aperture-no-telemetry`, and
`gate-8-aperture-probe-gold`. They are COMMENTED sketches to be wired by aperture
DELIVER Slices 03/06, NOT live jobs today. This feature changes NO probe
BEHAVIOUR (the probe still bites identically; only its production-path call SITE
is consolidated from two to one), adds no validator, no outbound traffic, no
layer-direction change, and no `prost` decode — so even once those gates are
eventually wired they are NOT perturbed by this deletion. In particular Gate 8
(the probe-gold runner) keeps entering at the probe surface and keeps passing.
**No action; not a delta this wave introduces.**

## Infrastructure Summary

- **New infrastructure**: none. No crate, no container, no service, no cloud
  resource, no IaC, no orchestration.
- **CI changes**: none. The five ADR-0005 gates are **inherited unchanged**; the
  single relevant Gate 5 job (`gate-5-mutants-aperture`, ci.yml:562-653) already
  path-filters `--in-diff` onto the modified compose.rs. No new job, no edit to
  an existing job.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for an
  internal single-crate deletion, NOT deploy targets. See `environments.yaml`.
- **Probe-substrate-lie test environment**: a spawned `aperture` child process +
  a LOCAL liar HTTP server bound on an ephemeral loopback port (200-OPTIONS /
  503-POST, the gold-runner fixture pattern) — a TEST concern, no infra, no real
  downstream, no privilege. Recorded in `environments.yaml >
  probe_substrate_lie_test_environment`.
- **Observability**: the EXISTING `event=health.startup.refused reason=%e` stderr
  event now actually reaches stderr (no new event, no new metric, no new
  dashboard, no new stack).
- **Rollback**: `git revert` (trunk-based); aperture is stateless and the
  wire/probe contracts, the probe decision, and the fail-closed exit are
  unchanged (the delta — a refusal now prints its reason — is additive), so a
  revert restores the prior silent behaviour cleanly.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** The existing
  `gate-5-mutants-aperture` job covers the modified compose.rs via `--in-diff`.
  DELIVER must NOT add a per-feature gate-5 job and must NOT add any new gate
  speculatively.
- **C-DEVOPS-2 — Deletion mutation-surface.** The fix is a net deletion;
  cargo-mutants cannot mutate removed lines. The live mutation surface is the
  REMAINING/changed compose.rs logic (the new pure-`wire_sink` arms + the
  now-single post-subscriber probe path + `probe_or_refuse`). If the changed
  lines yield a small/zero generated-mutant count, the NEW probe-substrate-lie
  subprocess test still guards the behaviour (it fails on the silent exit-1).
  100% kill must hold on whatever mutants the `--in-diff` filter does generate.
- **C-DEVOPS-3 — NO public-API break, NO semver bump, NO new dependency.**
  aperture's change is fully internal (`pub(crate)` deletion). aperture is NOT
  enrolled in Gate 2/Gate 3, AND there is no break to flag anyway. DELIVER must
  NOT bump `crates/aperture/Cargo.toml` (stays `0.1.0`), there is NO public-api
  baseline to update, and DELIVER must NOT add any dependency (reuse the existing
  test deps + the gold-runner fixture pattern). NEVER 1.0.0 — Andrea's call.
- **C-DEVOPS-4 — The probe-substrate-lie test must be deterministic and run in
  BOTH the local pre-commit hook AND CI Gate 1.** Spawn the binary against a
  local liar HTTP server; assert stderr presence of `event=health.startup.refused`
  (naming sink + error), exit non-zero, and NO listener bound — boolean / exit-code
  / port-not-bound assertions, NO wall-clock threshold — so the hook does not
  flake under overnight load (the p95-flake class does NOT apply).
- **C-DEVOPS-5 — Falsifiability is mandatory.** The visibility AC MUST fail on
  today's code (the pre-subscriber probe wins the race, its event has no
  subscriber and is dropped, the operator sees a silent exit 1) and pass ONLY on
  the deletion-and-surface fix. Do NOT inherit a test that passes on the silent
  swallow.
- **C-DEVOPS-6 — Guardrails must stay green.** The existing
  `tests/probe_gold_runner.rs` gold suite (the probe still bites on the lie), the
  healthy-downstream negative control (NO refusal line, listeners bind), and the
  config-error negative control (`event=config_validation_failed`, exit 2,
  `emit_config_error`/main.rs:80-82 UNTOUCHED) must not regress. The ADR-0066
  post-init tracing path must not regress. Gate 5 must reach 100% kill on
  whatever mutants the changed compose.rs lines generate.
- **C-DEVOPS-7 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy
  is already pinned (D9).
- **C-DEVOPS-8 — Gates 6/7/8 are not perturbed.** This deletion changes no probe
  behaviour, validator, outbound-traffic, layer-direction, or prost-decode
  concern; DELIVER need do nothing for the sketched aperture service-gates (Gate
  8's probe-gold runner keeps passing).

## Upstream Changes

**None.** DESIGN resolved the mechanism to option (c) and ADR-0071 already states
the no-public-API / no-semver / no-new-dependency posture; this DEVOPS wave
CONFIRMS (rather than corrects) that posture against the live source
(`compose.rs` probe sites, `Cargo.toml` version 0.1.0, the pre-push Gate 2/3
enrollment list) and the workflow. No shared assumption needed correcting; no
story re-scoping; no DISCUSS/DESIGN delta.

## Production Readiness (scoped to an internal, stateless-service deletion)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance defined: the probe-substrate-lie subprocess test (visibility +
      exit non-zero + no-listener-bound) and the healthy / config-error negative
      controls via the local liar HTTP server seam; DISTILL authors them, DELIVER
      turns them green (KPI-1/2).
- [x] Mutation gate (Gate 5, 100% kill) auto-covers the modified compose.rs via
      the existing `gate-5-mutants-aperture --in-diff` job, with the
      deletion-surface caveat recorded (C-DEVOPS-2).
- [x] Operator signal surfaced on the existing channel (D5): the existing
      `event=health.startup.refused reason=%e` stderr event now reaches stderr;
      fail-closed exit (non-zero, no listener bound) unchanged.
- [x] No new event family / metric / dashboard / observability stack (ADR-0071 +
      KPI handoff); the fleet-level alert is a future separate feature.
- [x] Rollback posture: `git revert`; aperture is stateless, wire/probe
      contracts + probe decision + fail-closed exit unchanged (delta additive),
      so a revert is clean.
- [x] No-public-API-break / no-semver-bump / no-new-dependency confirmed against
      the live source and the workflow (aperture stays 0.1.0).
- [n/a] Canary / blue-green / rolling — no deployment surface (the change makes
      aperture a better orchestration citizen but adds no rollout).
- [n/a] On-call / runbook — operators run / orchestrate the binary; the
      now-visible `event=health.startup.refused` line + non-zero exit ARE the
      operator-facing signal (the very thing the feature adds). A fleet alert on
      the silent-exit guardrail breach is a future separate feature.

## Peer Review

The `nw-platform-architect-reviewer` Agent could not be invoked as a nested
subagent from within this subagent context (the identical constraint was recorded
for the prior slim-DEVOPS features, e.g.
`aperture-serve-loop-error-surfacing-v0/devops/wave-decisions.md` and
`cinder-wal-error-surfacing-v0/devops/self-review.md`). Per the established
slim-DEVOPS precedent on this project, a structured self-review was conducted
against the reviewer's exact dimensions (external validity -> pipeline quality ->
CI-contract confirmation -> infrastructure soundness -> deployment readiness ->
observability completeness -> handoff completeness); see the inline review below.
The dispatch carried the nWave-order reminder (no code/tests/CI exist at DEVOPS
time — that absence is expected, not a rejection reason). Verdict:
**APPROVED_PENDING_INDEPENDENT_REVIEW**, 0 blocking issues. An independent
top-level `nw-platform-architect-reviewer` run is recommended before DISTILL.

```yaml
review:
  feature: aperture-presubscriber-probe-stderr-v0
  wave: devops
  mode: slim
  verdict: APPROVED_PENDING_INDEPENDENT_REVIEW
  external_validity:
    status: PASS (scoped to a no-deploy, internal single-crate deletion)
    findings:
      - No deployment path required: the feature surfaces a previously-SILENT
        startup refusal inside the existing live aperture crate; operators
        run/orchestrate the binary, Kaleidoscope deploys nothing. Deploy/canary/
        rollout items are N/A by construction, documented as such (not omitted).
      - Observability present and correct, and it IS the substance of the fix:
        the existing event=health.startup.refused reason=%e (ADR-0009 closed
        vocab) now reaches stderr via the post-subscriber probe; no new stack.
        KPIs are in-suite falsifiability (the new subprocess test) + Bea A19/A20.
      - Rollback present: git revert; aperture is stateless and the wire/probe
        contracts, probe decision, and fail-closed exit are unchanged (delta
        additive). Documented in both artefacts.
      - Security gate: Gate 4 (cargo deny) inherited unchanged; NO new dependency
        (a pure deletion + a test reusing the gold-runner fixture and existing
        test deps), so the supply-chain gate is a no-op confirmation, not a gap.
        No new port/input/credential/path; the reason field carries a probe error
        string (a transport/runtime reason), not request- or operator-supplied
        data — no stderr-injection vector.
  dimensions:
    pipeline_quality:
      status: PASS
      findings:
        - aperture verified to own a path-filtered gate-5-mutants-aperture
          --in-diff job (ci.yml:562-653; cited). It mutates the modified
          compose.rs via git diff ... -- 'crates/aperture/**'. No new CI job;
          trunk-based, no required status checks.
        - Gate 1 (cargo test --workspace --all-targets --locked) runs the new
          subprocess test AND the gold-runner / negative-control guardrails in
          BOTH the local pre-commit hook and CI gate-1-test — identical
          invocation, local<->CI parity confirmed.
    ci_contract_confirmation:
      status: PASS (a confirmation, not a correction — and not a blocker)
      severity: informational
      findings:
        - Nothing to correct: ADR-0071 and DESIGN already state the change is an
          internal pub(crate) deletion with no public-API/semver/dependency
          concern, and CI inspection AGREES — aperture is not enrolled in Gate 2
          (pre-push line 54 lists only otlp-conformance-harness/spark/sieve/
          codex), and the ripple is entirely behind the crate boundary.
        - Evidence-based consequence: NO semver bump. aperture stays 0.1.0
          (verified Cargo.toml:3). DELIVER must NOT bump it; NO public-api
          baseline to update. NEVER 1.0.0.
        - Deletion mutation-surface CALLED OUT (the one feature-specific nuance):
          cargo-mutants cannot mutate removed lines; the live surface is the
          remaining/changed compose.rs logic; if that yields few/zero mutants the
          new subprocess test still guards the behaviour. No new gate added
          speculatively.
        - Recommendation (taken): do NOT enrol aperture into Gate 2/Gate 3
          speculatively — graduation is a separate deliberate decision. Flagged,
          not actioned.
    infrastructure_soundness:
      status: PASS
      findings:
        - No infrastructure introduced. Internal single-crate deletion only.
        - environments.yaml scoped to clean + with-pre-commit + ci (the standard
          build/test matrix), correct for a no-deploy-surface feature and
          mirroring the prior slim-DEVOPS shape.
    deployment_readiness:
      status: PASS (N/A surface)
      findings:
        - No deployment surface; rollback = git revert; aperture stateless,
          contracts unchanged (delta additive).
        - Canary/blue-green/rolling and on-call/runbook are N/A and documented as
          such; the now-visible refusal line + non-zero exit ARE the operator
          signal the feature adds; a fleet alert is a future separate feature.
    observability_completeness:
      status: PASS
      findings:
        - The operator signal (the now-visible event=health.startup.refused line)
          rides the EXISTING channel and the existing closed vocabulary; ZERO new
          constants (HEALTH_STARTUP_REFUSED already exists). No new metric/
          dashboard/stack.
        - The guardrails (healthy = no refusal line; config-error =
          config_validation_failed exit 2 unchanged; ADR-0066 post-init path
          unchanged; fail-closed exit non-zero, no listener bound) are part of
          the observability correctness and are themselves falsifiable negative
          controls (KPI guardrails).
    probe_substrate_lie_test_environment:
      status: PASS (the load-bearing DEVOPS concern for this feature)
      findings:
        - The seam is a spawned aperture child process + a local liar HTTP server
          on an ephemeral loopback port (200-OPTIONS/503-POST, the gold-runner
          fixture pattern) — fully local, no infra, no real downstream, no
          privilege. The aperture analogue of cinder's FailingFsyncBackend.
        - Determinism: stderr presence/absence + exit-code equality + port-not-
          bound, NO wall-clock threshold — so the local hook does not flake under
          overnight load (the p95-flake class does NOT apply).
        - Falsifiability mandated (C-DEVOPS-5): the test must fail on today's
          silent exit-1 (the pre-subscriber probe's event has no subscriber and
          is dropped) and pass only on the deletion-and-surface fix.
    handoff_completeness:
      status: PASS
      findings:
        - DEVOPS artefacts: wave-decisions.md, environments.yaml. Eight
          constraints (C-DEVOPS-1..8) handed to DISTILL/DELIVER, including the
          no-break/no-bump/no-new-dep confirmation, the no-new-gate finding, the
          deletion mutation-surface caveat, the determinism+falsifiability
          mandate, the guardrail non-regression set, and the gates-6/7/8-not-
          perturbed note.
        - Upstream changes = none; ADR-0071 + DESIGN already state the posture and
          CI agrees, so no DISCUSS/DESIGN re-scoping.
  dora_assessment:
    note: >
      DORA deploy-frequency / lead-time / change-failure / restore metrics are
      keyed to a deployment pipeline; this feature has no deploy surface, so the
      DORA frame is N/A. The operative quality compass here is the ADR-0005
      gate-pass signal (100% mutation kill on the changed compose.rs lines + a
      green probe-substrate-lie subprocess test + unchanged gold-runner / negative
      controls), the project's K6 raw-observation idiom. The feature itself
      IMPROVES the operated fleet's time-to-restore on a real probe refusal (the
      operator now reads the cause from stderr instead of debugging a silent exit
      1) — a DORA-restore improvement at the OPERATED-fleet level even though this
      wave ships no pipeline change.
  blocking_issues: none
  open_items:
    - Independent top-level nw-platform-architect-reviewer run recommended before
      DISTILL (this is a self-review).
    - DELIVER: NO aperture Cargo.toml version bump (stays 0.1.0 — no public-API
      break); NO public-api baseline update (none exists for aperture); NO new
      gate-5 job; NO new dependency; NEVER 1.0.0.
    - DELIVER: keep the probe-substrate-lie subprocess test deterministic (local
      liar HTTP server, boolean/exit-code/port-not-bound assertions, no
      wall-clock) so the local pre-commit hook does not flake; keep the
      gold-runner and the healthy / config-error negative controls green.
```

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (the existing
  `gate-5-mutants-aperture` job is untouched; trunk-based, no required checks).
- Does not enrol aperture into Gate 2/Gate 3 (a separate graduation decision;
  flagged, not actioned).
- Does not wire the sketched aperture service-gates 6/7/8 (their DELIVER slices
  own that; this deletion does not perturb them).
- Does not write production code or the probe-substrate-lie / negative-control
  tests (crafter owns DELIVER; acceptance-designer owns the test specs in
  DISTILL).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not bump any `Cargo.toml` version and adds no dependency (aperture stays
  0.1.0 — NO break, NO bump, NO new dep, NEVER 1.0.0).
- Does not touch `docs/evolution/`.
- Does not commit (Andrea commits).
- Does not proceed into DISTILL.
