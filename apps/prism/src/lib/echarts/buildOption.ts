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

// ADR-0030 — Pure buildOption.
//
// Takes a QueryOutcome plus a BuildOptionContext (palette + range +
// prefersReducedMotion) and returns an EChartsOption. No React, no
// DOM, no I/O, no Date.now(). KPI 3 fidelity invariants are locked
// here at the option-level configuration:
//
//   - `smooth: false` — no Bezier curve
//   - `connectNulls: false` — gaps render as gaps, never bridged
//   - `sampling: 'none'` — no auto-downsampling
//   - data points pass through verbatim from QueryOutcome.series
//
// Mutation-evidence anchor for KPI 3:
// invariant-fidelity.test.ts deterministically kills any option
// mutation that flips these flags.

import type { QueryOutcome, Series, LabelSet } from '../promql/types';
import type { TimeRange } from '../url-state/types';

export type Palette = 'okabe-ito' | 'tableau10';

export interface BuildOptionContext {
  readonly palette: Palette;
  readonly range: TimeRange;
  readonly prefersReducedMotion: boolean;
}

// EChartsOption is a structural type defined by ECharts. The shape
// below names just the fields buildOption produces; consumers pass
// the result into chart.setOption({notMerge:true}) which accepts
// any superset.
export type EChartsOption = Record<string, unknown>;

// Okabe-Ito 8-colour palette: deuteranopia + protanopia safe.
// ADR-0030 names it as the v0 default.
const OKABE_ITO: readonly string[] = [
  '#000000',
  '#E69F00',
  '#56B4E9',
  '#009E73',
  '#F0E442',
  '#0072B2',
  '#D55E00',
  '#CC79A7',
];

// Tableau 10 alternative palette: operator opt-in via the URL
// `palette=tableau10` parameter (Slice 06).
const TABLEAU_10: readonly string[] = [
  '#4E79A7',
  '#F28E2C',
  '#E15759',
  '#76B7B2',
  '#59A14F',
  '#EDC949',
  '#AF7AA1',
  '#FF9DA7',
  '#9C755F',
  '#BAB0AB',
];

function paletteColours(p: Palette): readonly string[] {
  return p === 'okabe-ito' ? OKABE_ITO : TABLEAU_10;
}

/**
 * Build a human-readable series name from a Prometheus LabelSet.
 * Pulls `__name__` to the front (if present); appends remaining
 * labels as `key="value"` pairs in alphabetical order.
 *
 * Example: `{__name__: "up", instance: "fixture", job: "x"}` →
 * `up{instance="fixture", job="x"}`.
 */
function seriesName(labels: LabelSet): string {
  const entries = Object.entries(labels);
  const named = labels['__name__'];
  const rest = entries
    .filter(([k]) => k !== '__name__')
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([k, v]) => `${k}="${v}"`)
    .join(', ');
  if (named === undefined) {
    return rest.length > 0 ? `{${rest}}` : '';
  }
  return rest.length > 0 ? `${named}{${rest}}` : named;
}

function buildSeries(series: ReadonlyArray<Series>): ReadonlyArray<Record<string, unknown>> {
  return series.map((s) => ({
    type: 'line',
    name: seriesName(s.labels),
    // Pass points through verbatim — no smoothing, no interpolation,
    // no resampling, no rounding, no bucketing. KPI 3.
    data: s.points.map(([ts, value]) => [ts, value]),
    // Locked fidelity invariants per ADR-0030 §fidelity.
    smooth: false,
    connectNulls: false,
    sampling: 'none',
    // No point markers; chart focuses on the line.
    symbol: 'none',
  }));
}

function xAxisName(range: TimeRange): string {
  if (range.kind === 'relative') {
    return `${range.from} → now`;
  }
  return `${range.from.toISOString()} → ${range.to.toISOString()}`;
}

/**
 * Compose the EChartsOption from the validated outcome.
 *
 * Success and empty outcomes produce a chart-ready option; the
 * three error arms (parse-error / transport-error / config-error)
 * produce an option with `series: []` so the chart renders empty.
 * The QueryPanel composes the inline banner separately based on
 * the outcome kind; buildOption never renders error text into the
 * chart itself.
 */
export function buildOption(outcome: QueryOutcome, ctx: BuildOptionContext): EChartsOption {
  const series =
    outcome.kind === 'success' ? buildSeries(outcome.series) : ([] as ReadonlyArray<unknown>);

  return {
    animation: !ctx.prefersReducedMotion,
    color: paletteColours(ctx.palette),
    xAxis: {
      type: 'time',
      name: xAxisName(ctx.range),
      nameLocation: 'middle',
      nameGap: 28,
    },
    yAxis: {
      type: 'value',
    },
    tooltip: {
      trigger: 'axis',
      // KPI 3: tooltip values must read the underlying point's
      // value, not a smoothed-curve value.
      axisPointer: { type: 'line' },
    },
    legend: {
      type: 'scroll',
      bottom: 0,
    },
    grid: {
      left: 60,
      right: 24,
      top: 32,
      bottom: 64,
    },
    series,
  };
}
