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

// Slice 08 — the logs search view + the pivot to the trace WHERE+WHY.
//
// I am a newcomer. A symptom is loose in the system. I open Prism, switch
// to Logs, search the log body for the symptom text, and the matching logs
// come back. One of them carries a trace_id — so I can PIVOT from the
// "what I saw" (the log) to the "where + why" (the trace's spans + its
// correlated logs) on the existing linked-view detail. A log with no
// trace_id is a dead end by honesty, not a broken link.
//
// Driving ports: the App composition root (routing) + the LogsExplorerPanel
// and TraceExplorerPanel components. Driven port mocked at the HTTP
// boundary (fetchFn). No real backend.

import { describe, expect, it, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { App } from '../src/app/App';
import { LogsExplorerPanel } from '../src/panels/logs/LogsExplorerPanel';
import { TraceExplorerPanel } from '../src/panels/traces/TraceExplorerPanel';
import type { LogView } from '../src/lib/logs/types';
import type { TraceWithLogs } from '../src/lib/traces/types';

const TEST_CONFIG = {
  backend: { url: '/api/v1', label: 'dev-local-prom' },
  prism: { version: '0.1.0' },
} as const;

const TRACE_ID = 'a'.repeat(32);

function makeLog(overrides: Partial<LogView> = {}): LogView {
  return {
    observed_time_unix_nano: 1_700_000_000_050_000_000,
    severity_number: 9,
    severity_text: 'INFO',
    body: 'checkout started',
    attributes: {},
    resource_attributes: { 'service.name': 'checkout' },
    ...overrides,
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

// A correlated symptom log (carries trace_id → pivotable) and a bare one.
const CORRELATED_LOG = makeLog({
  severity_number: 17,
  severity_text: 'ERROR',
  body: 'card declined: insufficient funds',
  trace_id: TRACE_ID,
  span_id: 'a1a1a1a1a1a1a1a1',
});
const BARE_LOG = makeLog({
  severity_number: 13,
  severity_text: 'WARN',
  body: 'card declined warning, no trace attached',
});

const DETAIL_TRACE: TraceWithLogs = {
  trace_id: TRACE_ID,
  spans: [
    {
      trace_id: TRACE_ID,
      span_id: 'a1a1a1a1a1a1a1a1',
      name: 'POST /checkout',
      kind: 'Server',
      start_time_unix_nano: 1_700_000_000_000_000_000,
      end_time_unix_nano: 1_700_000_000_100_000_000,
      status: { code: 'Error', message: 'payment gateway returned 402' },
      attributes: {},
      resource_attributes: { 'service.name': 'checkout' },
      events: [],
      links: [],
    },
  ],
  logs: [
    makeLog({
      severity_number: 17,
      severity_text: 'ERROR',
      body: 'card declined: insufficient funds',
      trace_id: TRACE_ID,
      span_id: 'a1a1a1a1a1a1a1a1',
    }),
  ],
};

interface RecordedFetch {
  readonly fetchFn: typeof fetch;
  readonly calls: string[];
}

function logsFetch(
  logsResponder: (url: string) => Response = () => jsonResponse([CORRELATED_LOG, BARE_LOG]),
): RecordedFetch {
  const calls: string[] = [];
  const fetchFn: typeof fetch = async (input) => {
    const url = String(input);
    calls.push(url);
    if (url.includes('/config.json')) return jsonResponse(TEST_CONFIG);
    if (url.includes('/traces/with_logs')) return jsonResponse(DETAIL_TRACE);
    return logsResponder(url);
  };
  return { fetchFn, calls };
}

function renderPanel(fetchFn: typeof fetch): void {
  render(
    <MemoryRouter initialEntries={['/logs']}>
      <LogsExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />
    </MemoryRouter>,
  );
}

async function runBodySearch(text: string): Promise<void> {
  const input = screen.getByTestId('log-body-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value: text } });
  await userEvent.click(screen.getByTestId('log-run-button'));
}

// =============================================================================
// BODY-CONTAINS SEARCH
// =============================================================================

describe('Slice 08 body-contains search — the symptom text finds the matching logs', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('searches the log body and lists the matching logs with severity and body', async () => {
    const { fetchFn, calls } = logsFetch();
    renderPanel(fetchFn);
    await runBodySearch('card declined');

    await waitFor(() => {
      expect(screen.getAllByTestId('log-row').length).toBe(2);
    });
    const issued = calls.filter((c) => c.includes('/logs')).at(-1)!;
    const params = new URL(issued, 'http://x').searchParams;
    expect(params.get('body_contains')).toBe('card declined');
    expect(params.has('min_severity')).toBe(false);

    const rows = screen.getAllByTestId('log-row');
    expect(rows.some((r) => r.textContent?.includes('card declined: insufficient funds'))).toBe(
      true,
    );
    expect(rows.some((r) => r.textContent?.includes('ERROR'))).toBe(true);
  });
});

// =============================================================================
// MIN-SEVERITY SEARCH + MUTUAL EXCLUSIVITY OF THE TWO MODES
// =============================================================================

describe('Slice 08 search modes — body-contains OR min-severity, never both', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('switches to the severity mode and searches by the floor, sending min_severity and never body_contains', async () => {
    const { fetchFn, calls } = logsFetch(() => jsonResponse([CORRELATED_LOG, BARE_LOG]));
    renderPanel(fetchFn);

    // Switch the mode to severity; the body input is replaced by the floor select.
    fireEvent.change(screen.getByTestId('log-search-mode'), { target: { value: 'severity' } });
    expect(screen.queryByTestId('log-body-input')).toBeNull();
    const severity = screen.getByTestId('log-severity-select') as HTMLSelectElement;
    fireEvent.change(severity, { target: { value: 'WARN' } });
    await userEvent.click(screen.getByTestId('log-run-button'));

    await waitFor(() => expect(calls.some((c) => c.includes('/logs'))).toBe(true));
    const issued = calls.filter((c) => c.includes('/logs')).at(-1)!;
    const params = new URL(issued, 'http://x').searchParams;
    expect(params.get('min_severity')).toBe('WARN');
    // The mutually-exclusive sibling is NEVER on the wire.
    expect(params.has('body_contains')).toBe(false);
  });

  it('shows exactly one mode control at a time — only the body input OR only the severity select', () => {
    const { fetchFn } = logsFetch();
    renderPanel(fetchFn);

    // Default mode shows the body input and no severity select.
    expect(screen.getByTestId('log-body-input')).not.toBeNull();
    expect(screen.queryByTestId('log-severity-select')).toBeNull();

    // Switching to severity shows the severity select and removes the body input.
    fireEvent.change(screen.getByTestId('log-search-mode'), { target: { value: 'severity' } });
    expect(screen.getByTestId('log-severity-select')).not.toBeNull();
    expect(screen.queryByTestId('log-body-input')).toBeNull();
  });
});

// =============================================================================
// OUTCOME ARMS — empty / parse-error / transport-error
// =============================================================================

describe('Slice 08 logs outcome arms — the page never blanks', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('renders a calm empty state when no logs match', async () => {
    const { fetchFn } = logsFetch(() => jsonResponse([]));
    renderPanel(fetchFn);
    await runBodySearch('nothing matches');
    const empty = await screen.findByTestId('logs-empty-state');
    expect(empty.textContent).toMatch(/no .*logs/i);
  });

  it('renders a transport-error banner naming the backend when the logs fetch fails', async () => {
    const { fetchFn } = logsFetch(() => {
      throw new TypeError('Failed to fetch');
    });
    renderPanel(fetchFn);
    await runBodySearch('boom');
    const banner = await screen.findByTestId('logs-transport-error-banner');
    expect(banner.textContent).toContain('dev-local-prom');
  });

  it('renders a parse-error banner when the body is not valid JSON', async () => {
    const { fetchFn } = logsFetch(() => new Response('<<not json>>', { status: 200 }));
    renderPanel(fetchFn);
    await runBodySearch('x');
    const banner = await screen.findByTestId('logs-parse-error-banner');
    expect(banner.textContent).toMatch(/could not|parse|read/i);
  });
});

// =============================================================================
// PIVOTABILITY — a correlated log pivots; a bare one is a dead end
// =============================================================================

describe('Slice 08 pivotability — only a log carrying a trace_id offers a pivot', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('renders a pivot affordance on the correlated log and none on the bare log', async () => {
    const { fetchFn } = logsFetch();
    renderPanel(fetchFn);
    await runBodySearch('card declined');

    await waitFor(() => {
      expect(screen.getAllByTestId('log-row').length).toBe(2);
    });
    // Exactly one pivot — on the correlated log.
    const pivots = screen.getAllByTestId('log-pivot');
    expect(pivots).toHaveLength(1);
    const correlatedRow = screen
      .getAllByTestId('log-row')
      .find((r) => r.textContent?.includes('insufficient funds'))!;
    expect(within(correlatedRow).queryByTestId('log-pivot')).not.toBeNull();
    const bareRow = screen
      .getAllByTestId('log-row')
      .find((r) => r.textContent?.includes('no trace attached'))!;
    expect(within(bareRow).queryByTestId('log-pivot')).toBeNull();
  });
});

// =============================================================================
// STATUS ANNOUNCEMENTS — WCAG 2.2 AA 4.1.3, mirroring the traces view
// =============================================================================

describe('Slice 08 status announcements — a screen-reader user is told the result count', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('exposes a polite live region empty until a result arrives, then announces the count', async () => {
    const { fetchFn } = logsFetch();
    renderPanel(fetchFn);

    const status = screen.getByTestId('logs-status');
    expect(status.getAttribute('role')).toBe('status');
    expect(status.getAttribute('aria-live')).toBe('polite');
    expect(status.textContent).toBe('');

    await runBodySearch('card declined');
    await waitFor(() => {
      expect(screen.getAllByTestId('log-row').length).toBe(2);
    });
    expect(screen.getByTestId('logs-status').textContent).toContain('2 logs found');
  });
});

// =============================================================================
// THE PIVOT — deep-link to /traces?trace=… and the trace auto-opens
// =============================================================================

describe('Slice 08 the pivot — from a log to the trace WHERE+WHY on one screen', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('clicking the pivot navigates to /traces with the trace_id and auto-opens the trace detail', async () => {
    const { fetchFn, calls } = logsFetch();
    render(<App fetchFn={fetchFn} />);

    // Land on Logs via the nav, search, pivot from the correlated log.
    await waitFor(() => expect(screen.queryByTestId('nav-logs')).not.toBeNull());
    await userEvent.click(screen.getByTestId('nav-logs'));
    await waitFor(() => expect(screen.queryByTestId('logs-explorer-panel')).not.toBeNull());
    await runBodySearch('card declined');
    await waitFor(() => expect(screen.getAllByTestId('log-row').length).toBe(2));

    await userEvent.click(screen.getByTestId('log-pivot'));

    // We are now on /traces, carrying the trace_id, and the linked-view
    // detail auto-opened: the error span message (WHERE) is shown.
    await waitFor(() => {
      expect(window.location.pathname).toBe('/traces');
    });
    expect(window.location.search).toContain(`trace=${TRACE_ID}`);
    const detail = await screen.findByTestId('trace-detail');
    expect(within(detail).getByTestId('span-status-message').textContent).toContain(
      'payment gateway returned 402',
    );
    // The deep pivot fetched the trace-with-logs for that id.
    expect(calls.some((c) => c.includes('/traces/with_logs') && c.includes(TRACE_ID))).toBe(true);
  });

  it('TraceExplorerPanel given an initialTraceId auto-opens that trace detail on mount (WHERE+WHY)', async () => {
    const { fetchFn } = logsFetch();
    render(
      <MemoryRouter initialEntries={[`/traces?trace=${TRACE_ID}`]}>
        <TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} initialTraceId={TRACE_ID} />
      </MemoryRouter>,
    );
    const detail = await screen.findByTestId('trace-detail');
    expect(within(detail).getByTestId('span-status-message').textContent).toContain(
      'payment gateway returned 402',
    );
    // WHY — the correlated cause log renders in the same detail region.
    expect(
      within(detail)
        .getAllByTestId('cause-log')
        .some((l) => l.textContent?.includes('insufficient funds')),
    ).toBe(true);
  });

  it('keeps the existing /traces manual-find behaviour when no trace param is present', async () => {
    const { fetchFn } = logsFetch();
    render(
      <MemoryRouter initialEntries={['/traces']}>
        <TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />
      </MemoryRouter>,
    );
    // No auto-open: the detail region shows its prompt, not a loaded trace.
    expect(screen.getByTestId('detail-prompt')).not.toBeNull();
    expect(screen.queryByTestId('trace-detail')).toBeNull();
  });
});

// =============================================================================
// ROUTING — Logs lives on its own route alongside Metrics and Traces
// =============================================================================

describe('Slice 08 routing — Logs is a first-class route', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('offers a Logs nav item that mounts the LogsExplorerPanel, leaving Metrics and Traces intact', async () => {
    const { fetchFn } = logsFetch();
    render(<App fetchFn={fetchFn} />);

    await waitFor(() => expect(screen.queryByTestId('query-panel')).not.toBeNull());
    expect(screen.getByTestId('nav-metrics')).not.toBeNull();
    expect(screen.getByTestId('nav-traces')).not.toBeNull();
    const logsLink = screen.getByTestId('nav-logs');

    await userEvent.click(logsLink);
    await waitFor(() => expect(screen.queryByTestId('logs-explorer-panel')).not.toBeNull());
    expect(screen.queryByTestId('query-panel')).toBeNull();
    expect(screen.queryByTestId('trace-explorer-panel')).toBeNull();

    // Traces still reachable and unbroken.
    await userEvent.click(screen.getByTestId('nav-traces'));
    await waitFor(() => expect(screen.queryByTestId('trace-explorer-panel')).not.toBeNull());
  });
});
