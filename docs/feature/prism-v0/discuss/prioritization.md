# Prioritisation — Prism v0

> **Wave**: DISCUSS — Phase 2.5.
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-07.
> **Companion documents**: `story-map.md`, `../slices/slice-*.md`, `outcome-kpis.md`.

---

## Release priority

Order is **outcome impact first, dependency-graph second, riskiest-assumption-first as tie-breaker**. Score = Value × Urgency / Effort, on a 1–5 scale (see `nw-user-story-mapping` skill for the formula).

| Priority | Slice | Target outcome | KPI moved | Value | Urgency | Effort | Score | Rationale |
|---|---|---|---|---|---|---|---|---|
| 1 | Slice 01 — walking skeleton | A real PromQL chart renders end-to-end against a real Prometheus / Mimir backend | KPI 1 | 5 | 5 | 4 | 6.25 | Walking skeleton; lands the highest-risk integration first; until this is green, every other slice is hypothetical. |
| 2 | Slice 02 — time range + relative presets | Operator picks 5 / 15 / 60 / 360 / 1440-minute relative ranges; URL encodes them | KPI 4 (partial) | 5 | 4 | 2 | 10.00 | Highest leverage post-skeleton: cheap to build, central to the operator's mental model, half of the URL-roundtrip property lights up here. |
| 3 | Slice 03 — error and empty states | Inline error rendering for parse / transport / empty-result; page never crashes | KPI 5 | 5 | 5 | 2 | 12.50 | Defuses the strongest demand-reducing force from `jtbd-four-forces.md` (data-fidelity anxiety + session-loss anxiety). High value, urgent (incident-time tool must not crash on bad input), low effort (the catch-and-render plumbing is small). |
| 4 | Slice 04 — auto-refresh | Auto-refresh re-issues same query at chosen interval; relative ranges slide; absolute ranges disable auto-refresh | KPI 3 | 4 | 4 | 3 | 5.33 | High value (incident-time ops watch a chart for minutes), high urgency (slice 03 must land first to make auto-refresh safe under errors), medium effort (timing + cancellation + visibility-API integration). |
| 5 | Slice 05 — absolute time range + full permalink | Operator picks ISO-8601 absolute from-and-to; URL encodes; postmortem reproduction works | KPI 4 (full) | 4 | 3 | 2 | 6.00 | Postmortem use case is real but less urgent than incident-time. Low effort because the picker UI from slice 02 extends naturally. |
| 6 | Slice 06 — accessibility pass | Keyboard-only operability, focus indicators, colour-blind-safe palette, WCAG 2.2 AA contrast | (quality bar) | 3 | 2 | 3 | 2.00 | Mandatory quality bar before v0 ships; spans all prior slices; remediation is more efficient as a single pass than per-slice. |

The Score column ranks slice 03 highest, but slice 01 must come first by **dependency**: there is nothing to render an error for, and no chart to remain usable, until slice 01's substrate exists. Slice 02 is sequenced before 03 by the same dependency reasoning (you need at least the time-range picker to test the error-state rendering on different time-range scenarios). The score column is a sanity check, not the override; the actual order is dependency-respecting.

---

## Backlog — story → slice → KPI mapping

The story IDs (US-PR-01 .. US-PR-07) match `user-stories.md`.

| Story | Slice | Priority | Outcome KPI link | Dependencies |
|---|---|---|---|---|
| US-PR-01 — Fresh page load is fast and focused | Slice 01 (partial), Slice 06 | P1 | KPI 1, KPI 2 | None |
| US-PR-02 — Compose a query with a time range | Slice 01 (default range), Slice 02 (presets), Slice 05 (absolute) | P1, P2, P5 | KPI 1, KPI 4 | None for Slice 01 part; US-PR-01 for the focus behaviour |
| US-PR-03 — PromQL passes through verbatim | Slice 01 (the substrate), Slice 03 (error rendering) | P1, P3 | KPI 2, KPI 3 (fidelity) | None |
| US-PR-04 — Errors render inline; URL stays usable | Slice 03 (mostly), Slice 05 (URL reproduction) | P3 | KPI 4, KPI 5 | US-PR-03 |
| US-PR-05 — Iterate (edit-and-rerun, auto-refresh) | Slice 04 | P4 | KPI 3 | US-PR-02, US-PR-03 |
| US-PR-06 — Single-backend deployment, named explicitly | Slice 01 (the substrate) | P1 | KPI 6 (operator survey) | None |
| US-PR-07 — Accessibility and keyboard operability | Slice 06 (audit pass), with foundations laid throughout | P6 | (quality bar) | All prior |

The mapping is **many-to-many**: most stories span multiple slices, and most slices touch multiple stories. The slice is the thin end-to-end demonstrable unit; the story is the unit of behaviour and of acceptance.

---

## MoSCoW classification

| Story | MoSCoW | Reasoning |
|---|---|---|
| US-PR-01 — Fresh page load | **Must** | Without this, Prism does not work at all |
| US-PR-02 — Compose a query | **Must** | Core action; Prism IS a query panel |
| US-PR-03 — PromQL passes through verbatim | **Must** | Data-fidelity property; without it the operator cannot trust Prism |
| US-PR-04 — Errors render inline | **Must** | Defuses anxiety force; without it Prism fails on first parse error |
| US-PR-05 — Iterate (auto-refresh) | **Must** | Incident-time use is impossible without auto-refresh |
| US-PR-06 — Single-backend, named | **Must** | Trust property: operator must know which backend they are looking at |
| US-PR-07 — Accessibility | **Should** | Mandatory quality bar (WCAG 2.2 AA), but not strictly blocking the walking-skeleton demo |

Six Musts and one Should. No Coulds at v0 — the JTBD analysis was used to cut anything not in the primary job before this prioritisation step.

---

## Risk overlay (where the prioritisation might be wrong)

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| The Prometheus HTTP API has cross-origin (CORS) constraints that the operator must configure on Mimir | Medium | High (slice 01 cannot ship without it) | Slice 01's brief must explicitly list CORS configuration as the operator-side prerequisite; the slice 01 demo command sets it up |
| Apache ECharts' default rendering does smoothing or interpolation we do not want | Low | High (data-fidelity invariant) | Slice 01's brief lists the ECharts config flags that must be off (`smooth: false`, `connectNulls: false`); CI assertion compares rendered points to data.result |
| The chart rendering is too slow for the 2 s p95 first-load budget | Medium | Medium | Slice 06 / pre-launch performance audit; the bundle size and the chart library's first-paint cost are the levers |
| Operators want LogQL / TraceQL at v0 instead of (or in addition to) PromQL | Low | High (would require re-scoping) | Wave brief locks PromQL-only at v0; secondary jobs deferred per `jtbd-job-stories.md`; revisit post-v0 with measured operator data |
| The single-backend assumption is wrong; operators want to query multiple backends from one Prism instance | Low | Medium | Wave brief and `wave-decisions.md` lock single-backend at v0; the URL parameter contract leaves room for `&backend=...` in v1 (non-breaking addition) |

---

## What changes if a slice slips

The slices are sequenced by dependency; if any slice slips:

- **Slice 01 slips**: every later slice is blocked. The whole feature slips.
- **Slice 02 slips**: slice 04 (auto-refresh on relative ranges) is blocked. Slice 03 can proceed against the slice-01 default range.
- **Slice 03 slips**: slice 04 (auto-refresh under transient errors) is technically possible but unsafe; ship slice 04 with a post-launch defect note.
- **Slice 04 slips**: incident-time use case is impaired but slice 05 + slice 06 can proceed; slice 04 is the only slice whose slip degrades a "Must" story.
- **Slice 05 slips**: postmortem reproduction is impaired; v0 still ships, with absolute-range reproduction as a v0.1 addendum.
- **Slice 06 slips**: WCAG audit becomes a launch blocker per the operator's compliance posture (acme-observability is a SaaS company; their compliance bar likely includes accessibility).

---

## Definition-of-Done check at v0 launch

Per the wave brief and the user stories, v0 ships when:

1. All six slices are demonstrable end-to-end from `slices/slice-NN-name.md`.
2. The seven user stories pass DoR (`dor-validation.md` confirms).
3. KPI 1, 3, 4, 5 (the structural KPIs) pass their respective UAT scenarios in CI.
4. KPI 6 (operator survey) baseline is established (interview at least three pilot operators 30 days post-launch).
5. The journey artefacts (`journey-incident-response-visual.md`, `.yaml`, `.feature`) match what the SPA actually does (a manual review against the rendered SPA).
6. SSOT updates (`docs/product/journeys/incident-response.yaml`, `docs/product/jobs.yaml`) are in place.
