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

// Slice 04 — Auto-refresh.
//
// I am Priya. I am watching a sustained incident. I want the chart
// to refresh itself every 10 seconds while I keep my eyes on the
// line. I do not want to press F5. I do not want the chart to flicker
// every tick. If I switch tabs, the refresh should pause; when I come
// back, I should see fresh data immediately. If the backend dies, the
// next ticks should back off (5/10/20/30s capped) until it recovers.
//
// Stories: US-PR-05.
// KPIs anchored: KPI 3 (fidelity across ticks — also covered by invariant-fidelity).
// ADRs: 0029 (auto-refresh state machine — pure reducer + Scheduler seam),
//       0027 (every tick is a fresh queryRange call).

import { describe, expect, it } from 'vitest';

import { reduce } from '../src/lib/auto-refresh/reducer';
import type {
  AutoRefreshState,
  AutoRefreshEvent,
  AutoRefreshEffect,
} from '../src/lib/auto-refresh/events';
import type { QueryOutcome } from '../src/lib/promql/types';
import type { TimeRange, RefreshInterval } from '../src/lib/url-state/types';

// ---------------------------------------------------------------------------
// State factories — each builds a canonical state with a default
// context (interval = '10s', rangeKind = 'relative'). Tests override
// fields where the scenario demands a different context.
// ---------------------------------------------------------------------------

function mkState<K extends AutoRefreshState['kind']>(
  kind: K,
  overrides: Partial<AutoRefreshState> = {},
): AutoRefreshState {
  const base = { interval: '10s' as RefreshInterval, rangeKind: 'relative' as const };
  if (kind === 'backoff') {
    return { ...base, kind: 'backoff', retry: 0, ...overrides } as AutoRefreshState;
  }
  return { ...base, kind, ...overrides } as AutoRefreshState;
}

const idle = (over: Partial<AutoRefreshState> = {}): AutoRefreshState => mkState('idle', over);
const running = (over: Partial<AutoRefreshState> = {}): AutoRefreshState =>
  mkState('running', over);
const backoff = (retry: 0 | 1 | 2, over: Partial<AutoRefreshState> = {}): AutoRefreshState =>
  mkState('backoff', { retry, ...over });
const hidden = (over: Partial<AutoRefreshState> = {}): AutoRefreshState => mkState('hidden', over);

// Canonical events.
const refreshChanged = (interval: RefreshInterval): AutoRefreshEvent => ({
  kind: 'refresh-changed',
  interval,
});
const rangeChanged = (range: TimeRange): AutoRefreshEvent => ({ kind: 'range-changed', range });
const tickFired: AutoRefreshEvent = { kind: 'tick-fired' };
const visibility = (h: boolean): AutoRefreshEvent => ({ kind: 'visibility-changed', hidden: h });
const fetchResult = (outcome: QueryOutcome): AutoRefreshEvent => ({
  kind: 'fetch-result',
  outcome,
});

const relativeRange: TimeRange = { kind: 'relative', from: '-15m' };
const absoluteRange: TimeRange = {
  kind: 'absolute',
  from: new Date('2026-05-07T03:00:00Z'),
  to: new Date('2026-05-07T03:15:00Z'),
};

const successOutcome: QueryOutcome = { kind: 'success', series: [], queryMs: 50 };
const networkOutcome: QueryOutcome = {
  kind: 'transport-error',
  cause: { kind: 'network', message: 'Failed to fetch' },
  queryMs: 0,
};
const parseOutcome: QueryOutcome = {
  kind: 'parse-error',
  backendError: 'parse failed',
  queryMs: 30,
};
const emptyOutcome: QueryOutcome = { kind: 'empty', queryMs: 25 };
const abortedOutcome: QueryOutcome = {
  kind: 'transport-error',
  cause: { kind: 'aborted' },
  queryMs: 0,
};

// Helpers for asserting effects.
function hasEffect(effects: ReadonlyArray<AutoRefreshEffect>, kind: string): boolean {
  return effects.some((e) => e.kind === kind);
}
function scheduleMs(effects: ReadonlyArray<AutoRefreshEffect>): number | null {
  const e = effects.find((x) => x.kind === 'schedule-timer');
  return e !== undefined && e.kind === 'schedule-timer' ? e.ms : null;
}
function indexOfKind(effects: ReadonlyArray<AutoRefreshEffect>, kind: string): number {
  return effects.findIndex((e) => e.kind === kind);
}

// =============================================================================
// US-PR-05 AC-5.1 — picker offers exactly: off, 5s, 10s, 30s, 1m
// =============================================================================

describe('Slice 04 reducer — Idle ↔ Running on refresh-changed', () => {
  it('moves Idle → Running with a schedule-timer effect when refresh != off and range is relative (AC-5.1, AC-5.2)', () => {
    const { next, effects } = reduce(idle({ interval: 'off' }), refreshChanged('10s'));
    expect(next.kind).toBe('running');
    expect(scheduleMs(effects)).toBe(10000);
  });

  it('stays in Idle when refresh changes to "off" from Idle (AC-5.1)', () => {
    const { next, effects } = reduce(idle({ interval: '10s' }), refreshChanged('off'));
    expect(next.kind).toBe('idle');
    expect(hasEffect(effects, 'schedule-timer')).toBe(false);
  });

  it('moves Running → Idle with cancel-timer when refresh changes to "off" (AC-5.1)', () => {
    const { next, effects } = reduce(running(), refreshChanged('off'));
    expect(next.kind).toBe('idle');
    expect(hasEffect(effects, 'cancel-timer')).toBe(true);
  });
});

// =============================================================================
// US-PR-05 AC-5.2 — every tick re-fetches the same query
// =============================================================================

describe('Slice 04 reducer — Running tick-fired emits fetch', () => {
  it('emits fetch on every tick-fired in Running (AC-5.2)', () => {
    const { next, effects } = reduce(running(), tickFired);
    expect(next.kind).toBe('running');
    expect(hasEffect(effects, 'fetch')).toBe(true);
  });

  it('emits cancel-fetch before fetch when a tick fires (AC of slice-04 brief)', () => {
    const { effects } = reduce(running(), tickFired);
    const cancelIdx = indexOfKind(effects, 'cancel-fetch');
    const fetchIdx = indexOfKind(effects, 'fetch');
    expect(cancelIdx).toBeGreaterThanOrEqual(0);
    expect(fetchIdx).toBeGreaterThan(cancelIdx);
  });

  it('ignores tick-fired in Idle (defensive) (ADR-0029 § 6 double-lock)', () => {
    const { next, effects } = reduce(idle({ interval: 'off' }), tickFired);
    expect(next.kind).toBe('idle');
    expect(effects.length).toBe(0);
  });
});

// =============================================================================
// US-PR-05 AC-5.3 — fidelity & no-flicker hooks (the chart wrapper does not re-mount)
// =============================================================================

describe('Slice 04 reducer — fetch-result transitions', () => {
  it('on fetch-result success in Running, stays in Running and schedules next tick (AC-5.3, AC-5.5)', () => {
    const { next, effects } = reduce(running({ interval: '10s' }), fetchResult(successOutcome));
    expect(next.kind).toBe('running');
    expect(scheduleMs(effects)).toBe(10000);
  });

  it('on fetch-result parse-error in Running, stays in Running (no backoff) and schedules next tick (ADR-0029 § 4)', () => {
    const { next, effects } = reduce(running({ interval: '10s' }), fetchResult(parseOutcome));
    expect(next.kind).toBe('running');
    expect(scheduleMs(effects)).toBe(10000);
  });

  it('on fetch-result empty in Running, stays in Running (empty is information, not error) (ADR-0029 § 4)', () => {
    const { next, effects } = reduce(running({ interval: '10s' }), fetchResult(emptyOutcome));
    expect(next.kind).toBe('running');
    expect(scheduleMs(effects)).toBe(10000);
  });

  it('on fetch-result transport-error in Running, transitions to backoff retry=0 with 5s timer (ADR-0029 § 4)', () => {
    const { next, effects } = reduce(running(), fetchResult(networkOutcome));
    expect(next.kind).toBe('backoff');
    if (next.kind === 'backoff') expect(next.retry).toBe(0);
    expect(scheduleMs(effects)).toBe(5000);
  });

  it('on fetch-result aborted in Running, stays in Running (ADR-0029 § 3)', () => {
    const { next, effects } = reduce(running(), fetchResult(abortedOutcome));
    expect(next.kind).toBe('running');
    expect(effects.length).toBe(0);
  });
});

// =============================================================================
// US-PR-05 backoff curve: 5s → 10s → 20s → 30s capped (ADR-0029 § 4)
// =============================================================================

describe('Slice 04 reducer — backoff curve 5/10/20/30 capped', () => {
  it('Backoff(0) + tick-fired-then-fail → Backoff(1) with 10s schedule', () => {
    // Apply tick-fired (issues fetch; state remains Backoff(0)).
    const afterTick = reduce(backoff(0), tickFired);
    expect(afterTick.next.kind).toBe('backoff');
    // Then fetch-result transport-error.
    const after = reduce(afterTick.next, fetchResult(networkOutcome));
    expect(after.next.kind).toBe('backoff');
    if (after.next.kind === 'backoff') expect(after.next.retry).toBe(1);
    expect(scheduleMs(after.effects)).toBe(10000);
  });

  it('Backoff(1) + tick-fired-then-fail → Backoff(2) with 20s schedule', () => {
    const afterTick = reduce(backoff(1), tickFired);
    const after = reduce(afterTick.next, fetchResult(networkOutcome));
    expect(after.next.kind).toBe('backoff');
    if (after.next.kind === 'backoff') expect(after.next.retry).toBe(2);
    expect(scheduleMs(after.effects)).toBe(20000);
  });

  it('Backoff(2) + tick-fired-then-fail stays at Backoff(2) with 30s schedule (capped)', () => {
    const afterTick = reduce(backoff(2), tickFired);
    const after = reduce(afterTick.next, fetchResult(networkOutcome));
    expect(after.next.kind).toBe('backoff');
    if (after.next.kind === 'backoff') expect(after.next.retry).toBe(2);
    expect(scheduleMs(after.effects)).toBe(30000);
  });

  it('any Backoff(n) + tick-fired-then-success → Running with picked-interval schedule (reset)', () => {
    const afterTick = reduce(backoff(2, { interval: '10s' }), tickFired);
    const after = reduce(afterTick.next, fetchResult(successOutcome));
    expect(after.next.kind).toBe('running');
    expect(scheduleMs(after.effects)).toBe(10000);
  });

  it('Backoff(n) + parse-error returns to Running with picked-interval schedule (parse is not transport)', () => {
    const afterTick = reduce(backoff(1, { interval: '10s' }), tickFired);
    const after = reduce(afterTick.next, fetchResult(parseOutcome));
    expect(after.next.kind).toBe('running');
    expect(scheduleMs(after.effects)).toBe(10000);
  });
});

// =============================================================================
// US-PR-05 AC-5.4 — Page Visibility pauses and resumes
// =============================================================================

describe('Slice 04 reducer — Page Visibility transitions', () => {
  it('Running + visibility hidden → Hidden with cancel-timer (AC-5.4)', () => {
    const { next, effects } = reduce(running(), visibility(true));
    expect(next.kind).toBe('hidden');
    expect(hasEffect(effects, 'cancel-timer')).toBe(true);
  });

  it('Hidden + visibility visible → Running with immediate fetch and schedule-timer (AC-5.4)', () => {
    const { next, effects } = reduce(hidden({ interval: '10s' }), visibility(false));
    expect(next.kind).toBe('running');
    expect(hasEffect(effects, 'fetch')).toBe(true);
    expect(scheduleMs(effects)).toBe(10000);
  });

  it('Backoff + visibility hidden → Hidden with cancel-timer (AC-5.4)', () => {
    const { next, effects } = reduce(backoff(2), visibility(true));
    expect(next.kind).toBe('hidden');
    expect(hasEffect(effects, 'cancel-timer')).toBe(true);
  });
});

// =============================================================================
// US-PR-05 absolute-disables-auto invariant (ADR-0029 § 6)
// =============================================================================

describe('Slice 04 reducer — absolute range disables auto-refresh', () => {
  it('Running + range-changed to absolute → Idle with cancel-timer and cancel-fetch (ADR-0029 § 6)', () => {
    const { next, effects } = reduce(running(), rangeChanged(absoluteRange));
    expect(next.kind).toBe('idle');
    expect(hasEffect(effects, 'cancel-timer')).toBe(true);
    expect(hasEffect(effects, 'cancel-fetch')).toBe(true);
  });

  it('Idle + range-changed to relative with non-off refresh → Running with schedule-timer', () => {
    const start = idle({ interval: '30s', rangeKind: 'absolute' });
    const { next, effects } = reduce(start, rangeChanged(relativeRange));
    expect(next.kind).toBe('running');
    expect(scheduleMs(effects)).toBe(30000);
  });
});

// =============================================================================
// Property test — no schedule-timer without prior cancel-timer in any sequence
// (ADR-0029 § Verification — "no timer leaks")
// =============================================================================

describe('Slice 04 reducer — property: no timer leaks', () => {
  it('every schedule-timer effect is preceded by a cancel-timer for any prior timer in any event sequence', () => {
    // Each sequence starts from Idle with refresh off and walks through
    // a representative trajectory. The invariant: at any moment the
    // count of outstanding timers (cumulative schedule-timer minus
    // cumulative cancel-timer scoped to each transition) is 0 or 1.
    // Equivalently: a schedule-timer at the start of a transition's
    // effects must be preceded by a cancel-timer if the prior state
    // already had a timer outstanding.
    // Each sequence is REALISTIC — every fetch-result is preceded by
    // a tick-fired (because the only way a fetch reaches the reducer
    // is via the tick → fetch effect path). The walker's pre-decrement
    // on tick-fired models the one-shot timer being consumed.
    const sequences: ReadonlyArray<ReadonlyArray<AutoRefreshEvent>> = [
      [refreshChanged('10s'), tickFired, fetchResult(successOutcome), refreshChanged('off')],
      [
        refreshChanged('10s'),
        tickFired,
        fetchResult(networkOutcome),
        tickFired,
        fetchResult(networkOutcome),
        tickFired,
        fetchResult(networkOutcome),
        tickFired,
        fetchResult(networkOutcome),
        tickFired,
        fetchResult(successOutcome),
      ],
      [refreshChanged('10s'), visibility(true), visibility(false), refreshChanged('off')],
      [refreshChanged('5s'), rangeChanged(absoluteRange), rangeChanged(relativeRange)],
    ];

    for (const seq of sequences) {
      let state: AutoRefreshState = idle({ interval: 'off' });
      let outstanding = 0;
      for (const event of seq) {
        // A timer is a one-shot device: when it fires it delivers
        // tick-fired and is consumed. The walker decrements first so
        // the post-fire reducer can legitimately schedule the next.
        if (event.kind === 'tick-fired') {
          outstanding = Math.max(0, outstanding - 1);
        }
        const { next, effects } = reduce(state, event);
        for (const eff of effects) {
          if (eff.kind === 'cancel-timer') {
            outstanding = 0;
          }
          if (eff.kind === 'schedule-timer') {
            // The invariant: cannot schedule a new timer while one is
            // still outstanding.
            expect(outstanding).toBe(0);
            outstanding += 1;
          }
        }
        state = next;
      }
      // At most one timer outstanding at the end of any sequence.
      expect(outstanding).toBeLessThanOrEqual(1);
    }
  });
});

// =============================================================================
// Property test — every aborted outcome is silently ignored (ADR-0029 § 3)
// =============================================================================

describe('Slice 04 reducer — property: aborted outcomes never produce error effects', () => {
  it('a fetch-result with transport-error: aborted does not trigger backoff in any state', () => {
    const states: ReadonlyArray<AutoRefreshState> = [
      idle(),
      running(),
      backoff(0),
      backoff(1),
      backoff(2),
      hidden(),
    ];
    for (const s of states) {
      const { next, effects } = reduce(s, fetchResult(abortedOutcome));
      // Either stays at the same kind, or (for hidden / idle defensive
      // paths) stays at idle/hidden. Critically: never transitions to
      // backoff just because of an aborted result, and never schedules
      // a fresh timer in response.
      if (s.kind === 'backoff') {
        expect(next.kind).toBe('backoff');
        if (next.kind === 'backoff') expect(next.retry).toBe(s.retry);
      } else {
        expect(next.kind).toBe(s.kind);
      }
      expect(hasEffect(effects, 'schedule-timer')).toBe(false);
    }
  });
});

// =============================================================================
// Property test — every Run press is a fresh fetch (US-PR-05 @property)
// =============================================================================

describe('Slice 04 — property: every tick is a fresh fetch (no client-side cache)', () => {
  it('two consecutive tick-fired events at time T and T+1 each emit one fetch effect (KPI 3 invariant + @property scenario)', () => {
    const first = reduce(running(), tickFired);
    const second = reduce(first.next, tickFired);
    expect(first.effects.filter((e) => e.kind === 'fetch').length).toBe(1);
    expect(second.effects.filter((e) => e.kind === 'fetch').length).toBe(1);
    // Neither effects array carries any cache-shaped effect; the
    // effect vocabulary is closed to {schedule-timer, cancel-timer,
    // fetch, cancel-fetch}, and the type system enforces this.
    for (const e of [...first.effects, ...second.effects]) {
      expect(['schedule-timer', 'cancel-timer', 'fetch', 'cancel-fetch']).toContain(e.kind);
    }
  });
});
