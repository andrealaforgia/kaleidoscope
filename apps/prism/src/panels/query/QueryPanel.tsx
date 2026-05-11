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

// ADR-0026 §3 — Single driving panel composing the slice 01 happy
// path: query input + run button + chart + inline error/empty
// states + footer. URL state read on mount; URL bar updated
// synchronously via history.replaceState on every state change.
// Slice 03 adds the operator-visible error & empty-state surfaces:
//   - malformed-URL banner naming every invalid parameter
//   - parse-error banner with verbatim backend error
//   - transport-error banner naming the backend label + last-fetch time
//   - calm empty-state message with the active range
// And the stale-data invariant (ADR-0027 §5): the chart canvas is
// removed from the DOM whenever the latest outcome is not `success`,
// so a stale chart never sits next to an error banner.

import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { queryRange } from '../../lib/promql/client';
import type { QueryOutcome } from '../../lib/promql/types';
import { decode, encode } from '../../lib/url-state/codec';
import type { TimeRange, UrlField, UrlState } from '../../lib/url-state/types';
import { buildOption } from '../../lib/echarts/buildOption';
import type { BuildOptionContext } from '../../lib/echarts/buildOption';
import { EChart } from '../../lib/echarts/EChart';
import type { RuntimeConfig } from '../../lib/config/types';
import { TimeRangePicker } from './TimeRangePicker';

export interface QueryPanelProps {
  readonly config: RuntimeConfig;
  /** Test seam for fetch; defaults to globalThis.fetch in production. */
  readonly fetchFn?: typeof fetch;
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
  // Malformed URL fallback per ADR-0028 §6: revert to defaults AND
  // surface the invalid field list so the chrome can name them.
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

export function QueryPanel({ config, fetchFn }: QueryPanelProps): JSX.Element {
  // Initial state read from URL on mount.
  const initial = useMemo(() => readUrlState(), []);
  const [state, setState] = useState<UrlState>(initial.state);
  const [outcome, setOutcome] = useState<QueryOutcome | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [tickCount, setTickCount] = useState(0);
  // ADR-0027 §5: timestamp of the most recent successful fetch.
  // Rendered alongside the transport-error banner so the operator
  // knows how stale the (now-removed) chart was.
  const [lastFetchTime, setLastFetchTime] = useState<Date | null>(null);
  // ADR-0028 §6: invalid URL parameters discovered on initial load.
  // Cleared on first picker change; never re-populated within a
  // session (a clean URL means we are back to a known state).
  const [invalidFields, setInvalidFields] = useState<ReadonlyArray<UrlField>>(
    initial.invalidFields,
  );
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Persist state changes to the URL bar synchronously.
  useEffect(() => {
    writeUrlState(state);
  }, [state]);

  // Focus the query input on first mount per the walking-skeleton
  // contract (operator types immediately without clicking).
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
    if (next.kind === 'success') {
      setLastFetchTime(new Date());
    }
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
    // Slice 02 contract: range change re-fetches synchronously, not
    // gated by the Run button. Auto-refresh disable for absolute
    // ranges happens at the codec level (ADR-0028 §4 double-lock).
    void executeQuery(range);
  }

  const showChart = outcome !== null && outcome.kind === 'success';

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

      <header className="prism-chrome">
        <span className="prism-backend-label" data-testid="backend-label">
          Backend: {config.backend.label}
        </span>
        <span className="prism-version" data-testid="prism-version">
          Prism v{config.prism.version}
        </span>
      </header>

      <form className="prism-query-form" onSubmit={onSubmit}>
        <label className="prism-query-label" htmlFor="prism-query-input">
          PromQL query
        </label>
        <TimeRangePicker range={state.range} onChange={onRangeChange} />
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
      </main>

      <footer className="prism-footer" data-testid="chart-footer">
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
