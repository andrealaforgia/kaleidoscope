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

// ADR-0029 — Auto-refresh state-machine vocabulary.
//
// The state carries context (current refresh interval, current range
// kind) so the reducer can decide the next schedule_ms without the
// caller having to thread that context into every event. Picks of
// interval and range come in through refresh-changed and
// range-changed events; the reducer updates the carried context
// and emits the appropriate effects.

import type { TimeRange, RefreshInterval } from '../url-state/types';
import type { QueryOutcome } from '../promql/types';

/**
 * Context every state arm carries. The reducer reads these to pick
 * the right schedule_ms on running/recovery transitions and to
 * enforce the absolute-disables-auto invariant (ADR-0029 §6).
 */
export interface AutoRefreshContext {
  readonly interval: RefreshInterval;
  readonly rangeKind: 'relative' | 'absolute';
}

export type AutoRefreshState = AutoRefreshContext &
  (
    | { readonly kind: 'idle' }
    | { readonly kind: 'running' }
    | { readonly kind: 'backoff'; readonly retry: 0 | 1 | 2 }
    | { readonly kind: 'hidden' }
  );

export type AutoRefreshEvent =
  | { readonly kind: 'refresh-changed'; readonly interval: RefreshInterval }
  | { readonly kind: 'range-changed'; readonly range: TimeRange }
  | { readonly kind: 'tick-fired' }
  | { readonly kind: 'fetch-result'; readonly outcome: QueryOutcome }
  | { readonly kind: 'visibility-changed'; readonly hidden: boolean };

export type AutoRefreshEffect =
  | { readonly kind: 'schedule-timer'; readonly ms: number }
  | { readonly kind: 'cancel-timer' }
  | { readonly kind: 'fetch' }
  | { readonly kind: 'cancel-fetch' };
