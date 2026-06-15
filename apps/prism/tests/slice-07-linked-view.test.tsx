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

// Slice 07 — The linked view.
//
// I am Sam, new on the on-call rotation. A checkout is failing. I open
// Prism, switch to Traces, search the checkout service for errors only,
// and the failed trace is RIGHT THERE — badged "Error" — without my
// opening every trace. I click it and on ONE screen I see the span that
// failed WITH its readable status message (WHERE) next to the correlated
// "card declined" log (WHY). I never open a second tab.
//
// Stories: the experimentable-stack-v0 linked-view goal.
// Driving ports: the App composition root (routing) + the TraceExplorerPanel
// component. Driven port mocked at the HTTP boundary (fetchFn). No real backend.

import { describe, expect, it, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { App } from '../src/app/App';
import { TraceExplorerPanel } from '../src/panels/traces/TraceExplorerPanel';
import type { LogView, Span, TraceWithLogs } from '../src/lib/traces/types';

const TEST_CONFIG = {
  backend: { url: '/api/v1', label: 'dev-local-prom' },
  prism: { version: '0.1.0' },
} as const;

function makeSpan(overrides: Partial<Span> & Pick<Span, 'trace_id' | 'span_id'>): Span {
  return {
    name: 'GET /checkout',
    kind: 'Server',
    start_time_unix_nano: 1_700_000_000_000_000_000,
    end_time_unix_nano: 1_700_000_000_100_000_000,
    status: { code: 'Unset', message: '' },
    attributes: {},
    resource_attributes: { 'service.name': 'checkout' },
    events: [],
    links: [],
    ...overrides,
  };
}

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

// The failed-checkout trace, as the listing endpoint returns it (a flat
// Span[]): a root POST span plus its Error-status charge span.
const FAILED_SPANS: readonly Span[] = [
  makeSpan({ trace_id: 'chk-failed', span_id: 'root', name: 'POST /checkout' }),
  makeSpan({
    trace_id: 'chk-failed',
    span_id: 'charge',
    parent_span_id: 'root',
    name: 'charge-card',
    status: { code: 'Error', message: 'payment gateway returned 402' },
  }),
];

// Three healthy neighbours for the same service — the successes the failed
// checkout sits AMONG on first paint (errors-only OFF by default).
const HEALTHY_SPANS: readonly Span[] = [
  makeSpan({ trace_id: 'ok-health', span_id: 'h1', name: 'GET /health' }),
  makeSpan({ trace_id: 'ok-products', span_id: 'p1', name: 'GET /products' }),
  makeSpan({ trace_id: 'ok-cart', span_id: 'c1', name: 'GET /cart' }),
];

// All four demo traces, as the UNFILTERED listing returns them — the
// failure among the three successes.
const ALL_SPANS: readonly Span[] = [...FAILED_SPANS, ...HEALTHY_SPANS];

// The same failed trace, deepened with its correlated logs, as
// /traces/with_logs returns it.
const DETAIL_TRACE: TraceWithLogs = {
  trace_id: 'chk-failed',
  spans: [
    makeSpan({ trace_id: 'chk-failed', span_id: 'root', name: 'POST /checkout' }),
    makeSpan({
      trace_id: 'chk-failed',
      span_id: 'charge',
      parent_span_id: 'root',
      name: 'charge-card',
      status: { code: 'Error', message: 'payment gateway returned 402' },
    }),
  ],
  logs: [
    makeLog({ body: 'checkout started', trace_id: 'chk-failed', span_id: 'root' }),
    makeLog({
      severity_number: 17,
      severity_text: 'ERROR',
      body: 'card declined: insufficient funds',
      trace_id: 'chk-failed',
      span_id: 'charge',
    }),
  ],
};

interface RecordedFetch {
  readonly fetchFn: typeof fetch;
  readonly calls: string[];
}

// The unfiltered list returns all four traces; error=true narrows to just
// the failed checkout — faithful to the backend's error filter, so the
// list reacts to the errors-only toggle exactly as the real backend would.
function defaultListResponder(url: string): Response {
  return url.includes('error=true') ? jsonResponse(FAILED_SPANS) : jsonResponse(ALL_SPANS);
}

/**
 * A fetch double that routes by path: /traces/with_logs → the deep trace,
 * /traces → the flat list (branching on the error filter). Records every
 * requested URL.
 */
function listAndDetailFetch(
  listResponder: (url: string) => Response = defaultListResponder,
  detailResponder: () => Response = () => jsonResponse(DETAIL_TRACE),
): RecordedFetch {
  const calls: string[] = [];
  const fetchFn: typeof fetch = async (input) => {
    const url = String(input);
    calls.push(url);
    if (url.includes('/config.json')) return jsonResponse(TEST_CONFIG);
    if (url.includes('/traces/with_logs')) return detailResponder();
    return listResponder(url);
  };
  return { fetchFn, calls };
}

async function runSearch(service = 'checkout'): Promise<void> {
  const input = screen.getByTestId('trace-service-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value: service } });
  await userEvent.click(screen.getByTestId('trace-run-button'));
}

// =============================================================================
// OPENING CONTRACT — errors-only OFF, the failure shown AMONG the successes
// =============================================================================

describe('Slice 07 opening — the failure is shown among the successes, errors-only OFF', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('opens with the errors-only toggle OFF and the first paint lists all four traces, badging only the failed one', async () => {
    const { fetchFn } = listAndDetailFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);

    // OPENING CONTRACT — no user interaction yet: the toggle is OFF, so a
    // newcomer's FIRST view is "the failure AMONG the successes", not
    // failures-only.
    const toggle = screen.getByTestId('errors-only-toggle') as HTMLInputElement;
    expect(toggle.checked).toBe(false);

    // The first search runs with NO error filter (the day-wide default
    // window), so all four demo traces come back.
    await runSearch();

    await waitFor(() => {
      expect(screen.getAllByTestId('trace-row').length).toBe(4);
    });
    const rows = screen.getAllByTestId('trace-row');
    // The failed checkout sits among the three successes.
    const failedRow = rows.find((r) => r.textContent?.includes('POST /checkout'));
    expect(failedRow).toBeDefined();
    expect(rows.some((r) => r.textContent?.includes('GET /health'))).toBe(true);
    expect(rows.some((r) => r.textContent?.includes('GET /products'))).toBe(true);
    expect(rows.some((r) => r.textContent?.includes('GET /cart'))).toBe(true);
    // Exactly one Error badge, and it lives in the failed row — so a
    // newcomer sees WHICH trace failed without opening any of them.
    const badges = screen.getAllByTestId('trace-error-badge');
    expect(badges).toHaveLength(1);
    expect(failedRow!.contains(badges[0]!)).toBe(true);
    // The badge carries an accessible label (programmatic distinguishability).
    expect(badges[0]!.getAttribute('aria-label')).toMatch(/error/i);
    // The failed row names its service.
    expect(failedRow!.textContent).toContain('checkout');
  });
});

// =============================================================================
// LINKED DETAIL — span message (WHERE) and cause log (WHY) on ONE screen
// =============================================================================

describe('Slice 07 linked detail — selecting the failed trace shows WHERE and WHY together', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('renders the error span status message and the cause log in the same detail region', async () => {
    const { fetchFn } = listAndDetailFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);
    await runSearch();

    await waitFor(() => {
      expect(screen.getAllByTestId('trace-row').length).toBe(4);
    });
    const failedRow = screen
      .getAllByTestId('trace-row')
      .find((r) => r.textContent?.includes('POST /checkout'))!;
    await userEvent.click(failedRow);

    // The detail region appears on the SAME screen.
    const detail = await screen.findByTestId('trace-detail');

    // (a) spans — every span is a row; the error span's readable status
    //     message is shown prominently (WHERE it failed).
    await waitFor(() => {
      expect(within(detail).getAllByTestId('span-row').length).toBe(2);
    });
    const statusMessage = within(detail).getByTestId('span-status-message');
    expect(statusMessage.textContent).toContain('payment gateway returned 402');

    // (b) logs — the correlated logs render, including the ERROR-severity
    //     cause log carrying "card declined" (WHY).
    const logRows = within(detail).getAllByTestId('log-row');
    expect(logRows.length).toBe(2);
    const causeLogs = within(detail).getAllByTestId('cause-log');
    expect(causeLogs.some((l) => l.textContent?.includes('card declined'))).toBe(true);

    // WHERE and WHY are visible together — no navigation away, no second tab.
    expect(detail.contains(statusMessage)).toBe(true);
    expect(causeLogs.every((l) => detail.contains(l))).toBe(true);
  });

  it('renders a transport-error banner in the detail region when the trace fetch fails', async () => {
    const { fetchFn } = listAndDetailFetch(
      () => jsonResponse(ALL_SPANS),
      () => {
        throw new TypeError('Failed to fetch');
      },
    );
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);
    await runSearch();
    await waitFor(() => {
      expect(screen.getAllByTestId('trace-row').length).toBe(4);
    });
    const failedRow = screen
      .getAllByTestId('trace-row')
      .find((r) => r.textContent?.includes('POST /checkout'))!;
    await userEvent.click(failedRow);
    const banner = await screen.findByTestId('detail-transport-error-banner');
    expect(banner.textContent).toContain('dev-local-prom');
  });
});

// =============================================================================
// ERRORS-ONLY TOGGLE — drives the error=true query path
// =============================================================================

describe('Slice 07 errors-only toggle — switches the query between all traces and failed-only', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('opens errors-only OFF (no error filter, all four traces), and toggling it ON issues error=true and narrows to the failed checkout', async () => {
    const { fetchFn, calls } = listAndDetailFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);

    // Default is errors-only OFF — the opening view is the whole picture.
    const toggle = screen.getByTestId('errors-only-toggle') as HTMLInputElement;
    expect(toggle.checked).toBe(false);
    await runSearch();
    await waitFor(() => expect(calls.some((c) => c.includes('/traces'))).toBe(true));
    const errorOffCall = calls
      .filter((c) => c.includes('/traces') && !c.includes('with_logs'))
      .at(-1)!;
    // The first (no-interaction) search carries NO error filter.
    expect(errorOffCall).not.toContain('error=true');
    // …and lists all four traces — the failure among the three successes.
    await waitFor(() => expect(screen.getAllByTestId('trace-row').length).toBe(4));

    // Turning it ON adds the error filter — the next search narrows to the
    // single failed checkout.
    await userEvent.click(toggle);
    expect(toggle.checked).toBe(true);
    await runSearch();
    await waitFor(() => {
      const all = calls.filter((c) => c.includes('/traces') && !c.includes('with_logs'));
      expect(all.length).toBeGreaterThanOrEqual(2);
    });
    const errorOnCall = calls
      .filter((c) => c.includes('/traces') && !c.includes('with_logs'))
      .at(-1)!;
    expect(errorOnCall).toContain('error=true');
    // Narrowed to exactly the failed checkout, still badged Error.
    await waitFor(() => expect(screen.getAllByTestId('trace-row').length).toBe(1));
    const remaining = screen.getAllByTestId('trace-row');
    expect(remaining[0]!.textContent).toContain('POST /checkout');
    expect(screen.getAllByTestId('trace-error-badge')).toHaveLength(1);
  });
});

// =============================================================================
// OUTCOME ARMS — empty / transport-error / parse-error render their banners
// =============================================================================

describe('Slice 07 list outcome arms — the page never blanks', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('renders a calm empty state when there are no failed traces', async () => {
    const { fetchFn } = listAndDetailFetch(() => jsonResponse([]));
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);
    await runSearch();
    const empty = await screen.findByTestId('trace-empty-state');
    expect(empty.textContent).toMatch(/no .*traces/i);
    expect(screen.queryByTestId('trace-transport-error-banner')).toBeNull();
  });

  it('renders a transport-error banner naming the backend when the list fetch fails', async () => {
    const { fetchFn } = listAndDetailFetch(() => {
      throw new TypeError('Failed to fetch');
    });
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);
    await runSearch();
    const banner = await screen.findByTestId('trace-transport-error-banner');
    expect(banner.textContent).toContain('dev-local-prom');
    expect(banner.textContent).toContain('network');
  });

  it('renders a parse-error banner when the list body is not valid JSON', async () => {
    const { fetchFn } = listAndDetailFetch(() => new Response('<<not json>>', { status: 200 }));
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);
    await runSearch();
    const banner = await screen.findByTestId('trace-parse-error-banner');
    expect(banner.textContent).toMatch(/could not|parse|rejected/i);
  });
});

// =============================================================================
// DEFAULT WINDOW — a newcomer sees a recently-seeded demo without widening
// =============================================================================

describe('Slice 07 default window — the seeded demo shows by default, within the backend cap', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('defaults to a day-wide window that stays strictly within the 86400s backend cap', async () => {
    const { fetchFn, calls } = listAndDetailFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);

    const nowSeconds = Math.floor(Date.now() / 1000);
    await runSearch();

    await waitFor(() => {
      expect(calls.some((c) => c.includes('/traces') && !c.includes('with_logs'))).toBe(true);
    });
    const listCall = calls.filter((c) => c.includes('/traces') && !c.includes('with_logs')).at(-1)!;
    const params = new URL(listCall, 'http://prism.test').searchParams;
    const start = Number(params.get('start'));
    const end = Number(params.get('end'));
    const windowSeconds = end - start;

    // (b) strictly within the backend's hard cap — never trips
    //     "window exceeds 86400 seconds" (query-http-common MAX_WINDOW_SECONDS).
    expect(windowSeconds).toBeLessThan(86_400);
    // (a) close to a day — wide enough that a demo seeded several hours ago
    //     is visible without the operator widening the range by hand.
    expect(windowSeconds).toBeGreaterThanOrEqual(20 * 3_600);
    // The window reaches back far enough to include a trace timestamped 12 h ago.
    expect(start).toBeLessThanOrEqual(nowSeconds - 12 * 3_600);
    // …and ends at "now".
    expect(end).toBeGreaterThanOrEqual(nowSeconds - 5);
  });

  it('exposes the chosen default in the picker so it is coherent and changeable', () => {
    const { fetchFn } = listAndDetailFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);
    const picker = screen.getByTestId('time-range-picker') as HTMLSelectElement;
    expect(picker.value).toBe('-24h');
  });
});

// =============================================================================
// ROUTING — the metrics route still works, nav switches to traces
// =============================================================================

describe('Slice 07 routing — Metrics and Traces live on separate routes', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('renders QueryPanel on / and switches to the TraceExplorerPanel via the nav', async () => {
    const { fetchFn } = listAndDetailFetch();
    render(<App fetchFn={fetchFn} />);

    // The metrics route renders the existing QueryPanel.
    await waitFor(() => {
      expect(screen.queryByTestId('query-panel')).not.toBeNull();
    });
    expect(screen.queryByTestId('trace-explorer-panel')).toBeNull();

    // The nav offers both destinations.
    expect(screen.getByTestId('nav-metrics')).not.toBeNull();
    const tracesLink = screen.getByTestId('nav-traces');

    // Switching to Traces mounts the linked-view panel and unmounts QueryPanel.
    await userEvent.click(tracesLink);
    await waitFor(() => {
      expect(screen.queryByTestId('trace-explorer-panel')).not.toBeNull();
    });
    expect(screen.queryByTestId('query-panel')).toBeNull();

    // And back again.
    await userEvent.click(screen.getByTestId('nav-metrics'));
    await waitFor(() => {
      expect(screen.queryByTestId('query-panel')).not.toBeNull();
    });
    expect(screen.queryByTestId('trace-explorer-panel')).toBeNull();
  });
});
