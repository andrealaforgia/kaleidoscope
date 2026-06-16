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

// The logs search view — find a symptom, then PIVOT to its trace.
//
// SEARCH: a time-range plus a search MODE that is EITHER a body-contains
// substring OR a min-severity floor — never both (the slice-01 backend
// rule). The mode toggle shows exactly one control at a time, so the two
// filters are mutually exclusive by construction and the client only ever
// puts one on the wire. RESULTS: the matching logs, each showing its
// severity and body. PIVOT: a log carrying a trace_id is pivotable —
// clicking its pivot deep-links to /traces?trace=<id>, where the existing
// linked view auto-opens the trace's spans (WHERE) + correlated logs
// (WHY). A log with no trace_id is an honest dead end, not a broken link.
//
// Mirrors TraceExplorerPanel's posture: a fetchFn seam, discriminated-
// outcome rendering, calm banners for every failure arm, data-testid +
// aria on every interactive element and key region, and a polite live
// region (WCAG 2.2 AA — 4.1.3) announcing the result count.

import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';
import { useNavigate } from 'react-router-dom';

import { findLogs } from '../../lib/logs/client';
import type { FindLogsOutcome, FindLogsRequest, LogView, LogsContext } from '../../lib/logs/types';
import type { RuntimeConfig } from '../../lib/config/types';
import type { TimeRange } from '../../lib/url-state/types';
import { TimeRangePicker } from '../query/TimeRangePicker';

export interface LogsExplorerPanelProps {
  readonly config: RuntimeConfig;
  /** Test seam for fetch; defaults to globalThis.fetch in production. */
  readonly fetchFn?: typeof fetch;
}

// A newcomer hunting a symptom in a demo seeded some hours ago wants a
// day-wide default window, matching the traces view. "-24h" is the widest
// operator-canonical preset the picker offers, so the default is coherent
// and changeable.
const DEFAULT_RANGE: TimeRange = { kind: 'relative', from: '-24h' };

const RELATIVE_SECONDS: Readonly<Record<string, number>> = {
  '-5m': 300,
  '-15m': 900,
  '-1h': 3600,
  '-6h': 21600,
  '-24h': 86400,
};

// The log read API refuses a window whose (end - start) STRICTLY exceeds
// this many seconds (query-http-common MAX_WINDOW_SECONDS), so a day-wide
// relative window resolves with an hour of slack to sit strictly under it.
const BACKEND_MAX_WINDOW_SECONDS = 86_400;
const WINDOW_HEADROOM_SECONDS = 3_600;
const MAX_RELATIVE_WINDOW_SECONDS = BACKEND_MAX_WINDOW_SECONDS - WINDOW_HEADROOM_SECONDS;

// The OTel severity names the backend's min_severity floor accepts
// (parse_min_severity, case-insensitive). ERROR is the operator's most
// common "show me the bad" floor, so it is the default.
const SEVERITY_FLOORS = ['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR', 'FATAL'] as const;
const DEFAULT_SEVERITY_FLOOR = 'ERROR';

type SearchMode = 'body' | 'severity';

interface EpochWindow {
  readonly start: number;
  readonly end: number;
}

function toEpochWindow(range: TimeRange): EpochWindow {
  if (range.kind === 'absolute') {
    return {
      start: Math.floor(range.from.getTime() / 1000),
      end: Math.floor(range.to.getTime() / 1000),
    };
  }
  const end = Math.floor(Date.now() / 1000);
  const requested = RELATIVE_SECONDS[range.from] ?? 3600;
  const span = Math.min(requested, MAX_RELATIVE_WINDOW_SECONDS);
  return { start: end - span, end };
}

/** A log is pivotable to its trace only when it carries a non-empty trace_id. */
function pivotTraceId(log: LogView): string | null {
  return log.trace_id !== undefined && log.trace_id.length > 0 ? log.trace_id : null;
}

/** What the polite live region announces; '' while loading, pending, or in an error arm. */
function listAnnouncement(outcome: FindLogsOutcome | null, loading: boolean): string {
  if (loading || outcome === null) return '';
  if (outcome.kind === 'success') {
    const count = outcome.logs.length;
    return `${count} ${count === 1 ? 'log' : 'logs'} found`;
  }
  if (outcome.kind === 'empty') {
    return 'No logs found for this search and window.';
  }
  return '';
}

export function LogsExplorerPanel({ config, fetchFn }: LogsExplorerPanelProps): JSX.Element {
  const navigate = useNavigate();

  const [range, setRange] = useState<TimeRange>(DEFAULT_RANGE);
  const [mode, setMode] = useState<SearchMode>('body');
  const [bodyContains, setBodyContains] = useState('');
  const [minSeverity, setMinSeverity] = useState<string>(DEFAULT_SEVERITY_FLOOR);

  const [outcome, setOutcome] = useState<FindLogsOutcome | null>(null);
  const [loading, setLoading] = useState(false);

  const logsContext = useMemo<LogsContext>(
    () => ({ backend: config.backend.url, fetchFn: fetchFn ?? globalThis.fetch.bind(globalThis) }),
    [config.backend.url, fetchFn],
  );

  // Switching mode CLEARS the inactive filter so only one is ever
  // populated — mutual exclusivity by construction, never both on the wire.
  const onModeChange = useCallback((next: SearchMode): void => {
    setMode(next);
    if (next === 'body') {
      setMinSeverity(DEFAULT_SEVERITY_FLOOR);
    } else {
      setBodyContains('');
    }
  }, []);

  const runSearch = useCallback(async (): Promise<void> => {
    const window = toEpochWindow(range);
    const request: FindLogsRequest =
      mode === 'body'
        ? { start: window.start, end: window.end, bodyContains }
        : { start: window.start, end: window.end, minSeverity };
    setLoading(true);
    const result = await findLogs(logsContext, request);
    setOutcome(result);
    setLoading(false);
  }, [range, mode, bodyContains, minSeverity, logsContext]);

  const onPivot = useCallback(
    (traceId: string): void => {
      navigate(`/traces?trace=${encodeURIComponent(traceId)}`);
    },
    [navigate],
  );

  function onSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    void runSearch();
  }

  const searchDisabled = loading || (mode === 'body' && bodyContains.length === 0);

  return (
    <div className="prism-panel" data-testid="logs-explorer-panel">
      <header className="prism-chrome" role="banner">
        <span className="prism-backend-label" data-testid="backend-label">
          Backend: {config.backend.label}
        </span>
        <span className="prism-version" data-testid="prism-version">
          Prism v{config.prism.version}
        </span>
      </header>

      <form className="prism-query-form" onSubmit={onSubmit} aria-label="Log search controls">
        <TimeRangePicker range={range} onChange={setRange} />

        <label className="prism-query-label" htmlFor="prism-log-search-mode">
          Search by
        </label>
        <select
          id="prism-log-search-mode"
          className="prism-time-range-select"
          value={mode}
          onChange={(e) => {
            onModeChange(e.target.value as SearchMode);
          }}
          aria-label="Log search mode"
          data-testid="log-search-mode"
        >
          <option value="body">Body contains</option>
          <option value="severity">Minimum severity</option>
        </select>

        {mode === 'body' ? (
          <input
            className="prism-query-input"
            type="text"
            value={bodyContains}
            onChange={(e) => {
              setBodyContains(e.target.value);
            }}
            placeholder="card declined"
            aria-label="Body contains text"
            data-testid="log-body-input"
          />
        ) : (
          <select
            className="prism-time-range-select"
            value={minSeverity}
            onChange={(e) => {
              setMinSeverity(e.target.value);
            }}
            aria-label="Minimum severity floor"
            data-testid="log-severity-select"
          >
            {SEVERITY_FLOORS.map((floor) => (
              <option key={floor} value={floor}>
                {floor}
              </option>
            ))}
          </select>
        )}

        <button
          type="submit"
          className="prism-run-button"
          disabled={searchDisabled}
          data-testid="log-run-button"
        >
          {loading ? 'Searching…' : 'Search'}
        </button>
      </form>

      <main className="prism-logs-layout" data-testid="logs-layout">
        <div
          className="prism-visually-hidden"
          role="status"
          aria-live="polite"
          data-testid="logs-status"
        >
          {listAnnouncement(outcome, loading)}
        </div>
        <LogResults
          outcome={outcome}
          loading={loading}
          backendLabel={config.backend.label}
          onPivot={onPivot}
        />
      </main>
    </div>
  );
}

interface LogResultsProps {
  readonly outcome: FindLogsOutcome | null;
  readonly loading: boolean;
  readonly backendLabel: string;
  readonly onPivot: (traceId: string) => void;
}

function LogResults({ outcome, loading, backendLabel, onPivot }: LogResultsProps): JSX.Element {
  if (loading) {
    return (
      <div className="prism-trace-loading" data-testid="logs-loading" aria-busy>
        Searching for logs…
      </div>
    );
  }

  if (outcome === null) {
    return (
      <div className="prism-empty-state" data-testid="logs-prompt">
        Search the log body for a symptom, or pick a minimum severity.
      </div>
    );
  }

  if (outcome.kind === 'transport-error') {
    return (
      <div
        role="alert"
        className="prism-banner prism-banner-warning"
        data-testid="logs-transport-error-banner"
      >
        <strong>Cannot reach {backendLabel}.</strong>
        <span className="prism-banner-detail">
          Transport failure: {outcome.cause.kind}
          {outcome.cause.kind !== 'aborted' && `: ${outcome.cause.message}`}
        </span>
      </div>
    );
  }

  if (outcome.kind === 'parse-error') {
    return (
      <div
        role="alert"
        className="prism-banner prism-banner-warning"
        data-testid="logs-parse-error-banner"
      >
        <strong>Could not read the log results.</strong>
        <pre className="prism-banner-detail">{outcome.message}</pre>
      </div>
    );
  }

  if (outcome.kind === 'empty') {
    return (
      <div className="prism-empty-state" data-testid="logs-empty-state">
        No logs found for this search and window.
      </div>
    );
  }

  return (
    <ul className="prism-trace-logs" data-testid="logs-rows" aria-label="Matching logs">
      {outcome.logs.map((log, i) => {
        const traceId = pivotTraceId(log);
        return (
          <li
            key={`${log.observed_time_unix_nano}-${i}`}
            className="prism-log-row"
            data-testid="log-row"
          >
            <span className="prism-log-severity" data-testid="log-severity">
              {log.severity_text}
            </span>
            <span className="prism-log-body">{log.body}</span>
            {traceId !== null && (
              <button
                type="button"
                className="prism-log-pivot"
                data-testid="log-pivot"
                aria-label={`View the trace for this log (${traceId})`}
                onClick={() => {
                  onPivot(traceId);
                }}
              >
                View trace
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}
