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

// Slice 01 — Walking skeleton, end-to-end against a real Prometheus.
//
// The brief: I am Priya. I open Prism on my laptop at 03:14 in the
// morning. I type `up` into the query input, press Enter, and within
// one second I see a line chart of the points the backend returned.
// The page chrome names the backend. The URL bar updates so I can
// paste it into Slack. A teammate clicks the link in Slack and sees
// the same chart.
//
// Strategy C "real local": this spec depends on
// playwright.config.ts > globalSetup having started a real
// prom/prometheus@<digest> container with the seeded `up` metric,
// per environments.yaml > external_fixtures > prometheus_container.
// Per wave-decisions.md > D1.
//
// Stories: US-PR-01, US-PR-04 (within-session reload), US-PR-06.
// KPIs anchored: KPI 1 (first-chart latency p95 < 2s), KPI 2 (iterate p95 < 800ms).

import { test, expect } from '@playwright/test';

// =============================================================================
// US-PR-01 — query → chart against a real Prometheus
// =============================================================================

test.describe('Slice 01 walking skeleton — query → chart end-to-end', () => {
  test('I type `up`, press Enter, and within one second I see a chart (AC-1.4, KPI 1, KPI 2)', async ({
    page,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN Playwright globalSetup has started a real prom/prometheus container
    // AND the container has scraped itself for at least 30 seconds (so `up` has 24h fixture)
    // AND a Prism preview server is running with /config.json pointing at the container
    //
    // WHEN I navigate to the Prism URL
    // AND I type "up" into the focused query input
    // AND I press Enter
    //
    // THEN within 1000 ms wall-clock the chart canvas paints
    // AND the chart shows at least one series with at least one point
    // AND the URL bar reads "?q=up&from=-15m&to=now"
    // AND the chrome shows "backend: dev-local-prom"
    //
    // await page.goto('/');
    // await expect(page.getByLabel(/PromQL query/i)).toBeFocused();
    // const t0 = Date.now();
    // await page.getByLabel(/PromQL query/i).fill('up');
    // await page.keyboard.press('Enter');
    // await page.waitForSelector('[data-prism-chart-painted="true"]');
    // const t1 = Date.now();
    // expect(t1 - t0).toBeLessThan(1000);
    // expect(page.url()).toContain('?q=up&from=-15m&to=now');
    // await expect(page.getByText(/backend: dev-local-prom/)).toBeVisible();
  });

  // -----------------------------------------------------------------
  // KPI 1 — first-chart latency p95 < 2s over 20 runs
  // -----------------------------------------------------------------

  test('the p95 of "page open → first chart paint" is under 2 seconds across 20 runs (KPI 1)', async ({
    context,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN a fresh browser context per kpi-instrumentation.md > 2.3
    // WHEN I run the walking-skeleton flow 20 times in a tight loop
    // THEN the 95th-percentile delta from DOMContentLoaded to the first
    //      [data-prism-chart-painted="true"] toggle is under 2000 ms
    // AND no single run exceeds 4000 ms (no extreme outliers either)
    //
    // const deltas: number[] = [];
    // for (let i = 0; i < 20; i++) {
    //   const page = await context.newPage();
    //   await page.goto('/?q=up&from=-15m&to=now');
    //   await page.waitForSelector('[data-prism-chart-painted="true"]');
    //   const delta = await page.evaluate(() => performance.now());
    //   deltas.push(delta);
    //   await page.close();
    // }
    // deltas.sort((a, b) => a - b);
    // const p95 = deltas[Math.floor(deltas.length * 0.95)];
    // expect(p95).toBeLessThan(2000);
  });

  // -----------------------------------------------------------------
  // KPI 2 — iterate latency p95 < 800ms over 20 runs
  // -----------------------------------------------------------------

  test('the p95 of "Run press → next chart paint" is under 800 ms across 20 iterate cycles (KPI 2)', async ({
    page,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I have already rendered a chart (KPI 1 path complete)
    // WHEN I press Run again 20 times with the same query
    // THEN the 95th-percentile of "click → next [data-prism-chart-painted] toggle" is under 800 ms
  });
});

// =============================================================================
// US-PR-06 — page chrome backend identification
// =============================================================================

test.describe('Slice 01 chrome — when I open Prism on a configured deployment', () => {
  test('the page chrome shows the backend label from /config.json (AC-6.1)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN /config.json has backend.label="dev-local-prom"
    // WHEN I open Prism
    // THEN the chrome shows "backend: dev-local-prom"
  });

  test('the chrome remains visible after a successful query (AC-6.3)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I have run a successful query
    // WHEN the chart paints
    // THEN the chrome still shows "backend: dev-local-prom"
  });
});

// =============================================================================
// US-PR-04 — within-session URL roundtrip
// =============================================================================

test.describe('Slice 01 URL roundtrip — when I open the same URL in a new tab', () => {
  test('a fresh tab on the same URL renders the same chart (AC-4.2)', async ({ context }) => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I have rendered a chart at URL "/?q=up&from=-15m&to=now"
    // AND I have captured the rendered series JSON via page.evaluate
    //
    // WHEN I open the same URL in a fresh browser context
    //
    // THEN the chart paints with the same series JSON (modulo time drift
    //      across the relative range: the structural assertion is
    //      "same number of series, same labels, same point count")
    //
    // const tab1 = await context.newPage();
    // await tab1.goto('/?q=up&from=-15m&to=now');
    // await tab1.waitForSelector('[data-prism-chart-painted="true"]');
    // const seriesA = await tab1.evaluate(() => /* read echart.getOption().series */);
    //
    // const tab2 = await context.newPage();
    // await tab2.goto('/?q=up&from=-15m&to=now');
    // await tab2.waitForSelector('[data-prism-chart-painted="true"]');
    // const seriesB = await tab2.evaluate(() => /* read echart.getOption().series */);
    //
    // expect(seriesA.length).toBe(seriesB.length);
    // for (let i = 0; i < seriesA.length; i++) {
    //   expect(seriesA[i].name).toBe(seriesB[i].name);
    // }
  });
});

// =============================================================================
// Cross-KPI guardrail — operator-time budget under 5 seconds
// =============================================================================

test.describe('Slice 01 operator-time guardrail', () => {
  test('the full walking-skeleton flow completes in under 5 seconds median (kpi-instrumentation.md § 7)', async ({
    page,
  }) => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN nothing
    // WHEN I time:
    //   - page.goto('/')
    //   - type "up"
    //   - press Run
    //   - wait for chart paint
    // THEN the total wall-clock is under 5000 ms (median across 5 runs)
  });
});
