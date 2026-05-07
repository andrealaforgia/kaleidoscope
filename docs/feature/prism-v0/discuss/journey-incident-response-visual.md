# Journey — Incident Response (visual) — `prism-v0`

> **Wave**: DISCUSS — Phase 2 (UX journey design).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-07.
> **Companion documents**: `journey-incident-response.yaml`, `journey-incident-response.feature`, `shared-artifacts-registry.md`, `jtbd-job-stories.md`, `jtbd-four-forces.md`.

This is Kaleidoscope's first frontend feature. Prior journeys (Codex, Spark, Sieve, Aperture, the harness) were CLI-output journeys with stderr lines and command output as their TUI mockups. Prism is a browser-side React + TypeScript SPA — the journey's mockups are HTML / browser-window shapes, drawn as ASCII boxes. The "TUI" of prior features becomes the "rendered DOM at this moment" here.

---

## Persona

Priya Raman, senior SRE at `acme-observability`, paged at 03:14 by `checkout-service p99 latency ≥ 800 ms`. Full persona profile in [`jtbd-job-stories.md`](jtbd-job-stories.md).

---

## End-to-end goal

From "PagerDuty notification on phone" to "decision made and pasted into Slack" in **under 5 minutes**, against the Mimir backend `acme-observability` already runs, with Prism as the only frontend tool in the chain.

---

## Emotional arc

This is **incident-time emotional design**. Cortisol is the dominant chemical; the arc is "stress → controlled stress → relief", not "curiosity → discovery → delight". The arc drives the design contract:

| Phase | Target emotion | Design lever |
|---|---|---|
| Page received | High stress, focus narrowing | Outside Prism's surface |
| Prism opens | Stress + suspicion (will this thing work?) | Calm chrome; query input is the first focusable element; loading state visible in 100 ms |
| First chart renders | Stress + cognitive engagement | Chart is monochrome by default, legend visible, units explicit; query is shown verbatim above the chart |
| Edit-and-rerun loop | Controlled stress, flow state | Time range stable across edits; auto-refresh holds steady; no surprise dialogs |
| Decision made | Stress recedes; confidence emerging | URL is permalink; copy-paste-into-Slack is one click; the chart she saw IS what her colleague will see |
| Postmortem | Calm | Same URL, days later, reproduces the same view |

Pattern selected from `nw-design-methodology` § Emotional Arc Patterns: **Problem Relief** (Frustrated → Hopeful → Relieved), modified for incident-time stress: the start is not "frustrated" but "stressed-and-suspicious"; the end is not "delighted" but "trusting-and-confident". Surface delight (animations, microcopy with personality) is **explicitly off-limits** — it would feel patronising at 03:14.

The full emotional-arc table is in `journey-incident-response.yaml :: journey.emotional_arc`.

---

## Backbone — six activities

The journey decomposes into six activities, left-to-right:

```
1. Receive page  →  2. Open Prism  →  3. Compose query  →  4. Read chart  →  5. Iterate  →  6. Share + decide
```

| # | Activity | What Priya does | What Prism owes her |
|---|---|---|---|
| 1 | Receive page | Reads the alert text on her phone; copies the metric name from the alert payload if present | (out of Prism's scope; PagerDuty / Beacon's job) |
| 2 | Open Prism | Clicks the Prism URL bookmark; tab loads | First-load p95 ≤ 2 s; query input focused; default 15-min time range |
| 3 | Compose query | Types or pastes a PromQL expression; picks a time range; presses Enter | Inline syntax error if invalid; "Run" enabled when query is non-empty |
| 4 | Read chart | Sees the rendered line chart, legend, units, query above the chart | Chart in 100 ms loading state, then in ≤ 2 s p95 from request fired |
| 5 | Iterate | Tweaks the query, re-runs; toggles auto-refresh on; widens the time range | Query edit does not lose time range; auto-refresh re-issues same query; chart redraws without flicker |
| 6 | Share + decide | Copies the URL, pastes into Slack; makes a triage decision | URL encodes query + range + backend; pasted URL on a fresh tab reproduces the view |

Prism v0 owns activities 2–6. Activity 1 is upstream (PagerDuty + alert text) and activity 6 ends with Priya's decision (downstream, outside Prism's surface).

---

## ASCII mockup — the SPA in five states

Mockups use a 100-column-wide browser-window box. Variables in `${...}` are tracked in [`shared-artifacts-registry.md`](shared-artifacts-registry.md).

### State A: Fresh page load (default — 15-min time range, no query yet)

```
+======================================================================================================+
| ⌘ ${prism_url}                                                                                       |
+======================================================================================================+
| Prism                                                              backend: ${backend_label}         |
| ─────                                                              v ${prism_version}                |
+------------------------------------------------------------------------------------------------------+
|                                                                                                      |
|  PromQL query                                                  Time range  [Last 15 min ▾]           |
|  ┌──────────────────────────────────────────────────────────┐                                        |
|  │ |                                                        │   [ Run ]   Auto-refresh [ off ▾ ]     |
|  └──────────────────────────────────────────────────────────┘                                        |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
|                                                                                                      |
|                                                                                                      |
|                              Type a PromQL query, then press Run.                                    |
|                                                                                                      |
|                              Examples:                                                               |
|                                rate(http_server_duration_seconds_count[5m])                          |
|                                histogram_quantile(0.99, rate(...[5m]))                               |
|                                                                                                      |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
| Backend ${backend_url}  ·  PromQL specification: prometheus.io/docs/prometheus/latest/querying       |
+======================================================================================================+
```

- Query input has focus (cursor in the box). Tab order: query → time range → Run → auto-refresh.
- Time-range picker default is 15 minutes (relative). Picker presets: 5 min / 15 min / 1 h / 6 h / 24 h / custom.
- Auto-refresh default is `off`. Picker presets: off / 5 s / 10 s / 30 s / 1 min.
- Empty state explains what will appear, with two example queries (educational, not promotional).
- Footer names the backend URL and links to PromQL canonical docs (Prometheus' own).

### State B: Query is loading (after Run pressed)

```
+======================================================================================================+
| ⌘ ${prism_url}/?q=rate(http_server_duration_seconds_count[5m])&from=-15m&to=now                      |
+======================================================================================================+
| Prism                                                              backend: ${backend_label}         |
+------------------------------------------------------------------------------------------------------+
|  PromQL query                                                  Time range  [Last 15 min ▾]           |
|  ┌──────────────────────────────────────────────────────────┐                                        |
|  │ rate(http_server_duration_seconds_count[5m])             │   [ Running... ]   Auto-refresh [off]  |
|  └──────────────────────────────────────────────────────────┘                                        |
+------------------------------------------------------------------------------------------------------+
|                                                                                                      |
|   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    |
|   ░  Loading skeleton — chart area placeholder                                                  ░    |
|   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    |
|                                                                                                      |
|                          Querying ${backend_label}…  fetching ${time_range_iso}                      |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
```

- The URL bar has UPDATED to encode the query, the relative time range (`from=-15m&to=now`), and (implicitly) the backend identity. Run pressed → URL written → request issued.
- The "Run" button text becomes "Running..." and is disabled until the response arrives or fails.
- Skeleton loader replaces the empty state within 100 ms of pressing Run. No spinner. Chart area is a grey skeleton at the right aspect ratio so the layout does not jump when the chart arrives.
- Loading caption names the backend and the time range, so Priya knows exactly what is being asked for.

### State C: Chart rendered (success)

```
+======================================================================================================+
| ⌘ ${prism_url}/?q=rate(http_server_duration_seconds_count[5m])&from=-15m&to=now                      |
+======================================================================================================+
| Prism                                                              backend: ${backend_label}         |
+------------------------------------------------------------------------------------------------------+
|  PromQL query                                                  Time range  [Last 15 min ▾]           |
|  ┌──────────────────────────────────────────────────────────┐                                        |
|  │ rate(http_server_duration_seconds_count[5m])             │   [ Run ]   Auto-refresh [ 30 s ▾ ]    |
|  └──────────────────────────────────────────────────────────┘                                        |
|                                              Last fetched ${last_fetch_time}  ·  next in 27 s        |
+------------------------------------------------------------------------------------------------------+
|                                                                                                      |
|     ↑ requests / s                                                                                   |
|  120┤                                                                              ▲                 |
|  100┤                                                                       ▲   ▲ ▲ ▲                |
|   80┤                                                                ▲    ▲   ▲                      |
|   60┤                                            ▲     ▲      ▲    ▲                                |
|   40┤        ▲    ▲     ▲     ▲     ▲       ▲                                                       |
|   20┤  ▲  ▲                                                                                         |
|     └──┬─────────┬─────────┬─────────┬─────────┬─────────┬─────────┬─────────┬──────►                |
|       03:00     03:02     03:04     03:06     03:08     03:10     03:12     03:14       (UTC)        |
|                                                                                                      |
|     ─── method="POST", route="/checkout"   instance="checkout-1"     |  56.2 r/s                     |
|     ─── method="POST", route="/checkout"   instance="checkout-2"     |  48.9 r/s                     |
|     ─── method="GET",  route="/checkout"   instance="checkout-1"     |  12.3 r/s                     |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
| Backend ${backend_url}  ·  ${num_series} series  ·  ${num_points} points  ·  fetched in ${query_ms} ms |
+======================================================================================================+
```

- URL has been written. Copying it now and pasting in a fresh tab reproduces State C exactly (URL roundtrip — KPI 4).
- Chart is monochrome-default. ECharts' `large` mode is implied (DESIGN owns the actual config). Y-axis labelled in the unit Prometheus returned (here: requests / second, derived from the metric's `_count` suffix and `rate(...[5m])`); x-axis labelled in UTC.
- Legend below the chart names each series by its **labels**, not by an opaque series-N. Operators identify-by-labels.
- "Last fetched" timestamp + "next in 27 s" is the auto-refresh status line. When auto-refresh is off, this line shows `Last fetched ${last_fetch_time}` only.
- Footer names the backend, the series count, the point count, and the query latency. These are the four numbers an operator needs to know whether the chart they are looking at is trustworthy.

### State D: PromQL parse error

```
+======================================================================================================+
| ⌘ ${prism_url}/?q=rate(http_server_duration_seconds_count%5B5m)&from=-15m&to=now                      |
+======================================================================================================+
| Prism                                                              backend: ${backend_label}         |
+------------------------------------------------------------------------------------------------------+
|  PromQL query                                                  Time range  [Last 15 min ▾]           |
|  ┌──────────────────────────────────────────────────────────┐                                        |
|  │ rate(http_server_duration_seconds_count[5m)              │   [ Run ]   Auto-refresh [ off ▾ ]     |
|  └──────────────────────────────────────────────────────────┘                                        |
|  ⚠ ${backend_error}                                                                                  |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
|                                                                                                      |
|                                                                                                      |
|                              Backend rejected this query.                                            |
|                                                                                                      |
|                              Fix the query above and press Run.                                      |
|                                                                                                      |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
| Backend ${backend_url}  ·  PromQL specification: prometheus.io/docs/prometheus/latest/querying       |
+======================================================================================================+
```

- Error message is rendered inline, immediately below the query input, with `${backend_error}` set to the backend's exact error text — typically something like `1:48: parse error: unclosed left bracket`. **Prism does not rewrite this string.** That decision is locked in `wave-decisions.md` (D2 — Backend-supplied error messages are rendered verbatim).
- The chart area shows a calm fallback message, not a stack trace, not a spinner stuck forever.
- The page never goes blank. URL still encodes the (broken) query so the user can fix the typo without losing context, and so a colleague pasted the URL gets the same broken state to look at.
- Tab focus returns to the query input. The user fixes the typo and presses Run.

### State E: Backend unreachable

```
+======================================================================================================+
| ⌘ ${prism_url}/?q=rate(...)&from=-15m&to=now                                                          |
+======================================================================================================+
| Prism                                                              backend: ${backend_label}         |
+------------------------------------------------------------------------------------------------------+
|  PromQL query                                                  Time range  [Last 15 min ▾]           |
|  ┌──────────────────────────────────────────────────────────┐                                        |
|  │ rate(http_server_duration_seconds_count[5m])             │   [ Run ]   Auto-refresh [ off ▾ ]     |
|  └──────────────────────────────────────────────────────────┘                                        |
|  ⚠ Cannot reach backend ${backend_url}: ${transport_error}                                            |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
|                                                                                                      |
|                              Backend is unreachable.                                                 |
|                                                                                                      |
|                              Check backend health, then press Run.                                   |
|                                                                                                      |
|                              Last successful fetch: ${last_fetch_time}                               |
|                                                                                                      |
+------------------------------------------------------------------------------------------------------+
| Backend ${backend_url}  ·  PromQL specification: prometheus.io/docs/prometheus/latest/querying       |
+======================================================================================================+
```

- Transport-level error (DNS, TCP, TLS, HTTP 5xx) renders inline like a parse error, but with text naming the backend identity and the transport-level cause.
- The page records the last successful fetch time so Priya knows how stale her last-good view was — a critical incident-time data point. (If the backend has been up but the SPA cannot reach it from her browser, the issue is between her browser and the backend, not in the backend itself; the timestamp helps her triangulate.)
- Auto-refresh, if on, keeps trying with exponential backoff (5 s → 10 s → 30 s, capped at 30 s) and updates the message on each retry. DESIGN owns the exact backoff curve; the contract is "retries are visible and bounded".

---

## Per-step expected output and tracked artefacts

Mapping per [`shared-artifacts-registry.md`](shared-artifacts-registry.md). The bracketed variables in the mockups above each have a single source-of-truth.

| Step | Variable | Source-of-truth |
|---|---|---|
| All states | `${prism_url}` | runtime `window.location.origin` of the SPA, set by the operator's reverse-proxy / hosting choice |
| All states | `${backend_label}` | configuration `prism.backend.label` (operator-set string, e.g. `"acme-prod-mimir"`) |
| All states | `${backend_url}` | configuration `prism.backend.url` (operator-set URL of the Prometheus-API-compatible backend) |
| All states | `${prism_version}` | `package.json :: version` |
| State C | `${last_fetch_time}` | client-side ISO-8601 timestamp at the moment the fetch resolved |
| State C | `${num_series}` | length of `data.result` array in Prometheus' `/api/v1/query_range` response |
| State C | `${num_points}` | sum of values arrays across all series in the response |
| State C | `${query_ms}` | client-measured wall time from request fired to response parsed |
| State C | `${time_range_iso}` | computed from the time-range picker — for `Last 15 min`: `from=NOW-15m, to=NOW` (resolved at fetch time, NOT at every render) |
| State D | `${backend_error}` | the `error` field of Prometheus' JSON response (verbatim) |
| State E | `${transport_error}` | the JS-level fetch error message (verbatim, no rewrites) |

Every variable in every mockup has a documented source. None is hard-coded.

---

## Failure modes per step

For DISTILL-wave (Quinn) error scenario generation. Each step's failure mode produces at least one Gherkin scenario in `journey-incident-response.feature`.

| Step | Failure | Expected behaviour |
|---|---|---|
| Open Prism | SPA bundle fails to load | Operator sees the host's reverse-proxy error page (out of Prism's scope) |
| Open Prism | SPA loads but configuration cannot be fetched (`/config.json` 404) | Page renders a single error: "Configuration is missing. Contact your Prism administrator." Backend label is "(unconfigured)". |
| Compose query | PromQL syntax error | State D — inline error from backend, page stays usable |
| Compose query | Query exceeds backend's max query-string length | Inline error: "Query is too long for backend `${backend_label}`. Split the query or shorten." |
| Read chart | Backend returns 504 / timeout | State E — "Backend is unreachable" + transport error |
| Read chart | Backend returns 200 but with `status:"error"` (Prometheus' error wrapper) | State D — error rendered inline; chart area shows the calm fallback |
| Read chart | Backend returns zero series (valid query, no matching data) | Chart area shows an empty state: "No data for `${time_range_iso}`. Check the metric name or widen the range." |
| Iterate | Auto-refresh fires but the page is in a background tab | DESIGN owns: pause auto-refresh on `document.hidden`? Resume on focus? UX requirement is "no surprise charges to the backend from a forgotten tab" |
| Iterate | Time-range picker set to a future range (clock drift) | Inline error: "Time range ends in the future. Set a range that ends at or before now." |
| Share + decide | URL is over the browser's max URL length | Inline warning when query approaches 2000 characters; documentation note on the v0 max-query-string contract |
| Share + decide | Pasted URL targets a different backend than the one Prism is currently configured for | Page renders the URL's encoded query and time range, but explicitly names the configured backend in the chrome; no silent backend swap (Prism v0 is single-backend per deployment) |

---

## Integration checkpoints

What must be validated before the journey is considered complete:

1. **First-load p95 ≤ 2 s** measured against the deployed Prism on the operator's hardware (KPI 1).
2. **URL roundtrip** — copy URL from State C, open in a fresh incognito tab, get pixel-equivalent State C (KPI 4).
3. **PromQL passthrough** — the same query string typed into Prism vs. into `curl https://${backend_url}/api/v1/query_range?...` returns identical `data.result` payloads (KPI 2).
4. **Time-range stability** — open State C with a manual absolute time range (postmortem use case); auto-refresh is OFF for absolute ranges (data does not move when the user has chosen a fixed window); confirm the chart does not redraw spontaneously.
5. **Auto-refresh fidelity** — auto-refresh fires every N seconds, re-issues the same `query_range` request with a sliding `to=NOW` window for relative ranges; client-side smoothing is OFF (KPI 3 — fidelity invariant).

These five checkpoints are the journey's testable contract. They appear as `@property`-tagged scenarios in `journey-incident-response.feature` and as guardrails in `outcome-kpis.md`.

---

## CLI vocabulary — well, URL vocabulary

Prism is browser-only; CLI vocabulary patterns from prior features do not apply directly. The equivalent contract is the **URL vocabulary**: the encoded query parameter names and value shapes. These are part of Prism's contract surface and tracked in `shared-artifacts-registry.md`. v0 vocabulary, locked at DISCUSS:

| URL parameter | Type | Source |
|---|---|---|
| `q` | URL-encoded PromQL string | the query input |
| `from` | relative (`-15m`, `-1h`, `-24h`) OR ISO-8601 absolute timestamp | time-range picker |
| `to` | `now` (default for relative) OR ISO-8601 absolute timestamp | time-range picker |
| `refresh` | `off` (default), `5s`, `10s`, `30s`, `1m` | auto-refresh picker |

Rename = breaking change requires major version bump. Addition of new params = non-breaking; old links continue to work with sensible defaults.

---

## Summary

Six activities; five rendered states (default / loading / chart / parse-error / backend-error); eleven failure modes; five integration checkpoints. The journey is **horizontally complete** — Priya can trace from page-received to decision-made entirely within Prism v0's surface, without leaving for another tool, against the same backend her alert fired on.
