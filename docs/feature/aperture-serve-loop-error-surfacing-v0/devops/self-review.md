# DEVOPS self-review — aperture-serve-loop-error-surfacing-v0

- **Reviewer**: Apex (`nw-platform-architect`), self-review against
  `nw-platform-architect-reviewer`'s DEVOPS dimensions.
- **Date**: 2026-06-05
- **Reason**: the `nw-platform-architect-reviewer` Agent (Task tool) is not
  invocable as a nested subagent from within this subagent context (the
  identical constraint was recorded for the prior slim-DEVOPS feature,
  `cinder-wal-error-surfacing-v0/devops/self-review.md`). This structured
  self-review substitutes against the reviewer's exact rubric (external
  validity -> evidence-based findings -> severity-driven -> DORA -> handoff
  completeness). **An independent top-level `nw-platform-architect-reviewer`
  run is recommended before DISTILL.**

## Reviewer dispatch note (nWave-order reminder, as it WOULD be sent)

> In nWave, DEVOPS runs BEFORE DISTILL and DELIVER. At DEVOPS time there is NO
> production code, NO tests, and NO CI-config change for this feature yet —
> that absence is the EXPECTED and CORRECT state, NOT a rejection reason. Every
> gate this feature relies on (ADR-0005's five) already exists and already runs
> on every commit to `main`. Review the two DEVOPS artefacts
> (`environments.yaml`, `wave-decisions.md`) and the CI-contract CONFIRMATION
> they make — not the non-existence of code or new pipeline files. The feature
> is an INTERNAL single-crate change to the existing live `aperture` crate with
> NO new infrastructure, NO deploy surface, and NO public-API break;
> external-validity items keyed to a deployment path are N/A by construction and
> must be assessed as "N/A (no deploy surface)", not as missing.

## Structured review

```yaml
review:
  feature: aperture-serve-loop-error-surfacing-v0
  wave: devops
  mode: slim
  verdict: APPROVED_PENDING_INDEPENDENT_REVIEW

  external_validity:
    status: PASS (scoped to a no-deploy, internal single-crate change)
    findings:
      - No deployment path is required: the feature surfaces a previously
        SWALLOWED serve-loop death inside the existing live aperture crate;
        operators run / orchestrate the binary, Kaleidoscope deploys nothing.
        Deploy-path / canary / rollout items are N/A by construction, documented
        as such (not omitted).
      - Observability present and correct for the posture, and it is the very
        SUBSTANCE of the feature: a dead serving loop now (1) emits one
        structured `serve_loop_failed` stderr event (closed vocabulary,
        ADR-0009), (2) flips `/readyz` to a sticky 503 "failed" so an
        orchestrator pulls the zombie from rotation, (3) exits 3 so a supervisor
        restarts it; `/healthz` stays 200. Existing platform conventions
        (stderr + readiness/liveness probes + exit code); no new stack. KPIs are
        in-suite falsifiability + 100% mutation-kill.
      - Rollback present: `git revert`; aperture is stateless (no WAL/snapshot/
        on-disk format) and the wire/probe contracts are unchanged — the only
        deltas (a `/readyz` 503 "failed" arm and exit code 3) are additive — so
        a revert is clean. Documented in both artefacts.
      - Security gates: Gate 4 (cargo deny) inherited unchanged; no new
        dependency introduced (ADR-0066 reuses tonic, axum, tracing,
        std::sync::atomic — pins untouched), so the supply-chain gate is a no-op
        confirmation, not a gap. ADR-0066 §Trade-off ATAM confirms no new
        attack surface: no new port/input/credential/path; the `error` field on
        serve_loop_failed carries a transport/runtime reason string, not
        request- or operator-supplied data, so no stderr injection vector.

  dimensions:
    pipeline_quality:
      status: PASS
      findings:
        - aperture verified to own a path-filtered gate-5-mutants-aperture
          --in-diff job (ci.yml:505-602); line numbers cited in
          wave-decisions.md. It mutates all five modified files
          (transport/shutdown/readiness/lib/observability) via
          `git diff ... -- 'crates/aperture/**'`. No new CI job; trunk-based, no
          required status checks.
        - Gate 1 (cargo test --workspace --all-targets --locked) runs the
          serve-failure surfacing tests AND the slice-08 graceful-shutdown
          negative-control suite in BOTH the local pre-commit hook
          (scripts/hooks/pre-commit) and CI gate-1-test (ci.yml:137-184) —
          identical invocation, local<->CI parity confirmed.

    ci_contract_confirmation:
      status: PASS (a confirmation, not a correction — and not a blocker)
      severity: informational
      findings:
        - CONTRAST with the cinder wave: there, DEVOPS CORRECTED a mis-assumption
          (the brief assumed Gate 2/Gate 3 fire on cinder; they do not). Here
          there is NOTHING to correct — ADR-0066 and the brief already state the
          change is INTERNAL with no public-API break, and CI inspection AGREES
          on both counts: (a) aperture is not enrolled in Gate 2 (ci.yml:328-349)
          or Gate 3 (ci.yml:357) — only otlp-conformance-harness/spark/sieve/
          codex are, mirrored by pre-push lines 54/77; and (b) the ripple is
          entirely behind the crate boundary (mod transport crate-private;
          ServeOutcome/ServeError pub(crate)), so there is no diff to flag even
          hypothetically.
        - Evidence-based consequence: NO semver bump. aperture stays 0.1.0
          (verified Cargo.toml). DELIVER must NOT bump it and there is NO
          public-api baseline to update (none exists for aperture). This is the
          explicit difference from cinder (manual 0.1.0 -> 0.2.0). NEVER 1.0.0 —
          Andrea's call; out of scope.
        - Actionable recommendation (taken): do NOT enrol aperture into
          Gate 2/Gate 3 speculatively — graduation is a separate deliberate
          decision (aperture's only public library surface is the dev-only
          aperture::testing seam, ADR-0007). Flagged, not actioned.

    infrastructure_soundness:
      status: PASS
      findings:
        - No infrastructure introduced. Internal single-crate change only. No
          crate, container, service, cloud resource, IaC, or orchestration.
        - environments.yaml scoped to clean + with-pre-commit + ci (the standard
          build/test matrix), correct for a no-deploy-surface feature and
          mirroring the prior slim-DEVOPS shape.

    deployment_readiness:
      status: PASS (N/A surface)
      findings:
        - No deployment surface; rollback = git revert; aperture stateless,
          wire/probe contracts unchanged (deltas additive).
        - Canary/blue-green/rolling and on-call/runbook are N/A and documented as
          such; the serve_loop_failed event + /readyz 503 + exit 3 ARE the
          operator-facing signals the feature adds. A fleet alert on the event is
          a future separate observability feature (outcome-kpis.md DEVOPS note).

    observability_completeness:
      status: PASS
      findings:
        - The three operator signals (stderr event, /readyz 503 "failed", exit 3)
          ride EXISTING channels and the existing closed vocabulary; one additive
          constant (SERVE_LOOP_FAILED). No new metric/dashboard/stack, per
          ADR-0066 (no external-integration handoff) and the KPI handoff.
        - The graceful-vs-fatal discriminator (shutdown_requested AtomicBool, D3)
          guarantees the signals fire ONLY on a genuine post-bind death, never on
          a normal SIGTERM — the false-alarm guard is part of the observability
          correctness, and is itself a falsifiable AC (KPI-4).

    serve_failure_test_environment:
      status: PASS (the load-bearing DEVOPS concern for this feature)
      findings:
        - The seam is two-layered and fully IN-PROCESS: (i) the hand-constructed
          ShutdownBundle exit-code seam (lib.rs:379-430) resolving a join to
          ServeOutcome::Failed with no shutdown sent -> exit 3; (ii) an injected
          serve future behind the spawn helper resolving Err / early-Ok post-bind
          -> the captured event + /readyz 503 + /healthz 200. No infra, no real
          accept-loop kill, no signals beyond the existing graceful oneshot — the
          aperture analogue of cinder's FailingFsyncBackend.
        - Determinism: stderr presence/absence + /readyz status + exit-code
          equality, NO wall-clock threshold — so the local hook does not flake
          under overnight load (the p95-flake class does NOT apply; these are
          boolean / exit-code assertions, not p95 latency).
        - Falsifiability mandated (C-DEVOPS-4): each failure AC must fail on
          today's `let _ = ...await` swallow (verified present at transport.rs:93
          gRPC and :153-157 HTTP) and pass only on the surfaced-and-reacted fix;
          the negative control must distinguish shutdown_requested true (clean)
          from false (fatal). The DISCUSS false-confidence guard.

    handoff_completeness:
      status: PASS
      findings:
        - DEVOPS artefacts: wave-decisions.md, environments.yaml,
          self-review.md. Seven constraints (C-DEVOPS-1..7) handed to
          DISTILL/DELIVER, including the no-break/no-bump confirmation, the
          no-new-gate-5-job finding, the determinism+falsifiability mandate, the
          guardrail (slice-08) non-regression, and the gates-6/7/8-not-perturbed
          note.
        - Upstream changes = none expected; nothing to correct (ADR-0066 + brief
          already state the no-break posture and CI agrees), so no DISCUSS/DESIGN
          re-scoping and no sharpening of the DELIVER versioning task beyond
          "do not bump".

  dora_assessment:
    note: >
      DORA deploy-frequency / lead-time / change-failure / restore metrics are
      keyed to a deployment pipeline; this feature has no deploy surface, so the
      DORA frame is N/A. The operative quality compass here is the ADR-0005
      gate-pass signal (100% mutation kill on the five modified files + green
      serve-failure ACs + unchanged slice-08 guardrail suite), the project's K6
      raw-observation idiom. Worth noting: the feature itself IMPROVES the fleet's
      time-to-restore on a real post-bind death (the zombie window moves from
      indefinite to the next probe/exit), which is a DORA-restore improvement at
      the OPERATED-fleet level even though this wave ships no pipeline change.

  blocking_issues: none

  open_items:
    - Independent top-level nw-platform-architect-reviewer run recommended
      before DISTILL (this is a self-review).
    - DELIVER: NO aperture Cargo.toml version bump (stays 0.1.0 — no public-API
      break); NO public-api baseline update (none exists for aperture); NO new
      gate-5 job (existing gate-5-mutants-aperture --in-diff covers all five
      modified files); NEVER 1.0.0.
    - DELIVER: keep the serve-failure tests deterministic (in-process injection,
      boolean/exit-code assertions, no wall-clock) so the local pre-commit hook
      does not flake; keep the slice-08 graceful-shutdown suite green.
```
