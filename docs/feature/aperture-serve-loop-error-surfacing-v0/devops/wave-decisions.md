# Wave Decisions — aperture-serve-loop-error-surfacing-v0 (DEVOPS)

- **Wave**: DEVOPS (nWave)
- **Agent**: Apex (`nw-platform-architect`)
- **Date**: 2026-06-05
- **Mode**: Autonomous overnight run. **SLIM** wave — an INTERNAL,
  single-crate change to the EXISTING live `aperture` crate; NO new crate, NO
  deploy surface, NO new infrastructure, NO public-API break.
- **Inputs read**: `design/wave-decisions.md` (D1-D3 resolved; the internal
  ripple map; Reuse = EXTEND-only),
  `docs/product/architecture/adr-0066-aperture-serve-loop-error-surfacing.md`,
  `docs/product/architecture/brief.md` (§"Application Architecture —
  aperture-serve-loop-error-surfacing-v0", incl. its DEVOPS handoff +
  For-Acceptance-Designer notes), `discuss/outcome-kpis.md` (KPI-1..6),
  ADR-0005 (the five workspace gates), `.github/workflows/ci.yml`,
  `scripts/hooks/{pre-commit,pre-push}`, `CLAUDE.md`, and the prior
  slim-DEVOPS shape (`cinder-wal-error-surfacing-v0/devops`).

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `design/wave-decisions.md` | D1-D3 resolved (typed `JoinHandle<ServeOutcome>` + self-react; sticky `ReadinessPhase::Failed` + exit 3; `shutdown_requested` AtomicBool discriminator); the complete internal ripple map (transport/shutdown/readiness/lib/observability, all pub(crate)); Reuse verdict = EXTEND-ONLY, zero net-new public types — all consumed | − none; DESIGN explicitly confirms NO public-API break and hands DEVOPS the mutation scope (the five modified files), addressed below |
| ADR-0066 | the mechanism (Option A); the public-API confirmation (C3/D1: `mod transport` crate-private, ServeOutcome/ServeError pub(crate)); the in-process injection seam (hand-constructed bundle + injectable serve future); NO new metric/dashboard | − none; ADR-0066 does NOT assume any Gate 2/Gate 3 fire (correctly states semver-MINOR-at-most, pre-1.0, NEVER 1.0.0 ONLY in the hypothetical leak case) — confirmed against CI below |
| `brief.md` aperture section | DEVOPS handoff (INTERNAL single-crate, five files, Gate 2/Gate 3 do NOT fire, mutation scope, no external integration / no contract test); For-Acceptance-Designer driving ports (stderr + /readyz + /healthz + exit code) | − none |
| `discuss/outcome-kpis.md` | KPI-1 (swallow sites 2 -> 0), KPI-2 (one event/arm), KPI-3 (process reaction observable), KPI-4 (false-alarm 0), KPI-5 (both arms, HTTP proven), KPI-6 (100% mutation kill); the DEVOPS note: the serve_loop_failed event stream + the /readyz flip / non-zero exit are the operator-facing signals worth wiring into fleet observability LATER | − none; the "wire into fleet observability" note is explicitly a future, out-of-this-wave concern |
| ADR-0005 (five gates) | Gate 1 (test), Gate 2 (public-api), Gate 3 (semver), Gate 4 (deny), Gate 5 (mutants, 100% kill) — all already run on every push to main | − Gate 2/Gate 3 enrolled for only 4 graduated packages; aperture is not among them (consistent with no-break, finding below) |
| `.github/workflows/ci.yml` | `gate-5-mutants-aperture` (:505-602) exists, `--in-diff` path-filtered on `crates/aperture/**`; Gate 1 (:137-184); Gate 4 (:84); the COMMENTED service-gates 6/7/8 sketch (:2855-2921) | − Gate 2 (:328-349) and Gate 3 (:357) list ONLY otlp-conformance-harness, spark, sieve, codex; gates 6/7/8 are not-yet-wired sketches (both noted, neither a blocker) |
| `scripts/hooks/{pre-commit,pre-push}` | pre-commit = Gate 4 + Gate 1 (the local mirror); pre-push = Gate 2/Gate 3 for the 4 graduated pkgs | − pre-push (lines 54, 77) confirms aperture absent from the public-api/semver package loop (consistent with internal-only) |

## Headline

**Every gate this feature relies on already exists and already runs on every
push to `main`. No new CI job is required, and no CI-config change is made by
this wave.** The feature modifies five existing source files inside the single
live crate `aperture` (`src/transport.rs`, `src/shutdown.rs`,
`src/readiness.rs`, `src/lib.rs`, `src/observability.rs`, plus a one-line doc
addition to `src/main.rs`). aperture already owns a path-filtered
`gate-5-mutants-aperture` `--in-diff` job that mutates exactly its changed
lines automatically.

**Confirmed against the live source**: the two swallow baselines are present —
`let _ = server.await;` (gRPC, `transport.rs:93`, disclosed by a comment) and
`let _ = axum::serve(listener, router).with_graceful_shutdown(async move { let
_ = shutdown.await; }).await` (HTTP, `transport.rs:153-157`, silent). aperture's
`Cargo.toml` is `version = "0.1.0"`.

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
| D1 | Deployment target | **N/A** | Internal change to a live library + binary. aperture is the operator-run / orchestrator-run OTLP gateway; Kaleidoscope deploys nothing. No deploy step is added or required. |
| D2 | Container orchestration | **N/A** | No container, no orchestration surface added by this wave. (The change makes aperture a BETTER orchestration citizen — `/readyz` 503 + exit 3 — but introduces no orchestration artefact.) |
| D3 | CI/CD platform | **Existing — GitHub Actions per ADR-0005** | The five-gate workflow (`.github/workflows/ci.yml`) already runs on every push to main and every PR. Unchanged. |
| D4 | Existing infrastructure | **Yes — inherits ADR-0005's five gates UNCHANGED** | Gates 1/4/5 fire on the five modified aperture files automatically (Gate 5 via the existing `gate-5-mutants-aperture --in-diff` job). Gate 2/Gate 3 do NOT cover aperture — see CI Contract finding (consistent with no-break). No new gate. |
| D5 | Observability | **Existing convention — STDERR structured event + readiness/exit-code probes** | The feature ADDS one structured `serve_loop_failed` event to stderr (aligned with the platform's stderr closed-vocabulary convention, ADR-0009), a sticky `/readyz` 503 "failed" arm, and a distinct exit code 3 (D2). `/healthz` stays 200. No new metric, no new dashboard, no new observability stack. The fleet-level alert/counter on the event stream is a FUTURE separate feature (outcome-kpis.md DEVOPS note), not this wave. |
| D6 | Deployment strategy | **N/A** | No rollout. "Rollback" = `git revert`; aperture is stateless (no WAL/snapshot/on-disk format), and the wire/probe contracts are unchanged (the only deltas — a new `/readyz` 503 "failed" arm and exit code 3 — are additive), so a revert restores the prior swallow with no data or wire-format consideration. |
| D7 | Continuous learning | **N/A** | No live telemetry loop; the KPIs are in-suite falsifiability + 100% mutation-kill (the K6 raw-observation idiom). |
| D8 | Git branching | **Trunk-based (existing)** | Short-lived branch / direct-to-main; the workflow triggers on `push:[main]` and `pull_request:[main]`. No change. |
| D9 | Mutation testing | **Per-feature, 100% kill rate (existing, ADR-0005 Gate 5 / CLAUDE.md)** | Already pinned in CLAUDE.md. Mutation scope = the five modified aperture files (`transport.rs`, `shutdown.rs`, `readiness.rs`, `lib.rs`, `observability.rs`). Covered by the existing `gate-5-mutants-aperture --in-diff` job. **No CLAUDE.md change needed.** |

## CI Contract — confirmation and findings

### Gate 5 (mutants, 100% kill) — CONFIRMED, no new job

| Touched path | Change in this feature | Existing gate-5 job | ci.yml line | Verified |
|--------------|------------------------|---------------------|-------------|----------|
| `crates/aperture/src/transport.rs` | both spawn helpers return `JoinHandle<ServeOutcome>`; the two former swallow sites (`:93`, `:153-157`) self-react; the `shutdown_requested` AtomicBool branch; the emit + `flip_to_failed` calls | `gate-5-mutants-aperture` | 505-602 | ✓ `--in-diff` on `crates/aperture/**` |
| `crates/aperture/src/shutdown.rs` | `ShutdownBundle` join type; `DrainOutcome::ServeFailed`; the orchestrator drain-future fold; `ServeFailed -> 3` exit map | `gate-5-mutants-aperture` | 505-602 | ✓ same job |
| `crates/aperture/src/readiness.rs` | `ReadinessPhase::Failed` (sticky) + `flip_to_failed()` CAS + the sticky-precedence no-ops; `/readyz` `Failed -> 503 "failed"` | `gate-5-mutants-aperture` | 505-602 | ✓ same job |
| `crates/aperture/src/lib.rs` | the run-loop no-SIGTERM death path; the two mechanical synthetic-join test updates | `gate-5-mutants-aperture` | 505-602 | ✓ same job |
| `crates/aperture/src/observability.rs` | one additive `SERVE_LOOP_FAILED` constant | `gate-5-mutants-aperture` | 505-602 | ✓ same job |

The job runs `cargo mutants --package aperture --in-diff "$DIFF_FILE"` against
`git diff "$BASELINE" HEAD -- 'crates/aperture/**'` (baseline cascade
`origin/main` -> `HEAD~1` -> full; ci.yml:566-596). The `--in-diff` filter means
the job mutates ONLY the lines this feature changes across all five files — a
mutant restoring `let _ = join.await`, deleting the `shutdown_requested` flag
read, weakening the `flip_to_failed` CAS, or collapsing `ServeFailed -> 3` must
be killed by the surfacing / exit-3 / negative-control tests (KPI-6). **No
per-feature wiring, no new gate-5 job.** aperture was already enrolled in the
per-crate `--in-diff` model (the close of `gate-5-mutants-batch-v0`), so this
feature inherits gating for free.

### Gate 2 (public-api) + Gate 3 (semver) — CONFIRMED: do NOT fire, NO semver bump (the load-bearing confirmation)

**DESIGN confirmed NO public-API break, and CI inspection confirms aperture is
not even enrolled in Gate 2/Gate 3 — the two facts agree.**

1. **aperture is NOT enrolled.** Gate 2 (`cargo public-api`, ci.yml:328-349) and
   Gate 3 (`cargo semver-checks`, ci.yml:357) are enrolled for ONLY the four
   **graduated** packages — `otlp-conformance-harness`, `spark`, `sieve`,
   `codex`. The local pre-push hook mirrors exactly that set
   (`scripts/hooks/pre-push` lines 54, 77:
   `for pkg in otlp-conformance-harness spark sieve codex`). aperture is not in
   the loop.
2. **And there is no break to flag anyway.** Per ADR-0066 §"Public-API
   confirmation (C3, D1)" and `design/wave-decisions.md` §"Reuse Analysis":
   `mod transport;` is declared WITHOUT `pub` at `lib.rs:47` (crate-private);
   `spawn_grpc`/`spawn_http` are `pub fn` only within the crate and are not
   re-exported (the only `pub mod` re-exports are `config`, `ports`, `testing`);
   `ShutdownBundle`, `ReadinessPhase`, `DrainOutcome`, `ServeOutcome`,
   `ServeError` are all `pub(crate)`, carried inside crate-private bundle / join
   types, never nameable from outside. **No public type is introduced or
   changed.** So even if aperture WERE enrolled, Gate 2/Gate 3 would find no
   diff.
3. **Therefore NO semver bump is needed. aperture stays `0.1.0`.** This is the
   explicit CONTRAST with `cinder-wal-error-surfacing-v0`: that feature was a
   genuine `TieringStore`/`Queue` trait-signature break and required a MANUAL
   `0.1.0 -> 0.2.0` bump (semver-MINOR, pre-1.0). **aperture does NOT** — its
   ripple is entirely behind the crate boundary. DELIVER must NOT bump
   `crates/aperture/Cargo.toml`. (Were a public type ever to leak in a future
   change, it would be semver-MINOR at most, pre-1.0, **NEVER 1.0.0** — Andrea's
   call; not in scope here.)
4. **Decision: do NOT enrol aperture into Gate 2/Gate 3 in this wave.**
   Graduating a crate into the public-surface lock is a separate, deliberate
   decision (as it was for spark/sieve/codex). aperture's only public library
   surface is `aperture::testing` (a dev-only seam, ADR-0007); locking it is its
   own DESIGN/DEVOPS decision, not this internal serve-loop change. Flagged, not
   actioned.

### Gates 1 and 4 — CONFIRMED unchanged

- **Gate 1 (`cargo test --workspace --all-targets --locked`, ci.yml:137-184)**
  runs the serve-failure surfacing tests and the SIGTERM negative control
  (DISTILL authors them; DELIVER turns them green) plus the existing slice-08
  graceful-shutdown guardrail suite, identically in the local pre-commit hook
  and CI. No change.
- **Gate 4 (`cargo deny`, ci.yml:84)** — no new dependency is introduced
  (ADR-0066 reuses `tonic`, `axum`, `tracing`, `std::sync::atomic`; the pins are
  untouched), so Gate 4 is a no-op confirmation.

### Gates 6/7/8 (aperture service-specific) — NOTED, not perturbed, not wired by this wave

aperture additionally has three SERVICE-specific gates SKETCHED (commented) in
ci.yml:2855-2921: `gate-6-aperture-architectural-rules` (xtask AST walks:
single-validator-per-signal, hexagonal layer direction, no `prost::Message::
decode` in `crates/aperture/src/`), `gate-7-aperture-no-telemetry`, and
`gate-8-aperture-probe-gold`. They are COMMENTED sketches to be wired by
aperture DELIVER Slices 03/06, NOT live jobs today. This feature adds no
validator, no outbound traffic, no probe behaviour, no layer-direction change,
and no `prost` decode — so even once those gates are eventually wired they are
NOT perturbed by this change. **No action; not a delta this wave introduces.**

## Infrastructure Summary

- **New infrastructure**: none. No crate, no container, no service, no cloud
  resource, no IaC, no orchestration.
- **CI changes**: none. The five ADR-0005 gates are inherited unchanged; the
  single relevant Gate 5 job (`gate-5-mutants-aperture`) already path-filters
  `--in-diff` onto all five modified aperture files. No new job, no edit to an
  existing job.
- **Environments**: `clean` + `with-pre-commit` (developer machine) + `ci`
  (GitHub Actions, ubuntu-latest) — the standard build/test matrix for an
  internal single-crate change, NOT deploy targets. See `environments.yaml`.
- **Serve-failure test environment**: in-process injected serve future +
  hand-constructed `ShutdownBundle` exit-code seam (lib.rs:379-430) — a TEST
  concern, no infra, no real accept-loop kill, no signals beyond the existing
  graceful-shutdown oneshot. Recorded in `environments.yaml >
  serve_failure_test_environment`.
- **Observability**: one additive structured `serve_loop_failed` stderr event +
  a sticky `/readyz` 503 "failed" arm + exit code 3 (existing platform
  conventions); no new metric, no new dashboard, no new stack.
- **Rollback**: `git revert` (trunk-based); aperture is stateless and the wire /
  probe contracts are unchanged (the new `/readyz` 503 arm and exit code 3 are
  additive), so a revert restores the prior swallow with no data or wire-format
  consideration.

## Constraints Established (for DISTILL / DELIVER)

- **C-DEVOPS-1 — No new CI job; no CI-config change.** The existing
  `gate-5-mutants-aperture` job covers all five modified files via `--in-diff`.
  DELIVER must NOT add a per-feature gate-5 job.
- **C-DEVOPS-2 — NO public-API break, NO semver bump.** aperture's change is
  fully internal (crate-private module + `pub(crate)` types). aperture is NOT
  enrolled in Gate 2/Gate 3, AND there is no break to flag anyway. DELIVER must
  NOT bump `crates/aperture/Cargo.toml` (stays `0.1.0`) and there is NO
  public-api baseline to update (none exists for aperture). This is the explicit
  CONTRAST with cinder-wal-error-surfacing-v0 (which DID need a manual 0.2.0
  bump). NEVER 1.0.0 — Andrea's call; out of scope.
- **C-DEVOPS-3 — Serve-failure tests must be deterministic and run in BOTH the
  local pre-commit hook AND CI Gate 1.** In-process injected serve future +
  exit-code seam; stderr presence/absence + /readyz status + exit-code
  assertions, NO wall-clock threshold — so the hook does not flake under
  overnight load (the p95-flake class does NOT apply; these are boolean /
  exit-code assertions, not p95 latency).
- **C-DEVOPS-4 — Falsifiability is mandatory.** Each failure AC MUST fail on
  today's `let _ = ...await` swallow (no event captured, `/readyz` still 200,
  exit still 0) and pass ONLY on the surfaced-and-reacted fix. Do NOT inherit a
  serve-failure test that passes on the swallow, nor a negative control that
  cannot tell a graceful shutdown (`shutdown_requested = true`) from a fatal
  death (`shutdown_requested = false`) — the DISCUSS false-confidence risk.
- **C-DEVOPS-5 — Guardrails must stay green.** The slice-08 graceful-shutdown
  acceptance suite (`tests/slice_08_graceful_shutdown.rs`) and every healthy /
  graceful negative control (SIGTERM exits 0, NO serve_loop_failed, `/readyz`
  never flaps back to 200) must not regress; Gate 5 must reach 100% kill on the
  five modified files (KPI-6).
- **C-DEVOPS-6 — No CLAUDE.md change.** Per-feature 100%-kill mutation strategy
  is already pinned (D9).
- **C-DEVOPS-7 — Gates 6/7/8 are not perturbed.** This feature touches no
  validator, outbound-traffic, probe, layer-direction, or prost-decode concern;
  DELIVER need do nothing for the sketched aperture service-gates.

## Upstream Changes

**None expected.** DESIGN resolved D1-D3 into locked ACs for DISTILL; this
DEVOPS wave confirms the existing ADR-0005 CI contract covers them and CONFIRMS
(rather than corrects) the no-public-API-break / no-semver-bump posture against
the live source and the workflow. No shared assumption needed correcting (in
contrast with the cinder wave, where DEVOPS corrected a Gate 2/Gate 3
mis-assumption); ADR-0066 and the brief already state the no-break posture and
CI inspection agrees. No story re-scoping; no DISCUSS/DESIGN delta.

## Production Readiness (scoped to an internal, stateless-service change)

No service deploy, no rollout, no rollback-of-traffic. Applicable items:

- [x] Acceptance tests defined for both surfaced arms (gRPC + HTTP serve-loop
      death) and the SIGTERM negative control via the in-process injection seam;
      DISTILL authors them, DELIVER turns them green (KPI-2/3/4/5).
- [x] Mutation gate (Gate 5, 100% kill) auto-covers all five modified aperture
      files via the existing `gate-5-mutants-aperture --in-diff` job (KPI-6).
- [x] Operator signals surfaced on existing channels (D2/D5): one structured
      `serve_loop_failed` stderr event; `/readyz` 503 "failed" (sticky); exit
      code 3; `/healthz` stays 200.
- [x] No new event family / metric / dashboard / observability stack (ADR-0066 +
      KPI handoff); the fleet-level alert is a future separate feature.
- [x] Rollback posture: `git revert`; aperture is stateless, wire/probe
      contracts unchanged (deltas additive), so a revert is clean.
- [x] No-public-API-break / no-semver-bump confirmed against the live source and
      the workflow (aperture stays 0.1.0).
- [n/a] Canary / blue-green / rolling — no deployment surface (the change makes
      aperture a better orchestration citizen but adds no rollout).
- [n/a] On-call / runbook — operators run / orchestrate the binary; the
      `serve_loop_failed` event + `/readyz` 503 + exit 3 ARE the operator-facing
      signals (the very thing the feature adds). A fleet alert on the event is a
      future separate observability feature.

## Peer Review

The `nw-platform-architect-reviewer` Agent could not be invoked as a nested
subagent from within this subagent context (the identical constraint was
recorded for the prior slim-DEVOPS features, e.g.
`cinder-wal-error-surfacing-v0/devops/self-review.md`). Per the established
slim-DEVOPS precedent on this project, a structured self-review was conducted
against the reviewer's exact dimensions (external validity -> evidence-based
findings -> severity-driven -> DORA -> handoff completeness); see
`self-review.md`. The dispatch carried the nWave-order reminder (no
code/tests/CI exist at DEVOPS time — that absence is expected, not a rejection
reason). Verdict: **APPROVED_PENDING_INDEPENDENT_REVIEW**, 0 blocking issues.
An independent top-level `nw-platform-architect-reviewer` run is recommended
before DISTILL.

## What this DEVOPS wave does NOT do

- Does not add, rename, or re-scope any CI job (the existing
  `gate-5-mutants-aperture` job is untouched; trunk-based, no required checks).
- Does not enrol aperture into Gate 2/Gate 3 (a separate graduation decision;
  flagged, not actioned).
- Does not wire the sketched aperture service-gates 6/7/8 (their DELIVER slices
  own that; this feature does not perturb them).
- Does not write production code or the serve-failure / negative-control tests
  (crafter owns DELIVER; acceptance-designer owns the test specs in DISTILL).
- Does not change `CLAUDE.md` (per-feature 100% mutation already pinned).
- Does not bump any `Cargo.toml` version (aperture stays 0.1.0 — NO break, NO
  bump, NEVER 1.0.0).
- Does not proceed into DISTILL.
