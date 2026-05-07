# Slice 05 — Absolute time range and full permalink

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-PR-02 (absolute-range portion), US-PR-04 (URL roundtrip portion).
> **Companion slice files**: depends on Slice 02 (relative-range picker UI), Slice 04 (auto-refresh disable behaviour).

## Outcome added

The operator can pick an absolute ISO-8601 time range (e.g. `from=2026-05-07T03:00:00Z to=2026-05-07T03:15:00Z`) via the picker's "Custom" mode. The URL encodes both timestamps. Auto-refresh is automatically disabled for absolute ranges (because the data does not move). A URL with absolute timestamps, opened days later, reproduces the **exact same chart** as long as the backend's retention window covers the range.

This slice **closes the postmortem-time use case**: an engineer writing an incident postmortem days later opens the URL Priya pasted into Slack at 03:14, and sees the chart Priya saw — provably the same data, not a re-derivation, not a re-query against current data.

## What it lights up

| Activity | Slice 05 coverage |
|---|---|
| Open Prism | URL parameters `from` and `to` carrying ISO-8601 timestamps now hydrate the picker into "Custom" mode |
| Compose query | Time-range picker's "Custom" option is now enabled (was: disabled in Slice 02). Two timestamp inputs (date + time, with timezone defaulting to UTC) appear when "Custom" is selected |
| Read chart | (reuse Slice 01); the chart x-axis labels use the absolute timestamps |
| Iterate | Picking a different absolute range or switching back to a relative preset re-fetches |
| Share + decide | URL parameter `from` and `to` carry full ISO-8601 strings (e.g. `from=2026-05-07T03:00:00Z`); auto-refresh forced off when the URL has absolute timestamps |
| Postmortem | The slice's headline: a URL crafted days ago with absolute timestamps reproduces the same view today |

## Demo command

```bash
# Same setup: real local Prometheus on :9090, Prism on :5173.
# Wait until Prometheus has at least 30 minutes of self-scrape data
# (or use an existing long-running Prometheus instance with retention).

# Browser: open http://localhost:5173/
# Type "up".
# Open the time-range picker; click "Custom".
# Two timestamp inputs appear; default to (now-15m) and now.
# Edit them to a specific past window, e.g. 2026-05-07T03:00Z and 2026-05-07T03:15Z.
# Press Run.
# Expected: chart renders for that absolute window; the URL bar updates to:
#   http://localhost:5173/?q=up&from=2026-05-07T03:00:00Z&to=2026-05-07T03:15:00Z
# The auto-refresh picker is disabled (greyed out).

# Copy the URL; close the browser; come back tomorrow; paste the URL into a
# fresh tab. Expected: the chart renders identically (assuming retention covers
# the range). The auto-refresh picker is disabled. The footer's series and
# point counts equal the curl reference.

# Curl reference for the same window:
curl 'http://localhost:9090/api/v1/query_range?query=up&start=2026-05-07T03:00:00Z&end=2026-05-07T03:15:00Z&step=15s' | jq '.data.result | length'
# Output: <N>  ← matches Prism's footer.
```

## Acceptance summary (full UAT in `user-stories.md` and `journey-incident-response.feature`)

- The time-range picker's "Custom" option, disabled in Slice 02, is now enabled and reveals two timestamp inputs.
- The timestamp inputs accept ISO-8601 (with seconds, with `Z` for UTC); DESIGN may also offer a date-and-time picker widget (browser-native or custom).
- The URL parameters `from` and `to` are written as full ISO-8601 timestamps for absolute ranges (e.g. `2026-05-07T03:00:00Z`).
- A URL with absolute `from` and `to` parameters loaded into a fresh tab hydrates the picker into "Custom" mode with the same timestamps and renders the chart for that absolute range.
- Whenever the time range is absolute (regardless of how it got that way — picker or URL), the auto-refresh picker is disabled and the URL parameter `refresh` is `off`.
- An absolute range with `from` later than `to` is rejected at the picker with an inline error `Time range start must be before end`; the Run button is disabled until the range is valid.
- An absolute range with `to` later than now is rejected at the picker with an inline error `Time range ends in the future. Set a range that ends at or before now.`; the Run button is disabled until the range is valid.
- A URL whose backend's retention window does not cover the requested absolute range renders the empty state from Slice 03: `No data for ${time_range_iso}. Check the metric name or widen the range.` (The operator can extend with "Check retention" wording at DESIGN-time if the backend exposes its retention.)

## Complexity drivers

- First use of timestamp parsing/formatting at the URL boundary. DESIGN decides whether to use the browser's `Date.parse` + `toISOString`, a small library (e.g. `date-fns`), or no library at all.
- First validation cluster: from-before-to, to-before-now. Inline errors must render at the picker, not via a fetch round-trip.
- The interaction with Slice 04: auto-refresh disable behaviour. The control bar must visibly reflect that the picker is disabled (not just unresponsive — operators must see why).
- Timezones. v0 contract: timestamps in URL are always UTC (`Z` suffix). The picker UI may display times in the operator's local timezone for ergonomics, but the URL is canonical UTC. DESIGN locks the picker's display-vs-storage convention.

## Known unknowns

- Whether to use the browser's native `<input type="datetime-local">` or a custom widget. Native is accessible and free but lacks UTC semantics by default; custom is more work but predictable. DESIGN decides.
- Whether to support a "shift earlier / shift later" pair of buttons (post-incident reading flow: read, shift back 15 min, read context, shift forward, etc.). v0 contract: **no** — out of v0 scope. The picker is the only way to change the range. A v0.1 addendum may add shift buttons.
- The exact wording of the "from later than to" and "ends in the future" errors. DESIGN polishes.

## Out of scope for this slice

- Calendar-style date picker UI — DESIGN may use a native `<input type="date">` or none at all; lift to a richer picker post-v0 if operators ask.
- Time zone selection in the URL (URL is always UTC) — out of v0; revisit if operator interviews show demand.
- Backend retention-aware messaging — out of v0; the empty state is generic.
