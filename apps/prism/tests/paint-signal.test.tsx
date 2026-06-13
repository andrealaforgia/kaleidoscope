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

// ADR-0075 D1 — paint-signal coverage (Gate 6 + the Gate 10 mutation
// anchor for the non-empty-series decision).
//
// The `false → true` browser transition itself is dead under jsdom by
// construction (no canvas-2D context ⇒ no echarts.init ⇒ no `finished`
// subscription); the Playwright slice-01/slice-03 specs are its real
// coverage. What jsdom CAN prove, and what StrykerJS needs to kill the
// logic-rich mutants without a browser, is:
//
//   1. the pure non-empty-series decision `seriesHasInk` — the
//      Array.isArray / .some / length > 0 predicate that gates the
//      flip (the empty option must NOT paint, the non-empty one must);
//   2. the literal "false" the component renders on mount, before any
//      real paint — the never-absent, never-true initial state.

import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';

import { seriesHasInk } from '../src/lib/echarts/paintSignal';
import { EChart } from '../src/lib/echarts/EChart';

// =============================================================================
// seriesHasInk — the pure non-empty-series decision (ADR-0075 D1)
// =============================================================================

describe('seriesHasInk — only a genuinely non-empty rendered series counts as painted', () => {
  const noInk: ReadonlyArray<[string, unknown]> = [
    ['undefined (no option yet)', undefined],
    ['null', null],
    ['a non-array (string)', 'up'],
    ['a non-array (number)', 42],
    ['a non-array (plain object)', { data: [[1, 2]] }],
    ['an empty series array', []],
    ['one series with an empty data array', [{ data: [] }]],
    ['one series with no data field', [{}]],
    ['one series whose data is not an array', [{ data: 'not-an-array' }]],
    ['a null series entry', [null]],
    ['an undefined series entry', [undefined]],
    ['every series empty', [{ data: [] }, { data: [] }]],
  ];

  it.each(noInk)('returns false for %s', (_label, input) => {
    expect(seriesHasInk(input)).toBe(false);
  });

  const inked: ReadonlyArray<[string, unknown]> = [
    ['one series with one point', [{ data: [[1, 2]] }]],
    ['one series with several points', [{ data: [[1, 2], [3, 4], [5, 6]] }]],
    // The empty-then-inked ordering kills the `.some` → `.every` mutant:
    // .every would be false because the first series is empty.
    ['an empty series followed by an inked one', [{ data: [] }, { data: [[1, 2]] }]],
    ['an inked series followed by an empty one', [{ data: [[1, 2]] }, { data: [] }]],
  ];

  it.each(inked)('returns true for %s', (_label, input) => {
    expect(seriesHasInk(input)).toBe(true);
  });
});

// =============================================================================
// EChart paint-signal initial state (ADR-0075 D1) — jsdom-observable
// =============================================================================

describe('EChart paint signal — the initial DOM state before any real paint', () => {
  it('renders data-prism-chart-painted="false" on mount and never reaches "true" under jsdom', () => {
    // GIVEN a mounted EChart with a non-empty option.
    // (Under jsdom the canvas probe is null, so echarts.init is skipped
    //  and the `finished` subscription is never made — the signal can
    //  only stay "false". The real flip is a Playwright concern.)
    const { container } = render(<EChart option={{ series: [{ type: 'line', data: [[1, 2]] }] }} />);
    const figure = container.querySelector('[role="figure"]');
    // THEN the signal is present and literally "false" — never absent,
    //   never "true" on mount.
    expect(figure).not.toBeNull();
    expect(figure?.getAttribute('data-prism-chart-painted')).toBe('false');
  });
});
