# Slice 01 — Walking skeleton

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-PR-01, US-PR-02 (default 15-min range only), US-PR-03, US-PR-06.
> **Companion slice files**: none upstream — this is the walking skeleton.

## Outcome added

A senior SRE on her laptop opens the Prism URL, types a real PromQL query (e.g. `up`) into a focused query input, presses Run, and sees a line chart rendered from a **real** Prometheus or Mimir backend's `/api/v1/query_range` response. The chart shows exactly the points the backend returned — no smoothing, no interpolation, no client-side aggregation. The page chrome names the backend label and Prism version. The URL bar updates to encode the query and the default 15-min relative range.

This is **the same Strategy C "real local" posture Aperture used** in its slice 01 — bind to a real local Prometheus container in dev mode, route a real HTTP request to it, render the real response. No mocks. No fixtures. The risk that "Prometheus' actual JSON response shape isn't what we think" lands here, not late.

## What it lights up (across the six backbone activities)

| Activity | Slice 01 coverage |
|---|---|
| Open Prism | Full: SPA bundle loads at the operator's URL; `/config.json` fetched; chrome shows backend label + Prism version; query input is the focused element on first load |
| Compose query | gRPC—er, PromQL string into a focused input. Default time range is "Last 15 min" (relative); Slice 02 lights up the picker with all five presets. Run button enabled when query is non-empty |
| Read chart | Full happy path: a successful query renders an Apache ECharts line chart with the right points, legend, axes, and footer (series count, point count, query latency) |
| Iterate | Trivial: pressing Run again re-fetches. Auto-refresh is "off only" until Slice 04 |
| Share + decide | URL is updated to encode the query, time range, and (implicitly) backend. Copying it works because the browser does that for free |
| Postmortem | Trivial: a fresh-tab paste of the same URL reproduces the same view (the URL parameter handling is bidirectional from slice 01) |

## Demo command

```bash
# Terminal 1: a real local Prometheus (single-binary, default config).
# This is the operator's Mimir / Prometheus stand-in for the dev environment.
docker run -p 9090:9090 prom/prometheus:v2.51.0

# Wait ~10 seconds for Prometheus to scrape itself; it now has the `up` metric
# pointing at its own scrape job.

# Terminal 2: build and run Prism v0 with default config pointed at the local Prom.
echo '{ "backend": { "url": "http://localhost:9090", "label": "dev-local-prom" } }' \
  > prism-v0/public/config.json
cd prism-v0 && pnpm dev

# Browser: navigate to http://localhost:5173/
# Expected: page loads in <1 s; query input is focused; chrome shows
#   "backend: dev-local-prom" and "v0.1.0"; default time-range is "Last 15 min".
# Type "up" into the query input.
# Press Run.
# Expected: a line chart renders with one or more series (one per Prometheus
#   scrape target). Footer reads "<N> series · <M> points · fetched in <Q> ms".
# URL bar reads:
#   http://localhost:5173/?q=up&from=-15m&to=now
# Copy that URL into an incognito tab; the same chart renders.
```

The demo passes when:

- Prism's chart points exactly match the points returned by `curl 'http://localhost:9090/api/v1/query_range?query=up&start=...&end=...&step=15s' | jq '.data.result'`.
- The footer's `series` count equals `data.result.length`.
- The footer's `points` count equals the sum of `data.result[i].values.length`.
- The URL roundtrips: copying it and pasting in a fresh tab reproduces the chart.

## Acceptance summary (full UAT in `user-stories.md` and `journey-incident-response.feature`)

- The SPA loads and is interactive within 2 s at p95 on a typical operator browser tab.
- The query input is the focused element on fresh page load.
- The default time range is "Last 15 min" (relative).
- The Run button is disabled until the query is non-empty; pressing Enter while focused on the input is equivalent to pressing Run.
- A successful PromQL query against a real Prometheus / Mimir backend renders a line chart with the exact series and point count the backend returned.
- The chart legend names each series by its labels (e.g. `instance="...", method="..."`), not as `series-1`, `series-2`, etc.
- The footer shows `${num_series} series · ${num_points} points · fetched in ${query_ms} ms`.
- The URL is updated to `?q=...&from=-15m&to=now` after a successful run.
- The page chrome shows `backend: ${backend_label}` and `${prism_version}`.
- A fresh-tab paste of the URL reproduces the same view.

## Complexity drivers

- First integration of Prism with a real Prometheus / Mimir HTTP API. CORS, auth headers, JSON response shape (`status:"success"` wrapper, `data.result` with `metric` + `values` arrays), error wrapper (`status:"error"`, `error` field).
- First use of Apache ECharts in the project. Chart config flags must be set to forbid smoothing/interpolation: `smooth: false`, `connectNulls: false`, no auto-downsampling.
- First definition of `/config.json` schema. DESIGN-wave decision: which keys, which defaults, fail-fast vs. fail-soft on missing config.
- First definition of the URL parameter vocabulary (`q`, `from`, `to`). DESIGN locks the encoding rules.
- The Vite + React + TypeScript skeleton is itself new (project's first frontend feature).

## Known unknowns

- Whether `pnpm` or `npm` is the right package manager for the project — DESIGN decides; consistent with prior frontend tooling preferences if any.
- Whether the SPA should ship as a single `index.html` + bundle directory, or with code-splitting at v0. The 2 s first-load budget at p95 is the lever; DESIGN decides the bundle strategy.
- The exact ECharts component and config required for "monochrome line chart with no smoothing, legend below, axes labelled in UTC and the response unit". DESIGN locks the config.
- The exact JSON shape of `/config.json` — DESIGN decides; this slice locks only the keys' names and consumers (per `shared-artifacts-registry.md`).

## Out of scope for this slice

- All four other relative time-range presets (5 / 1h / 6h / 24h) — Slice 02.
- Absolute time ranges (ISO-8601 `from` / `to`) — Slice 05.
- Inline error rendering for parse errors / transport errors / empty results — Slice 03.
- Auto-refresh — Slice 04.
- Accessibility audit (focus indicators, contrast, palette) — Slice 06 (foundations laid here, audit happens later).
- Saved queries, log search, trace waterfall — out of v0 scope (post-v0 slices or later phases).
