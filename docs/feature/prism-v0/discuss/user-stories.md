# User Stories — Prism v0

> **Wave**: DISCUSS — Phase 3 (Requirements and User Stories).
> **Author**: Luna (`nw-product-owner`); finalised by Bea (orchestrator) after Luna's mid-wave overload at the slice-mapping boundary.
> **Date**: 2026-05-07.
> **Companion documents**: `story-map.md`, `journey-incident-response.yaml`, `dor-validation.md`, `outcome-kpis.md`, `wave-decisions.md`.

## Reading guide

Seven user stories cover Prism v0's six slices. Each story's "Companion slices" line points to the slice files in `../slices/` that ship the story end-to-end. The walking skeleton (Slice 01) lights up four stories at the thinnest possible level; subsequent slices deepen specific stories. No story is attached to a single slice in isolation — every story is sliced thinly across the carpaccio.

Every story has a complete **Elevator Pitch**: Before, After (the exact UI action and its observable output), and the Decision the operator gets to make with that output. The Decision is the JTBD connection — if it is not real, the story is infrastructure, not value.

Personas: **Priya Raman** (the senior SRE on the on-call rota at `acme-observability`), per `jtbd-job-stories.md`. The postmortem-time engineer is the secondary persona, surfacing in US-PR-04.

---

## US-PR-01 — query → chart, end-to-end

**As** Priya, the on-call SRE,
**I want** to type a PromQL query into a focused input and see the result rendered as a line chart from a real Prometheus / Mimir backend,
**so that** I can see the shape of the misbehaving signal within seconds of opening Prism.

### Elevator Pitch

Before: Priya cannot see Kaleidoscope-routed telemetry through Prism at all; the SPA renders no chart.
After: open `https://prism.acme-observability.internal`, type `up` into the query input, press Enter → sees an Apache ECharts line chart with the points the backend returned (no smoothing, no interpolation).
Decision enabled: which metric to investigate next, based on whether `up` shows the failing service flat at 0 or 1.

### Acceptance criteria

- AC-1.1: Typing a non-empty PromQL string into the focused query input and pressing Enter (or clicking Run) issues a single `GET /api/v1/query_range` to the configured backend.
- AC-1.2: The backend's `application/json` response with `status:"success"` and a non-empty `data.result` array renders as a line chart with one series per labelled timeseries.
- AC-1.3: The chart points are exactly the values the backend returned, in the order returned. No client-side smoothing, no interpolation across gaps, no aggregation.
- AC-1.4: The whole round-trip from Enter-press to chart-rendered completes in under 1 second on the developer's laptop against a local Prometheus container with one metric and 24 hours of retention.

### Companion slices

- Slice 01 (walking skeleton) ships AC-1.1, AC-1.2, AC-1.3, AC-1.4 against the default 15-min relative range.

---

## US-PR-02 — pick the time range

**As** Priya,
**I want** to pick the chart's time range from a small set of operator-canonical relative presets (Last 5 min, 15 min, 1 h, 6 h, 24 h) and from absolute ISO-8601 timestamps,
**so that** I can zoom into the alert window without typing dates by hand at 03:14, and lock the window for postmortem URLs.

### Elevator Pitch

Before: Priya is stuck on whatever the page chose by default; she cannot widen to 24h to see prior baseline or narrow to 5m to focus on the spike.
After: click the time-range picker dropdown → pick "Last 6 h" or type a custom ISO range (e.g. `2026-05-07T03:00 → 03:15`) → chart re-fetches with the new range, URL updates with `from`/`to` parameters.
Decision enabled: whether the spike is sustained (24h shows it; 5m only shows the noise) or transient (5m shows it; 24h flattens it into the baseline).

### Acceptance criteria

- AC-2.1: The picker offers the five relative presets and a "Custom" mode for absolute ISO-8601 timestamps.
- AC-2.2: A change in the picker re-fetches the chart with the new range; the URL updates synchronously so a copy-paste of the URL reproduces the picked range.
- AC-2.3: For relative presets, the URL encodes `from=now-Xs` and `to=now`; the chart slides forward on auto-refresh ticks (Slice 04).
- AC-2.4: For absolute ranges, the URL encodes `from=<iso>` and `to=<iso>`; auto-refresh is disabled at the picker level.
- AC-2.5: An invalid custom-mode entry (e.g. malformed ISO) is rejected at the picker boundary; the chart is not re-fetched until the entry is valid.

### Companion slices

- Slice 02 ships AC-2.1 (relative presets only), AC-2.2 (relative URL roundtrip), AC-2.3.
- Slice 05 ships AC-2.1 (Custom mode), AC-2.4, AC-2.5 (absolute URL roundtrip + auto-refresh disable).

---

## US-PR-03 — trust the data: fidelity and calm errors

**As** Priya,
**I want** Prism to either show me the backend's data verbatim or show me a calm, inline error message naming what went wrong,
**so that** I can trust what I see during an incident and decide whether to retry, fix the query, or check whether the backend itself is unhealthy.

### Elevator Pitch

Before: Priya cannot tell whether a wobble in the line is real (the system) or rendered (the SPA); a malformed query crashes the page and loses her session.
After: a malformed query → inline warning showing the backend's error text verbatim, query input keeps focus, URL still encodes the broken query so it can be shared. A backend unreachable → inline warning naming the backend URL and transport error, no stale chart shown. An empty result → calm "No data for {range}" message, not an error banner.
Decision enabled: whether to fix the query (inline parse error), check the backend's health (transport error), or widen the range (empty result).

### Acceptance criteria

- AC-3.1: The chart's points match the backend's `data.result.values` byte-for-byte, with no client-side smoothing, interpolation, or aggregation.
- AC-3.2: A backend response with `status:"error"` renders as an inline warning banner above the chart area, showing the backend's `error` field verbatim. The chart area shows a calm fallback message; the previous successful chart (if any) is hidden.
- AC-3.3: A transport-level failure (DNS, TCP refused, TLS, 5xx) renders as an inline warning naming the backend URL and the transport-level error class. The chart area shows `Last successful fetch: {iso_timestamp}` if any prior fetch succeeded; otherwise an empty state.
- AC-3.4: A backend response with `data.result: []` renders as `No data for {range_iso}. Check the metric name or widen the range.` — without a warning banner (this is not an error).
- AC-3.5: At no point does Prism show a stale successful chart alongside a transport error. Stale charts lie.

### Companion slices

- Slice 01 ships AC-3.1 (data fidelity).
- Slice 03 ships AC-3.2, AC-3.3, AC-3.4, AC-3.5.

---

## US-PR-04 — share the view: permalink and postmortem reproduction

**As** Priya during the incident, **and as** the postmortem-time engineer days later,
**I want** the URL bar to encode the entire viewable state of the page (query, time range, refresh interval, backend selector if applicable),
**so that** I can paste the URL into Slack at 03:14 and a teammate sees exactly what I see, and the postmortem-time engineer can reproduce the same chart days later from the same URL.

### Elevator Pitch

Before: Priya cannot share what she sees; a Slack screenshot is static and leaves the teammate with no way to interact with the data.
After: copy the URL bar from Prism → paste into Slack → teammate clicks → sees the same chart, against the same backend, with the same range and the same query. Days later in the postmortem doc, the same paste produces the same chart provably (because absolute ranges encode timestamps, not relative-now).
Decision enabled: whether the teammate's read of the same data agrees with Priya's; whether the postmortem timeline holds up against the data Priya saw at 03:14.

### Acceptance criteria

- AC-4.1: Every state-affecting picker change (query, time range, refresh interval) updates the URL synchronously via `history.replaceState`.
- AC-4.2: A fresh page load against the same URL reproduces the same chart, against the same backend, with the same range, in the same query state.
- AC-4.3: For absolute time ranges, AC-4.2 holds at any later time provided the backend's retention window covers the range. For relative ranges, the chart shows the analogous "now-relative" view (which is the desired semantic for live monitoring, not for postmortem reproduction).
- AC-4.4: The URL is the only "save" / "share" mechanism at v0; there is no Prism-side saved-queries or shared-dashboard surface.

### Companion slices

- Slice 01 ships AC-4.1 (query + default range), AC-4.2 (within-session reload).
- Slice 02 ships AC-4.1 (relative range encoded), AC-4.2 (cross-session reload with relative range).
- Slice 05 ships AC-4.1 (absolute range encoded), AC-4.3 (postmortem-time reproduction).

---

## US-PR-05 — auto-refresh without flicker

**As** Priya watching a live chart during a sustained incident,
**I want** the chart to refresh itself at a chosen interval (5 s, 10 s, 30 s, 1 min) without flickering or losing my scroll position,
**so that** I can keep my eyes on the line and not on the F5 key, and I can be confident the data behind every tick is the backend's data, not a smoothed client-side average.

### Elevator Pitch

Before: Priya must press F5 every few seconds to see new data; each F5 wipes the chart, the legend, and any interaction state, and she has to re-orient.
After: pick "Auto: 10 s" → the chart re-fetches every 10 s in place; the line updates, the legend stays, the scroll position holds; the URL encodes the refresh interval so a teammate landing on the URL gets the same auto-refresh behaviour.
Decision enabled: whether the misbehaving signal is recovering (line trending back to baseline) or worsening (line still climbing), without disrupting Priya's read.

### Acceptance criteria

- AC-5.1: An auto-refresh picker offers the four intervals plus "Off"; the URL encodes the picked interval as `refresh=Xs`.
- AC-5.2: Each tick re-fetches the same query against the same backend with the same range. For relative ranges, `to=now` slides forward; for absolute ranges, the picker disables auto-refresh.
- AC-5.3: Mid-tick rendering does not flicker the chart; React updates the data without re-mounting the ECharts instance.
- AC-5.4: Auto-refresh pauses while the browser tab is in the background (per the `Page Visibility API`), and resumes (with a fresh fetch) when the tab returns to foreground.
- AC-5.5: Every tick honours the same fidelity invariants as the initial fetch: no client-side smoothing, no caching, no interpolation across ticks.

### Companion slices

- Slice 04 ships AC-5.1, AC-5.2, AC-5.3, AC-5.4, AC-5.5.

---

## US-PR-06 — page chrome: backend identification

**As** Priya,
**I want** the page chrome to show the backend label (which Prometheus / Mimir Prism is querying) and the Prism version,
**so that** I can confirm I am looking at the right backend before I trust the chart, and tell teammates which Prism version a screenshot came from.

### Elevator Pitch

Before: Priya cannot tell from the page whether the chart is against `prom-prod`, `prom-staging`, or some leftover dev backend; she also cannot tell which Prism version produced the screenshot she just pasted.
After: open Prism → page chrome shows `Backend: prom-prod` and `Prism v0.1.0` (read from `/config.json`).
Decision enabled: whether to retry against a different backend (wrong backend), or to accept the chart as authoritative (right backend).

### Acceptance criteria

- AC-6.1: On page load, Prism fetches `/config.json` and displays the `backend.label` and the Prism version it was built with.
- AC-6.2: If `/config.json` is unreachable, Prism shows a calm error state naming the missing config; it does NOT silently fall back to a hard-coded backend.
- AC-6.3: The backend label is visible on every page state including error states.

### Companion slices

- Slice 01 ships AC-6.1, AC-6.3.
- Slice 03 ships AC-6.2 (error state when config is unreachable).

---

## US-PR-07 — accessibility: keyboard, contrast, screen reader

**As** any operator (including Priya, including teammates with low vision, motor impairment, or colour-blindness),
**I want** Prism to be fully operable with a keyboard, readable with a screen reader, and legible on a colour-blind-safe palette,
**so that** every operator on the rota — not only sighted, mouse-using, full-colour-vision operators — can run a query and read a chart at 03:14.

### Elevator Pitch

Before: Prism's interactive elements may have no visible focus indicators, no keyboard shortcuts, no screen-reader labels; the chart palette may be red-green collision-prone.
After: every interactive element shows a visible focus ring on Tab; every chart has a screen-reader-readable summary; the palette is colour-blind-safe (Okabe-Ito or equivalent); contrast ratios meet WCAG 2.2 AA on every text-on-background pair.
Decision enabled: whether to use Prism in production at `acme-observability` (a deployment with operators who use screen readers and keyboard-only navigation). Without AC-7, Prism is unsuitable for that production.

### Acceptance criteria

- AC-7.1: Every interactive element (query input, picker dropdown, run button, refresh picker) has a visible focus indicator and is reachable in a logical Tab order.
- AC-7.2: The chart has an accessible name and a textual summary alternative; an operator with a screen reader can read the highest, lowest, and most recent point's value and timestamp.
- AC-7.3: The chart palette is colour-blind-safe (no red-green collisions); selectable from at least two colour-blind-safe presets.
- AC-7.4: All text-on-background pairs meet WCAG 2.2 AA contrast ratios (4.5:1 normal text, 3:1 large text).
- AC-7.5: Animations honour `prefers-reduced-motion: reduce`; the auto-refresh tick is silent and does not animate.
- AC-7.6: The end-to-end keyboard-only journey (page load → query → range pick → run → read chart → copy URL) has no mouse-only step.

### Companion slices

- Slice 06 audits AC-7.1 through AC-7.6 across the cumulative Slice 01-05 surface and remediates anything that fails.

---

## Story-to-slice traceability

| Story | Slices | KPI(s) moved |
|---|---|---|
| US-PR-01 query → chart | 01 | KPI 1 |
| US-PR-02 time range pick | 02, 05 | KPI 4 (partial then full) |
| US-PR-03 data fidelity + calm errors | 01, 03 | KPI 3, KPI 5 |
| US-PR-04 URL permalink + postmortem | 01, 02, 05 | KPI 4 |
| US-PR-05 auto-refresh | 04 | KPI 3 |
| US-PR-06 backend chrome | 01, 03 | KPI 1, KPI 5 |
| US-PR-07 accessibility | 06 | (no behavioural KPI; quality bar) |

KPI definitions in [`outcome-kpis.md`](outcome-kpis.md).

---

## Story → Job traceability

Per the Phase-1 JTBD analysis (`jtbd-job-stories.md`):

- **Primary job — "see the shape of the signal"**: served by US-PR-01, US-PR-02, US-PR-03, US-PR-05, US-PR-06.
- **Secondary job — "share what I'm seeing"**: served by US-PR-04.
- **Quality-bar requirement (cross-cutting)**: served by US-PR-07.

No user story is orphaned from a job; every job has at least one story. The Phase-1 JTBD document's explicitly-deferred secondary jobs (logs panel, traces panel, saved queries) have no v0 stories; they are post-v0 by design.

---

## Requirements completeness score

`completeness = covered_acceptance_criteria / total_acceptance_criteria = 30 / 30 = 1.00`.

Above the 0.95 threshold. Every AC across the seven stories has a slice that ships it. No AC is unscoped or hand-waved to a future slice.
