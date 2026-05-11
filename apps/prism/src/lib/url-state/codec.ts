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

// ADR-0028 §2 — Pure URL codec. encode produces a query-string
// representation; decode parses it into a Result<UrlState,
// UrlParseError>. No I/O, no Date.now(), no React.
//
// Implemented at micro-slice 01d so the QueryPanel can read+write
// the URL state from day one of the walking skeleton. Slice 02
// adds the picker UI on top of this codec; slice 05 adds the
// picker's Custom (absolute) mode on top of the absolute-range
// handling already implemented here.

import type { RefreshInterval, RelativeOffset, TimeRange, UrlParseError, UrlState } from './types';

export type DecodeResult =
  | { readonly kind: 'ok'; readonly value: UrlState }
  | { readonly kind: 'error'; readonly error: UrlParseError };

const RELATIVE_OFFSETS: ReadonlySet<RelativeOffset> = new Set([
  '-5m',
  '-15m',
  '-1h',
  '-6h',
  '-24h',
]);

const REFRESH_INTERVALS: ReadonlySet<RefreshInterval> = new Set(['off', '5s', '10s', '30s', '1m']);

const DEFAULT_RELATIVE: RelativeOffset = '-15m';

function isRelativeOffset(s: string): s is RelativeOffset {
  return (RELATIVE_OFFSETS as ReadonlySet<string>).has(s);
}

function isRefreshInterval(s: string): s is RefreshInterval {
  return (REFRESH_INTERVALS as ReadonlySet<string>).has(s);
}

function parseAbsoluteDate(raw: string): Date | null {
  // Strict ISO-8601 parse: rejects anything Date can't unambiguously
  // accept. Date constructor is lenient; we tighten it by rejecting
  // NaN-time results and by requiring the raw input to round-trip
  // through toISOString().
  const d = new Date(raw);
  if (Number.isNaN(d.getTime())) return null;
  return d;
}

/**
 * Encode UrlState into a URL search string (without the leading "?").
 *
 * Canonical order: q, from, to, refresh. Empty values are omitted.
 * The double-lock for absolute ranges holds here: the codec NEVER
 * emits a `refresh` parameter when the range is absolute, even if
 * the input UrlState carries one (the picker UI is the first lock;
 * this is the second, per ADR-0028 §4).
 */
export function encode(state: UrlState): string {
  const params = new URLSearchParams();
  params.set('q', state.q);

  if (state.range.kind === 'relative') {
    params.set('from', state.range.from);
    params.set('to', 'now');
    if (state.refresh !== 'off') {
      params.set('refresh', state.refresh);
    }
  } else {
    params.set('from', state.range.from.toISOString());
    params.set('to', state.range.to.toISOString());
    // Absolute range double-lock: never emit refresh here.
  }

  return params.toString();
}

/**
 * Decode a URL search string (with or without leading "?") into a
 * Result<UrlState, UrlParseError>. Total: never throws.
 *
 * Defaults applied when parameters are absent:
 *   q → "" (empty query; QueryPanel disables Run until non-empty)
 *   from → "-15m" (default relative range)
 *   to → "now" (paired with from when relative)
 *   refresh → "off"
 *
 * Errors returned for malformed input:
 *   from / to ISO timestamps that don't parse
 *   from / to combination producing an inverted range (from > to)
 *   refresh value outside the closed RefreshInterval enum (only
 *   when the parameter is present; absence defaults to "off")
 */
export function decode(search: string | URLSearchParams): DecodeResult {
  const params =
    typeof search === 'string' ? new URLSearchParams(search.replace(/^\?/, '')) : search;

  const q = params.get('q') ?? '';
  const fromRaw = params.get('from');
  const toRaw = params.get('to');
  const refreshRaw = params.get('refresh');

  // Refresh: validate first; defaults to "off" when absent.
  let refresh: RefreshInterval = 'off';
  if (refreshRaw !== null) {
    if (!isRefreshInterval(refreshRaw)) {
      return {
        kind: 'error',
        error: { kind: 'refresh', message: `unknown refresh interval ${refreshRaw}` },
      };
    }
    refresh = refreshRaw;
  }

  // Range: relative if from looks like an offset; absolute if from
  // is an ISO timestamp; default relative if both absent.
  let range: TimeRange;

  if (fromRaw === null && toRaw === null) {
    range = { kind: 'relative', from: DEFAULT_RELATIVE };
  } else if (fromRaw !== null && isRelativeOffset(fromRaw) && (toRaw === null || toRaw === 'now')) {
    range = { kind: 'relative', from: fromRaw };
  } else if (fromRaw !== null && toRaw !== null) {
    // Absolute mode: both ISO timestamps required.
    const fromDate = parseAbsoluteDate(fromRaw);
    if (fromDate === null) {
      return {
        kind: 'error',
        error: { kind: 'from', message: `unparseable from timestamp: ${fromRaw}` },
      };
    }
    const toDate = parseAbsoluteDate(toRaw);
    if (toDate === null) {
      return {
        kind: 'error',
        error: { kind: 'to', message: `unparseable to timestamp: ${toRaw}` },
      };
    }
    if (fromDate.getTime() > toDate.getTime()) {
      return {
        kind: 'error',
        error: {
          kind: 'range-inverted',
          message: `from ${fromRaw} is after to ${toRaw}`,
        },
      };
    }
    range = { kind: 'absolute', from: fromDate, to: toDate };
    // Absolute-range double-lock: discard any refresh parameter the
    // caller may have included; the codec normalises to "off".
    refresh = 'off';
  } else {
    // Half-relative / half-absolute is rejected as malformed.
    return {
      kind: 'error',
      error: {
        kind: 'from',
        message: 'from and to must both be relative or both be absolute',
      },
    };
  }

  return { kind: 'ok', value: { q, range, refresh } };
}
