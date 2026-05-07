# Feature: Prism v0 — incident-response query journey
#
# Wave: DISCUSS — Phase 2 (UX journey design).
# Author: Luna (nw-product-owner).
# Date: 2026-05-07.
# Companion documents: journey-incident-response-visual.md, journey-incident-response.yaml,
#   user-stories.md, jtbd-job-stories.md, jtbd-four-forces.md.
#
# Persona: Priya Raman, senior SRE on-call at acme-observability, paged by a
# checkout-service latency alert. Backend: Mimir / Prometheus, single-tenant,
# operator-deployed.
#
# Stack constraint (locked in wave-decisions.md): React + TypeScript + Vite +
# Apache ECharts. Backend protocol (locked): Prometheus HTTP API
# (/api/v1/query_range, /api/v1/query, /api/v1/labels). PromQL passes through
# verbatim — Prism does NOT parse, rewrite, or extend PromQL.
#
# Scenarios are grouped by user story (US-PR-01 .. US-PR-07). The story IDs
# match user-stories.md.

Feature: Prism v0 — incident-response query journey
  # Platform: web (browser-side SPA)
  # Key Nielsen heuristics: 1 (visibility of system status), 5 (error prevention),
  #   6 (recognition over recall), 9 (help with errors)
  # Accessibility: WCAG 2.2 AA — keyboard-operable, focus indicators visible,
  #   colour-blind-safe chart palette
  # Data fidelity: no client-side smoothing, interpolation, aggregation, or caching

  # =========================================================================
  # US-PR-01 — Fresh page load is fast and focused
  # =========================================================================

  Scenario: Fresh page load focuses the query input within the latency budget
    Given the operator has navigated to the Prism URL on a typical operator browser
    When the SPA bundle finishes loading
    Then the query input is the focused element
    And the time-range picker defaults to "Last 15 min"
    And the auto-refresh picker defaults to "off"
    And the Run button is disabled because the query is empty
    And the page is interactive within 2 seconds at the 95th percentile
    And the chrome names the configured backend label

  Scenario: Configuration is missing
    Given the operator has navigated to the Prism URL
    And /config.json returns 404
    When the SPA finishes loading
    Then the page renders a single error: "Configuration is missing. Contact your Prism administrator."
    And the chrome backend label reads "(unconfigured)"
    And no fetch to /api/v1/query_range is attempted

  # =========================================================================
  # US-PR-02 — Compose a query with a time range
  # =========================================================================

  Scenario: Run is enabled when the query becomes non-empty
    Given the operator has loaded a fresh Prism page
    And the query input is empty
    When the operator types a non-empty PromQL expression
    Then the Run button becomes enabled
    And pressing Enter while focused in the query input is equivalent to pressing Run

  Scenario: Time-range picker offers the operator-canonical relative presets
    Given the operator has loaded a fresh Prism page
    When the operator opens the time-range picker
    Then the relative presets offered are "Last 5 min", "Last 15 min", "Last 1 h", "Last 6 h", "Last 24 h"
    And a "Custom" option allows entering an absolute from-and-to pair

  Scenario: Absolute time range disables auto-refresh
    Given the operator has set the time range to an absolute pair "from=2026-05-07T00:00Z to=2026-05-07T01:00Z"
    Then the auto-refresh picker is disabled
    And the URL parameter "refresh" is "off"

  Scenario: Absolute time range with start later than end is rejected at the picker
    Given the operator opens the time-range picker
    When the operator enters an absolute range with from later than to
    Then the picker shows an inline error "Time range start must be before end"
    And the Run button is disabled until the range is valid

  Scenario: Time range ending in the future is rejected at the picker
    Given the operator opens the time-range picker
    When the operator enters an absolute range whose to-timestamp is later than now
    Then the picker shows an inline error "Time range ends in the future. Set a range that ends at or before now."
    And the Run button is disabled until the range is valid

  # =========================================================================
  # US-PR-03 — PromQL passes through verbatim and chart matches backend data
  # =========================================================================

  Scenario: Query input passes through unmodified to the backend
    Given the operator has typed "rate(http_server_duration_seconds_count[5m])" into the query input
    When the operator presses Run
    Then Prism issues a GET request to "/api/v1/query_range" on the configured backend
    And the request "query" parameter is exactly "rate(http_server_duration_seconds_count[5m])"
    And no Prism-side parsing or rewriting has occurred

  Scenario: Chart contains exactly the points the backend returned
    Given the operator has run a PromQL query
    And the backend returned 3 series with a total of 240 points
    When the chart renders
    Then the chart contains exactly 3 series
    And the total number of plotted points is exactly 240
    And the footer reports "3 series · 240 points · fetched in <Q> ms"
    And no point has been interpolated, smoothed, or aggregated client-side

  @property
  Scenario: Prism's PromQL result matches a reference curl query
    Given the same PromQL query "rate(http_server_duration_seconds_count[5m])" issued via curl directly to the backend's /api/v1/query_range
    When the operator runs the same query at the same time range in Prism
    Then the data.result payload from the curl command and the data Prism plots are point-for-point identical
    And the legend names each series by its labels, in the same order as data.result

  Scenario: Legend names each series by its labels
    Given the backend has returned 3 series with distinct label sets
    When the chart renders
    Then the legend below the chart shows one entry per series
    And each entry names the labels (e.g. method, route, instance) verbatim, not "series-1", "series-2", etc.

  # =========================================================================
  # US-PR-04 — Errors render inline; URL stays usable
  # =========================================================================

  Scenario: PromQL parse error renders inline without crashing the page
    Given the operator has typed a syntactically invalid PromQL query "rate(metric_name[5m"
    When the operator presses Run
    Then the backend returns 400 with body containing an "error" field
    And Prism renders a warning banner with the backend's error text verbatim
    And the chart area shows a calm fallback message "Backend rejected this query."
    And the page remains interactive
    And the URL still encodes the (invalid) query "rate(metric_name[5m" so a colleague can see the same broken state

  Scenario: Backend unreachable renders the last successful fetch time
    Given Prism has previously rendered a successful chart at "${last_fetch_time}"
    And the backend has since become unreachable (TCP refused)
    When the operator presses Run
    Then Prism renders a warning banner naming the backend URL and the transport-level error
    And the body region shows "Last successful fetch: ${last_fetch_time}"
    And the previously-rendered chart is no longer shown (no stale-data lying)

  Scenario: Backend returns empty result for a valid query
    Given the operator has run a syntactically valid PromQL query
    And the backend returned 0 series for the requested time range
    When the chart area would render
    Then the chart area shows the empty state "No data for ${time_range_iso}. Check the metric name or widen the range."
    And no warning banner is shown (an empty result is not an error)
    And the URL still encodes the query (so the operator can share the empty-state observation)

  Scenario: URL roundtrip reproduces the exact view
    Given the operator has rendered a chart at URL "${prism_url}/?q=...&from=-15m&to=now&refresh=30s"
    When a colleague opens that URL in a fresh browser tab
    Then the query input contains the same PromQL string
    And the time-range picker reflects "Last 15 min"
    And the auto-refresh picker reflects "30 s"
    And the chart that renders is identical to the original (modulo time drift for relative ranges)

  # =========================================================================
  # US-PR-05 — Iterate (edit-and-rerun, auto-refresh)
  # =========================================================================

  Scenario: Editing the query keeps the time range stable
    Given the operator has rendered a chart with time range "Last 15 min"
    When the operator edits the query and presses Run again
    Then the time range remains "Last 15 min"
    And the chart redraws against the new query without flicker
    And the URL is updated to encode the new query

  Scenario: Auto-refresh re-issues the same query
    Given the operator has rendered a chart with time range "Last 15 min"
    And auto-refresh is set to 30 seconds
    When 30 seconds elapse since the last successful fetch
    Then Prism issues a new GET to /api/v1/query_range with the same "query" parameter
    And the time range slides forward (to=NOW resolves at fetch time)
    And no client-side smoothing or interpolation occurs across the refresh
    And the chart redraws without flicker
    And the status line reads "Last fetched <time> · next in 30 s" between ticks

  Scenario: Auto-refresh ticks do not overlap
    Given auto-refresh is set to 30 seconds
    And a previous fetch is still in flight when the next tick fires
    When the next tick fires
    Then the previous fetch is cancelled
    And only the new fetch's result is rendered
    And no overlapping renders occur

  Scenario: Auto-refresh in a background tab is paused
    Given auto-refresh is set to 30 seconds
    When the browser tab becomes hidden (operator switches tabs)
    Then no further fetches are issued until the tab becomes visible again
    And on regaining visibility, a fresh fetch is issued immediately, then the regular interval resumes

  @property
  Scenario: Pressing Run always fetches fresh data, never a cached result
    Given the operator has just received a chart from a query at time T
    When the operator presses Run again at time T+1 second with the same query
    Then a new HTTP request is issued
    And no client-side cache is consulted

  # =========================================================================
  # US-PR-06 — Single-backend deployment, named explicitly
  # =========================================================================

  Scenario: Page chrome names the configured backend
    Given Prism is configured for backend label "acme-prod-mimir" with URL "https://mimir.acme.internal"
    When the SPA loads
    Then the page chrome (top-right) shows "backend: acme-prod-mimir"
    And the footer shows the URL "https://mimir.acme.internal"

  Scenario: Pasted URL targets a different backend than the current configuration
    Given Prism is currently configured for backend label "acme-prod-mimir"
    And a colleague has pasted a URL crafted against backend label "acme-staging-mimir"
    When the operator opens that URL in this Prism instance
    Then the page chrome explicitly shows "backend: acme-prod-mimir"
    And the query in the URL is loaded into the query input
    And no silent backend swap occurs
    And on Run, data is fetched from "acme-prod-mimir", not "acme-staging-mimir"

  # =========================================================================
  # US-PR-07 — Accessibility and keyboard operability
  # =========================================================================

  Scenario: Keyboard tab order follows the natural flow
    Given the operator is on a fresh page load
    When the operator tabs through the interactive elements
    Then the focus order is: query input → time-range picker → Run button → auto-refresh picker
    And focus indicators are visible on every focused element

  Scenario: Enter in the query input is equivalent to pressing Run
    Given the query input is focused and contains a non-empty query
    When the operator presses Enter
    Then Prism issues the same fetch that pressing Run would have issued

  Scenario: Chart palette is colour-blind-safe by default
    Given the chart renders with N series
    When the chart palette is selected
    Then the palette is one of the documented colour-blind-safe palettes
    And no information conveyed by the chart is colour-only (legend names labels in text)

  Scenario: Text contrast ratio meets WCAG 2.2 AA
    Given any text on the page
    When the contrast ratio is measured against its background
    Then the ratio is at least 4.5:1 for normal text
    And at least 3:1 for large text
