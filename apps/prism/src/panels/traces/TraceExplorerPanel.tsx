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

// The linked view — a master-detail trace explorer on ONE route.
//
// FIND: service + time-range + an errors-only toggle drive the listing
// query (findFailedTraces / findTraces). RESULTS: traces grouped per
// trace_id; an error trace is badged so the failed one is identifiable
// WITHOUT opening it. LINKED DETAIL: selecting a trace fetches it with
// its correlated logs (getTraceWithLogs) and renders, on the same
// screen, the spans (with the error span's readable status MESSAGE —
// WHERE) together with the logs (the ERROR-severity cause log — WHY).
//
// Mirrors QueryPanel's posture: a fetchFn seam, discriminated-outcome
// rendering, calm banners for every failure arm, data-testid + aria on
// every interactive element and key region.

import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';

import { findFailedTraces, findTraces, getTraceWithLogs } from '../../lib/traces/client';
import type {
  FailedTracesOutcome,
  LogView,
  Span,
  TraceGroup,
  TracesContext,
  TraceWithLogsOutcome,
} from '../../lib/traces/types';
import type { RuntimeConfig } from '../../lib/config/types';
import type { TimeRange } from '../../lib/url-state/types';
import { TimeRangePicker } from '../query/TimeRangePicker';

export interface TraceExplorerPanelProps {
  readonly config: RuntimeConfig;
  /** Test seam for fetch; defaults to globalThis.fetch in production. */
  readonly fetchFn?: typeof fetch;
}

// A newcomer opening Traces is hunting a failure in a demo that was
// seeded some hours ago, not in the last few minutes — so the view
// defaults to a day-wide window. "-24h" is the widest operator-canonical
// preset (the picker offers it, so the default is coherent and changeable).
const DEFAULT_RANGE: TimeRange = { kind: 'relative', from: '-24h' };

const RELATIVE_SECONDS: Readonly<Record<string, number>> = {
  '-5m': 300,
  '-15m': 900,
  '-1h': 3600,
  '-6h': 21600,
  '-24h': 86400,
};

// The trace/log read APIs refuse a window whose (end - start) STRICTLY
// exceeds this many seconds (ADR-0050; query-http-common
// MAX_WINDOW_SECONDS). A day-wide relative window builds end - start with
// zero headroom, so we resolve every relative window to sit strictly
// under the cap with an hour of slack — the default never brushes it.
const BACKEND_MAX_WINDOW_SECONDS = 86_400;
const WINDOW_HEADROOM_SECONDS = 3_600;
const MAX_RELATIVE_WINDOW_SECONDS = BACKEND_MAX_WINDOW_SECONDS - WINDOW_HEADROOM_SECONDS;

/** OTel ERROR severity range starts at 17. Logs at or above are causes. */
const ERROR_SEVERITY_FLOOR = 17;

interface EpochWindow {
  readonly start: number;
  readonly end: number;
}

/** Resolve a TimeRange to an epoch-second window the backend understands. */
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

/** The service a trace belongs to, read from its first span's resources. */
function traceService(group: TraceGroup): string {
  const first = group.spans[0];
  return first?.resource_attributes['service.name'] ?? '(unknown service)';
}

/** The primary operation name — the root span's, else the first span's. */
function primaryOperation(group: TraceGroup): string {
  const root = group.spans.find((s) => s.parent_span_id === undefined);
  return (root ?? group.spans[0])?.name ?? '(unnamed)';
}

/** A trace has failed if any of its spans carries an Error status. */
function isErrorTrace(group: TraceGroup): boolean {
  return group.spans.some((s) => s.status.code === 'Error');
}

function isCauseLog(log: LogView): boolean {
  return log.severity_number >= ERROR_SEVERITY_FLOOR;
}

export function TraceExplorerPanel({ config, fetchFn }: TraceExplorerPanelProps): JSX.Element {
  const [service, setService] = useState('');
  const [range, setRange] = useState<TimeRange>(DEFAULT_RANGE);
  const [errorsOnly, setErrorsOnly] = useState(true);

  const [listOutcome, setListOutcome] = useState<FailedTracesOutcome | null>(null);
  const [listLoading, setListLoading] = useState(false);

  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const [detailOutcome, setDetailOutcome] = useState<TraceWithLogsOutcome | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  const tracesContext = useMemo<TracesContext>(
    () => ({ backend: config.backend.url, fetchFn: fetchFn ?? globalThis.fetch.bind(globalThis) }),
    [config.backend.url, fetchFn],
  );

  const runSearch = useCallback(async (): Promise<void> => {
    if (service.length === 0) return;
    setSelectedTraceId(null);
    setDetailOutcome(null);
    setListLoading(true);
    const window = toEpochWindow(range);
    const request = { service, start: window.start, end: window.end };
    const finder = errorsOnly ? findFailedTraces : findTraces;
    const outcome = await finder(tracesContext, request);
    setListOutcome(outcome);
    setListLoading(false);
  }, [service, range, errorsOnly, tracesContext]);

  const selectTrace = useCallback(
    async (traceId: string): Promise<void> => {
      setSelectedTraceId(traceId);
      setDetailOutcome(null);
      setDetailLoading(true);
      const outcome = await getTraceWithLogs(tracesContext, traceId);
      setDetailOutcome(outcome);
      setDetailLoading(false);
    },
    [tracesContext],
  );

  function onSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    void runSearch();
  }

  return (
    <div className="prism-panel" data-testid="trace-explorer-panel">
      <header className="prism-chrome" role="banner">
        <span className="prism-backend-label" data-testid="backend-label">
          Backend: {config.backend.label}
        </span>
        <span className="prism-version" data-testid="prism-version">
          Prism v{config.prism.version}
        </span>
      </header>

      <form className="prism-query-form" onSubmit={onSubmit} aria-label="Trace search controls">
        <label className="prism-query-label" htmlFor="prism-trace-service">
          Service
        </label>
        <TimeRangePicker range={range} onChange={setRange} />
        <input
          id="prism-trace-service"
          className="prism-query-input"
          type="text"
          value={service}
          onChange={(e) => {
            setService(e.target.value);
          }}
          placeholder="checkout"
          aria-label="Service name"
          data-testid="trace-service-input"
        />
        <label className="prism-trace-toggle">
          <input
            type="checkbox"
            checked={errorsOnly}
            onChange={(e) => {
              setErrorsOnly(e.target.checked);
            }}
            aria-label="Errors only"
            data-testid="errors-only-toggle"
          />
          <span>Errors only</span>
        </label>
        <button
          type="submit"
          className="prism-run-button"
          disabled={service.length === 0 || listLoading}
          data-testid="trace-run-button"
        >
          {listLoading ? 'Searching…' : 'Search'}
        </button>
      </form>

      <main className="prism-trace-layout" data-testid="trace-layout">
        <section className="prism-trace-list" aria-label="Traces" data-testid="trace-list">
          <TraceList
            outcome={listOutcome}
            loading={listLoading}
            errorsOnly={errorsOnly}
            backendLabel={config.backend.label}
            selectedTraceId={selectedTraceId}
            onSelect={(id) => void selectTrace(id)}
          />
        </section>

        <section
          className="prism-trace-detail"
          aria-label="Selected trace detail"
          data-testid="trace-detail-region"
        >
          <TraceDetail
            selectedTraceId={selectedTraceId}
            outcome={detailOutcome}
            loading={detailLoading}
            backendLabel={config.backend.label}
          />
        </section>
      </main>
    </div>
  );
}

interface TraceListProps {
  readonly outcome: FailedTracesOutcome | null;
  readonly loading: boolean;
  readonly errorsOnly: boolean;
  readonly backendLabel: string;
  readonly selectedTraceId: string | null;
  readonly onSelect: (traceId: string) => void;
}

function TraceList({
  outcome,
  loading,
  errorsOnly,
  backendLabel,
  selectedTraceId,
  onSelect,
}: TraceListProps): JSX.Element {
  if (loading) {
    return (
      <div className="prism-trace-loading" data-testid="trace-list-loading" aria-busy>
        Searching for traces…
      </div>
    );
  }

  if (outcome === null) {
    return (
      <div className="prism-empty-state" data-testid="trace-prompt">
        Enter a service and search to find traces.
      </div>
    );
  }

  if (outcome.kind === 'transport-error') {
    return (
      <div
        role="alert"
        className="prism-banner prism-banner-warning"
        data-testid="trace-transport-error-banner"
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
        data-testid="trace-parse-error-banner"
      >
        <strong>Could not read the trace list.</strong>
        <pre className="prism-banner-detail">{outcome.message}</pre>
      </div>
    );
  }

  if (outcome.kind === 'empty') {
    return (
      <div className="prism-empty-state" data-testid="trace-empty-state">
        {errorsOnly
          ? 'No failed traces for this service and window.'
          : 'No traces for this service and window.'}
      </div>
    );
  }

  return (
    <ul className="prism-trace-rows" data-testid="trace-rows">
      {outcome.traces.map((group) => {
        const failed = isErrorTrace(group);
        const selected = group.trace_id === selectedTraceId;
        const operation = primaryOperation(group);
        const svc = traceService(group);
        return (
          <li key={group.trace_id} className="prism-trace-row-item">
            <button
              type="button"
              className={`prism-trace-row${selected ? ' prism-trace-row-selected' : ''}${failed ? ' prism-trace-row-error' : ''}`}
              data-testid="trace-row"
              aria-pressed={selected}
              aria-label={`Trace ${operation} on ${svc}${failed ? ', Error' : ''}`}
              onClick={() => {
                onSelect(group.trace_id);
              }}
            >
              <span className="prism-trace-op">{operation}</span>
              <span className="prism-trace-svc">{svc}</span>
              {failed && (
                <span
                  className="prism-trace-error-badge"
                  data-testid="trace-error-badge"
                  aria-label="Error trace"
                >
                  Error
                </span>
              )}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

interface TraceDetailProps {
  readonly selectedTraceId: string | null;
  readonly outcome: TraceWithLogsOutcome | null;
  readonly loading: boolean;
  readonly backendLabel: string;
}

function TraceDetail({
  selectedTraceId,
  outcome,
  loading,
  backendLabel,
}: TraceDetailProps): JSX.Element {
  if (selectedTraceId === null) {
    return (
      <div className="prism-empty-state" data-testid="detail-prompt">
        Select a trace to see its spans and correlated logs.
      </div>
    );
  }

  if (loading) {
    return (
      <div className="prism-trace-loading" data-testid="detail-loading" aria-busy>
        Loading trace…
      </div>
    );
  }

  if (outcome === null) {
    return <div className="prism-trace-loading" data-testid="detail-loading" aria-busy />;
  }

  if (outcome.kind === 'transport-error') {
    return (
      <div
        role="alert"
        className="prism-banner prism-banner-warning"
        data-testid="detail-transport-error-banner"
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
        data-testid="detail-parse-error-banner"
      >
        <strong>Could not read this trace.</strong>
        <pre className="prism-banner-detail">{outcome.message}</pre>
      </div>
    );
  }

  if (outcome.kind === 'empty') {
    return (
      <div className="prism-empty-state" data-testid="detail-empty-state">
        This trace carried no spans.
      </div>
    );
  }

  return (
    <div className="prism-trace-detail-body" data-testid="trace-detail">
      <SpanTable spans={outcome.trace.spans} />
      <LogList logs={outcome.trace.logs} />
    </div>
  );
}

function SpanTable({ spans }: { readonly spans: readonly Span[] }): JSX.Element {
  return (
    <table className="prism-trace-table" data-testid="span-table" aria-label="Trace spans">
      <caption>Spans ({spans.length})</caption>
      <thead>
        <tr>
          <th scope="col">Span</th>
          <th scope="col">Status</th>
          <th scope="col">Message</th>
        </tr>
      </thead>
      <tbody>
        {spans.map((span) => {
          const isError = span.status.code === 'Error';
          return (
            <tr
              key={span.span_id}
              className={isError ? 'prism-span-row-error' : undefined}
              data-testid="span-row"
            >
              <td>{span.name}</td>
              <td>
                <span
                  className={`prism-span-status${isError ? ' prism-span-status-error' : ''}`}
                  data-testid="span-status-code"
                >
                  {span.status.code}
                </span>
              </td>
              <td>
                {isError && span.status.message.length > 0 ? (
                  <span className="prism-span-status-message" data-testid="span-status-message">
                    {span.status.message}
                  </span>
                ) : (
                  <span aria-hidden="true">—</span>
                )}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function LogList({ logs }: { readonly logs: readonly LogView[] }): JSX.Element {
  if (logs.length === 0) {
    return (
      <div className="prism-empty-state" data-testid="logs-empty">
        No correlated logs for this trace.
      </div>
    );
  }
  return (
    <ul className="prism-trace-logs" data-testid="log-list" aria-label="Correlated logs">
      {logs.map((log, i) => {
        const cause = isCauseLog(log);
        return (
          <li
            key={`${log.observed_time_unix_nano}-${i}`}
            className={`prism-log-row${cause ? ' prism-log-row-cause' : ''}`}
            data-testid="log-row"
            {...(cause && { 'data-cause': 'true' })}
          >
            <span className="prism-log-severity" data-testid="log-severity">
              {log.severity_text}
            </span>
            <span className="prism-log-body" {...(cause && { 'data-testid': 'cause-log' })}>
              {log.body}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
