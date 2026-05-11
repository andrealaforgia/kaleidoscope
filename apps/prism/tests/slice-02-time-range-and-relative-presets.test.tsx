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

// Slice 02 — Time range and relative presets.
//
// The brief: I am Priya. I have just opened Prism and seen a chart at
// the default 15-min range. The spike I am triaging looks transient.
// I want to widen to 6h to see whether the same shape happened
// earlier today, and narrow to 5min to focus on the peak. I do not
// want to type ISO timestamps at 03:14.
//
// Stories: US-PR-02 (relative-range portion), US-PR-04 (relative URL roundtrip).
// KPIs anchored: KPI 4 (relative cross-tab — the Playwright counterpart in
// e2e/slice-02-time-range-and-relative-presets.spec.ts pins the byte-equal
// view assertion).
// ADRs: 0028 (URL state codec — relative ranges).

import { describe, expect, it, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { decode, encode } from '../src/lib/url-state/codec';
import type { UrlState, RelativeOffset } from '../src/lib/url-state/types';
import { QueryPanel } from '../src/panels/query/QueryPanel';

import promqlSuccessFixture from './fixtures/promql-success.json' with { type: 'json' };

const TEST_CONFIG = {
  backend: { url: '/api/v1', label: 'dev-local-prom' },
  prism: { version: '0.1.0' },
} as const;

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

// =============================================================================
// US-PR-02 AC-2.1 (relative) — picker offers the five canonical presets
// =============================================================================

describe('Slice 02 picker — when I open the time-range picker', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('offers exactly the five operator-canonical relative presets (AC-2.1)', () => {
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlSuccessFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const picker = screen.getByTestId('time-range-picker') as HTMLSelectElement;
    const labels = Array.from(picker.options).map((o) => o.text);
    expect(labels).toEqual([
      'Last 5 min',
      'Last 15 min',
      'Last 1 h',
      'Last 6 h',
      'Last 24 h',
      'Custom',
    ]);
    // Custom is rendered enabled at slice 05; this slice-02 stage-gate
    // formerly asserted disabled. The picker offers the six options;
    // Custom's behaviour is covered by slice-05 codec tests.
    const customOption = picker.options[picker.options.length - 1]!;
    expect(customOption.value).toBe('custom');
  });

  it('defaults to "Last 15 min" on a fresh page load (AC-2.1)', () => {
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlSuccessFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const picker = screen.getByTestId('time-range-picker') as HTMLSelectElement;
    expect(picker.value).toBe('-15m');
  });
});

// =============================================================================
// US-PR-02 AC-2.2 — picker change re-fetches and updates URL
// =============================================================================

describe('Slice 02 picker change — when I pick a different relative preset', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('re-fetches the chart with the new range (AC-2.2)', async () => {
    const calls: string[] = [];
    const fakeFetch: typeof fetch = async (input) => {
      calls.push(typeof input === 'string' ? input : (input as URL).toString());
      return jsonResponse(promqlSuccessFixture);
    };
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    // First run a query so subsequent range changes re-fetch.
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => expect(calls.length).toBeGreaterThanOrEqual(1));
    const callsBefore = calls.length;
    await user.selectOptions(screen.getByTestId('time-range-picker'), '-1h');
    await waitFor(() => expect(calls.length).toBe(callsBefore + 1));
    // The new request's start parameter is roughly now-3600s.
    const lastUrl = calls[calls.length - 1]!;
    const params = new URL(lastUrl, 'http://x').searchParams;
    const start = Number.parseFloat(params.get('start')!);
    const end = Number.parseFloat(params.get('end')!);
    expect(end - start).toBeCloseTo(3600, -1);
  });

  it('updates the URL synchronously to encode the picked range (AC-2.2)', async () => {
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlSuccessFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.selectOptions(screen.getByTestId('time-range-picker'), '-1h');
    await waitFor(() => {
      expect(window.location.search).toContain('from=-1h');
      expect(window.location.search).toContain('to=now');
    });
  });

  it('preserves the query when I change the range (journey integration checkpoint)', async () => {
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlSuccessFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.selectOptions(screen.getByTestId('time-range-picker'), '-6h');
    expect((screen.getByTestId('query-input') as HTMLInputElement).value).toBe('up');
    expect(window.location.search).toContain('q=up');
  });
});

// =============================================================================
// US-PR-02 AC-2.3 — relative URL encodes from=now-Xs and to=now
// =============================================================================

describe('Slice 02 URL codec — for every relative preset', () => {
  const presetEncodings: Array<{ offset: RelativeOffset; expected: string }> = [
    { offset: '-5m', expected: '-5m' },
    { offset: '-15m', expected: '-15m' },
    { offset: '-1h', expected: '-1h' },
    { offset: '-6h', expected: '-6h' },
    { offset: '-24h', expected: '-24h' },
  ];

  for (const { offset, expected } of presetEncodings) {
    it(`encodes "${offset}" as from=${expected}&to=now (AC-2.3)`, () => {
      const state: UrlState = {
        q: 'up',
        range: { kind: 'relative', from: offset },
        refresh: 'off',
      };
      const search = encode(state);
      const params = new URLSearchParams(search);
      expect(params.get('from')).toBe(expected);
      expect(params.get('to')).toBe('now');
    });

    it(`decodes from=${expected}&to=now back to range.from === "${offset}"`, () => {
      const result = decode(`from=${expected}&to=now`);
      expect(result.kind).toBe('ok');
      if (result.kind === 'ok') {
        expect(result.value.range.kind).toBe('relative');
        if (result.value.range.kind === 'relative') {
          expect(result.value.range.from).toBe(offset);
        }
      }
    });
  }
});

// =============================================================================
// US-PR-02 AC-2.5 — invalid relative entries fall back gracefully
// =============================================================================

describe('Slice 02 forgiving codec — when the URL has a malformed relative offset', () => {
  it('decodes a non-canonical relative offset (e.g. "-3m") as an error (AC-2.5)', () => {
    const result = decode('from=-3m');
    // ADR-0028 §3: strict-rejection. The decoder returns an error
    // arm; QueryPanel's readUrlState falls back to defaults and
    // renders the malformed-URL banner per ADR-0028 §6.
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.kind).toBe('from');
    }
  });

  it('decodes an absolute timestamp in "from" with relative-shaped "to" as invalid (AC-2.5)', () => {
    const result = decode('from=2026-05-07T03:00:00Z&to=now');
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      // The codec attempts absolute parsing first (since from doesn't
      // match a relative preset); to=now then fails the absolute
      // parse, yielding a "to" error.
      expect(['from', 'to']).toContain(result.error.kind);
    }
  });
});

// =============================================================================
// US-PR-02 AC-2.2 (cross-load) — URL hydrates the picker
// =============================================================================

describe('Slice 02 URL hydration — when I open Prism with a relative-range URL', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/?q=up&from=-1h&to=now');
  });

  it('shows the picker pre-set to the URL-encoded preset (AC-2.2)', () => {
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlSuccessFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const picker = screen.getByTestId('time-range-picker') as HTMLSelectElement;
    expect(picker.value).toBe('-1h');
  });
});
