# Slice 04 — Auto-refresh

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-PR-05.
> **Companion slice files**: depends on Slice 01 (substrate), Slice 02 (relative ranges), Slice 03 (error resilience).

## Outcome added

The operator can turn on auto-refresh at a chosen interval — 5 s, 10 s, 30 s, or 1 min — and the chart re-fetches itself, updating in place without flicker, while the operator continues to read the chart. For relative time ranges, the `to=NOW` slides forward at every tick (so the chart's right edge keeps up). For absolute time ranges, auto-refresh is **disabled** at the picker level (because the data does not change). The fidelity invariants from Slice 01 hold across every refresh tick: no client-side smoothing, no interpolation, no caching.

## What it lights up

| Activity | Slice 04 coverage |
|---|---|
| Open Prism | URL parameter `refresh` is read on page load; the auto-refresh picker reflects it |
| Compose query | Auto-refresh picker added to the controls bar (per the journey-visual State C mockup) |
| Read chart | Auto-refresh status line above the chart: `Last fetched ${last_fetch_time} · next in <N> s` |
| Iterate | The headline of the slice: auto-refresh is on, the chart updates in place, the operator stares and triages |
| Share + decide | URL parameter `refresh=...` is encoded; pasting the URL elsewhere reproduces the auto-refresh interval |
| Postmortem | (reuse Slice 01) |

## Demo command

```bash
# Same setup: real local Prometheus on :9090, Prism on :5173.

# Browser: open http://localhost:5173/
# Type "rate(prometheus_http_requests_total[1m])" (a self-scrape metric Prom always has).
# Press Run.
# Expected: chart renders.
# Open the auto-refresh picker; observe the five options:
#   off  ← default
#   5 s
#   10 s
#   30 s
#   1 min
# Pick "10 s".
# Expected: every 10 seconds the chart re-fetches; the URL bar updates to:
#   http://localhost:5173/?q=rate(...)&from=-15m&to=now&refresh=10s
# The status line above the chart shows: "Last fetched 03:14:32 · next in 7 s"
# (counting down to 0, then re-fetching).

# Switch the time range to a custom absolute pair (Slice 05's UI wiring is enough
# here even though the absolute mode lights up in Slice 05 — at Slice 04 the
# disable-auto-refresh-on-absolute behaviour can be tested by URL):
# http://localhost:5173/?q=up&from=2026-05-07T00:00Z&to=2026-05-07T01:00Z
# Expected: the auto-refresh picker is disabled (greyed out, not interactable);
# URL parameter "refresh" is "off" regardless of what the URL claimed.
```

## Acceptance summary (full UAT in `user-stories.md` and `journey-incident-response.feature`)

- The auto-refresh picker offers exactly: off (default), 5 s, 10 s, 30 s, 1 min.
- Selecting an interval starts a timer that fires at that cadence; each tick re-issues the same `/api/v1/query_range` request the most recent successful fetch did.
- For relative time ranges, every tick resolves `to=NOW` afresh; the chart's right edge slides forward.
- For absolute time ranges, the auto-refresh picker is disabled and the URL parameter `refresh` is forced to `off` regardless of the URL.
- If a tick fires while a previous fetch is still in flight, the previous fetch is cancelled and only the new tick's result is rendered (no overlapping renders).
- If the browser tab becomes hidden, no further ticks fire until the tab becomes visible again. On regaining visibility, an immediate fresh fetch is issued, then the regular cadence resumes.
- If a tick's fetch fails (transport or application error), the error renders per Slice 03, but auto-refresh continues; the next tick attempts again.
- The fidelity invariants hold across ticks: no client-side smoothing or interpolation; no caching of prior responses.

## `@property`-tagged fidelity scenario (from `journey-incident-response.feature`)

```gherkin
@property
Scenario: Pressing Run always fetches fresh data, never a cached result
  Given the operator has just received a chart from a query at time T
  When the operator presses Run again at time T+1 second with the same query
  Then a new HTTP request is issued
  And no client-side cache is consulted
```

This `@property` scenario applies equally to auto-refresh: every tick is a fresh fetch.

## Complexity drivers

- First use of `setInterval` / a timer in Prism. DESIGN decides the implementation (`setInterval` with a `clearInterval` on unmount, a `useEffect` hook, or a custom hook).
- First use of the Page Visibility API (`document.hidden`, `visibilitychange` event) — required to pause refresh in background tabs.
- First use of `AbortController` for fetch cancellation — required to prevent overlapping renders.
- The interaction with Slice 03's error states: an auto-refresh tick that fails must render the error per Slice 03's contract, AND the next tick must still fire. DESIGN locks the timer-vs-error semantics.
- The interaction with absolute time ranges (Slice 05): auto-refresh picker must be disabled when the range is absolute. The picker itself comes from Slice 05; this slice locks the disabled-when-absolute behaviour.

## Known unknowns

- Whether to add a "Refresh now" button alongside the auto-refresh picker. v0 contract: no separate button — the operator presses Run to refresh manually, the picker controls auto. A separate button would be redundant.
- The exact countdown UI for "next in 7 s" — DESIGN polishes; the contract is that the timestamp of last fetch and the time until next tick are visible.
- Whether to use exponential backoff on consecutive failed ticks. v0 contract: **no** — auto-refresh re-tries at the chosen interval regardless. The exception is **transport errors**, which can backoff to 30 s cap (per the journey-visual State E description). DESIGN locks the backoff-on-transport-error policy.

## Out of scope for this slice

- Absolute time range UI (the picker's "Custom" mode) — Slice 05.
- Saving the auto-refresh interval as a per-operator preference (e.g. localStorage) — out of v0 scope; the URL is the only state.
- Rate limiting or quota-protection on the backend side — operator's concern, not Prism's.
