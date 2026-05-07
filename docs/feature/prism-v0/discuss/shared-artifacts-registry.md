# Shared Artefacts Registry — `prism-v0`

> **Wave**: DISCUSS — Phase 2 / 5 (Coherence validation).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-07.
> **Companion documents**: `journey-incident-response-visual.md`, `journey-incident-response.yaml`, `journey-incident-response.feature`.

Every `${variable}` named in the journey mockups, the user stories, the Gherkin scenarios, or the URL parameter contract has a single source of truth. This file is the registry. It is the protection against horizontal-integration failures (a value rendered in one place from one source and in another place from a different source).

The registry is small at v0 because Prism v0 is intentionally a thin SPA. It will grow as later slices add log, trace, and saved-query surfaces.

---

## Configuration-sourced artefacts (set by the operator at deployment time)

These are operator-facing configuration values. They live in the SPA's `/config.json` (fetched once on page load) — DESIGN locks the file format and load mechanism; this registry locks the keys and their consumers.

### `prism.backend.url`

- **Source of truth**: `/config.json :: backend.url` (operator-set; no default — Prism cannot start without it)
- **Displayed as**: `${backend_url}`
- **Consumers**:
  - The actual fetch target for `/api/v1/query_range`, `/api/v1/query`, etc.
  - The page footer (visible to the operator)
  - Transport-level error messages ("Cannot reach backend ${backend_url}: …")
  - The DEVOPS handoff: this is what the platform-architect needs to know about for operator-facing documentation
- **Integration risk**: HIGH — a mismatch between the URL in the chrome and the URL the SPA actually fetches from breaks operator trust. The whole "same backend the alert fired on" property of the journey rests on this single URL being honest.
- **Validation**: integration test in CI loads `/config.json` with a known URL, then asserts both the displayed footer and the captured `fetch()` URL match.

### `prism.backend.label`

- **Source of truth**: `/config.json :: backend.label` (operator-set; default `"(unconfigured)"` if missing — but Prism logs a warning to the browser console if missing)
- **Displayed as**: `${backend_label}`
- **Consumers**:
  - The page chrome (top-right): `backend: ${backend_label}`
  - The loading-state caption: `Querying ${backend_label}…`
  - All inline error messages: `Cannot reach backend ${backend_label}: …`
- **Integration risk**: MEDIUM — a label drift would not change behaviour but would confuse operators sharing URLs across Prism deployments.
- **Validation**: the integration test above also asserts the label rendered in chrome equals `/config.json`'s value.

### `prism.backend.headers` (optional)

- **Source of truth**: `/config.json :: backend.headers` (operator-set; default empty)
- **Displayed as**: not rendered to the user; sent on every fetch
- **Consumers**: every outgoing fetch from Prism to the backend
- **Integration risk**: HIGH for security — these headers may carry the operator's tenant token / bearer token / basic auth. They must be kept out of error messages and out of any logging surface in DEVOPS handoff.
- **Validation**: a CI invariant — never log `backend.headers` values; only key names. (DESIGN locks the implementation; DISCUSS locks the contract.)

---

## Build-time artefacts (set when Prism is built)

### `prism_version`

- **Source of truth**: `package.json :: version` (single SemVer string)
- **Displayed as**: `${prism_version}`
- **Consumers**:
  - The page chrome (small, top-right, below backend label)
  - Available via the page's `<title>` attribute or hover
  - Useful for operator-side support diagnostics ("which Prism are you on?")
- **Integration risk**: LOW — Prism is single-instance per deployment; version skew across operator deployments is expected and handled by the operator's release pipeline, not by Prism itself.
- **Validation**: build-time injected; CI test asserts the rendered version equals the build-time `package.json` value.

---

## Runtime-derived artefacts (computed on each render)

### `prism_url`

- **Source of truth**: runtime `window.location.origin` of the SPA, set by the operator's reverse-proxy / hosting choice
- **Displayed as**: `${prism_url}`
- **Consumers**:
  - The browser URL bar (browser-controlled)
  - The shareable URL that an operator copies into Slack
  - The permalink an engineer pastes into a postmortem doc
- **Integration risk**: LOW — `window.location.origin` is a browser-truthful value; Prism cannot misrepresent it.

### `last_fetch_time`

- **Source of truth**: client-side ISO-8601 timestamp captured when the most recent successful fetch resolved
- **Displayed as**: `${last_fetch_time}`
- **Consumers**:
  - The auto-refresh status line above the chart: `Last fetched ${last_fetch_time} · next in 27 s`
  - The backend-unreachable fallback in the body: `Last successful fetch: ${last_fetch_time}`
- **Integration risk**: LOW — purely client-side state; failure mode is "shows the wrong time", which is detectable by the operator.

### `num_series`, `num_points`, `query_ms`

- **Source of truth**:
  - `num_series` = `data.result.length` from Prometheus' `/api/v1/query_range` JSON response
  - `num_points` = sum of `result[i].values.length` across all series
  - `query_ms` = client-measured wall time from `fetch()` start to JSON parse complete
- **Displayed as**: `${num_series}`, `${num_points}`, `${query_ms}`
- **Consumers**: the chart-area footer line: `${num_series} series · ${num_points} points · fetched in ${query_ms} ms`
- **Integration risk**: MEDIUM — these numbers are how the operator audits whether the chart shows what the backend returned. If the chart renders 200 points but the footer says 240, trust collapses. CI must assert these numbers match the parsed response.
- **Validation**: a UAT scenario ("Chart contains exactly the points the backend returned") asserts the rendered chart's series and point counts equal `data.result`-derived values.

### `time_range_iso`

- **Source of truth**: computed from the time-range picker at fetch time
  - Relative ranges (e.g. "Last 15 min") resolve to absolute ISO-8601 timestamps at the moment of fetch
  - Absolute ranges are already ISO-8601
- **Displayed as**: `${time_range_iso}`
- **Consumers**:
  - The loading caption: `Querying ${backend_label}… fetching ${time_range_iso}`
  - The chart's x-axis boundary labels
  - The empty-state message: `No data for ${time_range_iso}. Check the metric name or widen the range.`
- **Integration risk**: HIGH — the chart's x-axis labels and the loading caption must match the actual time range sent to the backend. A drift here means the chart is mislabelled.
- **Validation**: the integration test asserts the `start` and `end` parameters of the actual `/api/v1/query_range` request match the boundaries displayed on the rendered chart's x-axis.

---

## URL parameter vocabulary (the v0 contract surface)

The URL is part of Prism's contract surface — pasting a URL across machines, browsers, and time must reproduce the view. The vocabulary is therefore versioned: renames are major version bumps, additions are non-breaking.

| Parameter | Type | Source | Consumers |
|---|---|---|---|
| `q` | URL-encoded PromQL string | `promql_query_string` | the query input on page load; the request payload to `/api/v1/query_range` |
| `from` | relative (`-15m`, `-1h`, `-6h`, `-24h`) OR ISO-8601 absolute timestamp | `time_range_picker_state` | the time-range picker on page load; the request `start` parameter |
| `to` | `now` (default for relative) OR ISO-8601 absolute timestamp | `time_range_picker_state` | the time-range picker on page load; the request `end` parameter |
| `refresh` | `off` (default), `5s`, `10s`, `30s`, `1m` | `auto_refresh_interval` | the auto-refresh picker on page load; the timer that schedules ticks |

**Validation**: the URL roundtrip UAT scenarios (US-PR-04) assert that for any URL Prism writes, the same URL re-loaded in a fresh tab produces the same view.

**Integration risk**: HIGH — these are external contract surface. Renames break every saved permalink in every postmortem doc. DESIGN must not rename them lightly.

---

## Non-artefacts — things deliberately NOT shared, NOT cached

These are values that must NOT be cached or shared across renders. They are listed here because someone might be tempted to add them, and the registry is the place to defend against that.

| Non-artefact | Why NOT to share |
|---|---|
| **Chart data** (the parsed `data.result` from a previous fetch) | A cached result + auto-refresh = stale-data lying. The data fidelity guardrail in `outcome-kpis.md` requires every fetch to round-trip to the backend. |
| **Query parse tree / tokens** | Prism v0 does NOT parse PromQL. There is no parse tree to cache. The PromQL string is a string. |
| **A "favourite queries" list / saved queries** | Out of scope for v0. The URL is the saved query. Adding a per-browser saved-queries list at v0 would split the truth between local-storage and URL, breaking the social-shareable property. |
| **Operator identity / user info** | Prism v0 has no notion of users. Aegis (Phase 2) handles authn/authz; in v0 the operator's identity is whatever the reverse proxy enforces, and Prism does not see it. |

---

## Validation summary

Per `nw-shared-artifact-tracking` skill's validation questions:

| Question | Answer |
|---|---|
| Does every `${variable}` in the mockups have a documented source? | Yes — every mockup variable is in this registry. |
| If the version changes, would all consumers automatically update? | Yes — `prism_version` is build-time injected from `package.json`. |
| Are there hardcoded values that should reference a shared artefact? | None at DISCUSS. The risk is in DESIGN/DELIVER if a developer hardcodes a backend URL or label; CI must catch this. |
| Do any two steps display the same data from different sources? | No — every `backend_url` reference goes back to `/config.json`; every `last_fetch_time` reference goes back to the same client-side timestamp. |

---

## Per-step artefact summary (cross-reference to journey YAML steps)

| Journey step | Artefacts read | Artefacts written |
|---|---|---|
| 1. Open Prism | `backend_url`, `backend_label`, `prism_version`, `prism_url` | (none — display only) |
| 2. Compose query | `time_range_picker_state` (initial) | `promql_query_string`, `time_range_picker_state` |
| 3. Read chart | `promql_query_string`, `time_range_picker_state`, `backend_url`, `backend_headers` | `last_fetch_time`, `num_series`, `num_points`, `query_ms`, `time_range_iso`, `url_query_param_q`, `url_query_param_from`, `url_query_param_to` |
| 4. Iterate | (same as step 3 for each rerun) | (same as step 3) |
| 5. Share + decide | `url_query_param_*` | (none — operator copy-pastes) |
| 6. Postmortem | `url_query_param_*` (read by fresh tab) | (same as steps 1-3 on the new tab) |

The cycle is: configuration sourced once (step 1), user input captured (step 2), backend fetched (step 3), URL written from inputs (step 3), URL read by fresh tab (step 6) → loop closes.
