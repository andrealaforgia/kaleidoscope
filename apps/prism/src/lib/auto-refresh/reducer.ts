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

// ADR-0029 §2 — Pure reducer for the auto-refresh state machine.
// Total function: (state, event) → { next, effects }. No I/O, no
// Date.now(), no setTimeout, no React. The QueryPanel wires this to
// the Scheduler seam (lib/auto-refresh/scheduler.ts) and the
// queryRange call; the reducer just emits effects.
//
// Transitions:
//
//   Idle ──(refresh != off, range relative)──▶ Running
//        ◀──(refresh = off)─── Running / Backoff
//
//   Running ──(tick-fired)──▶ Running [fetch]
//           ──(fetch-result success/empty/parse)──▶ Running [schedule]
//           ──(fetch-result transport)──▶ Backoff(0) [schedule 5s]
//           ──(visibility hidden)──▶ Hidden
//
//   Backoff(n) ──(tick-fired)──▶ Backoff(n) [fetch]
//              ──(fetch-result success/empty/parse)──▶ Running [schedule]
//              ──(fetch-result transport)──▶ Backoff(min(n+1, 2))
//                  [schedule 10s / 20s / 30s cap]
//              ──(visibility hidden)──▶ Hidden
//
//   Hidden ──(visibility visible)──▶ Running [fetch, schedule]
//          (other events update context but stay Hidden)
//
// All transitions out of Running or Backoff that LEAVE the
// timer-active region emit cancel-timer; all transitions in that
// abort an in-flight fetch also emit cancel-fetch. The "no timer
// leaks" property test pins this discipline.

import type { AutoRefreshEffect, AutoRefreshEvent, AutoRefreshState } from './events';
import type { RefreshInterval } from '../url-state/types';

export interface ReduceResult {
  readonly next: AutoRefreshState;
  readonly effects: ReadonlyArray<AutoRefreshEffect>;
}

const INTERVAL_MS: Readonly<Record<RefreshInterval, number>> = {
  off: 0,
  '5s': 5000,
  '10s': 10000,
  '30s': 30000,
  '1m': 60000,
};

// Backoff curve per ADR-0029 §4. The schedule_ms is determined by
// the OUTGOING retry: entering Backoff(0) schedules 5s, entering
// Backoff(1) schedules 10s, entering Backoff(2) for the first time
// schedules 20s, staying at Backoff(2) on subsequent failures
// schedules 30s (the cap).
const BACKOFF_MS_BY_RETRY: Readonly<Record<0 | 1 | 2, number>> = {
  0: 5000,
  1: 10000,
  2: 20000,
};
const BACKOFF_CAP_MS = 30000;

function pickedMs(interval: RefreshInterval): number {
  return INTERVAL_MS[interval];
}

export function reduce(state: AutoRefreshState, event: AutoRefreshEvent): ReduceResult {
  switch (event.kind) {
    case 'refresh-changed':
      return onRefreshChanged(state, event.interval);
    case 'range-changed': {
      const rangeKind: 'relative' | 'absolute' = event.range.kind;
      return onRangeChanged(state, rangeKind);
    }
    case 'tick-fired':
      return onTickFired(state);
    case 'fetch-result':
      return onFetchResult(state, event.outcome.kind, isAborted(event));
    case 'visibility-changed':
      return onVisibilityChanged(state, event.hidden);
  }
}

function isAborted(event: Extract<AutoRefreshEvent, { kind: 'fetch-result' }>): boolean {
  return event.outcome.kind === 'transport-error' && event.outcome.cause.kind === 'aborted';
}

function makeIdle(state: AutoRefreshState): AutoRefreshState {
  return { ...state, kind: 'idle' };
}

function makeRunning(state: AutoRefreshState): AutoRefreshState {
  return { ...state, kind: 'running' };
}

function makeBackoff(state: AutoRefreshState, retry: 0 | 1 | 2): AutoRefreshState {
  return { ...state, kind: 'backoff', retry };
}

function makeHidden(state: AutoRefreshState): AutoRefreshState {
  return { ...state, kind: 'hidden' };
}

function shouldRun(state: AutoRefreshState): boolean {
  return state.interval !== 'off' && state.rangeKind === 'relative';
}

// ---------------------------------------------------------------------------
// Event handlers
// ---------------------------------------------------------------------------

function onRefreshChanged(state: AutoRefreshState, interval: RefreshInterval): ReduceResult {
  const ctx = { ...state, interval } as AutoRefreshState;

  if (state.kind === 'idle') {
    if (interval !== 'off' && state.rangeKind === 'relative') {
      return {
        next: makeRunning(ctx),
        effects: [{ kind: 'schedule-timer', ms: pickedMs(interval) }],
      };
    }
    return { next: makeIdle(ctx), effects: [] };
  }

  if (state.kind === 'running') {
    if (interval === 'off') {
      return {
        next: makeIdle(ctx),
        effects: [{ kind: 'cancel-timer' }, { kind: 'cancel-fetch' }],
      };
    }
    // Reschedule with the new interval.
    return {
      next: makeRunning(ctx),
      effects: [{ kind: 'cancel-timer' }, { kind: 'schedule-timer', ms: pickedMs(interval) }],
    };
  }

  if (state.kind === 'backoff') {
    if (interval === 'off') {
      return {
        next: makeIdle(ctx),
        effects: [{ kind: 'cancel-timer' }, { kind: 'cancel-fetch' }],
      };
    }
    // Backoff schedule is failure-driven, not interval-driven; keep
    // the current retry rung. Just update the context interval.
    return { next: ctx, effects: [] };
  }

  // Hidden: update context only, stay hidden.
  return { next: ctx, effects: [] };
}

function onRangeChanged(state: AutoRefreshState, rangeKind: 'relative' | 'absolute'): ReduceResult {
  const ctx = { ...state, rangeKind } as AutoRefreshState;

  if (rangeKind === 'absolute') {
    // ADR-0029 §6 absolute-disables-auto double-lock.
    if (state.kind === 'running' || state.kind === 'backoff') {
      return {
        next: makeIdle(ctx),
        effects: [{ kind: 'cancel-timer' }, { kind: 'cancel-fetch' }],
      };
    }
    return { next: makeIdle(ctx), effects: [] };
  }

  // rangeKind === 'relative'
  if (state.kind === 'idle' && state.interval !== 'off') {
    return {
      next: makeRunning(ctx),
      effects: [{ kind: 'schedule-timer', ms: pickedMs(state.interval) }],
    };
  }

  return { next: ctx, effects: [] };
}

function onTickFired(state: AutoRefreshState): ReduceResult {
  if (state.kind === 'running' || state.kind === 'backoff') {
    // Always cancel any in-flight fetch before issuing a fresh one;
    // cancel-fetch is a no-op if no fetch is in flight.
    return {
      next: state,
      effects: [{ kind: 'cancel-fetch' }, { kind: 'fetch' }],
    };
  }
  // Idle or Hidden: defensive ignore.
  return { next: state, effects: [] };
}

function onFetchResult(
  state: AutoRefreshState,
  outcomeKind: 'success' | 'empty' | 'parse-error' | 'transport-error' | 'config-error',
  aborted: boolean,
): ReduceResult {
  // Aborted is always silent — it came from our own cancel-fetch.
  if (aborted) return { next: state, effects: [] };

  if (state.kind === 'running') {
    if (outcomeKind === 'transport-error' || outcomeKind === 'config-error') {
      return {
        next: makeBackoff(state, 0),
        effects: [{ kind: 'schedule-timer', ms: BACKOFF_MS_BY_RETRY[0] }],
      };
    }
    // success / empty / parse-error stays Running and reschedules.
    return {
      next: makeRunning(state),
      effects: [{ kind: 'schedule-timer', ms: pickedMs(state.interval) }],
    };
  }

  if (state.kind === 'backoff') {
    if (outcomeKind === 'transport-error' || outcomeKind === 'config-error') {
      // Move forward along the curve, or stay at the cap with 30s.
      if (state.retry === 0) {
        return {
          next: makeBackoff(state, 1),
          effects: [{ kind: 'schedule-timer', ms: BACKOFF_MS_BY_RETRY[1] }],
        };
      }
      if (state.retry === 1) {
        return {
          next: makeBackoff(state, 2),
          effects: [{ kind: 'schedule-timer', ms: BACKOFF_MS_BY_RETRY[2] }],
        };
      }
      // retry === 2 — capped.
      return {
        next: makeBackoff(state, 2),
        effects: [{ kind: 'schedule-timer', ms: BACKOFF_CAP_MS }],
      };
    }
    // success / empty / parse-error recovers to Running.
    return {
      next: makeRunning(state),
      effects: [{ kind: 'schedule-timer', ms: pickedMs(state.interval) }],
    };
  }

  // Idle or Hidden: defensive — ignore stray fetch results.
  return { next: state, effects: [] };
}

function onVisibilityChanged(state: AutoRefreshState, hidden: boolean): ReduceResult {
  if (hidden) {
    if (state.kind === 'running' || state.kind === 'backoff') {
      return {
        next: makeHidden(state),
        effects: [{ kind: 'cancel-timer' }, { kind: 'cancel-fetch' }],
      };
    }
    if (state.kind === 'hidden') return { next: state, effects: [] };
    // Idle: still record the hidden status so that becoming visible
    // again does not surprise us. We stay Idle (no work to do).
    return { next: makeHidden(state), effects: [] };
  }

  // hidden === false: become visible.
  if (state.kind !== 'hidden') return { next: state, effects: [] };

  if (shouldRun(state)) {
    return {
      next: makeRunning(state),
      effects: [{ kind: 'fetch' }, { kind: 'schedule-timer', ms: pickedMs(state.interval) }],
    };
  }
  return { next: makeIdle(state), effects: [] };
}

