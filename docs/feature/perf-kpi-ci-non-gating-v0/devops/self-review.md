# Self-Review — perf-kpi-ci-non-gating-v0 (DEVOPS)

- **Reviewer role**: `nw-platform-architect-reviewer` dimensions, applied as a
  STRUCTURED SELF-REVIEW. Nested sub-agent invocation is unavailable in this
  autonomous context, so Apex critiques the package against the platform
  reviewer's dimensions (slim-DEVOPS precedent:
  `aperture-serve-loop-error-surfacing-v0/devops/self-review.md`).
- **Subject**: `devops/environments.yaml`, `devops/wave-decisions.md`.
- **Date**: 2026-06-06.
- **Verdict**: **APPROVED_PENDING_INDEPENDENT_REVIEW** — 0 blocking, 0 high.

```yaml
review:
  feature: perf-kpi-ci-non-gating-v0
  wave: devops
  iteration: 1
  reviewer: nw-platform-architect-reviewer (self-review mode)
  verdict: APPROVED_PENDING_INDEPENDENT_REVIEW
  blocking_count: 0
  high_count: 0
  dimensions:

    - dimension: pipeline_quality
      assessment: >
        The restructure is two minimal edits to one file. gate-1-test loses only
        its perf env block (ci.yml:140-141); its invocation (ci.yml:184),
        toolchain step, cache, needs, and KPI-4 artefact steps are unchanged. The
        new perf-kpis job MIRRORS gate-1-test's setup exactly (same ubuntu-latest
        runner, same dtolnay/rust-toolchain stable step, same actions/cache
        cargo-stable namespace, same needs: gate-4-deny) and adds only
        continue-on-error: true + the perf env. No new action, no new toolchain,
        no new cache namespace. Shift-left preserved: the local pre-commit hook
        already mirrors the gating commit stage (Gate 4 + Gate 1) and correctly
        omits the perf var.
      severity: none
      finding: PASS

    - dimension: gate_classification
      assessment: >
        Correct use of the gate taxonomy. gate-1-test stays a BLOCKING (pipeline)
        gate; the perf-kpis job is correctly classed ADVISORY (visible, reported,
        non-blocking) via continue-on-error: true — the GitHub-native advisory
        mechanism, not a `|| true` step wrapper that would hide the breach
        (correctly rejected, per ADR-0070 §3). The job is explicitly NOT a sixth
        ADR-0005 gate; it is a non-gating signal alongside the five.
      severity: none
      finding: PASS

    - dimension: infrastructure_soundness
      assessment: >
        No new infrastructure. Reuse-heavy and justified: the ADR-0058 self-skip
        guard (unchanged), the existing assert-message got-value print, the
        continue-on-error primitive, the NIGHTLY_PIN job-level-literal pattern.
        The ONE new asset (the perf-kpis job) is justified — de-gating removes
        perf from Gate 1, so something must still run the family for C4. Placement
        recommended (job in ci.yml, sibling of gate-1-test off gate-4-deny, in
        parallel, off the critical path) with the separate-file variant flagged as
        allowed-with-rationale. Simplest-solution check satisfied: a single
        continue-on-error job is the minimal shape; richer alternatives (separate
        workflow, self-hosted perf runner) documented and rejected in ADR-0070.
      severity: none
      finding: PASS

    - dimension: deployment_readiness_and_rollback
      assessment: >
        Rollback is documented FIRST and is trivial: git revert of the single
        workflow edit restores gate-1-test setting the var and removes the perf
        job; no data, wire format, crate version, or consumer affected; the 28
        test files untouched in both directions (trunk-based, no deployed
        artefact). Detection of a defective change is named (gate-1-test still
        reding on variance, or the perf job failing the workflow) and is caught by
        the DISTILL structural acceptance before merge. No deploy surface, so no
        canary/blue-green applies — correctly N/A.
      severity: none
      finding: PASS

    - dimension: observability_completeness
      assessment: >
        Matches the outcome-kpis.md DEVOPS handoff exactly: log-only surface at
        v0, no dashboard, no alerting/paging, the job must never fail the
        workflow. Visibility-on-breach is real and reused (the assert message
        prints the p95 on a panic) — no new mechanism needed. All four outcome
        KPIs are mapped to a measurement: KPI-1/3 (CI run-history classification),
        KPI-2 (p95 presence in the perf-kpis log), KPI-4 (git log on durable-op
        literals). The implicit guardrail (correctness gating not loosened, US-03)
        is mapped to the unchanged ci.yml:184 invocation + the DISTILL negative
        control. The informal first-runs baseline is correctly framed as a
        post-DELIVER reading, not a wave artefact.
      severity: none
      finding: PASS

    - dimension: security_and_supply_chain
      assessment: >
        No new attack surface. No new action (reuses the pinned checkout /
        toolchain / cache SHAs already in the file), no new tool, no new
        toolchain, no new permission (default read-only permissions unchanged).
        The perf tests are in-process tempdir I/O — no network, no secret, no
        external service, no consumer-driven contract. D8 captures this.
      severity: none
      finding: PASS

    - dimension: single_setter_safety
      assessment: >
        The load-bearing check is done and correct. Repo-wide grep
        (`grep -rn KALEIDOSCOPE_PERF_TESTS .github/ scripts/`) returns EXACTLY one
        match (ci.yml:141). gate-1-test is the sole current setter; the new
        perf-kpis job becomes the second-and-only setter. No other CI job and no
        hook sets or reads the variable; Gate 5 mutation is unaffected; both hooks
        confirmed clean (pre-commit no env at :92-93; pre-push zero references).
      severity: none
      finding: PASS

    - dimension: handoff_completeness_and_routing
      assessment: >
        The DELIVER act is correctly routed to Apex (platform-architect), NOT the
        crafter, with the CLAUDE.md rule quoted (crafter writes only crates/*/src;
        all others write workflow YAML). The DELIVER package is paste-ready: the
        exact perf-kpis job YAML, the exact gate-1-test diff, placement +
        needs: wiring resolved, and the NO-version-bump / NO-Cargo / NO-new-gate /
        NO-hook-change constraints restated. The DISTILL seam (structural
        acceptance + behavioural negative control) is named. The nWave-order note
        pre-empts the "the ci.yml edit is missing" false finding.
      severity: none
      finding: PASS

    - dimension: nwave_order_and_scope
      assessment: >
        Correct slim/CI-config scope. DEVOPS designs/operationalises the ADR-0070
        restructure and does NOT edit ci.yml (that is DELIVER) — honoured. No
        commit (Andrea commits). docs/evolution/ untouched. The supersede of
        ADR-0058 §3 is correctly attributed to DESIGN (already done), not
        re-decided here. Mutation strategy (D9) confirmed unchanged with no
        CLAUDE.md edit (nothing to mutate; the per-feature 100%-kill strategy
        stands).
      severity: none
      finding: PASS

  notes:
    - The package contains NO ci.yml edit by design (nWave order). The exact spec
      is supplied for DELIVER. An independent reviewer should review the SPEC, not
      the absence of the edit.
    - APPROVED_PENDING_INDEPENDENT_REVIEW reflects the self-review mode; an
      independent platform-architect-reviewer pass remains available if wanted,
      but there are 0 blocking and 0 high findings to address.
```
