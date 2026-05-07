# Story Map — Prism v0

> **Wave**: DISCUSS — Phase 2.5 (User Story Mapping + Elephant Carpaccio).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-07.
> **Companion documents**: `prioritization.md`, `../slices/slice-*.md`, `journey-incident-response.yaml`, `wave-decisions.md`.

---

## User: senior SRE on the on-call rota (Priya Raman, `acme-observability`), with the postmortem-time engineer as a secondary persona

## Goal: see the shape of a misbehaving signal, fast enough to make a triage decision within five minutes of a paging alert, against the same Mimir / Prometheus backend the alert fired on, with a shareable URL as the artefact

---

## Backbone

The map's six activities, left-to-right, taken from `journey-incident-response.yaml`. Each column is one activity in the operator's journey through Prism; each row is one slice of incremental capability across all activities.

| 1. Open Prism | 2. Compose query | 3. Read chart | 4. Iterate | 5. Share + decide | 6. Postmortem |
|---|---|---|---|---|---|
| Load SPA bundle | Type a PromQL string | Render line chart | Edit-and-rerun | Copy URL | Reproduce view from URL |
| Fetch `/config.json` | Pick relative time range | Render legend by labels | (reuse compose) | (reuse from State C URL) | Read absolute time range from URL |
| Focus query input | Pick absolute time range | Footer: series, points, latency | Toggle auto-refresh on/off | (URL is already permalink) | (reuse all) |
| Default 15-min range | Run on Enter / Run button | Empty-state message | Auto-refresh interval picker | (no Prism-side share button) | (reuse all) |
| Show backend label | Inline syntax error | Backend-unreachable error | Pause auto-refresh in bg tab | | |

Walking-skeleton row is **Slice 01**. Each subsequent slice is a thin end-to-end slice that adds capability to one or two columns while keeping the rest functioning.

---

## Walking Skeleton — Slice 01

The thinnest possible end-to-end slice, with all six activities lit (some trivially):

1. **Open Prism** — SPA bundle loads at the operator's URL; `/config.json` is fetched; chrome shows backend label and Prism version.
2. **Compose query** — query input is focused, time-range picker defaults to "Last 15 min" (relative), Run button is enabled when query is non-empty.
3. **Read chart** — the operator pastes a known-good PromQL query (e.g. `up`), presses Run, sees a line chart rendered from a real Prometheus / Mimir backend's `/api/v1/query_range` response. **Real backend, not mocked.** This matches Strategy C "real local" posture from prior features (Aperture, Sieve), adapted to the browser: the SPA queries a real local Prometheus instance running in the dev environment.
4. **Iterate** — at slice 01 this is trivial: re-pressing Run re-fetches.
5. **Share + decide** — the URL is updated to encode the query, time range, and (implicitly) backend; copying it works because the browser does that for free.
6. **Postmortem** — at slice 01 this is trivial: a fresh-tab paste of the same URL reproduces the same view.

Andrea's recommendation in the wave brief was: "Slice 01 = render a single static PromQL query result (a known-good Prom timeseries) as a line chart in the browser, against a real Mimir or Prometheus instance the operator already runs". Real backend, not mocked. This is the slice 01 contract.

The acceptance proof for Slice 01 is in [`../slices/slice-01-walking-skeleton.md`](../slices/slice-01-walking-skeleton.md): a developer runs a real local Prometheus container, deploys Prism v0 in dev mode, opens the Prism URL, types `up` into the query input, presses Run, sees a line chart with one or more series. The DEMO command produces a screenshot identical to the State-C mockup in `journey-incident-response-visual.md`.

---

## Release slices (one per file in `../slices/slice-NN-name.md`)

Each slice is sized to be demonstrable in a single session and to deliver one verifiable user-observable capability. Each is a thin end-to-end slice across the six activities; none is a single-column vertical.

| # | Slice | Outcome added | KPI moved |
|---|---|---|---|
| 01 | `slice-01-walking-skeleton.md` | Operator sees a chart from a real Prometheus query, end-to-end | KPI 1 — first chart rendered from real backend |
| 02 | `slice-02-time-range-and-relative-presets.md` | Operator picks 5 min / 15 min / 1 h / 6 h / 24 h relative time ranges; URL encodes them | KPI 4 — URL roundtrip (partial) |
| 03 | `slice-03-error-and-empty-states.md` | PromQL parse errors, transport errors, and empty-results states render inline; page never crashes | KPI 5 — page-stays-usable invariant |
| 04 | `slice-04-auto-refresh.md` | Auto-refresh ticker re-issues the same query at a chosen interval; relative-range `to=NOW` slides; absolute ranges disable auto-refresh | KPI 3 — fidelity invariant under auto-refresh |
| 05 | `slice-05-absolute-time-range-and-permalink.md` | Operator chooses absolute from-and-to; URL encodes ISO-8601; postmortem-time reproduction works | KPI 4 — URL roundtrip (full) |
| 06 | `slice-06-accessibility-pass.md` | Keyboard-only operability, focus indicators, colour-blind-safe palette, WCAG 2.2 AA contrast | (no behavioural KPI; operator-facing quality bar) |

Six slices total. Slice 01 is the walking skeleton; Slices 02–06 each add one concrete user-observable capability.

> **Out-of-scope for v0** (deferred to post-v0 slices or later phases):
>
> - Saved queries surface (URL paste is the v0 substitute)
> - LogQL / log-tail panel (post-v0 slice; depends on Loki access)
> - TraceQL / trace waterfall (Phase 5 — Ray)
> - Exemplar deep-links (Phase 6 — Strata)
> - Multi-panel dashboards (Loom — Phase 2)
> - Authn / authz (Aegis — Phase 2; v0 inherits the reverse proxy's auth)

---

## Priority Rationale

Order is **outcome impact first, dependency-graph second, riskiest-assumption-first as tie-breaker**. The full Value × Urgency / Effort table is in [`prioritization.md`](prioritization.md); the rationale for the ordering is here.

1. **Slice 01 (walking skeleton)** is first because Andrea chose this shape explicitly: a real backend at slice 01 lands the highest-risk integration (browser SPA → external Prometheus HTTP API + cross-origin handling + auth headers + JSON parsing + ECharts setup) at slice 01 rather than late. Until Slice 01 is green, no other slice has a substrate to add to. KPI 1 (first chart rendered) is the walking-skeleton tripwire.

2. **Slice 02 (time range + relative presets)** is second because the operator's first-load is the moment-of-truth for the journey's emotional arc. A fresh Prism page must default to a sensible time range and offer the operator-canonical presets (5 min / 15 min / 1 h / 6 h / 24 h). Without slice 02, slice 01's chart is rendered against a hard-coded range, which is incident-time hostile. KPI 4's "URL roundtrip" partially lights up here (relative ranges encode in the URL).

3. **Slice 03 (error + empty states)** is third because the data-fidelity anxiety identified in `jtbd-four-forces.md` ("what if the SPA crashes on a malformed query and I lose my session?") is the strongest demand-reducing force. Slice 03 defuses it. KPI 5's "page-stays-usable" invariant goes from null to enforced.

4. **Slice 04 (auto-refresh)** is fourth because incident-time ops staring at a chart for 5 minutes need it to update without F5. The fidelity-under-refresh invariant (KPI 3 — no client-side smoothing across ticks) is the heart of why the operator can trust auto-refresh; slice 04 lands both the feature and the invariant.

5. **Slice 05 (absolute time range + full permalink)** is fifth because postmortem-time URL reproduction is the social-shareable property that makes Prism a teammate, not a private tool. KPI 4 fully lights up here.

6. **Slice 06 (accessibility pass)** is last because the substrate (slices 01–05) must be complete before the WCAG-AA audit and remediation are meaningful. Sliced last not because accessibility is unimportant — it is a mandatory quality bar — but because it spans every prior slice, and remediating five surfaces at once is more efficient than gating each prior slice on its own audit.

A dependency graph view (slice → depends on):

- Slice 01: nothing (walking skeleton)
- Slice 02: depends on Slice 01 (extends the time-range picker that slice 01 stubs as "Last 15 min only")
- Slice 03: depends on Slice 01 (extends the chart-area and query-input components)
- Slice 04: depends on Slice 01, Slice 02 (auto-refresh needs the time-range picker to know whether the range is relative or absolute)
- Slice 05: depends on Slice 02 (extends the time-range picker with the absolute mode)
- Slice 06: depends on all prior slices (audits the cumulative surface)

---

## Scope Assessment: PASS — 6 stories ≈ 7 user stories, 1 module (the SPA), estimated 6–10 working days across 6 slices

Per the Elephant Carpaccio gate in the orchestrator workflow:

| Signal | Threshold | Prism v0 |
|---|---|---|
| > 10 user stories? | No | 7 stories (US-PR-01 .. US-PR-07) |
| > 3 bounded contexts or modules? | No | 1 module (a single SPA crate / package) |
| Walking skeleton requires > 5 integration points? | No | 1 integration point (Prometheus HTTP API) |
| Estimated effort > 2 weeks? | No | 6 slices, each ≤ 1 day per Andrea's brief; total ~6–10 working days |
| Multiple independent user outcomes that could ship separately? | No | One outcome (incident-time query panel); other panels (logs, traces) are explicit post-v0 |

All five signals: PASS. Scope is right-sized.
