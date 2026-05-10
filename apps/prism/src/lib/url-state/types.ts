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

// ADR-0028 — URL state schema. The five v0 parameters are
// q | from | to | refresh, plus the codec error type.

/** Operator-canonical relative-range presets. ADR-0028 §2. */
export type RelativeOffset = '-5m' | '-15m' | '-1h' | '-6h' | '-24h';

export interface RelativeTimeRange {
  readonly kind: 'relative';
  readonly from: RelativeOffset;
}

export interface AbsoluteTimeRange {
  readonly kind: 'absolute';
  readonly from: Date;
  readonly to: Date;
}

export type TimeRange = RelativeTimeRange | AbsoluteTimeRange;

/** Closed enum of refresh intervals. ADR-0028 §2 / ADR-0029 §1. */
export type RefreshInterval = 'off' | '5s' | '10s' | '30s' | '1m';

export interface UrlState {
  readonly q: string;
  readonly range: TimeRange;
  readonly refresh: RefreshInterval;
}

/** Discriminated codec error per ADR-0028 §6. */
export interface UrlParseError {
  readonly kind: 'q' | 'from' | 'to' | 'refresh' | 'range-inverted';
  readonly message: string;
}
