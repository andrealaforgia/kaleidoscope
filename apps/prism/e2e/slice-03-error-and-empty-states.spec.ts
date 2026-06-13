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
      if (msg.type() !== 'error') return;
      const text = msg.text();
      // Chromium logs failed HTTP responses (4xx/5xx) as resource-load
      // console errors. The error-path scenarios below deliberately provoke
      // 400/500 responses that the app handles gracefully into VISIBLE
      // banners; those browser network messages are expected noise, not
      // uncaught app errors. The invariant targets APP-emitted errors — in
      // particular the ADR-0075 D3 catch-and-surface "[prism] ECharts
      // setOption failed" console.error — and genuine uncaught exceptions
      // (captured separately via the 'pageerror' listener above).
      if (text.startsWith('Failed to load resource')) return;
      consoleErrors.push(text);
    });
  });

  test.afterEach(() => {
    expect(pageErrors).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });

  // ---------------------------------------------------------------
  // Failure mode 1 — PromQL parse error (AC-3.2)
  // ---------------------------------------------------------------
  test('a PromQL parse error renders inline; the page stays interactive (AC-3.2, KPI 5, US-PE-03)', async ({
    page,
  }) => {
    // GIVEN the digest-pinned Prometheus fixture is running, and I open Prism
    //   with an invalid PromQL pre-filled in the URL.
    await page.goto(`/?q=${encodeURIComponent('invalid syntax)(')}`);
    // WHEN I run it (Prism does not auto-run on mount — see distill notes).
    await page.getByTestId('run-button').click();
    // THEN the backend's rejection surfaces as a VISIBLE warning banner with
    //   the backend's verbatim error text — never a swallowed blank.
    await expect(page.getByTestId('parse-error-banner')).toBeVisible();
    await expect(page.getByTestId('parse-error-fallback')).toHaveText(
      /Backend rejected this query\./,
    );
    // AND no chart is painted: the paint signal attribute is absent from the
    //   DOM entirely (the error state never mounts the chart) — visible error
    //   text is distinct from a blank canvas.
    await expect(page.locator('[data-prism-chart-painted]')).toHaveCount(0);
    // AND the query input is still interactive.
    await expect(page.getByTestId('query-input')).toBeEnabled();
    // AND the URL still encodes the broken query so it stays shareable.
    expect(page.url()).toContain('q=');
  });

  // ---------------------------------------------------------------
  // Failure mode 2 — transport network failure (AC-3.3)
  // ---------------------------------------------------------------
  // fixme: FM2 stops the SHARED global-setup container mid-suite, which
  // conflicts with the shared-fixture model; the route-fulfilled 500
  // (FM3) is the in-scope transport proof (ADR-0075 D5). Named future work.
  test.fixme(
    'a backend-unreachable error renders the backend label (AC-3.3, KPI 5)',
    async ({ page }) => {
      throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
      // GIVEN I have rendered a successful chart at time T
      // WHEN I stop the Prometheus container
      // AND I press Run again
      // THEN a warning banner appears naming "dev-local-prom"
      // AND the body shows "Last successful fetch: ${T-as-iso}"
      // AND the chart canvas from the previous successful render is removed
      // AND no uncaught console error
    },
  );

  // ---------------------------------------------------------------
  // Failure mode 3 — transport HTTP 500 (AC-3.3)
  // ---------------------------------------------------------------
  test('an HTTP 500 from the backend renders inline; the page stays interactive (AC-3.3, KPI 5, US-PE-03)', async ({
    page,
  }) => {
    // GIVEN every query_range call is intercepted to return HTTP 500.
    await page.route('**/api/v1/query_range*', (route) =>
      route.fulfill({ status: 500, contentType: 'text/plain', body: 'internal server error' }),
    );
    // WHEN I open Prism and run `up`.
    await page.goto('/?q=up&from=-15m&to=now');
    await page.getByTestId('run-button').click();
    // THEN a VISIBLE transport-error banner names the backend and the failure
    //   class — not a swallowed blank.
    await expect(page.getByTestId('transport-error-banner')).toBeVisible();
    await expect(page.getByTestId('transport-error-banner')).toContainText(
      /Cannot reach Pulse \(durable\)/,
    );
    await expect(page.getByTestId('transport-error-banner')).toContainText(/http-status/);
    // AND no chart is painted (no paint signal in the DOM).
    await expect(page.locator('[data-prism-chart-painted]')).toHaveCount(0);
    // AND the page stays interactive.
    await expect(page.getByTestId('query-input')).toBeEnabled();
  });

  // ---------------------------------------------------------------
  // Failure mode 4 — empty result (AC-3.4)
  // ---------------------------------------------------------------
  test('a valid query returning empty data renders a calm empty-state, not a blank canvas (AC-3.4, US-PE-02)', async ({
    page,
  }) => {
    // GIVEN the digest-pinned Prometheus fixture is running, and I open Prism
    //   with a valid query that matches no series.
    await page.goto(`/?q=${encodeURIComponent('up{job="nonexistent"}')}&from=-15m&to=now`);
    // WHEN I run it.
    await page.getByTestId('run-button').click();
    // THEN the empty-state shows honest VISIBLE guidance text — distinct from a
    //   blank canvas — telling the operator how to recover.
    await expect(page.getByTestId('empty-state')).toBeVisible();
    await expect(page.getByTestId('empty-state')).toContainText(/No data for/);
    // AND no chart is painted (empty never mounts the chart; no paint signal).
    await expect(page.locator('[data-prism-chart-painted]')).toHaveCount(0);
    // AND no warning banner is shown (empty is not an error).
    await expect(page.getByTestId('parse-error-banner')).toHaveCount(0);
    await expect(page.getByTestId('transport-error-banner')).toHaveCount(0);
    // AND the URL still encodes the (empty-yielding) query.
    expect(page.url()).toContain('q=');
  });

  // ---------------------------------------------------------------
  // Failure mode 5 — /config.json unreachable (AC-6.2)
  // ---------------------------------------------------------------
  // fixme: config-error (AC-6.2) is not in US-PE-02/03 scope; graduates
  // with the broader slice-03 feature (ADR-0075 D5). Named future work.
  test.fixme(
    'a missing /config.json renders the composition-root error UI (AC-6.2, KPI 5)',
    async ({ page, context }) => {
      throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
      // GIVEN I intercept /config.json returning 404
      // WHEN I open Prism
      // THEN I see "Configuration is missing. Contact your Prism administrator."
      // AND the chrome backend label reads "(unconfigured)"
      // AND no fetch to /api/v1/query_range happens (assert via route handler)
    },
  );

  // ---------------------------------------------------------------
  // Failure mode 6 — malformed URL (AC-3 + KPI 5)
  // ---------------------------------------------------------------
  // fixme: malformed-URL codec behaviour is slice-05 territory, not
  // paint/banner; out of US-PE-02/03 scope (ADR-0075 D5). Named future work.
  test.fixme(
    'a hand-edited URL with bad "from" lands on the calm banner (KPI 5)',
    async ({ page }) => {
      throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
      // GIVEN I open Prism at "/?q=up&from=garbage"
      // WHEN the page loads
      // THEN the malformed-URL banner appears
      // AND the banner names "from" as the invalid field
      // AND the picker shows "Last 15 min"
      // AND the page is interactive
    },
  );

  // ---------------------------------------------------------------
  // Cumulative state — multiple failures in sequence
  // ---------------------------------------------------------------
  test('a parse-error → empty → success sequence ends on a painted chart with no stale banner', async ({
    page,
  }) => {
    // GIVEN an interactive Prism page.
    await page.goto('/');

    // WHEN I run an invalid query — a parse-error banner appears.
    await page.getByTestId('query-input').fill('invalid syntax)(');
    await page.getByTestId('run-button').click();
    await expect(page.getByTestId('parse-error-banner')).toBeVisible();

    // AND THEN an empty-yielding query — the calm empty-state replaces the
    //   banner (no stale parse-error left behind).
    await page.getByTestId('query-input').fill('up{job="nonexistent"}');
    await page.getByTestId('run-button').click();
    await expect(page.getByTestId('empty-state')).toBeVisible();
    await expect(page.getByTestId('parse-error-banner')).toHaveCount(0);

    // AND THEN a successful `up` query — the chart paints. This exercises the
    //   paint-signal RESET across queries (ADR-0075 D1): the attribute is set
    //   "false" before each setOption and re-flips to "true" on the next
    //   `finished`. RED against HEAD: no signal exists, so this wait times out.
    await page.getByTestId('query-input').fill('up');
    await page.getByTestId('run-button').click();
    await page.waitForSelector('[data-prism-chart-painted="true"]', { timeout: 15_000 });

    // THEN no stale banner or empty-state from the earlier queries remains.
    await expect(page.getByTestId('parse-error-banner')).toHaveCount(0);
    await expect(page.getByTestId('empty-state')).toHaveCount(0);
  });
});
