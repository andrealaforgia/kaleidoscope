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

// Invariant — KPI 3 data fidelity.
//
// I am Priya. When I see a chart, I trust it. The wobble in the line
// is the system's wobble, not the SPA's smoothing artefact. The gap
// at 03:14 is a real gap in the backend's data, not an interpolation
// across missing points. The peak's height matches what curl would
// have shown.
//
// This is the structural enforcement of KPI 3 (per outcome-kpis.md
// and ADR-0030 §fidelity invariants). The pure function `buildOption`
// from the EChartsOption builder is the testable seam; we drive it
// with a known fixture (5 points, two NaN gaps, boundary values at
// the extremes) and assert the resulting EChartsOption's series
// data matches byte-for-byte.
//
// Mutation testing on `buildOption` (StrykerJS Gate 10) generates
// mutants like "smooth: false → smooth: true", "connectNulls: false →
// connectNulls: true", "data filter NaN → data identity". Any such
// mutation must FAIL this test. If a mutation survives, the test is
// weaker than it should be — a test integrity defect, not a code
// defect.
//
// ADRs: 0030 (buildOption purity + fidelity invariants),
//       0027 (queryRange returns the backend's points verbatim),
//       0029 (auto-refresh tick re-runs buildOption on fresh data).

import { describe, it, expect } from 'vitest';
import { buildOption } from '../src/lib/echarts/buildOption';
import type { BuildOptionContext } from '../src/lib/echarts/buildOption';
import type { QueryOutcome, Series } from '../src/lib/promql/types';
import type { TimeRange } from '../src/lib/url-state/types';
import fidelityAnchor from './fixtures/promql-fidelity-anchor.json' with { type: 'json' };

// The anchor fixture has 1 series with 5 points; values are
// [1, NaN, 3, NaN, 5] at uniform 15-second timestamps. NaN gaps at
// indices 1 and 3 catch any client-side interpolation; boundary
// values 1 and 5 catch any rounding/truncation/bucketing mutation;
// byte-equality between rendered and source timestamps catches any
// resampling mutation.

/** Convert Prometheus matrix-result JSON shape into Series[]. */
function fixtureToSeries(): Series[] {
  return fidelityAnchor.data.result.map((entry) => ({
    labels: entry.metric as Record<string, string>,
    points: entry.values.map(([tsSec, valueStr]) => {
      const tsMs = (tsSec as number) * 1000;
      const value = valueStr === 'NaN' ? Number.NaN : Number.parseFloat(valueStr as string);
      return [tsMs, value] as readonly [number, number];
    }),
  }));
}

const relativeRange: TimeRange = { kind: 'relative', from: '-15m' };
const baseCtx: BuildOptionContext = {
  palette: 'okabe-ito',
  range: relativeRange,
  prefersReducedMotion: false,
};

/** Coerce the unknown `series` field on EChartsOption into a typed array for assertions. */
function getSeries(option: Record<string, unknown>): Array<Record<string, unknown>> {
  return option['series'] as Array<Record<string, unknown>>;
}

describe('Invariant — KPI 3 data fidelity (buildOption is byte-equivalent to backend response)', () => {
  it('the fixture loads with the expected shape (sanity)', () => {
    expect(fidelityAnchor).toMatchObject({
      data: { result: expect.any(Array) },
    });
  });

  it('rendered series count equals the backend result count (no merging) (KPI 3)', () => {
    const outcome: QueryOutcome = {
      kind: 'success',
      series: fixtureToSeries(),
      queryMs: 1,
    };
    const option = buildOption(outcome, baseCtx);
    expect(getSeries(option).length).toBe(1);
  });

  it('rendered point count equals the backend point count (no smoothing) (KPI 3)', () => {
    const outcome: QueryOutcome = {
      kind: 'success',
      series: fixtureToSeries(),
      queryMs: 1,
    };
    const option = buildOption(outcome, baseCtx);
    const data = getSeries(option)[0]?.['data'] as Array<readonly [number, number]>;
    expect(data.length).toBe(5);
  });

  it('NaN gaps at indices 1 and 3 are preserved as NaN in the rendered data (no interpolation) (KPI 3)', () => {
    const outcome: QueryOutcome = {
      kind: 'success',
      series: fixtureToSeries(),
      queryMs: 1,
    };
    const option = buildOption(outcome, baseCtx);
    const data = getSeries(option)[0]?.['data'] as Array<readonly [number, number]>;
    expect(Number.isNaN(data[1]?.[1])).toBe(true);
    expect(Number.isNaN(data[3]?.[1])).toBe(true);
    // Non-NaN neighbours must NOT have been interpolated into the gap.
    expect(data[0]?.[1]).toBe(1);
    expect(data[2]?.[1]).toBe(3);
    expect(data[4]?.[1]).toBe(5);
  });

  it('the rendered points match the backend timestamps byte-for-byte (no resampling) (KPI 3)', () => {
    const series = fixtureToSeries();
    const outcome: QueryOutcome = { kind: 'success', series, queryMs: 1 };
    const option = buildOption(outcome, baseCtx);
    const data = getSeries(option)[0]?.['data'] as Array<readonly [number, number]>;
    // Each rendered timestamp equals the source timestamp; any
    // uniform-resample mutation would change the deltas and fail
    // this byte-equality check.
    for (let i = 0; i < series[0]!.points.length; i++) {
      expect(data[i]?.[0]).toBe(series[0]!.points[i]![0]);
    }
  });

  it('the rendered values match the backend values byte-for-byte (no rounding) (KPI 3)', () => {
    const series = fixtureToSeries();
    const outcome: QueryOutcome = { kind: 'success', series, queryMs: 1 };
    const option = buildOption(outcome, baseCtx);
    const data = getSeries(option)[0]?.['data'] as Array<readonly [number, number]>;
    // Non-NaN values match byte-for-byte; NaN entries verified separately above.
    expect(data[0]?.[1]).toBe(series[0]!.points[0]![1]);
    expect(data[2]?.[1]).toBe(series[0]!.points[2]![1]);
    expect(data[4]?.[1]).toBe(series[0]!.points[4]![1]);
  });

  it('the smooth option is false (no Bezier curve) (KPI 3, ADR-0030)', () => {
    const outcome: QueryOutcome = {
      kind: 'success',
      series: fixtureToSeries(),
      queryMs: 1,
    };
    const option = buildOption(outcome, baseCtx);
    expect(getSeries(option)[0]?.['smooth']).toBe(false);
  });

  it('the connectNulls option is false (no gap-bridging) (KPI 3, ADR-0030)', () => {
    const outcome: QueryOutcome = {
      kind: 'success',
      series: fixtureToSeries(),
      queryMs: 1,
    };
    const option = buildOption(outcome, baseCtx);
    expect(getSeries(option)[0]?.['connectNulls']).toBe(false);
  });

  it('the dataset is not auto-downsampled (KPI 3, ADR-0030)', () => {
    // Synthesise a 10000-point series large enough to trigger
    // ECharts' default downsample heuristics if they were enabled.
    const largePoints: Array<readonly [number, number]> = [];
    for (let i = 0; i < 10_000; i++) {
      largePoints.push([1_746_576_900_000 + i * 15_000, i * 0.1]);
    }
    const largeSeries: Series = {
      labels: { __name__: 'synthetic_large' },
      points: largePoints,
    };
    const outcome: QueryOutcome = {
      kind: 'success',
      series: [largeSeries],
      queryMs: 1,
    };
    const option = buildOption(outcome, baseCtx);
    const s0 = getSeries(option)[0]!;
    expect(s0['sampling']).toBe('none');
    // `large` is explicitly NOT enabled; ECharts treats omission and
    // explicit-false equivalently.
    expect(s0['large'] === undefined || s0['large'] === false).toBe(true);
  });
});

describe('Invariant — KPI 3 data fidelity at boundaries', () => {
  it('empty outcome produces an empty series array (no synthetic placeholder)', () => {
    const outcome: QueryOutcome = { kind: 'empty', queryMs: 1 };
    const option = buildOption(outcome, baseCtx);
    expect(getSeries(option)).toEqual([]);
  });

  it('single-point series renders with exactly one point (no extrapolation)', () => {
    const series: Series[] = [
      {
        labels: { __name__: 'one_point' },
        points: [[1_746_576_900_000, 42]],
      },
    ];
    const outcome: QueryOutcome = { kind: 'success', series, queryMs: 1 };
    const option = buildOption(outcome, baseCtx);
    const data = getSeries(option)[0]?.['data'] as Array<readonly [number, number]>;
    expect(data.length).toBe(1);
    expect(data[0]).toEqual([1_746_576_900_000, 42]);
  });

  it('parse-error outcome produces empty series (chart shows nothing, QueryPanel renders banner)', () => {
    const outcome: QueryOutcome = {
      kind: 'parse-error',
      backendError: 'unexpected token',
      queryMs: 1,
    };
    const option = buildOption(outcome, baseCtx);
    expect(getSeries(option)).toEqual([]);
  });

  it('transport-error outcome produces empty series (no stale data shown)', () => {
    const outcome: QueryOutcome = {
      kind: 'transport-error',
      cause: { kind: 'network', message: 'Failed to fetch' },
      queryMs: 0,
    };
    const option = buildOption(outcome, baseCtx);
    expect(getSeries(option)).toEqual([]);
  });
});

describe('Invariant — KPI 3 buildOption respects the reduced-motion context', () => {
  it('animation is disabled when prefersReducedMotion is true', () => {
    const series = fixtureToSeries();
    const outcome: QueryOutcome = { kind: 'success', series, queryMs: 1 };
    const option = buildOption(outcome, { ...baseCtx, prefersReducedMotion: true });
    expect(option['animation']).toBe(false);
  });

  it('animation is enabled when prefersReducedMotion is false', () => {
    const series = fixtureToSeries();
    const outcome: QueryOutcome = { kind: 'success', series, queryMs: 1 };
    const option = buildOption(outcome, { ...baseCtx, prefersReducedMotion: false });
    expect(option['animation']).toBe(true);
  });
});

describe('Invariant — KPI 3 buildOption swaps palettes via context', () => {
  it('uses Okabe-Ito colours when palette = "okabe-ito"', () => {
    const series = fixtureToSeries();
    const outcome: QueryOutcome = { kind: 'success', series, queryMs: 1 };
    const option = buildOption(outcome, { ...baseCtx, palette: 'okabe-ito' });
    const colours = option['color'] as readonly string[];
    // Okabe-Ito begins with #000000 (black) per the canonical
    // 8-colour set.
    expect(colours[0]).toBe('#000000');
  });

  it('uses Tableau 10 colours when palette = "tableau10"', () => {
    const series = fixtureToSeries();
    const outcome: QueryOutcome = { kind: 'success', series, queryMs: 1 };
    const option = buildOption(outcome, { ...baseCtx, palette: 'tableau10' });
    const colours = option['color'] as readonly string[];
    // Tableau 10 begins with #4E79A7 (blue) per the canonical set.
    expect(colours[0]).toBe('#4E79A7');
  });
});
