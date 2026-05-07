# Peer review — Prism v0 DESIGN, iteration 1

- **Date**: 2026-05-08
- **Reviewer**: `@nw-solution-architect-reviewer` (Atlas), Haiku model
- **Wave**: DESIGN — gate before DEVOPS + DISTILL parallel handoff
- **Artefact set**: 7 ADRs (0026-0032) + 3 wave-scoped documents (component-design, workspace-layout, wave-decisions); 10 files, ~140 KB total
- **Verdict**: **APPROVED** — proceed to DEVOPS + DISTILL parallel handoff
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

The DESIGN wave is architecturally sound with no critical or
high-severity issues. The seven ADRs form a coherent cluster,
ports-and-adapters internal structure is enforced (not merely named),
KPIs are structurally locked, downstream handoffs are correctly
routed, and the British-English house style is consistent. For a
first-frontend feature, this is exceptional work — Morgan delivered a
production-ready architecture in one pass without stalling, breaking
the recovery-pattern streak that had hit 5 prior dispatches.

The design is ready for parallel DEVOPS + DISTILL handoff. Four
non-blocking suggestions for downstream waves to consider.

---

## Strengths

`praise:` Exceptional clarity in error classification (ADR-0027). The
`QueryOutcome` discriminated union with five arms plus the five-case
flowchart in §4 makes every rendering path explicit. Mutation testing
at DELIVER will catch dropped arms. Textbook exhaustive sum-type design.

`praise:` Pure-function cores with structural testing (ADR-0028, 0029,
0030). Three leaves of the architecture (`url-state` codec,
`echarts/buildOption`, `auto-refresh` reducer) are genuinely pure
functions with property tests and mutation-test coverage planned. This
maximises testability and minimises the scope of integration testing.

`praise:` Earned-trust three-layer enforcement applied consistently
(wave-decisions.md § Earned-Trust). All three driven adapters name
their lie surfaces, probes, and "wire then probe then use" gates. The
composition root (`main.tsx`) is the structural enforcement point.
This pattern is recognisable from the Rust codebase and correctly
adapted to TypeScript.

`praise:` Dependency-direction enforcement named explicitly (ADR-0031
§7). The `eslint-plugin-boundaries` rule is the language-appropriate
enforcement of ADR-0026's split. The rule is named, the configuration
is sketched, the CI gate is clear. Principle 11 is honoured.

`praise:` Bundle-size constraint is quantified and escapeable (ADR-0030
§7, wave-decisions.md). The 300 KB gate is justified with composition
breakdown (67% ECharts, 20% React, 13% Prism). The escape hatch
(lazy-import) is named and its boundary is clean. Not a wish; a plan.

`praise:` CORS posture is operationally sound (ADR-0027 §5). Same-
origin reverse-proxy is the default (matches operator expectations
from Grafana/Prometheus deployments). Cross-origin is an operator
opt-in. Minimum-surprise design.

`praise:` Slice → ADR → module mapping is exhaustive (component-design
§12). Every slice routes to touched modules; every module is justified
by at least one slice. No orphan complexity.

`praise:` Rejection rationale specific to Prism's scope. Microservices
rejected because "no team boundary to respect (one operator, one
Andrea, one designer)"; SSR rejected because "the operator runs Prism
behind their reverse proxy; SSR adds a Node runtime for no
incident-time benefit"; micro-frontends rejected as a v1+ refactor
when `packages/ui/` becomes load-bearing. Each rejection is specific,
not generic "they're complex".

---

## Detailed findings

### ADR coherence — cross-ADR consistency: PASS

ADR-0027's `QueryOutcome` matches ADR-0029's transport-error trigger
(reducer keys off `outcome.kind === 'transport-error'`). ADR-0028's
URL vocabulary (`q`/`from`/`to`/`refresh`) is consumed cleanly by
ADR-0027's request builder and ADR-0029's range-changed event.
ADR-0029's absolute-range disable chains to ADR-0028's serializer
rule "refresh NEVER emitted on absolute ranges" at both picker and
reducer (double lock). No contradictions detected.

### Design discipline — ports-and-adapters: PASS

Driving vs driven boundaries are explicit and testable. Pure-function
leaves are genuinely pure (no React, no DOM, no I/O). Adapter test
seams (`fetchFn`, `Scheduler`, instance control) are named.

Minor observation (non-blocking): ADR-0026 §3 notes that
`lib/echarts/` straddles the line — option builder pure, wrapper
imperative. Mitigation (separate files: `buildOption.ts` vs
`EChart.tsx`) is sound. No redesign needed.

### DISCUSS-to-DESIGN traceability: PASS

Every user story (US-PR-01..07), KPI (1-5), and slice (01-06) routes
to an ADR or module. KPIs have structural enforcement, not
aspirational pinning:

- KPI 1 (first-chart < 2s p95) — tree-shaken ECharts + pure option builder + no state-mgmt library
- KPI 2 (iterate < 800 ms) — `setOption({notMerge:true})` + no re-mount
- KPI 3 (data fidelity 100%) — `smooth:false`, `connectNulls:false`, no auto-downsampling locked in option builder
- KPI 4 (URL roundtrip 100%) — `encode(decode(URL)) === URL` property test on the codec
- KPI 5 (page-stays-usable 100%) — total-function `QueryOutcome` (no throws) + error boundary + malformed-URL calm banner

### First-frontend-feature paradigm consistency: PASS

Rust patterns correctly inherited: exact-minor pinning, pre-commit
hooks, AGPL file-level headers, composition root + wiring probes,
Nygard ADR template. Rust patterns intentionally NOT over-applied: no
`#[derive]` macros forced, no pnpm equivalent of `cargo deny` (audit
is separate per ADR-0031), no monorepo orchestrator at v0 (deferred
when 3+ packages).

### Earned-trust three-layer enforcement: PASS

For `/api/v1/query_range` client, `/config.json` loader, ECharts
wrapper, and header redaction, each constraint names a subtype check,
a structural check, and a behavioural check. No single-layer
enforcement.

### Open items routed downstream: PASS

All routed items are genuinely downstream. To crafter: Vite plugin
set, CSS scoping, `useReducer` shape, `Result` type location. To
Apex: CI YAML, bundle-size script, Playwright `globalSetup`,
contract-testing posture (Pact-JS vs container fixture), KPI metric
emission. To Scholar: Gherkin scenarios, KPI 3 fixture, KPI 4
Playwright spec. None are architectural decisions in disguise.

### External integration handoff: PASS

ADR-0027's external-integration handoff to Apex is specific. Names
two testing approaches (Pact-JS for cross-service contracts;
container-fixture for single backend). Identifies the four known
response shapes. Apex has enough guidance to pick.

### Bundle-size gate plausibility: PASS

Quantified composition (67% ECharts, 20% React + react-router-dom, 13%
Prism source). Tree-shaking enforcement is direct modular ECharts
imports (LineChart + 6 components, not full bundle). CI script in
`apps/prism/scripts/check-bundle-size.ts` enforces the gate.

### Architectural-style choice: PASS

Modular monolith with ports-and-adapters internal structure, justified
by Prism's actual scope (one panel, one endpoint, one bundle).
Microservices, SSR, micro-frontends rejected with specific rationale.

### British English + house style: PASS

Spot check across artefacts: "colour", "behaviour", "-ise" suffix,
serial comma usage, em-dash (no surrounding spaces), code formatting
all consistent. No systematic deviations.

---

## Non-blocking suggestions

`suggestion (non-blocking):` ADR-0028 §4 encoding rule belt-and-
suspenders. The "refresh NEVER emitted on absolute ranges" rule is
double-locked (picker + serializer). Consider a Vitest assertion that
the encoder cannot produce a `refresh` parameter when `range.kind ===
'absolute'`, even if the input is malformed. The double lock is
sufficient; the third lock is reinforcement for KPI 5 invariant
defence-in-depth.

`suggestion (non-blocking):` ADR-0027 §2 `queryMs` measurement
documentation. The field is correctly present on every `QueryOutcome`
kind. Consider documenting in Verification § how `queryMs` is
measured (wall-clock time from `fetch()` start to response ready? to
JSON parse completion?). For crafter clarity during GREEN, not a
design gap.

`suggestion (non-blocking):` ADR-0029 §8 countdown-timer lifecycle.
The status-line countdown is described as separate from the state-
machine timer (intentional; prevents chart re-fetch every second).
Consider a Vitest test asserting both timers are cleaned up on
unmount (no timer leaks). The property test in Verification is good;
this is a unit-level companion.

`suggestion (non-blocking):` Wave-decisions.md recovery posture note.
Morgan completed without stalling on this dispatch — sixth dispatch
where the pattern could have hit, first to break the streak. The note
in wave-decisions.md is accurate. Keep the note as a template for
future handoffs; it documents the methodology's recovery affordance
even when not exercised.

---

## Verdict

**APPROVED** for parallel DEVOPS + DISTILL handoff.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.

Bea dispatches in parallel:

- `@nw-platform-architect` (Apex) — DEVOPS wave with the full DESIGN
  artefact set + the DISCUSS `outcome-kpis.md`. Apex designs CI YAML,
  bundle-size enforcement, Playwright `globalSetup`, browser-matrix
  sharding, Prometheus contract-testing posture, KPI metric emission
  pipeline.
- `@nw-acceptance-designer` (Scholar) — DISTILL wave with the full
  DESIGN artefact set + the user-stories.md. Scholar writes the
  acceptance-test scenarios for the six slices: Vitest unit tests
  against the pure cores, Playwright E2E against the rendered SPA, the
  KPI 3 fixture (data-fidelity), the KPI 4 fixture (URL roundtrip).

Both proceed in parallel after this gate.

---

**Reviewer**: Atlas (Solution Architecture Reviewer)
**Confidence**: High. All ten artefacts read in full; all ten review
scope items checked; evidence cited for every judgment.
