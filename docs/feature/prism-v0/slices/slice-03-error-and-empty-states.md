# Slice 03 — Error and empty states

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-PR-04, US-PR-03 (error-rendering portion).
> **Companion slice files**: depends on Slice 01.

## Outcome added

Three failure modes that previously crashed or hung the page now render as **calm, inline, recoverable** states — without losing the operator's context, without crashing the SPA, and without lying about stale data.

1. **PromQL parse error** (backend returns 400 with `status:"error"` JSON wrapper): inline warning banner showing the backend's error text verbatim; chart area shows a calm fallback message; query input keeps focus; URL still encodes the (broken) query so the operator can share the same broken state.
2. **Backend unreachable** (TCP refused, DNS failure, TLS error, 5xx): inline warning naming the backend URL and the transport-level error; body shows `Last successful fetch: ${last_fetch_time}`; the previous chart is **not** shown (no stale-data lying).
3. **Empty result** (200 OK with `data.result: []`): chart area shows `No data for ${time_range_iso}. Check the metric name or widen the range.` This is **not** an error — no warning banner.

This slice **defuses the strongest demand-reducing forces** identified in `jtbd-four-forces.md`: data-fidelity anxiety, session-loss anxiety, and the page-crash fear.

## What it lights up

| Activity | Slice 03 coverage |
|---|---|
| Open Prism | (reuse Slice 01); the page never goes blank on a config error either |
| Compose query | Inline error renders below the query input on parse failure; the input keeps focus; the query string stays in the input so the operator can correct the typo |
| Read chart | All three failure modes render their respective states; the chart from a previous successful fetch is dropped on backend-unreachable (no stale lying) but is preserved on parse-error (the chart-area calm fallback explains "Backend rejected this query") |
| Iterate | Operator can correct the query and re-press Run without losing the time range or any other state |
| Share + decide | URL is preserved on every error state — pasting the URL elsewhere reproduces the same error (so a colleague can see the same error message) |
| Postmortem | (reuse Slice 01) |

## Demo command

```bash
# Same setup: real local Prometheus on :9090, Prism on :5173.

# Demo 1 — PromQL parse error
# Browser: open http://localhost:5173/
# Type "rate(metric_name[5m" (note: missing closing bracket).
# Press Run.
# Expected: warning banner reading "1:48: parse error: unclosed left bracket"
# (or whatever Prometheus' actual error text is).
# Chart area shows: "Backend rejected this query. Fix the query above and press Run."
# Query input remains focused with cursor at end.
# URL bar:  http://localhost:5173/?q=rate(metric_name%5B5m&from=-15m&to=now

# Demo 2 — Backend unreachable
# Stop the Prometheus container.
# Browser: re-press Run on a query that previously worked (e.g. "up").
# Expected: warning banner reading
#   "Cannot reach backend dev-local-prom: TypeError: Failed to fetch"
# Body region shows: "Last successful fetch: 2026-05-07T03:14:22Z"
# Previous chart is gone (no stale-data lying).

# Demo 3 — Empty result
# Restart Prometheus.
# Type a query that has no data: "up{job=\"nonexistent\"}"
# Press Run.
# Expected: NO warning banner (this is not an error).
# Chart area: "No data for 2026-05-07T03:00Z .. 2026-05-07T03:15Z. Check the metric name or widen the range."
```

## Acceptance summary (full UAT in `user-stories.md` and `journey-incident-response.feature`)

- A 400 from the backend with a `status:"error"` body renders the `error` field verbatim in an inline warning banner.
- The chart area, on parse error, shows "Backend rejected this query. Fix the query above and press Run." (or DESIGN's locked equivalent text).
- A transport-level fetch failure renders an inline warning naming the configured backend URL and the JS-level fetch error message.
- After a transport-level failure, the previous chart is dropped from the DOM (no stale rendering) and the body shows `Last successful fetch: ${last_fetch_time}`.
- A successful 200 with `data.result: []` renders the empty state and **no** warning banner.
- In every error state, the URL still encodes the (broken or empty-result-yielding) query for shareability.
- In every error state, the page remains interactive — the query input is focusable, the time-range picker can be opened, the Run button is usable.
- The page never throws an uncaught JavaScript exception that blanks the document.

## Complexity drivers

- First introduction of error boundaries at the React layer. DESIGN decides whether to use React's `<ErrorBoundary>`, custom catch wrapping, or both.
- First differentiation between "transport error" (no response) and "application error" (response with `status:"error"`). The Prometheus HTTP API distinguishes these; Prism must honour the distinction.
- The "drop the previous chart on transport error" behaviour is subtle — DESIGN must not accidentally cache the previous render when the new fetch fails.
- The empty-state vs. error-state distinction must be visually clear: empty is information, error is alarm.

## Known unknowns

- Whether the warning banner is dismissible. v0 default: **not dismissible** (it persists until the next Run); DESIGN may adjust if user testing surfaces a need.
- Whether the inline warning banner uses an emoji or a Unicode warning glyph. v0 contract: a calm visual treatment; the WCAG audit in Slice 06 confirms.
- The exact wording of the chart-area fallback messages — v0 stub is in the journey-visual file; DESIGN may polish.

## Out of scope for this slice

- Auto-refresh resilience under errors — Slice 04.
- Multi-error UI (e.g. a "history" of recent errors) — out of v0 scope.
- Did-you-mean suggestions for typos — out of v0 scope; the backend's error text is verbatim.
