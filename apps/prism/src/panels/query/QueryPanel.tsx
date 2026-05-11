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

// ADR-0026 §3 — Driving panel composing the cumulative slice 01-06
// surface: query input + range picker + auto-refresh picker + Run
// + chart + banners + footer. URL state read on mount; URL bar
// updated synchronously via history.replaceState. Auto-refresh
// state machine (ADR-0029) drives periodic re-fetches: the reducer
// emits effects (schedule-timer, cancel-timer, fetch, cancel-fetch)
// and this panel routes them to the Scheduler seam, queryRange,
// and AbortController.

import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { queryRange } from '../../lib/promql/client';
import type { QueryOutcome } from '../../lib/promql/types';
import { decode, encode } from '../../lib/url-state/codec';
import type { RefreshInterval, TimeRange, UrlField, UrlState } from '../../lib/url-state/types';
import { buildOption } from '../../lib/echarts/buildOption';
import type { BuildOptionContext } from '../../lib/echarts/buildOption';
import { EChart } from '../../lib/echarts/EChart';
import type { RuntimeConfig } from '../../lib/config/types';
import { reduce } from '../../lib/auto-refresh/reducer';
import type {
  AutoRefreshEffect,
  AutoRefreshEvent,
  AutoRefreshState,
} from '../../lib/auto-refresh/events';
import {
  DefaultScheduler,
  type Scheduler,
  type TimerHandle,
} from '../../lib/auto-refresh/scheduler';
import { TimeRangePicker } from './TimeRangePicker';
import { AutoRefreshPicker } from './AutoRefreshPicker';

export interface QueryPanelProps {
  readonly config: RuntimeConfig;
  /** Test seam for fetch; defaults to globalThis.fetch in production. */
  readonly fetchFn?: typeof fetch;
  /** Test seam for the auto-refresh Scheduler; defaults to DefaultScheduler. */
  readonly scheduler?: Scheduler;
}

const DEFAULT_STATE: UrlState = {
  q: '',
  range: { kind: 'relative', from: '-15m' },
  refresh: 'off',
};

const RELATIVE_LABELS: Readonly<Record<string, string>> = {
  '-5m': 'last 5 minutes',
  '-15m': 'last 15 minutes',
  '-1h': 'last 1 hour',
  '-6h': 'last 6 hours',
  '-24h': 'last 24 hours',
};

function rangeIso(range: TimeRange): string {
  if (range.kind === 'relative') {
    return RELATIVE_LABELS[range.from] ?? range.from;
  }
  return `${range.from.toISOString()} to ${range.to.toISOString()}`;
}

interface HydratedUrlState {
  readonly state: UrlState;
  readonly invalidFields: ReadonlyArray<UrlField>;
}

function readUrlState(): HydratedUrlState {
  const decoded = decode(window.location.search);
  if (decoded.kind === 'ok') return { state: decoded.value, invalidFields: [] };
  return { state: DEFAULT_STATE, invalidFields: decoded.error.fields };
}

function writeUrlState(state: UrlState): void {
  const search = encode(state);
  const newUrl = `${window.location.pathname}?${search}${window.location.hash}`;
  window.history.replaceState(null, '', newUrl);
}

function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return false;
  }
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export function QueryPanel({ config, fetchFn, scheduler }: QueryPanelProps): JSX.Element {
  const initial = useMemo(() => readUrlState(), []);
  const [state, setState] = useState<UrlState>(initial.state);
  const [outcome, setOutcome] = useState<QueryOutcome | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [tickCount, setTickCount] = useState(0);
  const [lastFetchTime, setLastFetchTime] = useState<Date | null>(null);
  const [invalidFields, setInvalidFields] = useState<ReadonlyArray<UrlField>>(
    initial.invalidFields,
  );
  const [refreshState, setRefreshState] = useState<AutoRefreshState>(() => ({
    kind: 'idle',
    interval: initial.state.refresh,
    rangeKind: initial.state.range.kind,
  }));

  const inputRef = useRef<HTMLInputElement | null>(null);
  const schedulerRef = useRef<Scheduler>(scheduler ?? new DefaultScheduler());
  const timerHandleRef = useRef<TimerHandle | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const stateRef = useRef(state);
  stateRef.current = state;

  // Persist state changes to the URL bar synchronously.
  useEffect(() => {
    writeUrlState(state);
  }, [state]);

  // Focus the query input on first mount.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const buildCtx = useMemo<BuildOptionContext>(
    () => ({
      palette: 'okabe-ito',
      range: state.range,
      prefersReducedMotion: prefersReducedMotion(),
    }),
    [state.range],
  );

  const option = useMemo(() => {
    if (outcome === null || outcome.kind !== 'success') {
      return buildOption({ kind: 'empty', queryMs: 0 }, buildCtx);
    }
    return buildOption(outcome, buildCtx);
  }, [outcome, buildCtx]);

  const dispatchRef = useRef<((event: AutoRefreshEvent) => void) | null>(null);

  const processEffects = useCallback(
    (effects: ReadonlyArray<AutoRefreshEffect>): void => {
      for (const eff of effects) {
        switch (eff.kind) {
          case 'schedule-timer': {
            timerHandleRef.current = schedulerRef.current.schedule(eff.ms, () => {
              timerHandleRef.current = null;
              dispatchRef.current?.({ kind: 'tick-fired' });
            });
            break;
          }
          case 'cancel-timer': {
            if (timerHandleRef.current !== null) {
              schedulerRef.current.cancel(timerHandleRef.current);
              timerHandleRef.current = null;
            }
            break;
          }
          case 'fetch': {
            const ac = new AbortController();
            abortRef.current = ac;
            const s = stateRef.current;
            if (s.q.length === 0) break;
            const fetcher = fetchFn ?? globalThis.fetch.bind(globalThis);
            setIsLoading(true);
            void queryRange(
              { q: s.q, range: s.range },
              { backend: config.backend.url, fetchFn: fetcher, signal: ac.signal },
            ).then((next) => {
              setOutcome(next);
              setTickCount((n) => n + 1);
              setIsLoading(false);
              if (next.kind === 'success') setLastFetchTime(new Date());
              dispatchRef.current?.({ kind: 'fetch-result', outcome: next });
            });
            break;
          }
          case 'cancel-fetch': {
            if (abortRef.current !== null) {
              abortRef.current.abort();
              abortRef.current = null;
            }
            break;
          }
        }
      }
    },
    [config.backend.url, fetchFn],
  );

  const dispatch = useCallback(
    (event: AutoRefreshEvent): void => {
      setRefreshState((prev) => {
        const { next, effects } = reduce(prev, event);
        queueMicrotask(() => processEffects(effects));
        return next;
      });
    },
    [processEffects],
  );

  useEffect(() => {
    dispatchRef.current = dispatch;
  }, [dispatch]);

  // Visibility listener — pause auto-refresh when the tab is hidden.
  useEffect(() => {
    const handler = (): void => {
      dispatch({ kind: 'visibility-changed', hidden: document.hidden });
    };
    document.addEventListener('visibilitychange', handler);
    return () => document.removeEventListener('visibilitychange', handler);
  }, [dispatch]);

  // Bootstrap the reducer on mount with the initial URL refresh value.
  // If the URL says refresh=10s and the range is relative, this kicks
  // the reducer into Running and schedules the first tick.
  useEffect(() => {
    dispatch({ kind: 'refresh-changed', interval: initial.state.refresh });
  }, [dispatch, initial.state.refresh]);

  async function executeQuery(range: TimeRange): Promise<void> {
    if (state.q.length === 0) return;
    setIsLoading(true);
    const fetcher = fetchFn ?? globalThis.fetch.bind(globalThis);
    const next = await queryRange(
      { q: state.q, range },
      { backend: config.backend.url, fetchFn: fetcher },
    );
    setOutcome(next);
    setTickCount((n) => n + 1);
    setIsLoading(false);
    if (next.kind === 'success') setLastFetchTime(new Date());
  }

  function onSubmit(e: FormEvent<HTMLFormElement>): void {
    e.preventDefault();
    void executeQuery(state.range);
  }

  function onQueryChange(value: string): void {
    setState((prev) => ({ ...prev, q: value }));
  }

  function onRangeChange(range: TimeRange): void {
    setState((prev) => ({ ...prev, range }));
    if (invalidFields.length > 0) setInvalidFields([]);
    dispatch({ kind: 'range-changed', range });
    void executeQuery(range);
  }

  function onRefreshChange(refresh: RefreshInterval): void {
    setState((prev) => ({ ...prev, refresh }));
    dispatch({ kind: 'refresh-changed', interval: refresh });
  }

  const showChart = outcome !== null && outcome.kind === 'success';
  const refreshPickerDisabled = state.range.kind === 'absolute';

  return (
    <div className="prism-panel" data-testid="query-panel">
      {invalidFields.length > 0 && (
        <div
          role="alert"
          className="prism-banner prism-banner-warning"
          data-testid="malformed-url-banner"
        >
          <strong>Some URL parameters were invalid.</strong>
          <span className="prism-banner-detail">Reset to defaults: {invalidFields.join(', ')}</span>
        </div>
      )}

      <header className="prism-chrome" role="banner">
        <span className="prism-backend-label" data-testid="backend-label">
          Backend: {config.backend.label}
        </span>
        <span className="prism-version" data-testid="prism-version">
          Prism v{config.prism.version}
        </span>
        <span className="prism-refresh-state" data-testid="refresh-state" aria-live="polite">
          Auto-refresh: {refreshState.kind}
        </span>
      </header>

      <form className="prism-query-form" onSubmit={onSubmit} aria-label="PromQL query controls">
        <label className="prism-query-label" htmlFor="prism-query-input">
          PromQL query
        </label>
        <TimeRangePicker range={state.range} onChange={onRangeChange} />
        <AutoRefreshPicker
          value={state.refresh}
          disabled={refreshPickerDisabled}
          onChange={onRefreshChange}
        />
        <input
          ref={inputRef}
          id="prism-query-input"
          className="prism-query-input"
          type="text"
          value={state.q}
          onChange={(e) => {
            onQueryChange(e.target.value);
          }}
          placeholder="up"
          aria-label="PromQL query"
          data-testid="query-input"
        />
        <button
          type="submit"
          className="prism-run-button"
          disabled={state.q.length === 0 || isLoading}
          data-testid="run-button"
        >
          {isLoading ? 'Running…' : 'Run'}
        </button>
      </form>

      <main className="prism-chart-area" data-testid="chart-area">
        {outcome !== null && outcome.kind === 'parse-error' && (
          <>
            <div
              role="alert"
              className="prism-banner prism-banner-warning"
              data-testid="parse-error-banner"
            >
              <strong>Backend rejected this query.</strong>
              <pre className="prism-banner-detail">{outcome.backendError}</pre>
            </div>
            <div className="prism-chart-fallback" data-testid="parse-error-fallback">
              Backend rejected this query.
            </div>
          </>
        )}

        {outcome !== null && outcome.kind === 'transport-error' && (
          <>
            <div
              role="alert"
              className="prism-banner prism-banner-warning"
              data-testid="transport-error-banner"
            >
              <strong>Cannot reach {config.backend.label}.</strong>
              <span className="prism-banner-detail">
                Transport failure: {outcome.cause.kind}
                {outcome.cause.kind !== 'aborted' && `: ${outcome.cause.message}`}
              </span>
            </div>
            {lastFetchTime !== null && (
              <div className="prism-last-fetch" data-testid="last-fetch-time">
                Last successful fetch: {lastFetchTime.toISOString()}
              </div>
            )}
          </>
        )}

        {outcome !== null && outcome.kind === 'empty' && (
          <div className="prism-empty-state" data-testid="empty-state">
            No data for {rangeIso(state.range)}. Check the metric name or widen the range.
          </div>
        )}

        {outcome !== null && outcome.kind === 'config-error' && (
          <div role="alert" className="prism-banner prism-banner-warning">
            Configuration error: {outcome.error.message}
          </div>
        )}

        {showChart && (
          <div className="prism-chart-canvas" data-testid="chart-canvas">
            <EChart option={option} tickCount={tickCount} />
          </div>
        )}

        {outcome !== null && outcome.kind === 'success' && (
          <table
            className="prism-chart-table"
            data-testid="chart-fallback-table"
            aria-label="Chart data as a table"
          >
            <caption>
              {outcome.series.length} series ·{' '}
              {outcome.series.reduce((acc, s) => acc + s.points.length, 0)} points
            </caption>
            <thead>
              <tr>
                <th scope="col">Series</th>
                <th scope="col">Points</th>
                <th scope="col">Latest value</th>
              </tr>
            </thead>
            <tbody>
              {outcome.series.map((s, i) => {
                const latest = s.points.length > 0 ? s.points[s.points.length - 1]![1] : null;
                const seriesName = Object.entries(s.labels)
                  .map(([k, v]) => `${k}="${v}"`)
                  .join(', ');
                return (
                  <tr key={`s-${i}`}>
                    <td>{seriesName.length > 0 ? seriesName : `series ${i + 1}`}</td>
                    <td>{s.points.length}</td>
                    <td>{latest === null || Number.isNaN(latest) ? '—' : latest.toString()}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </main>

      <footer className="prism-footer" data-testid="chart-footer" role="contentinfo">
        {outcome !== null && outcome.kind === 'success' && (
          <span>
            {outcome.series.length} series •{' '}
            {outcome.series.reduce((acc, s) => acc + s.points.length, 0)} points • {outcome.queryMs}{' '}
            ms
          </span>
        )}
      </footer>
    </div>
  );
}
