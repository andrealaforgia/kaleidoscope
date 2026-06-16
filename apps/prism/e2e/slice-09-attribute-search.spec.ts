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

// Slice 09 — the identifier journey, cold flow in a REAL browser.
//
// I am a newcomer. The demo-checkout service is busy — many customers,
// many traces. I want ONE customer: Alice. I switch to Traces, type the
// attribute key `customer.id` and value `alice`, Search, and the list
// collapses from the whole crowd to just Alice's traces. I open her failed
// checkout and read, on the SAME screen, WHERE it broke (the error span's
// status message) next to WHY (the correlated "card declined" cause log).
//
// -----------------------------------------------------------------------
// BACKEND-PROVISIONING CHOICE — APPROACH 2 (page.route interception),
// identical to slices 07/08. The Prism e2e harness has no Rust traces
// backend; webServer runs the real Vite-served SPA and globalSetup
// provisions ONLY a Prometheus container for the metrics slices. So we
// drive the REAL served SPA in a real browser with Playwright `page.route`
// fulfilling the same-origin trace calls with canned JSON faithful to the
// wire shapes (ray::Span[] for the find surface; {trace_id, spans, logs}
// for with_logs). The route branches on the `attr_value` query param,
// modelling the backend's attribute filter: attr_value=alice returns only
// Alice's traces; absent returns the whole crowd. The backend filter
// itself is verified independently (Rust-side) and by the client's vitest
// suite over these shapes.
// -----------------------------------------------------------------------

import { test, expect, type Page } from '@playwright/test';

const SERVICE = 'demo-checkout';
const WHERE_MESSAGE = 'checkout failed: card declined';
const WHY_LOG_BODY = 'card declined: insufficient funds';

const BASE_MS = 1_733_000_000_000;
const nanos = (offsetMs: number): number => (BASE_MS + offsetMs) * 1_000_000;

interface WireSpan {
  trace_id: string;
  span_id: string;
  parent_span_id?: string;
  name: string;
  kind: string;
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  status: { code: 'Unset' | 'Ok' | 'Error'; message: string };
  attributes: Record<string, string>;
  resource_attributes: Record<string, string>;
  events: unknown[];
  links: unknown[];
}

function span(over: Partial<WireSpan> & Pick<WireSpan, 'trace_id' | 'span_id' | 'name'>): WireSpan {
  return {
    kind: 'Server',
    start_time_unix_nano: nanos(0),
    end_time_unix_nano: nanos(120),
    status: { code: 'Ok', message: '' },
    attributes: {},
    resource_attributes: { 'service.name': SERVICE },
    events: [],
    links: [],
    ...over,
  };
}

// Alice's two traces — a failed checkout (Error + readable WHERE message)
// and a healthy cart. Both tagged customer.id=alice.
const aliceSpans: WireSpan[] = [
  span({
    trace_id: 'a'.repeat(32),
    span_id: 'a1a1a1a1a1a1a1a1',
    name: 'POST /api/v1/checkout',
    attributes: { 'customer.id': 'alice' },
    status: { code: 'Error', message: WHERE_MESSAGE },
  }),
  span({
    trace_id: 'a'.repeat(31) + '2',
    span_id: 'a2a2a2a2a2a2a2a2',
    name: 'GET /api/v1/cart',
    attributes: { 'customer.id': 'alice' },
  }),
];

// Bob's and Carol's traces — the crowd Alice sits among, each with an
// operation unique to that customer so the filter's effect is visible.
const otherSpans: WireSpan[] = [
  span({
    trace_id: 'b'.repeat(32),
    span_id: 'b1b1b1b1b1b1b1b1',
    name: 'GET /api/v1/products',
    attributes: { 'customer.id': 'bob' },
  }),
  span({
    trace_id: 'c'.repeat(32),
    span_id: 'c1c1c1c1c1c1c1c1',
    name: 'GET /api/v1/health',
    attributes: { 'customer.id': 'carol' },
  }),
];

const allSpans: WireSpan[] = [...aliceSpans, ...otherSpans];

const aliceWithLogs = {
  trace_id: 'a'.repeat(32),
  spans: aliceSpans,
  logs: [
    {
      observed_time_unix_nano: nanos(10),
      severity_number: 9,
      severity_text: 'INFO',
      body: 'checkout started for alice',
      attributes: {},
      resource_attributes: { 'service.name': SERVICE },
      trace_id: 'a'.repeat(32),
      span_id: 'a1a1a1a1a1a1a1a1',
    },
    {
      observed_time_unix_nano: nanos(95),
      severity_number: 17,
      severity_text: 'ERROR',
      body: WHY_LOG_BODY,
      attributes: { 'payment.decline_code': 'insufficient_funds' },
      resource_attributes: { 'service.name': SERVICE },
      trace_id: 'a'.repeat(32),
      span_id: 'a1a1a1a1a1a1a1a1',
    },
  ],
};

// Same-origin trace API from canned data: with_logs → Alice's deep trace;
// the find surface branches on attr_value (the attribute filter) and on
// error (the errors-only filter), exactly as the real backend composes them.
async function stubTracesBackend(page: Page): Promise<void> {
  await page.route(/\/api\/v1\/traces/, async (route) => {
    const url = new URL(route.request().url());
    const json = (body: unknown): Promise<void> =>
      route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

    if (url.pathname.endsWith('/traces/with_logs')) return json(aliceWithLogs);

    const errorOnly = url.searchParams.get('error') === 'true';
    if (url.searchParams.get('attr_value') === 'alice') {
      return json(errorOnly ? [aliceSpans[0]] : aliceSpans);
    }
    return json(errorOnly ? [aliceSpans[0]] : allSpans);
  });
}

test.describe('Slice 09 attribute search — the identifier journey, one customer among many', () => {
  let pageErrors: Error[] = [];

  test.beforeEach(async ({ page }) => {
    pageErrors = [];
    page.on('pageerror', (err) => pageErrors.push(err));
    await stubTracesBackend(page);
  });

  test.afterEach(() => {
    expect(pageErrors).toEqual([]);
  });

  test('searches customer.id=alice, narrows the crowd to Alice, and reads WHERE + WHY on one screen', async ({
    page,
  }) => {
    // GIVEN the Traces view.
    await page.goto('/');
    await page.getByTestId('nav-traces').click();
    await expect(page.getByTestId('trace-explorer-panel')).toBeVisible();

    // The whole crowd first: a plain service search lists all four traces.
    await page.getByTestId('trace-service-input').fill(SERVICE);
    await page.getByTestId('trace-run-button').click();
    await expect(page.getByTestId('trace-row')).toHaveCount(4);

    // BOTH-OR-NEITHER: filling only the attribute KEY disables Search and
    // surfaces inline validation — no partial request that the backend 400s.
    await page.getByTestId('trace-attr-key-input').fill('customer.id');
    await expect(page.getByTestId('trace-attr-validation')).toBeVisible();
    await expect(page.getByTestId('trace-run-button')).toBeDisabled();

    // Completing the pair clears the validation and re-enables Search.
    await page.getByTestId('trace-attr-value-input').fill('alice');
    await expect(page.getByTestId('trace-attr-validation')).toHaveCount(0);
    await expect(page.getByTestId('trace-run-button')).toBeEnabled();

    // WHEN I search for that one identifier.
    await page.getByTestId('trace-run-button').click();

    // THEN the list collapses to Alice's two traces — Bob's and Carol's
    // operations are gone.
    const rows = page.getByTestId('trace-row');
    await expect(rows).toHaveCount(2);
    await expect(page.getByText('GET /api/v1/products')).toHaveCount(0);
    await expect(page.getByText('GET /api/v1/health')).toHaveCount(0);
    await expect(rows.filter({ has: page.getByTestId('trace-error-badge') })).toContainText(
      'POST /api/v1/checkout',
    );

    // Open Alice's failed checkout.
    await rows.filter({ has: page.getByTestId('trace-error-badge') }).click();

    // The linked payoff, in ONE detail region: WHERE next to WHY.
    const detailBody = page.getByTestId('trace-detail-region').getByTestId('trace-detail');
    await expect(detailBody).toBeVisible();
    const where = detailBody.getByTestId('span-status-message');
    await expect(where).toContainText(WHERE_MESSAGE);
    const why = detailBody.getByTestId('cause-log');
    await expect(why).toContainText('card declined');

    expect(new URL(page.url()).pathname).toBe('/traces');
    expect(page.context().pages()).toHaveLength(1);
  });
});
