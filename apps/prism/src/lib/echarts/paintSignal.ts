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

// ADR-0075 D1 — the pure decision behind the paint signal.
//
// `data-prism-chart-painted` flips to "true" only once ECharts' real
// `finished` event reports a genuinely non-empty rendered series — a
// chart that drew ink for real data, not a blank canvas. The DOM and
// event wiring live in EChart.tsx and are browser-only (jsdom never
// reaches them: the canvas-2D probe is null, so echarts.init and the
// `finished` subscription never happen). That wiring is covered by the
// Playwright slice-01/slice-03 specs (Gate 7).
//
// This module extracts the non-empty-series decision the wiring
// consults so Gate 10 (StrykerJS, vitest/jsdom) can kill the logic-
// rich predicate — the `Array.isArray`, the `.some`, the `length > 0`
// boundary — without a real browser. No React, no DOM, no I/O.

/** Minimal structural shape of an ECharts series entry we inspect. */
interface SeriesLike {
  readonly data?: unknown;
}

/**
 * True iff `series` (the `getOption().series` ECharts reports on
 * `finished`) is a non-empty array carrying at least one series with
 * at least one data point — a genuinely painted, non-empty chart.
 *
 * An empty option (no series, or series whose `data` is empty) must
 * NOT flip the paint signal: that is the honest empty/error state,
 * which never mounts the chart in the first place (ADR-0075 D6).
 */
export function seriesHasInk(series: unknown): boolean {
  if (!Array.isArray(series)) {
    return false;
  }
  return series.some((entry) => {
    const data = (entry as SeriesLike | null | undefined)?.data;
    return Array.isArray(data) && data.length > 0;
  });
}
