# Peer review — Prism v0 DEVOPS, iteration 1

- **Date**: 2026-05-08
- **Reviewer**: `@nw-platform-architect-reviewer` (Forge), Haiku model
- **Wave**: DEVOPS — gate before DELIVER
- **Artefact set**: 8 files in `docs/feature/prism-v0/devops/`
- **Verdict**: **CONDITIONALLY APPROVED** — 5 CRITICAL specification gaps must be resolved before DELIVER can begin; 3 HIGH-severity items are inline-note candidates
- **Confidence**: High. All artefacts read in full.

---

## Executive summary

DEVOPS is architecturally sound and operationally mature. External
validity passes (deployment path complete, observability enabled,
rollback capability present, security gates present). Earned-Trust
three-layer enforcement is gold-standard for a frontend SPA. KPI
instrumentation spans CI and production. Path B (same-origin POST)
is the right call.

The CONDITIONAL APPROVAL is on five SPECIFICATION GAPS, not design
flaws. All are addressable without restructuring; iteration budget
1 of 2 is sufficient.

---

## Critical issues (must resolve before DELIVER)

### CRITICAL-1 — Mutation gate baseline cascade pseudocode missing

`ci-cd-pipeline.md` §3.5 documents Gate 10's baseline cascade in
prose only. The crafter must write `apps/prism/scripts/run-stryker.sh`
without explicit pseudocode and the `origin/main` baseline assumption
breaks on stale fork PRs.

**Fix**: add pseudocode showing `origin/main → HEAD~1 → full`
selection logic with skip-when-no-prism-changes early exit.

### CRITICAL-2 — Bundle-size report JSON schema unspecified

`ci-cd-pipeline.md` §3.3 describes `apps/prism/dist/bundle-size-report.json`
shape in prose. Downstream consumers cannot deserialise reliably.

**Fix**: add a TypeScript-style schema block with
`total_gzipped_bytes`, `limit_gzipped_bytes`, `passed`, and
`chunks: Array<{path, gzipped_bytes, percentage_of_limit}>`.

### CRITICAL-3 — Prometheus container digest-pinning mechanism unclear

Gates 7 and 11 both run Prometheus containers but the digest-pinning
mechanism is specified differently (Playwright `globalSetup` vs
GitHub Actions `services.prometheus.image`). No specification ensures
the two gates use the same digest.

**Fix**: clarify in `environments.yaml > external_fixtures` that
both gates MUST share the digest; provide a Playwright `globalSetup`
template; document the digest-bump process (single commit updates
both files).

### CRITICAL-4 — KPI 5 production visibility gap unmitigated

`kpi-instrumentation.md` §6.2 acknowledges KPI 5 has zero production
telemetry: novel failure modes (outside the four documented) are
invisible to operators.

**Fix**: add explicit operator mitigation in `monitoring-alerting.md`
§3 (KPI 5 row): operators MUST enable their existing JS error
tracking tool (Sentry, Bugsnag, or similar) at production; v0.x
graduation adds a `prism.uncaught_error_count` gauge through
Aperture.

### CRITICAL-5 — Pact-JS migration trigger logic ambiguous

`wave-decisions.md` D11 names two triggers (second backend, second
consumer) but lacks a decision rule for v0.x and does not call out
that container-fixture cannot test Mimir-specific shape divergence.

**Fix**: clarify in D11 with explicit IF/ELSE decision tree, name
the Mimir-shape limitation, and assign owner (Andrea wearing
Aperture and Loom hats).

---

## High-severity suggestions

### HIGH-1 — Pre-commit hook wall-clock claim not verified

`branching-strategy.md` §3.1 claims ~30s Rust-only / ~90s full-stack
hook timings. Aspirational, not benchmarked. If actual is 5min,
contributors bypass the hook.

**Fix (inline note)**: Slice 01 crafter benchmarks on a clean
machine and reports actual timings; if >2 min, revisit gate set.

### HIGH-2 — KPI 1/2 fixture flakiness budget unrealistic

`kpi-instrumentation.md` §2.3 says "0 over 100 CI runs". Shared-pool
hardware (ubuntu-latest) makes 0% flakiness unachievable.

**Fix (inline note)**: budget is "≤5% transient spikes" rather than
0; transient spike → re-run; persistent spikes → investigate runner
allocation.

### HIGH-3 — Licence compliance: transitive npm deps not gated

`ci-cd-pipeline.md` §3.4 acknowledges `pnpm audit` is informational
at v0; transitive licence drift is unmonitored.

**Fix (inline note)**: v0.x considers `license-checker` to gate;
not a v0 blocker because core dependencies are all compatible.

---

## Strengths (Radical Candor demands `praise:`)

`praise:` Comprehensive Earned-Trust three-layer enforcement.
Every load-bearing element has subtype + structural + behavioural
defence. Same-origin posture: TS types + grep CI step + Playwright
network log. Browser-emitted metrics: TS type + Vitest header-
redaction test + Playwright 100ms assertion. Bundle size: Gate 8
ceiling + Playwright performance coupling. Mutation gate: TS
exhaustive switch + Gate 10 100% kill rate + surviving-mutant
points to missing test. Gold-standard defensive layering.

`praise:` Existing-infrastructure reuse is principled. CI workflow
extended (not replaced); pre-commit hook conditionally extended (Rust
contributors pay zero cost); Aperture's Strategy C reused for Gates
7 + 11; cargo-mutants pattern mirrored via StrykerJS. No new CI
vendors, registries, or orchestration platforms.

`praise:` KPI instrumentation spans CI and production. Latency KPIs
have CI-fixture structural enforcement AND production telemetry
through Aperture. Invariant KPIs (3, 4, 5) are CI-only because they
are 100%-or-fail. The KPI → Gate → Slice traceability matrix is
complete and bidirectional.

`praise:` Observability design is minimal and sound. Path B (same-
origin POST to `/v1/metrics`) reuses operator's reverse proxy with
no new origin and no CORS preflight. 50-line custom emitter justified
vs OTel-JS browser SDK (bundle constraint). Graceful-degradation
documented if Aperture ingestion path is delayed (D21).

`praise:` Branching strategy honours project realities. Pure trunk-
based without required-status-checks is correct for one person. CI
is feedback, not a gate. Pre-commit hook is contributor-friendly
(skip-with-yellow-warning on missing tools).

`praise:` DORA metrics: no regression. Static-bundle deploy
(`pnpm build + cp dist/`); operator controls cadence. Rollback fast
(no K8s / Docker dependency). Gate 10 mutation testing catches
structural drift; Gate 8 catches size regressions. Fix-forward
absorbs flakes.

---

## Traceability check

| Artefact | Present? | Bidirectional? |
|---|---|---|
| `outcome-kpis.md` (DISCUSS) | yes | yes — every KPI maps to a gate via `kpi-instrumentation.md` §10 |
| ADRs 0026-0032 (DESIGN) | yes | yes — every DEVOPS decision stays within DESIGN locked scope |
| DELIVER contracts (future) | implied | yes — crafter writes YAML/Bash/TS per spec |

Complete.

---

## Verdict

**CONDITIONALLY APPROVED**.

Conditions:
1. Apex resolves all 5 CRITICAL issues within 1 revision cycle.
2. Apex addresses HIGH issues 1-3 as inline notes.
3. No back-propagation to DESIGN required.

Bea's recovery posture: with the stuck-process flag from Andrea on
Scholar's dispatch, Bea finalises the revisions directly rather
than re-dispatching Apex. The 5 CRITICAL fixes are bounded
specification additions; iteration 1's revised artefact set goes
back to Forge for iteration-2 sign-off (Haiku, fast).

Once iteration 2 is APPROVED, DELIVER can begin alongside DISTILL's
own approval gate.
