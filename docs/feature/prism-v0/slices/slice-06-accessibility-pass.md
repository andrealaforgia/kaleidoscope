# Slice 06 — Accessibility pass

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-PR-07.
> **Companion slice files**: depends on Slices 01–05 (audits the cumulative surface).

## Outcome added

The cumulative SPA from Slices 01–05 passes a WCAG 2.2 AA conformance audit. The operator can use Prism end-to-end with keyboard alone, with a screen reader, and on a colour-blind-safe palette. Focus indicators are visible on every interactive element. Text contrast ratios meet AA. The SPA respects `prefers-reduced-motion`.

This slice exists as a **separate audit-and-remediate pass** rather than per-slice gates because:

1. WCAG-AA audit is most efficient as a single-pass review of a complete UI surface.
2. Several WCAG criteria (e.g. focus order, error association) are about the page as a whole, not about individual components.
3. Per-slice gates would slow each slice down and risk premature lock-in on patterns that need to change once the surface is complete.

## What it lights up

| Activity | Slice 06 coverage |
|---|---|
| Open Prism | Page has a descriptive `<title>`; first focusable element is the query input; landmark roles in place |
| Compose query | Query input has an accessible name (`aria-label="PromQL query"`); time-range picker is keyboard-operable; auto-refresh picker is keyboard-operable |
| Read chart | Chart is rendered with a colour-blind-safe palette; chart's textual fallback (a `<table>` of series and points) is in the DOM, screen-reader-readable; chart legend names labels in text (not colour-only) |
| Iterate | Focus management on chart redraw: focus is preserved on the element that initiated the redraw (Run button, time-range picker, etc.) |
| Share + decide | URL bar interaction is browser-controlled (already accessible) |
| Postmortem | (reuse Slice 01) |

## Demo command

```bash
# Manual audit — keyboard only:
# Open Prism. Press Tab; verify focus lands on the query input.
# Press Tab again; verify focus moves to the time-range picker.
# Press Tab again; verify focus moves to the Run button.
# Press Tab again; verify focus moves to the auto-refresh picker.
# Press Shift+Tab; verify reverse order works.
# At each focused element, verify a visible focus ring is present.

# Manual audit — screen reader (e.g. VoiceOver on macOS):
# Enable VoiceOver. Open Prism.
# Verify VoiceOver announces:
#   "Prism, backend: dev-local-prom" (page title)
#   "PromQL query, edit text" (query input)
#   "Time range, Last 15 min, popup button" (picker)
#   "Run, button" (Run)
# Type "up", press Run.
# Verify VoiceOver announces the chart's textual table:
#   "1 series, 12 points, fetched in 47 ms"
#   "Series 1: instance=localhost:9090, latest value 1"

# Automated audit — axe-core or Lighthouse:
# Run axe-core against the dev server; expect zero AA violations.
# Run Lighthouse Accessibility category; expect score >= 95.

# Contrast audit:
# Verify text-on-background contrast >= 4.5:1 for normal text, >= 3:1 for large.
# Verify the warning banner's text and icon contrast meets AA.
# Verify the chart legend text-on-background meets AA.

# Reduced motion:
# Set the OS to "Reduce motion" preference.
# Reload Prism; press Run.
# Expected: no animated transitions on the chart redraw or skeleton fade-in.
```

## Acceptance summary (full UAT in `user-stories.md` and `journey-incident-response.feature`)

- Keyboard tab order is: query input → time-range picker → Run button → auto-refresh picker. Reverse tab works.
- Focus indicators are visible on every interactive element. (DESIGN locks the visual style; the contract is "visible".)
- Pressing Enter while focused on the query input is equivalent to pressing Run (already from US-PR-02; explicitly tested here).
- Screen readers announce the page title, the query input's accessible name, the time-range picker's current value, and chart updates.
- Chart palette is colour-blind-safe (DESIGN picks a specific palette; common safe choices: Tableau 10, ColorBrewer Set2, Wong's eight-colour palette). No information is conveyed by colour alone — the legend names labels in text.
- Text contrast meets WCAG 2.2 AA: at least 4.5:1 for normal text, 3:1 for large text.
- Touch / click targets are at least 24 × 24 CSS pixels.
- The page respects `prefers-reduced-motion`: skeleton loaders fade in instantly (no opacity transitions), chart redraws happen in a single frame (no animated point migration).
- Run an automated audit (axe-core, Lighthouse) and have zero AA-level violations.

## Complexity drivers

- ECharts' default rendering is a `<canvas>` element, which is opaque to screen readers. DESIGN must add a textual fallback (e.g. an SR-only `<table>` summarising the series and their values) — ECharts has built-in support for this via `aria` config, or it can be hand-rolled.
- Custom dropdown pickers (time range, auto-refresh) need ARIA-compliant `role="listbox"` and `role="option"` semantics, OR they should be standard `<select>` elements. DESIGN decides which approach.
- Focus management on chart redraw and on auto-refresh tick: focus must NOT jump unexpectedly (would break screen-reader users mid-read). v0 contract: focus stays where it was; DESIGN ensures.
- The `prefers-reduced-motion` media query must be honoured by both CSS-driven and JS-driven motion. Audit Slices 01–05 for animations that need a reduced-motion fallback.

## Known unknowns

- The exact colour-blind-safe palette. DESIGN picks; the constraint is "passes a deuteranopia and protanopia simulation".
- Whether to also support `prefers-contrast: high` (WCAG 2.2 SC 1.4.6 AAA, not AA but increasingly common). v0 contract: **AA only**, AAA-style high-contrast modes are out of scope for v0; revisit in v1.
- Whether the chart's textual fallback is always rendered (and visually hidden) or only rendered when an assistive-tech user is detected. v0 contract: **always rendered**, visually hidden via `clip: rect(0 0 0 0)` etc. Detection-based rendering is fragile; always-present is robust.

## Out of scope for this slice

- AAA-level conformance — out of v0; v0 targets AA.
- Internationalisation / right-to-left layouts — out of v0; v0 is English-only with UTC timestamps.
- High-contrast mode — out of v0; revisit if operator interviews surface demand.
- Voice-control compatibility (e.g. Voice Control on macOS) — implied by keyboard accessibility but not explicitly tested at v0.
