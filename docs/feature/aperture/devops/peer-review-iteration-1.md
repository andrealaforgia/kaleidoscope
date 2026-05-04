# DEVOPS-wave peer review — `aperture` — iteration 1

**Reviewer**: Forge (`nw-platform-architect-reviewer`) | **Date**: 2026-05-04 | **Iteration**: 1 of 2

## Verdict: **APPROVED**

Zero blockers, zero critical issues. Five medium findings, all justified, all documented, all defensible. External validity passes on all four dimensions.

## Per-dimension verdicts

| Dimension | Verdict | Evidence |
|---|---|---|
| 1. CI/CD pipeline correctness | PASS | Five existing gates + three future gates documented; staged graduation per A2/A4; single workflow; proper sequencing |
| 2. Environment inventory coverage | PASS | Four environments documented (linux/macos/wsl/CI); Aperture-specific notes (free-port, no new tools) recorded |
| 3. Observability design alignment | PASS | All eight KPIs have a measurement path; 20-event closed vocabulary locked; operator query patterns provided; no `/metrics` per DISCUSS Q6 |
| 4. Infrastructure security and deployment strategy | PASS | cargo-deny passes with Aperture's transitive deps (Cargo.toml version-pin fix in A3); operator-side rolling deployments supported by drain-respecting shutdown |

## External validity

| Criterion | Status |
|---|---|
| Deployment path complete | PASS — author → test → build → operator deployment → operator rollback (Kaleidoscope owns 1-2-3, operator owns 4-5) |
| Observability enabled | PASS — eight KPIs each with measurement path; six operator query patterns; four guardrail alerting rules |
| Rollback capability | PASS — operator-side; binary swap; drain-respecting shutdown; no data rollback needed (Aperture is stateless) |
| Security gates integrated | PASS — five existing + three future; cargo-deny clean; no SAST/DAST needed (no eval, no FFI, forbid unsafe_code) |

## Apex's five self-flagged points — all resolved

1. **Gate 1 / Gate 5 stay scoped to harness during DELIVER** — SOUND. Per-slice graduation rejected (drift risk, review burden); one-shot lockstep at DELIVER close is correct. Same pattern as harness DEVOPS precedent.
2. **Three Aperture-specific gates documented but not wired today** — CORRECT. `if: false` pre-wiring rejected (clutter, quota); the slice that delivers the underlying test IS the natural moment to wire its gate.
3. **Cargo.toml version pin (A3)** — CONFIRMED in place at `crates/aperture/Cargo.toml` line 45. cargo-deny passes.
4. **Pact-style contract test deferred to Phase 1 pilot engagement** — DEFENSIBLE. Three-layer defence airtight at v0 (probe at startup, wiremock gold-test in Slice 06, pinned OTLP/protobuf v1.5.0 wire spec). Pact adds infrastructure, not value at v0.
5. **KPI 5 + KPI 8 load tests deferred to release wave** — DEFENSIBLE. Per-commit load tests would be wasteful (~1 hour wall-clock for KPI 5); per-slice property tests defend the logic, load tests defend the volume. Release cadence is the appropriate frequency.

## Findings

### Issue 1 (medium) — KPI 5/8 load-test deferral carries late-cycle discovery risk

Justified, documented, three-layer defence at unit/property level. Recommendation: document the deferral explicitly in the Phase 1 release-wave template so load tests wire at v0.1 release moment. No action required at DEVOPS close.

### Issue 2 (medium) — Gate 7 (`no_telemetry_on_telemetry`) is Linux-only

Justified (Linux is the load-bearing deployment platform; CI runs ubuntu-latest). Skip message documented in `environments.yaml`. Recommendation: make the compile-time skip message explicit in the gate-7 skeleton ("test skipped on non-Linux; CI's Linux runner enforces this invariant"). No action required.

### Issue 3 (medium) — Pact-style contract test deferred to Phase 1

Defensible. Three-layer defence at v0 (probe + wiremock + spec). Pact at v0 would add infrastructure (broker, consumer CI step) without provider-version churn to defend against. Recommendation: document the re-evaluation trigger ("if Phase 1 pilot operators report repeated drift, add Pact then"). No action required.

### Issue 4 (medium) — KPI 3 survey component is qualitative; baseline measurement deferred to Phase 1

Sound. Structural defence airtight at v0 (slice 02 + slice 08 tests assert the three-state machine); survey at 30 days post-launch is the natural validation moment. Recommendation: document survey timing and ownership in Phase 1 planning. No action required.

### Issue 5 (medium) — ci.yml graduation comment snippet not shown directly in review

Mitigated; the comments are documented inline in `wave-decisions.md` A2 and the actual comment blocks are present in the workflow file. Future DEVOPS waves should include the comment snippets directly when they are load-bearing. No action required.

## Strengths called out

- **Boundary clarity** between Kaleidoscope (ships CI + docs) and operators (own runtime + dashboards). Ten "what Kaleidoscope does NOT ship" items, each justified. Prevents scope creep.
- **Staging strategy for RED scaffold graduation**: Gate 1 stays harness-scoped during DELIVER; one-shot lockstep edit at DELIVER close. Three alternatives explicitly rejected.
- **KPI instrumentation acknowledges build-time vs runtime split** without ceremony: structural defence in CI for what CI can defend, operator-side queries for what operators can measure, release-cadence load tests for what costs would be wasted per-commit.
- **Three new CI gates scoped to the slices that deliver them**: gate-6 by Slice 03, gate-7 + gate-8 by Slice 06. Documented but not pre-wired with `if: false`. Cleaner pattern than placeholder-job approach.
- **Cargo.toml version-pin fix (A3)**: small, load-bearing, honours the workspace's existing `bans.wildcards = "deny"` policy. No deny.toml changes needed.

## Iteration budget

Iteration 1 of 2 maximum per the skill. Zero blockers; no iteration 2 required. DEVOPS wave is closed for `aperture`.

## Handoff readiness

DELIVER (Crafty) inherits a complete CI/operational substrate. The three future gates have skeletons in `ci-cd-pipeline.md` ready for Slice 03 and Slice 06 wire-up. Gate 1 / 5 / pre-commit-hook graduation is documented as DELIVER's final commit (one lockstep edit, four files).

After DELIVER closes:
- Gate 1: `--workspace --all-targets --locked`
- Gate 5: `--package otlp-conformance-harness --package aperture`
- Pre-commit hook: remove `--exclude aperture`
- Release-wave template: wire KPI 5/8 load tests at v0.1 release
