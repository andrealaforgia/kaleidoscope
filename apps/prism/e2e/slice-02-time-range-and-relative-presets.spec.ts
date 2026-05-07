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

// Slice 02 — Time range and relative presets, end-to-end.
//
// I am Priya. I open Prism, type a query, see a chart at the default
// 15-min range. The chart shows the spike but I want to see whether
// it happened before. I open the picker, pick "Last 6 h", and the
// chart re-fetches. The URL bar updates so a teammate clicks it and
// sees the same widened view.
//
// Stories: US-PR-02 (relative), US-PR-04 (relative URL roundtrip).
// KPIs anchored: KPI 4 (relative cross-tab byte-equality).

import { test, expect } from '@playwright/test';

test.describe('Slice 02 picker — when I pick a different relative preset', () => {
  test('the URL updates synchronously when I pick "Last 6 h" (AC-2.2)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 02 DELIVER');
    // GIVEN I have rendered a chart at the default range
    // WHEN I open the picker and select "Last 6 h"
    // THEN the URL bar updates to "?q=up&from=-6h&to=now"
    // AND the chart re-fetches against the wider range
  });

  test('a fresh tab on the same URL reproduces the picker state (AC-2.2, KPI 4)', async ({ context }) => {
    throw new Error('UNIMPLEMENTED — Slice 02 DELIVER');
    // GIVEN I have rendered a chart with picker at "Last 6 h"
    // WHEN I open the same URL in a fresh tab
    // THEN the picker in the fresh tab shows "Last 6 h"
    // AND the chart in the fresh tab paints with the same series count
    // AND the rendered series JSON matches byte-for-byte (modulo time drift on
    //     the now-anchored end of the range — assert series count + label set
    //     equality, not point-by-point equality)
    //
    // const tab1 = await context.newPage();
    // await tab1.goto('/?q=up&from=-6h&to=now');
    // await tab1.waitForSelector('[data-prism-chart-painted="true"]');
    // const seriesA = await tab1.evaluate(() => /* echart.getOption().series */);
    //
    // const tab2 = await context.newPage();
    // await tab2.goto('/?q=up&from=-6h&to=now');
    // await tab2.waitForSelector('[data-prism-chart-painted="true"]');
    // const seriesB = await tab2.evaluate(() => /* echart.getOption().series */);
    //
    // expect(seriesA.map((s: any) => s.name)).toEqual(seriesB.map((s: any) => s.name));
  });

  test('changing the picker preserves the query I typed (journey integration checkpoint)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 02 DELIVER');
    // GIVEN I have typed "up" and rendered the chart at "Last 15 min"
    // WHEN I select "Last 1 h" in the picker
    // THEN the query input still contains "up"
    // AND the URL still has q=up
  });
});

test.describe('Slice 02 keyboard — when I drive the picker with the keyboard', () => {
  test('I can open the picker, navigate, and select with arrow keys (anticipates Slice 06 a11y)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 02 DELIVER');
    // GIVEN I have loaded a fresh Prism page
    // WHEN I tab to the time-range picker
    // AND I press Down to open it
    // AND I press Down to highlight "Last 1 h"
    // AND I press Enter
    // THEN the picker closes with "Last 1 h" selected
    // AND the URL updates to from=-1h
    // AND focus returns to the picker button
  });
});

test.describe('Slice 02 forgiving URL — when a hand-edited URL has a malformed offset', () => {
  test('the SPA does not blank on "from=garbage" (KPI 5 partial)', async ({ page }) => {
    throw new Error('UNIMPLEMENTED — Slice 02 DELIVER');
    // GIVEN I open Prism at "/?q=up&from=garbage&to=now"
    // WHEN the page loads
    // THEN the SPA shows a calm banner naming "from" as the bad parameter
    // AND the picker shows the default "Last 15 min"
    // AND the page is interactive (the input is focusable)
  });
});
