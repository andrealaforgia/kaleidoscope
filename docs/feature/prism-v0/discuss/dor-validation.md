# Definition of Ready Validation — Prism v0

> **Wave**: DISCUSS — Phase 3 (DoR validation gate before DESIGN handoff).
> **Author**: Luna (`nw-product-owner`); finalised by Bea after Luna's mid-wave overload.
> **Date**: 2026-05-07.
> **Companion documents**: `user-stories.md`, `outcome-kpis.md`, `wave-decisions.md`.

The Definition of Ready is a hard gate before DESIGN. All nine items must pass with evidence, otherwise DISCUSS does not hand off.

---

## Item 1 — Persona named, role specified, context grounded

**Status**: PASS.

**Evidence**: `jtbd-job-stories.md > Persona — the operator on incident call`. Priya Raman, senior SRE at `acme-observability`, specified with role, context (PagerDuty paged at 03:14, 90 s ack budget, 5–10 min triage budget), what is in her hands (laptop, Mimir backend, Prism URL), what is in her head (service map, PromQL fluency), what she does NOT have (patience for new DSLs at 03:14). Postmortem-time engineer specified as the secondary persona in `journey-incident-response.yaml > personas`.

---

## Item 2 — Job-to-be-done framed in job-story format

**Status**: PASS.

**Evidence**: `jtbd-job-stories.md > Primary job` and the three secondary jobs each in canonical "When [situation], I want to [motivation], so I can [outcome]" form. Functional, emotional, and social dimensions explicit per job. Four Forces mapped in `jtbd-four-forces.md`. Opportunity scores in `jtbd-opportunity-scores.md` show the primary job is high-importance + low-current-satisfaction (the strongest wedge); the three secondary jobs are deferred to post-v0.

---

## Item 3 — Journey discovered with mental model, happy path, emotional arc, shared artefacts, error paths

**Status**: PASS.

**Evidence**:

- Mental model: `journey-incident-response-visual.md > The operator's mental model` (Priya thinks in PromQL, expects an alert URL to round-trip to the same view, treats spinner-without-progress as crash).
- Happy path: same file > `Happy path` and `journey-incident-response.yaml > steps`. Six steps from "alert fired" to "decision made"; each step has expected output.
- Emotional arc: `journey-incident-response-visual.md > Emotional arc`. Anxiety on entry → diminishing as data lands → confidence at decision point. Slice 03's calm-error treatment is the slice that protects the arc most explicitly.
- Shared artefacts: `shared-artifacts-registry.md`. URL parameters (`q`, `from`, `to`, `refresh`), `/config.json` shape, the chart's series shape — every cross-step variable has a single documented source.
- Error paths: `journey-incident-response.yaml > error_paths` and `journey-incident-response.feature` Gherkin scenarios for each. Five error paths covered: PromQL parse error, transport failure, empty result, `/config.json` unreachable, browser back/forward navigation through state changes.

---

## Item 4 — Story map produced with backbone, walking skeleton, slices

**Status**: PASS.

**Evidence**: `story-map.md`. Backbone is the six-activity column header (Open Prism / Compose query / Read chart / Iterate / Share + decide / Postmortem). Walking skeleton is the Slice 01 row, end-to-end across all six activities (some trivially). Slices 01–06 each ship one user-observable outcome, sized for one working day.

---

## Item 5 — Elephant carpaccio discipline applied (≤1 day per slice, named learning hypothesis, production data, dogfood moment)

**Status**: PASS.

**Evidence**: each `../slices/slice-NN-name.md` brief has:

- (a) End-to-end value across all six backbone activities.
- (b) ≤1 working day estimate (story-map "6–10 working days across 6 slices" budget).
- (c) Named learning hypothesis ("disproves X if it fails"). Slice 01's hypothesis: "we believe a real Prometheus HTTP API can drive an ECharts line chart with no smoothing, end-to-end, in under 1 day; if it cannot, the React + Vite + ECharts stack pick is wrong, surfacing the highest-risk integration first." Each subsequent slice has its own.
- (d) Production data, not synthetic. Slice 01 queries a real local Prometheus container with a real `up` metric across 24 hours of retention. Subsequent slices add real failure modes (404 from a real backend, timeouts from a real backend) and real PromQL strings.
- (e) Dogfood moment within the same day: each slice ends with Andrea opening Prism on his laptop and running the slice's flow against the local Prometheus.
- (f) Explicit IN / OUT scope lists in every brief.

Slice taste tests (`story-map.md > Scope Assessment`):

- 4+ new components in any slice? No (Slice 01 ships React app shell + query input + ECharts mount + URL serialiser = 4, with the rest deferred; Slices 02-06 add 1-2 components each).
- Every slice depends on a new abstraction? No (the abstractions land at slice 01 in the walking skeleton).
- No slice disproves any pre-commitment? No (every slice has a learning hypothesis).
- Synthetic data only? No (real Prometheus throughout).
- 2+ slices identical except for scale? No (each adds a distinct capability).

All taste tests pass.

---

## Item 6 — User stories drafted with LeanUX format and complete Elevator Pitch

**Status**: PASS.

**Evidence**: `user-stories.md`. Seven stories US-PR-01 through US-PR-07, each in LeanUX format (As [persona] I want [capability] so that [outcome]). Each story has a complete Elevator Pitch (Before / After / Decision enabled) where the After line names the exact UI action and observable output, and the Decision line names a real decision the operator gets to make. No `@infrastructure`-only story.

---

## Item 7 — Acceptance criteria embedded in each story, testable, complete

**Status**: PASS.

**Evidence**: 30 acceptance criteria across seven stories. Every AC is verifiable end-to-end through a developer's laptop or a CI Playwright fixture. Every AC has a slice that ships it (`user-stories.md > Story-to-slice traceability`). Requirements completeness score: 30/30 = 1.00, above the 0.95 threshold.

---

## Item 8 — Outcome KPIs defined with numeric targets and measurement plans

**Status**: PASS.

**Evidence**: `outcome-kpis.md`. Five KPIs with numeric targets and `Measurement Plan` lines that DEVOPS will instrument:

- KPI 1 — first-chart latency p95 < 2s
- KPI 2 — iterate latency p95 < 800ms
- KPI 3 — data fidelity 100% (Vitest structural)
- KPI 4 — URL roundtrip 100% (Playwright)
- KPI 5 — page-stays-usable 100% (Playwright)

Plus three cross-KPI guardrails (operator-time, bundle size, browser matrix).

---

## Item 9 — Story-to-job traceability complete

**Status**: PASS.

**Evidence**: `user-stories.md > Story → Job traceability`. Every story references at least one job from `jtbd-job-stories.md`. Every Phase-1 job has at least one v0 story (the deferred secondary jobs are explicitly post-v0). No orphaned stories, no orphaned jobs in the v0 surface.

---

## Verdict

All nine DoR items: **PASS**. DISCUSS hands off to:

- DESIGN (`@nw-solution-architect` Morgan) with the full artefact set
- DEVOPS (`@nw-platform-architect` Apex) with `outcome-kpis.md` only

Per the orchestrator's parallel-handoff posture, DESIGN and DEVOPS run in parallel after this gate.

The reviewer (`@nw-product-owner-reviewer`) runs before the handoff to validate the artefact set against `nw-product-owner-reviewer`'s checklist (journey coherence, emotional arc quality, DoR completeness, LeanUX antipatterns, story sizing). Bea dispatches the reviewer next.
