# Outcome KPIs — Prism v0

> **Wave**: DISCUSS — Phase 3 (Requirements and User Stories).
> **Author**: Luna (`nw-product-owner`); finalised by Bea after Luna's mid-wave overload.
> **Date**: 2026-05-07.
> **Companion documents**: `user-stories.md`, `journey-incident-response.yaml`, `wave-decisions.md`.

Five KPIs measure whether Prism v0 actually serves the operator's incident-time job. Each KPI has a numeric target and a measurement plan that DEVOPS will instrument.

---

## KPI 1 — first-chart-rendered latency

**What it measures**: how fast the operator sees a chart from a fresh page load.

**Target**: 95th percentile of "page open → first ECharts canvas paint with the backend's data" under 2 seconds, on a developer's laptop against a local Prometheus container with one metric and 24 hours of retention.

**Measured by**: the SPA captures `performance.now()` deltas between `DOMContentLoaded` and the first `series.setData()` call after a successful fetch; emits the delta as a `prism.first_chart_latency_ms` metric to Aperture (which forwards to the backend per Phase 1 — Aperture's role).

**Measurement plan**: DEVOPS instruments a synthetic developer-laptop fixture in CI that runs the Slice 01 acceptance flow and asserts the 95th-percentile delta is under 2 seconds across 20 runs. Flakiness budget: 0 over 100 CI runs.

**Story coverage**: US-PR-01, US-PR-06.

**Slice that lights it up**: 01 (walking skeleton).

---

## KPI 2 — query-to-chart-update latency on iterate

**What it measures**: how fast a re-issued query (after edit) replaces the chart.

**Target**: 95th percentile of "Run pressed → chart updated" under 800 ms on the same fixture as KPI 1.

**Measured by**: the SPA captures `performance.now()` between Run-press and the first `series.setData()` call; emits `prism.iterate_latency_ms`.

**Measurement plan**: same CI fixture as KPI 1; assertion runs across 20 iterate cycles, 95th percentile under 800 ms.

**Story coverage**: US-PR-01.

**Slice that lights it up**: 01.

---

## KPI 3 — data-fidelity invariant (no client-side smoothing)

**What it measures**: does the rendered chart match the backend's `data.result.values` byte-for-byte, with zero client-side smoothing, interpolation, or aggregation?

**Target**: 100%. Any deviation is a bug.

**Measured by**: a Vitest unit test that mocks the backend's HTTP layer with a known fixture (e.g. five points: 1, NaN, 3, NaN, 5) and asserts the chart's rendered series has exactly those five points in the order returned, with no NaN-interpolation, no Lerp, no Bezier.

**Measurement plan**: the test suite catches every regression at PR time. CI fails if the test fails. There is no production telemetry for this invariant because the unit test is the structural enforcement.

**Story coverage**: US-PR-03, US-PR-05 (under auto-refresh ticks).

**Slices that light it up**: 01, 03, 04.

---

## KPI 4 — URL roundtrip fidelity

**What it measures**: does pasting a Prism URL into a fresh tab reproduce exactly the same view (query, time range, refresh interval, backend)?

**Target**: 100% on within-session reload. For absolute time ranges, 100% on cross-day reload provided the backend's retention covers the range.

**Measured by**: a Playwright test opens Prism with a hard-coded URL (encoded query + absolute range), waits for the chart, snapshots the chart's rendered series; opens a fresh page with the same URL and asserts the snapshot matches.

**Measurement plan**: Playwright suite runs in CI; the assertion is byte-equality on the rendered series JSON.

**Story coverage**: US-PR-04, US-PR-02 (URL encodes the picker state).

**Slices that light it up**: 02 (relative roundtrip), 05 (absolute roundtrip).

---

## KPI 5 — page-stays-usable invariant under failure

**What it measures**: does Prism stay usable through every documented failure mode, without crashing or losing the operator's session?

**Target**: 100%. Every failure mode in `journey-incident-response.yaml` and `slice-03-error-and-empty-states.md` produces a calm inline state, NOT a blank page, a JavaScript exception, or a stuck spinner.

**Measured by**: Playwright tests that drive each failure mode (PromQL parse error, transport failure, empty result, `/config.json` unreachable) and assert (a) the SPA does not crash (no uncaught console errors), (b) the query input remains focused or refocusable, (c) the URL still encodes the broken state so it is shareable.

**Measurement plan**: Playwright suite in CI; assertions cover the four failure modes plus the cumulative state after several failures in sequence.

**Story coverage**: US-PR-03, US-PR-06.

**Slices that light it up**: 03, 06 (accessibility audit confirms keyboard recoverability from every state).

---

## Cross-KPI guardrails

- **Operator-time** (cross-cutting): the entire walking-skeleton flow (open Prism → type query → see chart) takes under 5 seconds median on a developer's laptop. KPIs 1 + 2 together cover the budget; this guardrail is the synthetic upper bound for the four-forces "demand" arrow in `jtbd-four-forces.md`.

- **Bundle size** (cross-cutting): Prism's gzipped JS bundle is under 300 KB at v0. ECharts is ~200 KB; the rest of the SPA fits in the remaining 100 KB. CI fails the bundle-size budget if exceeded.

- **Browser support** (cross-cutting): Prism v0 supports the latest two stable versions of Chrome, Firefox, and Safari. No IE, no legacy Edge. Playwright runs on all three engines in CI.

---

## DEVOPS handoff

DEVOPS receives only this file (per the orchestrator's parallel-handoff design: DESIGN gets the full DISCUSS artefact set; DEVOPS gets only the KPI file). DEVOPS designs:

- The data-collection pipeline for KPIs 1, 2, 3 (SPA-emitted metrics through Aperture to the backend).
- The Playwright fixture in CI for KPIs 4, 5 (browser-based assertion).
- The Vitest fixture in CI for KPI 3's structural enforcement (unit test against a mocked backend).
- The bundle-size budget gate in CI.
- The browser-matrix Playwright runner.

Each KPI's `Measured By` and `Measurement Plan` sections are the DEVOPS spec.
