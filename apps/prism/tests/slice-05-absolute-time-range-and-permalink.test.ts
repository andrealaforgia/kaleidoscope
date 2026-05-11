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

// Slice 05 — Absolute time range and full permalink.
//
// The brief: I am the postmortem-time engineer. Five days after the
// incident, I open the URL Priya pasted in Slack at 03:14. I expect
// the same chart she saw, against the same backend, with the same
// ISO-8601 from-and-to timestamps. The auto-refresh picker is
// disabled — the data does not move; refresh would be a no-op.
//
// Stories: US-PR-02 (absolute-range portion), US-PR-04 (absolute URL
//          roundtrip — postmortem-time reproduction).
// KPIs anchored: KPI 4 — the Playwright counterpart in
//                e2e/slice-05-absolute-time-range-and-permalink.spec.ts
//                pins the cross-day byte-equal view assertion.
// ADRs: 0028 (URL codec absolute mode), 0029 (auto-refresh disabled
//       on absolute-range double-lock), 0027 (queryRange honours
//       the absolute timestamps verbatim).

import { describe, expect, it, vi } from 'vitest';

import { decode, encode } from '../src/lib/url-state/codec';
import type { UrlState, AbsoluteTimeRange } from '../src/lib/url-state/types';

// Helpers — canonical absolute ranges drawn from the alert window.
const incidentWindow: AbsoluteTimeRange = {
  kind: 'absolute',
  from: new Date('2026-05-07T03:00:00Z'),
  to: new Date('2026-05-07T03:15:00Z'),
};

// =============================================================================
// US-PR-02 AC-2.1 (Custom mode) — encoder emits ISO-8601 from + to
// =============================================================================

describe('Slice 05 codec — when I encode an absolute time range', () => {
  it('emits ISO-8601 from + to as URL parameters (AC-2.1 Custom)', () => {
    const state: UrlState = { q: 'up', range: incidentWindow, refresh: 'off' };
    const url = encode(state);
    expect(url).toContain('from=2026-05-07T03%3A00%3A00.000Z');
    expect(url).toContain('to=2026-05-07T03%3A15%3A00.000Z');
    expect(url).not.toContain('refresh=');
  });

  it('serialises ISO-8601 with millisecond precision so cross-day equality holds (AC-2.1 Custom)', () => {
    const state: UrlState = { q: 'up', range: incidentWindow, refresh: 'off' };
    const url = encode(state);
    const decoded = decode(url);
    expect(decoded.kind).toBe('ok');
    if (decoded.kind === 'ok' && decoded.value.range.kind === 'absolute') {
      expect(decoded.value.range.from.getTime()).toBe(incidentWindow.from.getTime());
      expect(decoded.value.range.to.getTime()).toBe(incidentWindow.to.getTime());
    } else {
      throw new Error(`expected ok absolute, got ${JSON.stringify(decoded)}`);
    }
  });
});

// =============================================================================
// US-PR-02 AC-2.4 — absolute disables auto-refresh at the codec layer
// =============================================================================

describe('Slice 05 codec — when the range is absolute', () => {
  it('refuses to encode a refresh parameter even if the input state carries one (AC-2.4 double-lock)', () => {
    // Malformed input — picker UI would never produce this — but the
    // codec is the second lock per ADR-0028 §4.
    const state: UrlState = { q: 'up', range: incidentWindow, refresh: '10s' };
    const url = encode(state);
    expect(url).not.toContain('refresh=');
    const decoded = decode(url);
    expect(decoded.kind).toBe('ok');
    if (decoded.kind === 'ok') {
      expect(decoded.value.refresh).toBe('off');
    }
  });

  it('emits refresh=10s on relative ranges when refresh is set (AC-2.4 contrast)', () => {
    const state: UrlState = {
      q: 'up',
      range: { kind: 'relative', from: '-15m' },
      refresh: '10s',
    };
    const url = encode(state);
    expect(url).toContain('refresh=10s');
  });
});

// =============================================================================
// US-PR-02 AC-2.5 — invalid ISO entries are rejected at codec boundary
// =============================================================================

describe('Slice 05 codec — when the URL contains a malformed absolute range', () => {
  it('rejects "from=banana" with UrlParseError(from) (AC-2.5)', () => {
    const result = decode('from=banana&to=2026-05-07T03:15:00Z');
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.fields).toContain('from');
    }
  });

  it('rejects "to=2026-13-99T03:00:00Z" (invalid month/day, AC-2.5)', () => {
    const result = decode('from=2026-05-07T03:00:00Z&to=2026-13-99T03:00:00Z');
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.fields).toContain('to');
    }
  });

  it('rejects from > to (logical AC-2.5 — operator typo at picker)', () => {
    const result = decode('from=2026-05-07T04:00:00Z&to=2026-05-07T03:00:00Z');
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.kind).toBe('range-inverted');
    }
  });
});

// =============================================================================
// US-PR-04 AC-4.3 — postmortem-time reproduction is byte-equal
// =============================================================================

describe('Slice 05 roundtrip — when the same URL is re-opened cross-day', () => {
  it('decode(encode(state)) === state for absolute ranges (AC-4.3)', () => {
    const state: UrlState = {
      q: 'rate(http_server_duration_seconds_count[5m])',
      range: incidentWindow,
      refresh: 'off',
    };
    const url = encode(state);
    const decoded = decode(url);
    expect(decoded.kind).toBe('ok');
    if (decoded.kind === 'ok' && decoded.value.range.kind === 'absolute') {
      expect(decoded.value.q).toBe(state.q);
      expect(decoded.value.range.kind).toBe('absolute');
      expect(decoded.value.range.from.getTime()).toBe(incidentWindow.from.getTime());
      expect(decoded.value.range.to.getTime()).toBe(incidentWindow.to.getTime());
      expect(decoded.value.refresh).toBe('off');
    }
  });

  it('cross-day reproduction does not depend on Date.now() (AC-4.3)', () => {
    const state: UrlState = { q: 'up', range: incidentWindow, refresh: 'off' };
    const urlDayD = encode(state);

    // Five days later: fake the wall clock and re-decode the same URL.
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date('2026-05-12T18:00:00Z'));
      const decodedDayD5 = decode(urlDayD);
      expect(decodedDayD5.kind).toBe('ok');
      if (decodedDayD5.kind === 'ok' && decodedDayD5.value.range.kind === 'absolute') {
        expect(decodedDayD5.value.range.from.getTime()).toBe(incidentWindow.from.getTime());
        expect(decodedDayD5.value.range.to.getTime()).toBe(incidentWindow.to.getTime());
      }
    } finally {
      vi.useRealTimers();
    }
  });

  it('encoded query parameter survives a percent-encoding round-trip (AC-4.3)', () => {
    const q = 'sum by (status_code) (rate(http_server_duration_seconds_count[5m]))';
    const state: UrlState = { q, range: incidentWindow, refresh: 'off' };
    const url = encode(state);
    const decoded = decode(url);
    expect(decoded.kind).toBe('ok');
    if (decoded.kind === 'ok') {
      expect(decoded.value.q).toBe(q);
    }
  });
});

// =============================================================================
// US-PR-04 AC-4.1 — every state-affecting change updates the URL synchronously
// =============================================================================

describe('Slice 05 codec — when a state change happens', () => {
  it('encoded URL is deterministic in parameter ordering (AC-4.1, mutation-evidence anchor)', () => {
    const state: UrlState = { q: 'up', range: incidentWindow, refresh: 'off' };
    expect(encode(state)).toBe(encode(state));
    // Canonical order: q, from, to. Refresh omitted because absolute.
    const url = encode(state);
    const qIdx = url.indexOf('q=');
    const fromIdx = url.indexOf('from=');
    const toIdx = url.indexOf('to=');
    expect(qIdx).toBeGreaterThanOrEqual(0);
    expect(fromIdx).toBeGreaterThan(qIdx);
    expect(toIdx).toBeGreaterThan(fromIdx);
  });
});
