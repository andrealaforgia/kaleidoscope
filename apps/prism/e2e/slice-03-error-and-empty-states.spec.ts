// Kaleidoscope Prism — operator-facing observability SPA
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

// Slice 03 — Error and empty states, end-to-end.
//
// I am Priya. The page must NEVER blank on me. Every failure mode
// renders inline. The query input keeps focus where it should. The
// URL stays shareable. No JavaScript exception escapes to the
// console.
//
// Stories: US-PR-03, US-PR-06 (AC-6.2), US-PR-04 (URL preserved).
// KPIs anchored: KPI 5 — page-stays-usable (the structural Playwright counterpart).

import { test, expect } from '@playwright/test';

// Per kpi-instrumentation.md > 6.3: every Playwright test in this spec
// accumulates page-error and console-error events; the test fails if
// any uncaught error escaped during the test.

test.describe('Slice 03 page-stays-usable — across every failure mode (KPI 5)', () => {
  let pageErrors: Error[] = [];
  let consoleErrors: string[] = [];

  test.beforeEach(({ page }) => {
    pageErrors = [];
    consoleErrors = [];
    page.on('pageerror', (err) => pageErrors.push(err));
    page.on('console', (msg) => {
      if (msg.type() === 'error') consoleErrors.push(msg.text());
    });
  });

  test.afterEach(() => {
    expect(pageErrors).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });

  // ---------------------------------------------------------------
  // Failure mode 1 — PromQL parse error (AC-3.2)
  // ---------------------------------------------------------------
  test('a PromQL parse error renders inline; the page stays interactive (AC-3.2, KPI 5)', async ({
    page,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN the real Prometheus container is running
    // WHEN I open Prism at "/?q=invalid syntax)("
    // AND wait for either the chart or the warning banner to render
    // THEN a warning banner appears with the backend's verbatim error text
    // AND the chart area shows "Backend rejected this query."
    // AND the query input is focusable
    // AND page.url() still encodes the broken query
  });

  // ---------------------------------------------------------------
  // Failure mode 2 — transport network failure (AC-3.3)
  // ---------------------------------------------------------------
  test('a backend-unreachable error renders the backend label (AC-3.3, KPI 5)', async ({
    page,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have rendered a successful chart at time T
    // WHEN I stop the Prometheus container
    // AND I press Run again
    // THEN a warning banner appears naming "dev-local-prom"
    // AND the body shows "Last successful fetch: ${T-as-iso}"
    // AND the chart canvas from the previous successful render is removed
    // AND no uncaught console error
  });

  // ---------------------------------------------------------------
  // Failure mode 3 — transport HTTP 500 (AC-3.3)
  // ---------------------------------------------------------------
  test('an HTTP 500 from the backend renders inline (AC-3.3, KPI 5)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I intercept /api/v1/query_range with route.fulfill returning 500
    // WHEN I run a query
    // THEN a warning banner appears mentioning HTTP 500
    // AND the page is interactive
  });

  // ---------------------------------------------------------------
  // Failure mode 4 — empty result (AC-3.4)
  // ---------------------------------------------------------------
  test('a valid query returning empty data renders calmly, no warning (AC-3.4)', async ({
    page,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN the real Prometheus container is running
    // WHEN I open Prism and run "up{job=\"nonexistent\"}"
    // THEN the chart area shows "No data for ${range}. Check the metric name or widen the range."
    // AND no warning banner is shown
    // AND the URL still encodes the (empty-yielding) query
  });

  // ---------------------------------------------------------------
  // Failure mode 5 — /config.json unreachable (AC-6.2)
  // ---------------------------------------------------------------
  test('a missing /config.json renders the composition-root error UI (AC-6.2, KPI 5)', async ({
    page,
    context,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I intercept /config.json returning 404
    // WHEN I open Prism
    // THEN I see "Configuration is missing. Contact your Prism administrator."
    // AND the chrome backend label reads "(unconfigured)"
    // AND no fetch to /api/v1/query_range happens (assert via route handler)
  });

  // ---------------------------------------------------------------
  // Failure mode 6 — malformed URL (AC-3 + KPI 5)
  // ---------------------------------------------------------------
  test('a hand-edited URL with bad "from" lands on the calm banner (KPI 5)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I open Prism at "/?q=up&from=garbage"
    // WHEN the page loads
    // THEN the malformed-URL banner appears
    // AND the banner names "from" as the invalid field
    // AND the picker shows "Last 15 min"
    // AND the page is interactive
  });

  // ---------------------------------------------------------------
  // Cumulative state — multiple failures in sequence
  // ---------------------------------------------------------------
  test('a sequence of parse-error → empty → success leaves the page in a sensible state', async ({
    page,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have an interactive Prism page
    // WHEN I run a parse-error query, then an empty-result query, then "up"
    // THEN at the end I see a chart from the "up" query
    // AND no stale banner from the previous parse-error remains
  });
});
