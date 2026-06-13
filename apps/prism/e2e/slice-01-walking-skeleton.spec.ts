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
  test('I type `up`, press Enter, and I see a genuinely painted chart (AC-1.4, US-PR-01)', async ({
    page,
  }) => {
    // GIVEN Playwright globalSetup has started the digest-pinned Prometheus
    //   fixture, which self-scrapes so the `up` metric carries real points,
    // AND the Vite dev server proxies /api/v1 to that fixture (vite.config.ts).
    //
    // WHEN I open Prism, type `up` into the focused query input, and run it.
    //
    // (The embedded "< 1000 ms" wall-clock line from the original pseudocode
    //  is intentionally dropped: that is the latency KPI family, out of scope
    //  per design/upstream-changes.md. This block asserts PAINT, not latency.)
    await page.goto('/');
    await expect(page.getByTestId('query-input')).toBeFocused();
    await page.getByTestId('query-input').fill('up');
    await page.keyboard.press('Enter');

    // THEN — the falsifiable three-part paint conjunction (ADR-0075 D1 ∧ D2 ∧ D3):
    //
    // Part 1 — the paint signal flips to "true" only once ECharts `finished`
    //   fired with a non-empty rendered series. Against HEAD this attribute
    //   does not exist, so this wait can only time out (RED-not-BROKEN).
    await page.waitForSelector('[data-prism-chart-painted="true"]', { timeout: 15_000 });

    // Part 2 — the rendered <canvas> carries real ink: sampled pixels are
    //   non-uniform (> 1 distinct value), defeating the blank-canvas case.
    const distinctColours = await page.evaluate(() => {
      const canvas = document.querySelector<HTMLCanvasElement>(
        '[data-testid="chart-canvas"] canvas',
      );
      if (canvas === null) return 0;
      const ctx = canvas.getContext('2d');
      if (ctx === null) return 0;
      const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const seen = new Set<string>();
      for (let i = 0; i < data.length; i += 4 * 64 /* stride */) {
        seen.add(`${data[i]},${data[i + 1]},${data[i + 2]},${data[i + 3]}`);
      }
      return seen.size;
    });
    expect(distinctColours).toBeGreaterThan(1);

    // Part 3 (corroborating) — the accessible fallback table caption confirms
    //   the data reached React: at least one series with at least one point.
    const captionText = await page
      .getByTestId('chart-fallback-table')
      .locator('caption')
      .innerText();
    const seriesCount = Number(/(\d+)\s+series/.exec(captionText)?.[1] ?? '0');
    const pointCount = Number(/(\d+)\s+points/.exec(captionText)?.[1] ?? '0');
    expect(seriesCount).toBeGreaterThanOrEqual(1);
    expect(pointCount).toBeGreaterThanOrEqual(1);

    // AND the URL bar carries the shareable query so I can paste it into Slack.
    expect(page.url()).toContain('q=up');
    expect(page.url()).toContain('from=-15m');
    expect(page.url()).toContain('to=now');
  });

  // -----------------------------------------------------------------
  // KPI 1 — first-chart latency p95 < 2s over 20 runs
  // -----------------------------------------------------------------

  // fixme: perf KPI, out of prism-echarts-paint-e2e-v0 scope (ADR-0075 D5);
  // deferred per MEMORY p95_wallclock_flakes_overnight. Named future work.
  test.fixme('the p95 of "page open → first chart paint" is under 2 seconds across 20 runs (KPI 1)', async ({
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

  // fixme: perf KPI, out of prism-echarts-paint-e2e-v0 scope (ADR-0075 D5);
  // deferred per MEMORY p95_wallclock_flakes_overnight. Named future work.
  test.fixme('the p95 of "Run press → next chart paint" is under 800 ms across 20 iterate cycles (KPI 2)', async ({
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
  test('the page chrome shows the backend label from /config.json (AC-6.1, US-PR-06)', async ({
    page,
  }) => {
    // GIVEN /config.json carries the backend label (public/config.json:
    //   "Pulse (durable)" — the real deployed label, not the placeholder the
    //   original pseudocode named; see distill/upstream-issues.md).
    // WHEN I open Prism.
    await page.goto('/');
    // THEN the chrome names the backend so the operator knows what she queries.
    await expect(page.getByTestId('backend-label')).toHaveText(/Backend:\s*Pulse \(durable\)/);
  });

  test('the chrome still names the backend after the chart paints (AC-6.3, US-PR-06)', async ({
    page,
  }) => {
    // GIVEN I open Prism and run a successful `up` query.
    await page.goto('/');
    await page.getByTestId('query-input').fill('up');
    await page.keyboard.press('Enter');
    // WHEN the chart paints (paint signal flips — RED against HEAD).
    await page.waitForSelector('[data-prism-chart-painted="true"]', { timeout: 15_000 });
    // THEN the chrome still names the backend; painting did not blank it.
    await expect(page.getByTestId('backend-label')).toHaveText(/Backend:\s*Pulse \(durable\)/);
  });
});

// =============================================================================
// US-PR-04 — within-session URL roundtrip
// =============================================================================

test.describe('Slice 01 URL roundtrip — when I open the same URL in a new tab', () => {
  test('a fresh tab on the same URL repaints the same chart (AC-4.2, US-PR-04)', async ({
    context,
  }) => {
    // GIVEN I share the URL "/?q=up&from=-15m&to=now". The query is encoded in
    //   the URL; the input is pre-filled from it on load.
    //
    // NOTE: Prism does not auto-execute the URL query on mount (the reducer
    //   bootstrap with refresh=off emits no fetch, and there is no mount-time
    //   query). So the operator (or teammate) presses Run; the same URL then
    //   repaints the same chart. Auto-run-on-mount is NOT in ADR-0075 scope —
    //   see distill/upstream-issues.md. We therefore drive Run explicitly.
    //
    // The structural roundtrip assertion is "same number of series" across
    //   tabs (point counts drift with the relative window between two loads,
    //   so series count is the stable invariant).
    const seriesCountFromCaption = async (caption: string): Promise<number> =>
      Number(/(\d+)\s+series/.exec(caption)?.[1] ?? '0');

    const tab1 = await context.newPage();
    await tab1.goto('/?q=up&from=-15m&to=now');
    await tab1.getByTestId('run-button').click();
    await tab1.waitForSelector('[data-prism-chart-painted="true"]', { timeout: 15_000 });
    const captionA = await tab1
      .getByTestId('chart-fallback-table')
      .locator('caption')
      .innerText();
    const seriesA = await seriesCountFromCaption(captionA);

    const tab2 = await context.newPage();
    await tab2.goto('/?q=up&from=-15m&to=now');
    await tab2.getByTestId('run-button').click();
    await tab2.waitForSelector('[data-prism-chart-painted="true"]', { timeout: 15_000 });
    const captionB = await tab2
      .getByTestId('chart-fallback-table')
      .locator('caption')
      .innerText();
    const seriesB = await seriesCountFromCaption(captionB);

    // THEN both tabs painted, and the same query yields the same series shape.
    expect(seriesA).toBeGreaterThanOrEqual(1);
    expect(seriesB).toBe(seriesA);
  });
});

// =============================================================================
// Cross-KPI guardrail — operator-time budget under 5 seconds
// =============================================================================

test.describe('Slice 01 operator-time guardrail', () => {
  // fixme: perf KPI guardrail, out of prism-echarts-paint-e2e-v0 scope
  // (ADR-0075 D5); deferred per MEMORY p95_wallclock_flakes_overnight.
  test.fixme('the full walking-skeleton flow completes in under 5 seconds median (kpi-instrumentation.md § 7)', async ({
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
