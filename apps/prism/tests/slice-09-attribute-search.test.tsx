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

// Slice 09 — the identifier journey, on screen.
//
// I am a newcomer. The demo service "checkout" is busy — many customers,
// many traces. I want ONE customer: Alice. I type the attribute key
// `customer.id` and the value `alice`, Search, and the list collapses
// from the whole crowd to just Alice's traces. I open her failed checkout
// and read, on the same screen, WHERE it broke (the error span's status
// message) next to WHY (the correlated "card declined" cause log).
//
// Driving port: the TraceExplorerPanel component. Driven port mocked at
// the HTTP boundary (fetchFn). No real backend.

import { describe, expect, it, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

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

// Alice's two traces — a failed checkout (Error span carrying the readable
// WHERE message) and a healthy cart. Both tagged customer.id=alice.
const ALICE_SPANS: readonly Span[] = [
  makeSpan({
    trace_id: 'alice-1',
    span_id: 'a-root',
    name: 'POST /checkout',
    attributes: { 'customer.id': 'alice' },
    status: { code: 'Error', message: 'payment gateway returned 402 for alice' },
  }),
  makeSpan({
    trace_id: 'alice-2',
    span_id: 'a-cart',
    name: 'GET /cart',
    attributes: { 'customer.id': 'alice' },
  }),
];

// Bob's and Carol's traces — the crowd Alice sits among. Each carries an
// operation unique to that customer, so the filter's discrimination is
// observable in the rendered rows.
const BOB_SPANS: readonly Span[] = [
  makeSpan({
    trace_id: 'bob-1',
    span_id: 'b1',
    name: 'POST /checkout',
    attributes: { 'customer.id': 'bob' },
  }),
  makeSpan({
    trace_id: 'bob-2',
    span_id: 'b2',
    name: 'GET /products',
    attributes: { 'customer.id': 'bob' },
  }),
];

const CAROL_SPANS: readonly Span[] = [
  makeSpan({
    trace_id: 'carol-1',
    span_id: 'c1',
    name: 'GET /health',
    attributes: { 'customer.id': 'carol' },
  }),
];

// The whole crowd: five traces across three customers for one service.
const ALL_SPANS: readonly Span[] = [...ALICE_SPANS, ...BOB_SPANS, ...CAROL_SPANS];

// Alice's failed checkout, deepened with its correlated logs.
const ALICE_DETAIL: TraceWithLogs = {
  trace_id: 'alice-1',
  spans: [
    makeSpan({ trace_id: 'alice-1', span_id: 'a-root', name: 'POST /checkout' }),
    makeSpan({
      trace_id: 'alice-1',
      span_id: 'a-charge',
      parent_span_id: 'a-root',
      name: 'charge-card',
      status: { code: 'Error', message: 'payment gateway returned 402 for alice' },
    }),
  ],
  logs: [
    makeLog({ body: 'checkout started for alice', trace_id: 'alice-1', span_id: 'a-root' }),
    makeLog({
      severity_number: 17,
      severity_text: 'ERROR',
      body: 'card declined: insufficient funds (alice)',
      trace_id: 'alice-1',
      span_id: 'a-charge',
    }),
  ],
};

interface RecordedFetch {
  readonly fetchFn: typeof fetch;
  readonly calls: string[];
}

/**
 * A fetch double faithful to the backend's attribute filter: a listing call
 * carrying attr_value=alice returns only Alice's traces (and, with
 * error=true, only her failed one); without the filter it returns the whole
 * crowd. /traces/with_logs returns Alice's deep trace.
 */
function crowdFetch(): RecordedFetch {
  const calls: string[] = [];
  const fetchFn: typeof fetch = async (input) => {
    const url = String(input);
    calls.push(url);
    if (url.includes('/traces/with_logs')) return jsonResponse(ALICE_DETAIL);
    const params = new URL(url, 'http://x').searchParams;
    const errorOnly = params.get('error') === 'true';
    if (params.get('attr_value') === 'alice') {
      return jsonResponse(errorOnly ? [ALICE_SPANS[0]!] : ALICE_SPANS);
    }
    return jsonResponse(errorOnly ? [ALICE_SPANS[0]!] : ALL_SPANS);
  };
  return { fetchFn, calls };
}

function setService(value = 'checkout'): void {
  fireEvent.change(screen.getByTestId('trace-service-input'), { target: { value } });
}
function setAttrKey(value: string): void {
  fireEvent.change(screen.getByTestId('trace-attr-key-input'), { target: { value } });
}
function setAttrValue(value: string): void {
  fireEvent.change(screen.getByTestId('trace-attr-value-input'), { target: { value } });
}
function lastListCall(calls: string[]): string {
  return calls.filter((c) => c.includes('/traces') && !c.includes('with_logs')).at(-1)!;
}

// =============================================================================
// THE IDENTIFIER JOURNEY — many customers, filtered to one, then WHERE+WHY
// =============================================================================

describe('Slice 09 — searching one identifier filters the crowd to one customer', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('narrows the whole crowd to just Alice when customer.id=alice is searched, carrying attr_key+attr_value, then opens her failed trace to show WHERE + WHY', async () => {
    const { fetchFn, calls } = crowdFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);

    // First, the whole crowd: a plain service search lists all five traces.
    setService('checkout');
    await userEvent.click(screen.getByTestId('trace-run-button'));
    await waitFor(() => expect(screen.getAllByTestId('trace-row').length).toBe(5));
    // The unfiltered call carried no attribute params.
    expect(lastListCall(calls)).not.toContain('attr_key');

    // Now I name ONE customer: customer.id = alice.
    setAttrKey('customer.id');
    setAttrValue('alice');
    await userEvent.click(screen.getByTestId('trace-run-button'));

    // The listing query carries the attribute filter on the wire.
    await waitFor(() => expect(lastListCall(calls)).toContain('attr_key=customer.id'));
    const issued = new URL(lastListCall(calls), 'http://x');
    expect(issued.searchParams.get('attr_key')).toBe('customer.id');
    expect(issued.searchParams.get('attr_value')).toBe('alice');

    // The list collapses to Alice's two traces only.
    await waitFor(() => expect(screen.getAllByTestId('trace-row').length).toBe(2));
    const rows = screen.getAllByTestId('trace-row');
    const rowText = rows.map((r) => r.textContent ?? '').join('|');
    expect(rowText).toContain('GET /cart'); // Alice's
    expect(rowText).not.toContain('GET /products'); // Bob's — gone
    expect(rowText).not.toContain('GET /health'); // Carol's — gone

    // Open Alice's failed checkout (the badged row).
    const failedRow = rows.find((r) => r.textContent?.includes('POST /checkout'))!;
    await userEvent.click(failedRow);
    const detail = await screen.findByTestId('trace-detail');

    // WHERE — the error span's readable status message.
    const where = within(detail).getByTestId('span-status-message');
    expect(where.textContent).toContain('payment gateway returned 402 for alice');
    // WHY — the correlated cause log, in the SAME region.
    const cause = within(detail).getAllByTestId('cause-log');
    expect(cause.some((l) => l.textContent?.includes('card declined'))).toBe(true);
  });

  it('composes the attribute filter with the errors-only toggle (both params on the wire)', async () => {
    const { fetchFn, calls } = crowdFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);

    setService('checkout');
    await userEvent.click(screen.getByTestId('errors-only-toggle'));
    setAttrKey('customer.id');
    setAttrValue('alice');
    await userEvent.click(screen.getByTestId('trace-run-button'));

    await waitFor(() => expect(lastListCall(calls)).toContain('attr_key=customer.id'));
    const issued = new URL(lastListCall(calls), 'http://x');
    expect(issued.searchParams.get('error')).toBe('true');
    expect(issued.searchParams.get('attr_key')).toBe('customer.id');
    expect(issued.searchParams.get('attr_value')).toBe('alice');

    // Narrowed to Alice's single failed checkout.
    await waitFor(() => expect(screen.getAllByTestId('trace-row').length).toBe(1));
    expect(screen.getAllByTestId('trace-row')[0]!.textContent).toContain('POST /checkout');
    expect(screen.getAllByTestId('trace-error-badge')).toHaveLength(1);
  });
});

// =============================================================================
// BOTH-OR-NEITHER — exactly one field never reaches the wire (no 400)
// =============================================================================

describe('Slice 09 — the attribute filter is both-or-neither in the UI', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('disables Search and shows inline validation when only one attribute field is filled, issuing no request', async () => {
    const { fetchFn, calls } = crowdFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);

    setService('checkout');
    // Only the key is filled — a partial filter the backend would 400 on.
    setAttrKey('customer.id');

    const button = screen.getByTestId('trace-run-button') as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    const validation = screen.getByTestId('trace-attr-validation');
    expect(validation.textContent).toMatch(/both|key and value/i);

    // The disabled button cannot fire a search; nothing partial on the wire.
    await userEvent.click(button);
    expect(calls.some((c) => c.includes('attr_key') || c.includes('attr_value'))).toBe(false);

    // Reverse: only the value filled is equally invalid.
    setAttrKey('');
    setAttrValue('alice');
    expect((screen.getByTestId('trace-run-button') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByTestId('trace-attr-validation')).not.toBeNull();

    // Completing the pair clears the validation and re-enables Search.
    setAttrKey('customer.id');
    expect((screen.getByTestId('trace-run-button') as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByTestId('trace-attr-validation')).toBeNull();
  });

  it('leaves the existing find unchanged when both attribute fields are empty (no attr params)', async () => {
    const { fetchFn, calls } = crowdFetch();
    render(<TraceExplorerPanel config={TEST_CONFIG} fetchFn={fetchFn} />);

    setService('checkout');
    await userEvent.click(screen.getByTestId('trace-run-button'));

    await waitFor(() => expect(screen.getAllByTestId('trace-row').length).toBe(5));
    const issued = new URL(lastListCall(calls), 'http://x');
    expect(issued.searchParams.has('attr_key')).toBe(false);
    expect(issued.searchParams.has('attr_value')).toBe(false);
  });
});
