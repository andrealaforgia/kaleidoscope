# Slice 02 — Time range and relative presets

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-PR-02 (relative-range part).
> **Companion slice files**: depends on Slice 01; precedes Slice 04 (auto-refresh) and Slice 05 (absolute ranges).

## Outcome added

The operator can pick the time range from the operator-canonical set of relative presets — Last 5 min, Last 15 min, Last 1 h, Last 6 h, Last 24 h — using a dropdown picker. The selected range is encoded in the URL parameter `from` (and `to=now`), so copying the URL and pasting it elsewhere reproduces the same time-range selection. The chart re-fetches when the range changes, with the same data-fidelity guarantees as Slice 01.

## What it lights up

| Activity | Slice 02 coverage |
|---|---|
| Open Prism | (reuse Slice 01) |
| Compose query | Time-range picker now offers all five relative presets (was: Last 15 min only). Custom-absolute mode still deferred to Slice 05 |
| Read chart | (reuse Slice 01); the chart now reflects whichever relative range was selected |
| Iterate | Picking a different range re-fetches; the query is preserved across range changes (the integration checkpoint from `journey-incident-response-visual.md` step 4) |
| Share + decide | URL parameter `from` now carries `-5m`, `-15m`, `-1h`, `-6h`, or `-24h` |
| Postmortem | (reuse Slice 01) |

## Demo command

```bash
# Same setup as Slice 01: real local Prometheus on :9090, Prism dev server on :5173.

# Browser: open http://localhost:5173/
# Type "up" into the query input.
# Open the time-range picker; observe the five preset options:
#   Last 5 min
#   Last 15 min  ← default
#   Last 1 h
#   Last 6 h
#   Last 24 h
# Pick "Last 1 h".
# Press Run.
# Expected: chart re-fetches with a wider time range; URL bar updates to:
#   http://localhost:5173/?q=up&from=-1h&to=now
# Copy URL → paste in fresh tab → same view.
# Pick "Last 5 min" → URL becomes ?q=up&from=-5m&to=now → chart redraws.
```

## Acceptance summary (full UAT in `user-stories.md` and `journey-incident-response.feature`)

- The time-range picker offers exactly these relative presets: Last 5 min, Last 15 min (default), Last 1 h, Last 6 h, Last 24 h.
- The picker also offers a "Custom" option, which is **disabled** at Slice 02 (it lights up in Slice 05).
- Selecting a preset re-fetches the chart immediately (no separate Run press needed).
- The URL parameter `from` is encoded as `-5m`, `-15m`, `-1h`, `-6h`, or `-24h` for the relative presets; `to=now` for all relative presets.
- Editing the query while a non-default range is selected preserves the range across the edit (per the journey integration checkpoint).
- Loading a URL with `from=-1h&to=now` shows the picker pre-set to "Last 1 h" and renders the chart for that range.

## Complexity drivers

- First introduction of state that must round-trip through the URL: time-range picker state ↔ URL parameters. DESIGN decides the URL-state synchronisation library / pattern (e.g. React Router's `useSearchParams`, manual `history.replaceState`, etc.).
- The picker UI must support five presets with the layout/styling agreed at Slice 01. The "Custom" option is rendered but disabled (visual affordance for what arrives in Slice 05).
- Time-range resolution: relative ranges resolve to absolute timestamps **at fetch time**, not at every render — this is a fidelity invariant (Slice 04 will lean on it for auto-refresh).

## Known unknowns

- Whether the picker is a `<select>` element or a custom dropdown. DESIGN decides; the WCAG audit in Slice 06 will gate either choice on focus / keyboard / aria-* compliance.
- Whether the picker also offers an "Apply on change" vs "Apply on Run" toggle. v0 contract: applying happens immediately on selection (no separate Run press); the rationale is that operators expect range changes to behave like a refresh, and there is no "preview" stage.

## Out of scope for this slice

- Absolute (custom) time ranges via ISO-8601 — Slice 05.
- Auto-refresh — Slice 04.
- Picker keyboard navigation audit — partial in this slice (focus order works); full WCAG-AA audit in Slice 06.
