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
// with a known fixture (5 points, NaN gap at index 2, boundary
// values at the extremes) and assert the resulting EChartsOption's
// series data matches byte-for-byte.
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
import type { QueryOutcome, Series } from '../src/lib/promql/types';
import fidelityAnchor from './fixtures/promql-fidelity-anchor.json' with { type: 'json' };

// =============================================================================
// The anchor fixture is shaped so each invariant has a deterministic
// kill: NaN at index 2, boundary value at index 0 (extreme low),
// boundary value at index 4 (extreme high), and the timestamps are
// non-uniform (different deltas between adjacent points) so a "uniform
// resample" mutation would fail.
// =============================================================================

describe('Invariant — KPI 3 data fidelity (buildOption is byte-equivalent to backend response)', () => {
  it('the fixture loads with the expected shape (sanity)', () => {
    expect(fidelityAnchor).toMatchObject({
      data: { result: expect.any(Array) },
    });
    // The anchor fixture has 1 series with 5 points; index 2 is the gap.
  });

  it('rendered series count equals the backend result count (no merging) (KPI 3)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN a QueryOutcome.Success with 1 series, 5 points
    // WHEN buildOption(outcome) runs
    // THEN result.series has length 1 (matches result.length)
    // KILLS: a mutation that filtered series by some predicate
  });

  it('rendered point count equals the backend point count (no smoothing) (KPI 3)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN the anchor fixture (5 raw points including a NaN at index 2)
    // WHEN buildOption(outcome) runs
    // THEN result.series[0].data has length 5 (NOT 4 — gap-removal would
    //      drop the NaN row)
    // KILLS: any mutation that drops gap rows
  });

  it('NaN at index 2 is preserved as NaN in the rendered data (no interpolation) (KPI 3)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN the anchor fixture with values [0.5, 1.0, NaN, 1.5, 2.0]
    // WHEN buildOption(outcome) runs
    // THEN result.series[0].data[2] is [<timestamp>, NaN] OR null OR a
    //      shape ECharts treats as a gap (NOT 1.25 — the linear
    //      interpolation of neighbours)
    // KILLS: a mutation that interpolates NaN as the average of neighbours
    // KILLS: a mutation that flips connectNulls from false to true
  });

  it('the rendered points match the backend timestamps byte-for-byte (no resampling) (KPI 3)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN the anchor fixture with non-uniform timestamps (deltas: 5s, 7s, 5s, 9s)
    //       i.e. timestamps not on a fixed grid
    // WHEN buildOption(outcome) runs
    // THEN every result.series[0].data[i][0] equals the backend's
    //      original timestamp at index i (millisecond precision)
    // KILLS: a mutation that resamples to a uniform grid (would force
    //        timestamps to deltas of 5s, 5s, 5s, 5s)
  });

  it('the rendered values match the backend values byte-for-byte (no rounding) (KPI 3)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN the anchor fixture with values [0.5, 1.0, NaN, 1.5, 2.0]
    // WHEN buildOption(outcome) runs
    // THEN result.series[0].data[i][1] equals the backend's value at i
    //      to full IEEE-754 precision (no toFixed, no Math.round, no
    //      bucketing, no integer cast)
    // KILLS: any mutation that rounds, truncates, or buckets values
  });

  it('the smooth option is false (no Bezier curve) (KPI 3, ADR-0030)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN any non-empty QueryOutcome.Success
    // WHEN buildOption(outcome) runs
    // THEN result.series[0].smooth is false (or omitted, which is the same)
    // KILLS: smooth: false → smooth: true mutation
  });

  it('the connectNulls option is false (no gap-bridging) (KPI 3, ADR-0030)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN any QueryOutcome.Success
    // WHEN buildOption(outcome) runs
    // THEN result.series[0].connectNulls is false (or omitted)
    // KILLS: connectNulls: false → connectNulls: true mutation
  });

  it('the dataset is not auto-downsampled (KPI 3, ADR-0030)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN a QueryOutcome.Success with 10000 points (large enough to
    //       trigger ECharts' default downsample if it were enabled)
    // WHEN buildOption(outcome) runs
    // THEN result.series[0].sampling is omitted OR explicitly 'none'
    // AND  result.series[0].large is false OR omitted
    // KILLS: a mutation that adds sampling: 'lttb' or large: true
  });
});

// =============================================================================
// Boundary cases — empty / single-point / single-series
// =============================================================================

describe('Invariant — KPI 3 data fidelity at boundaries', () => {
  it('empty outcome produces an empty series array (no synthetic placeholder)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN a QueryOutcome.Empty
    // WHEN buildOption(outcome) runs
    // THEN result.series is an empty array (length 0)
    // KILLS: a mutation that returns a placeholder series with zero points
  });

  it('single-point series renders with exactly one point (no extrapolation)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 / DELIVER buildOption');
    // GIVEN a QueryOutcome.Success with 1 series, 1 point
    // WHEN buildOption(outcome) runs
    // THEN result.series[0].data has length 1
    // KILLS: a mutation that synthesises neighbours via extrapolation
  });
});

// =============================================================================
// Mutation-evidence anchor: this test file is the canonical KPI 3 lock.
// Stryker's `--in-diff` mode runs against the apps/prism/src/lib/echarts/
// directory; every mutation generated there must be killed by one of the
// tests above. If StrykerJS reports a survivor, add a test here that
// kills it and document the addition in the slice's completion file.
// =============================================================================
