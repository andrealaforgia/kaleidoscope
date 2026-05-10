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

import type { TimeRange } from '../url-state/types';
import type { ConfigError } from '../config/types';

// ADR-0027 §2 — total-function queryRange returns a QueryOutcome
// 5-arm discriminated union. Every failure mode is encoded as a
// distinct arm; queryRange never throws.

export type LabelSet = Readonly<Record<string, string>>;

export interface Series {
  readonly labels: LabelSet;
  /** Each point is [timestamp_ms, value]; value may be NaN for gaps. */
  readonly points: ReadonlyArray<readonly [number, number]>;
}

export type TransportCause =
  | { readonly kind: 'network'; readonly message: string }
  | { readonly kind: 'http-status'; readonly status: number; readonly message: string }
  | { readonly kind: 'invalid-json'; readonly message: string }
  | { readonly kind: 'shape'; readonly message: string }
  | { readonly kind: 'aborted' };

export type QueryOutcome =
  | { readonly kind: 'success'; readonly series: ReadonlyArray<Series>; readonly queryMs: number }
  | { readonly kind: 'empty'; readonly queryMs: number }
  | { readonly kind: 'parse-error'; readonly backendError: string; readonly queryMs: number }
  | {
      readonly kind: 'transport-error';
      readonly cause: TransportCause;
      readonly queryMs: number;
    }
  | { readonly kind: 'config-error'; readonly error: ConfigError; readonly queryMs: number };

// ADR-0027 §1, §7 — request and context shapes. The fetchFn seam
// keeps queryRange testable without globalThis.fetch monkey-patching.

export interface QueryRangeRequest {
  readonly q: string;
  readonly range: TimeRange;
}

export interface QueryRangeContext {
  /** Backend base URL, e.g. `/api/v1` (same-origin) or `https://prom/api/v1`. */
  readonly backend: string;
  /** Test seam for fetch. Defaults to globalThis.fetch in production. */
  readonly fetchFn: typeof fetch;
  /** Optional abort signal honoured by the fetch call. */
  readonly signal?: AbortSignal;
}
