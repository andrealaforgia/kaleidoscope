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
// Slices 02-05 extend the picker UI; this v01d shape carries the
// minimal "default 15-min relative range" walking-skeleton flow.

import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { queryRange } from '../../lib/promql/client';
import type { QueryOutcome } from '../../lib/promql/types';
import { decode, encode } from '../../lib/url-state/codec';
import type { UrlState } from '../../lib/url-state/types';
import { buildOption } from '../../lib/echarts/buildOption';
import type { BuildOptionContext } from '../../lib/echarts/buildOption';
import { EChart } from '../../lib/echarts/EChart';
import type { RuntimeConfig } from '../../lib/config/types';

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

function readUrlState(): UrlState {
  const decoded = decode(window.location.search);
  if (decoded.kind === 'ok') return decoded.value;
  // Malformed URL fallback per ADR-0028 §6: revert to defaults.
  return DEFAULT_STATE;
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
  const [state, setState] = useState<UrlState>(() => readUrlState());
  const [outcome, setOutcome] = useState<QueryOutcome | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [tickCount, setTickCount] = useState(0);
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
    if (outcome === null) {
      // No outcome yet; render an empty chart shell.
      return buildOption({ kind: 'empty', queryMs: 0 }, buildCtx);
    }
    return buildOption(outcome, buildCtx);
  }, [outcome, buildCtx]);

  async function runQuery(): Promise<void> {
    if (state.q.length === 0) return;
    setIsLoading(true);
    const fetcher = fetchFn ?? globalThis.fetch.bind(globalThis);
    const next = await queryRange(
      { q: state.q, range: state.range },
      { backend: config.backend.url, fetchFn: fetcher },
    );
    setOutcome(next);
    setTickCount((n) => n + 1);
    setIsLoading(false);
  }

  function onSubmit(e: FormEvent<HTMLFormElement>): void {
    e.preventDefault();
    void runQuery();
  }

  function onQueryChange(value: string): void {
    setState((prev) => ({ ...prev, q: value }));
  }

  return (
    <div className="prism-panel" data-testid="query-panel">
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
          <div
            role="alert"
            className="prism-banner prism-banner-warning"
            data-testid="parse-error-banner"
          >
            <strong>Backend rejected this query.</strong>
            <pre className="prism-banner-detail">{outcome.backendError}</pre>
          </div>
        )}

        {outcome !== null && outcome.kind === 'transport-error' && (
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
        )}

        {outcome !== null && outcome.kind === 'empty' && (
          <div className="prism-empty-state" data-testid="empty-state">
            No data for the selected range. Check the metric name or widen the range.
          </div>
        )}

        {outcome !== null && outcome.kind === 'config-error' && (
          <div role="alert" className="prism-banner prism-banner-warning">
            Configuration error: {outcome.error.message}
          </div>
        )}

        <div
          className="prism-chart-canvas"
          data-testid="chart-canvas"
          style={{
            display: outcome !== null && outcome.kind === 'success' ? 'block' : 'none',
          }}
        >
          <EChart option={option} tickCount={tickCount} />
        </div>
      </main>

      <footer className="prism-footer" data-testid="chart-footer">
        {outcome !== null && outcome.kind === 'success' && (
          <span>
            {outcome.series.length} series •{' '}
            {outcome.series.reduce((acc, s) => acc + s.points.length, 0)} points •{' '}
            {outcome.queryMs} ms
          </span>
        )}
      </footer>
    </div>
  );
}
